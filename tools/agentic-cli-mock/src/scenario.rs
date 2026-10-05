//! Loading and validating the scenario document.
//!
//! Every failure here is a configuration error (ACM-FR-23 `check: "scenario"`),
//! categorised by ACM-FR-25's fixed vocabulary. Validation is written out by
//! hand against `serde_json::Value` rather than derived, because the categories
//! are part of the contract: a derived error would have to be recovered from a
//! prose message, and the message is not something callers may depend on.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::fmt;

use crate::diagnostics::MismatchType;

/// ACM-FR-14: the documented bound on a configured delay.
pub const MAX_DELAY_MS: u64 = 10_000;

/// ACM-FR-12: the three distinct states of `expected.stdin`. Absent and `null`
/// are different scenarios, not two spellings of one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StdinExpectation {
    /// Key absent — stdin is not validated.
    Unchecked,
    /// `null` — the caller must have provided zero bytes.
    Empty,
    /// A payload — the received bytes must equal these exactly.
    Exact(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requirement {
    /// ACM-FR-11: diagnostic metadata. Never compared against anything.
    pub name: Option<String>,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expected {
    pub args: Option<Vec<String>>,
    pub required_args: Option<Vec<Requirement>>,
    pub stdin: StdinExpectation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: u8,
    pub delay_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scenario {
    pub expected: Expected,
    pub response: Response,
}

/// A shape that accepts any JSON *except* an object repeating a key.
///
/// ACM-FR-16: the mock never resolves a duplicate by taking the first or last
/// occurrence, and `serde_json::Value` cannot express the problem — its map
/// silently keeps one. Detecting it therefore has to happen while the document
/// is still a token stream, which is what this visitor does.
struct DuplicateKeyScan;

impl<'de> Deserialize<'de> for DuplicateKeyScan {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(DuplicateKeyVisitor)
    }
}

struct DuplicateKeyVisitor;

impl<'de> Visitor<'de> for DuplicateKeyVisitor {
    type Value = DuplicateKeyScan;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("any JSON value")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut seen: HashSet<String> = HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !seen.insert(key) {
                // The only `Category::Data` error this scan can produce, which
                // is what lets `parse` tell a duplicate from a syntax error
                // without reading the message.
                return Err(de::Error::custom("duplicate key"));
            }
            map.next_value::<DuplicateKeyScan>()?;
        }
        Ok(DuplicateKeyScan)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        while seq.next_element::<DuplicateKeyScan>()?.is_some() {}
        Ok(DuplicateKeyScan)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(DuplicateKeyScan)
    }
    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(DuplicateKeyScan)
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
        Ok(DuplicateKeyScan)
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
        Ok(DuplicateKeyScan)
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
        Ok(DuplicateKeyScan)
    }
    // Not reachable through `deserialize_any` today, but serde's defaults for
    // these return a `Category::Data` error — which `parse` would then read as
    // a duplicate key. Accepting them explicitly keeps the classification below
    // true no matter how a number reaches the visitor.
    fn visit_i128<E: de::Error>(self, _: i128) -> Result<Self::Value, E> {
        Ok(DuplicateKeyScan)
    }
    fn visit_u128<E: de::Error>(self, _: u128) -> Result<Self::Value, E> {
        Ok(DuplicateKeyScan)
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
        Ok(DuplicateKeyScan)
    }
    fn visit_str<E: de::Error>(self, _: &str) -> Result<Self::Value, E> {
        Ok(DuplicateKeyScan)
    }
}

/// Parse and validate a scenario document.
pub fn parse(text: &str) -> Result<Scenario, MismatchType> {
    // Pass one: syntax, and the duplicate keys a `Value` would swallow.
    if let Err(error) = serde_json::from_str::<DuplicateKeyScan>(text) {
        return Err(match error.classify() {
            serde_json::error::Category::Syntax | serde_json::error::Category::Eof => {
                MismatchType::MalformedJson
            }
            // The scan accepts every well-formed JSON value, so the only data
            // error it can raise is the duplicate above.
            _ => MismatchType::DuplicateKey,
        });
    }

    let document: Value = serde_json::from_str(text).map_err(|_| MismatchType::MalformedJson)?;
    let document = object(&document)?;
    deny_unknown(document, &["version", "expected", "response"])?;

    // ACM-FR-08: the integer 1 and nothing else — not "1", not 1.0.
    match document.get("version").and_then(Value::as_i64) {
        Some(1) => {}
        _ => return Err(MismatchType::BadVersion),
    }

    let expected = parse_expected(object(document.get("expected").ok_or(MismatchType::BadType)?)?)?;
    let response = parse_response(object(document.get("response").ok_or(MismatchType::BadType)?)?)?;

    Ok(Scenario { expected, response })
}

fn parse_expected(map: &Map<String, Value>) -> Result<Expected, MismatchType> {
    deny_unknown(map, &["args", "required_args", "stdin"])?;

    // ACM-FR-10: an exact ordered list of strings. An empty list is valid.
    let args = match map.get("args") {
        None => None,
        Some(value) => {
            let items = value.as_array().ok_or(MismatchType::BadType)?;
            let mut args = Vec::with_capacity(items.len());
            for item in items {
                args.push(item.as_str().ok_or(MismatchType::BadType)?.to_owned());
            }
            Some(args)
        }
    };

    // ACM-FR-11: `{ value }` or `{ name, value }`. An empty list is valid.
    let required_args = match map.get("required_args") {
        None => None,
        Some(value) => {
            let items = value.as_array().ok_or(MismatchType::BadType)?;
            let mut requirements = Vec::with_capacity(items.len());
            for item in items {
                let item = object(item)?;
                deny_unknown(item, &["name", "value"])?;
                let value = item
                    .get("value")
                    .and_then(Value::as_str)
                    .ok_or(MismatchType::BadType)?
                    .to_owned();
                let name = match item.get("name") {
                    None => None,
                    Some(name) => Some(name.as_str().ok_or(MismatchType::BadType)?.to_owned()),
                };
                requirements.push(Requirement { name, value });
            }
            Some(requirements)
        }
    };

    // ACM-FR-12: absent, `null`, and a payload are three distinct states.
    let stdin = match map.get("stdin") {
        None => StdinExpectation::Unchecked,
        Some(Value::Null) => StdinExpectation::Empty,
        Some(value) => StdinExpectation::Exact(parse_payload(value)?),
    };

    Ok(Expected {
        args,
        required_args,
        stdin,
    })
}

fn parse_response(map: &Map<String, Value>) -> Result<Response, MismatchType> {
    deny_unknown(map, &["stdout", "stderr", "exit_code", "delay_ms"])?;

    let stdout = parse_payload(map.get("stdout").ok_or(MismatchType::BadType)?)?;
    let stderr = parse_payload(map.get("stderr").ok_or(MismatchType::BadType)?)?;

    // ACM-FR-13: 0..=255 and never a reserved code, so a scenario can never
    // make a simulated failure indistinguishable from a failure of the mock.
    let exit_code = match map.get("exit_code").and_then(Value::as_i64) {
        Some(code)
            if (0..=255).contains(&code)
                && code != i64::from(crate::diagnostics::EXIT_CONFIGURATION_ERROR)
                && code != i64::from(crate::diagnostics::EXIT_EXPECTATION_MISMATCH) =>
        {
            code as u8
        }
        _ => return Err(MismatchType::BadExitCode),
    };

    // ACM-FR-14: optional, defaults to 0, bounded.
    let delay_ms = match map.get("delay_ms") {
        None => 0,
        Some(value) => match value.as_u64() {
            Some(delay) if delay <= MAX_DELAY_MS => delay,
            _ => return Err(MismatchType::BadDelay),
        },
    };

    Ok(Response {
        stdout,
        stderr,
        exit_code,
        delay_ms,
    })
}

/// ACM-FR-09: a bare string, or an object naming one of the two encodings. The
/// bare form is exactly the `utf8` object form. Decoding happens once, here;
/// everything downstream deals in bytes and never re-interprets them.
fn parse_payload(value: &Value) -> Result<Vec<u8>, MismatchType> {
    match value {
        Value::String(text) => Ok(text.as_bytes().to_vec()),
        Value::Object(map) => {
            deny_unknown(map, &["encoding", "value"])?;
            let encoding = map
                .get("encoding")
                .and_then(Value::as_str)
                .ok_or(MismatchType::BadEncoding)?;
            let text = map
                .get("value")
                .and_then(Value::as_str)
                .ok_or(MismatchType::BadType)?;

            match encoding {
                "utf8" => Ok(text.as_bytes().to_vec()),
                "base64" => BASE64.decode(text).map_err(|_| MismatchType::BadEncoding),
                _ => Err(MismatchType::BadEncoding),
            }
        }
        _ => Err(MismatchType::BadType),
    }
}

fn object(value: &Value) -> Result<&Map<String, Value>, MismatchType> {
    value.as_object().ok_or(MismatchType::BadType)
}

/// ACM-FR-15: an unrecognised key is never ignored, at any level.
fn deny_unknown(map: &Map<String, Value>, allowed: &[&str]) -> Result<(), MismatchType> {
    for key in map.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(MismatchType::UnknownField);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"{
        "version": 1,
        "expected": {},
        "response": {"stdout": "", "stderr": "", "exit_code": 0}
    }"#;

    fn with_response(body: &str) -> String {
        format!(
            r#"{{"version":1,"expected":{{}},"response":{}}}"#,
            body
        )
    }

    fn with_expected(body: &str) -> String {
        format!(
            r#"{{"version":1,"expected":{},"response":{{"stdout":"","stderr":"","exit_code":0}}}}"#,
            body
        )
    }

    #[test]
    fn the_minimal_document_parses() {
        let scenario = parse(MINIMAL).expect("valid");
        assert_eq!(scenario.expected.args, None);
        assert_eq!(scenario.expected.required_args, None);
        assert_eq!(scenario.expected.stdin, StdinExpectation::Unchecked);
        assert_eq!(scenario.response.stdout, b"");
        assert_eq!(scenario.response.exit_code, 0);
        assert_eq!(scenario.response.delay_ms, 0);
    }

    // --- ACM-FR-08: version ------------------------------------------------

    #[test]
    fn version_must_be_the_integer_one() {
        for bad in ["2", "\"1\"", "1.0", "null", "true"] {
            let text = format!(
                r#"{{"version":{bad},"expected":{{}},"response":{{"stdout":"","stderr":"","exit_code":0}}}}"#
            );
            assert_eq!(parse(&text), Err(MismatchType::BadVersion), "for {bad}");
        }
    }

    #[test]
    fn a_missing_version_is_a_version_problem() {
        let text = r#"{"expected":{},"response":{"stdout":"","stderr":"","exit_code":0}}"#;
        assert_eq!(parse(text), Err(MismatchType::BadVersion));
    }

    // --- ACM-FR-15 / ACM-FR-16: unknown fields and duplicate keys ----------

    #[test]
    fn an_unknown_field_is_rejected_at_every_level() {
        let cases = [
            r#"{"version":1,"timeout_ms":5,"expected":{},"response":{"stdout":"","stderr":"","exit_code":0}}"#.to_string(),
            with_expected(r#"{"argv":[]}"#),
            with_response(r#"{"stdout":"","stderr":"","exit_code":0,"retries":1}"#),
            with_expected(r#"{"required_args":[{"value":"x","label":"y"}]}"#),
            with_response(r#"{"stdout":{"encoding":"utf8","value":"","extra":1},"stderr":"","exit_code":0}"#),
        ];
        for text in cases {
            assert_eq!(parse(&text), Err(MismatchType::UnknownField), "for {text}");
        }
    }

    #[test]
    fn a_duplicate_key_is_rejected_rather_than_resolved() {
        let cases = [
            r#"{"version":1,"version":1,"expected":{},"response":{"stdout":"","stderr":"","exit_code":0}}"#,
            r#"{"version":1,"expected":{"args":[],"args":[]},"response":{"stdout":"","stderr":"","exit_code":0}}"#,
            r#"{"version":1,"expected":{},"response":{"stdout":"","stdout":"x","stderr":"","exit_code":0}}"#,
            r#"{"version":1,"expected":{"required_args":[{"value":"a","value":"b"}]},"response":{"stdout":"","stderr":"","exit_code":0}}"#,
            // Nested inside a payload object.
            r#"{"version":1,"expected":{},"response":{"stdout":{"encoding":"utf8","encoding":"base64","value":"x"},"stderr":"","exit_code":0}}"#,
            // Deeper inside an array element.
            r#"{"version":1,"expected":{"required_args":[{"value":"a"},{"name":"n","name":"m","value":"b"}]},"response":{"stdout":"","stderr":"","exit_code":0}}"#,
            // Escaped and literal spellings of the same key are the same key.
            r#"{"version":1,"version":1,"expected":{},"response":{"stdout":"","stderr":"","exit_code":0}}"#,
        ];
        for text in cases {
            assert_eq!(parse(text), Err(MismatchType::DuplicateKey), "for {text}");
        }
    }

    /// The classification in `parse` treats every non-syntax error from the
    /// scan as a duplicate key. That is only sound while the scan accepts every
    /// well-formed JSON value, so exercise the value types it has to survive.
    #[test]
    fn the_duplicate_scan_accepts_every_json_value_type() {
        // Unknown fields are rejected later, by name — but the scan itself must
        // have walked all of this without raising a data error, which is what
        // `unknown_field` (rather than `duplicate_key`) proves.
        let text = r#"{
            "version": 1,
            "menagerie": {
                "nested": [[[{"deep": [1, -2, 3.5, 1e10, -1.7e-3, true, false, null, "", "é"]}]]],
                "empty_object": {},
                "empty_array": [],
                "escaped": "key",
                "big": 18446744073709551615,
                "small": -9223372036854775808
            },
            "expected": {},
            "response": {"stdout": "", "stderr": "", "exit_code": 0}
        }"#;
        assert_eq!(parse(text), Err(MismatchType::UnknownField));
    }

    /// No well-formed, duplicate-free document is ever reported as a duplicate.
    #[test]
    fn no_valid_json_is_ever_misreported_as_a_duplicate_key() {
        let documents = [
            "1e400",
            "-9223372036854775809",
            "123456789012345678901234567890",
            "0e0",
            "1E2",
            "[]",
            "{}",
            r#"{"a":{"b":{"c":[1,2,{"d":null}]}}}"#,
            MINIMAL,
        ];
        for text in documents {
            match parse(text) {
                Ok(_) => {}
                Err(mismatch) => assert_ne!(
                    mismatch,
                    MismatchType::DuplicateKey,
                    "misreported {text} as a duplicate key"
                ),
            }
        }
    }

    #[test]
    fn malformed_json_is_distinguished_from_a_duplicate_key() {
        for text in ["{", "", "not json", r#"{"version":1,}"#] {
            assert_eq!(parse(text), Err(MismatchType::MalformedJson), "for {text}");
        }
    }

    /// The duplicate scan must not reject a key that merely repeats across
    /// sibling objects — only within one.
    #[test]
    fn the_same_key_in_sibling_objects_is_fine() {
        let text = with_expected(r#"{"required_args":[{"value":"a"},{"value":"b"}]}"#);
        assert!(parse(&text).is_ok());
    }

    // --- ACM-FR-09: payloads ------------------------------------------------

    #[test]
    fn a_bare_string_is_the_utf8_object_form() {
        let bare = parse(&with_response(
            r#"{"stdout":"héllo\n","stderr":"","exit_code":0}"#,
        ))
        .unwrap();
        let object = parse(&with_response(
            r#"{"stdout":{"encoding":"utf8","value":"héllo\n"},"stderr":"","exit_code":0}"#,
        ))
        .unwrap();
        assert_eq!(bare.response.stdout, object.response.stdout);
        assert_eq!(bare.response.stdout, "héllo\n".as_bytes());
    }

    #[test]
    fn base64_decodes_to_bytes_that_need_not_be_utf8() {
        let scenario = parse(&with_response(
            r#"{"stdout":{"encoding":"base64","value":"/w7/"},"stderr":"","exit_code":0}"#,
        ))
        .unwrap();
        assert_eq!(scenario.response.stdout, vec![0xff, 0x0e, 0xff]);
        assert!(String::from_utf8(scenario.response.stdout).is_err());
    }

    #[test]
    fn an_empty_payload_decodes_to_no_bytes() {
        let scenario = parse(&with_response(
            r#"{"stdout":"","stderr":{"encoding":"base64","value":""},"exit_code":0}"#,
        ))
        .unwrap();
        assert!(scenario.response.stdout.is_empty());
        assert!(scenario.response.stderr.is_empty());
    }

    #[test]
    fn an_unknown_encoding_or_undecodable_base64_is_rejected() {
        for body in [
            r#"{"stdout":{"encoding":"utf-8","value":"x"},"stderr":"","exit_code":0}"#,
            r#"{"stdout":{"encoding":"hex","value":"ff"},"stderr":"","exit_code":0}"#,
            r#"{"stdout":{"encoding":"base64","value":"!!!!"},"stderr":"","exit_code":0}"#,
            r#"{"stdout":{"value":"x"},"stderr":"","exit_code":0}"#,
        ] {
            assert_eq!(
                parse(&with_response(body)),
                Err(MismatchType::BadEncoding),
                "for {body}"
            );
        }
    }

    #[test]
    fn a_payload_that_is_neither_string_nor_object_is_rejected() {
        for body in [
            r#"{"stdout":42,"stderr":"","exit_code":0}"#,
            r#"{"stdout":["a"],"stderr":"","exit_code":0}"#,
            r#"{"stdout":null,"stderr":"","exit_code":0}"#,
        ] {
            assert_eq!(
                parse(&with_response(body)),
                Err(MismatchType::BadType),
                "for {body}"
            );
        }
    }

    #[test]
    fn stdout_and_stderr_are_both_required() {
        assert_eq!(
            parse(&with_response(r#"{"stderr":"","exit_code":0}"#)),
            Err(MismatchType::BadType)
        );
        assert_eq!(
            parse(&with_response(r#"{"stdout":"","exit_code":0}"#)),
            Err(MismatchType::BadType)
        );
    }

    // --- ACM-FR-13: exit code ----------------------------------------------

    #[test]
    fn exit_code_accepts_the_whole_portable_range_but_the_reserved_codes() {
        for code in [0u16, 1, 42, 63, 66, 254, 255] {
            let text = with_response(&format!(
                r#"{{"stdout":"","stderr":"","exit_code":{code}}}"#
            ));
            assert_eq!(parse(&text).unwrap().response.exit_code, code as u8);
        }
    }

    #[test]
    fn exit_code_rejects_out_of_range_non_integer_and_reserved_values() {
        for bad in ["-1", "256", "64", "65", "1.5", "\"0\"", "null"] {
            let text = with_response(&format!(r#"{{"stdout":"","stderr":"","exit_code":{bad}}}"#));
            assert_eq!(parse(&text), Err(MismatchType::BadExitCode), "for {bad}");
        }
    }

    #[test]
    fn a_missing_exit_code_is_an_exit_code_problem() {
        assert_eq!(
            parse(&with_response(r#"{"stdout":"","stderr":""}"#)),
            Err(MismatchType::BadExitCode)
        );
    }

    // --- ACM-FR-14: delay ---------------------------------------------------

    #[test]
    fn delay_defaults_to_zero_and_accepts_up_to_the_bound() {
        assert_eq!(parse(MINIMAL).unwrap().response.delay_ms, 0);
        for delay in [0, 1, 250, MAX_DELAY_MS] {
            let text = with_response(&format!(
                r#"{{"stdout":"","stderr":"","exit_code":0,"delay_ms":{delay}}}"#
            ));
            assert_eq!(parse(&text).unwrap().response.delay_ms, delay);
        }
    }

    #[test]
    fn delay_rejects_negative_over_bound_and_non_integer_values() {
        for bad in ["-1", "10001", "\"0\"", "1.5", "null"] {
            let text = with_response(&format!(
                r#"{{"stdout":"","stderr":"","exit_code":0,"delay_ms":{bad}}}"#
            ));
            assert_eq!(parse(&text), Err(MismatchType::BadDelay), "for {bad}");
        }
    }

    // --- ACM-FR-10 / ACM-FR-11 / ACM-FR-12: expectations -------------------

    #[test]
    fn args_is_an_ordered_list_of_strings_and_may_be_empty() {
        let scenario = parse(&with_expected(r#"{"args":["-p","",  "x"]}"#)).unwrap();
        assert_eq!(
            scenario.expected.args,
            Some(vec!["-p".to_string(), String::new(), "x".to_string()])
        );

        let scenario = parse(&with_expected(r#"{"args":[]}"#)).unwrap();
        assert_eq!(scenario.expected.args, Some(vec![]));
    }

    #[test]
    fn args_rejects_a_non_list_or_a_non_string_element() {
        for body in [r#"{"args":"-p"}"#, r#"{"args":[1]}"#, r#"{"args":[null]}"#] {
            assert_eq!(
                parse(&with_expected(body)),
                Err(MismatchType::BadType),
                "for {body}"
            );
        }
    }

    #[test]
    fn required_args_accepts_both_forms_and_an_empty_list() {
        let scenario =
            parse(&with_expected(r#"{"required_args":[{"value":"-p"},{"name":"prompt","value":"hi"}]}"#))
                .unwrap();
        assert_eq!(
            scenario.expected.required_args,
            Some(vec![
                Requirement {
                    name: None,
                    value: "-p".into()
                },
                Requirement {
                    name: Some("prompt".into()),
                    value: "hi".into()
                },
            ])
        );

        let scenario = parse(&with_expected(r#"{"required_args":[]}"#)).unwrap();
        assert_eq!(scenario.expected.required_args, Some(vec![]));
    }

    #[test]
    fn a_requirement_must_carry_a_string_value() {
        for body in [
            r#"{"required_args":[{}]}"#,
            r#"{"required_args":[{"name":"a"}]}"#,
            r#"{"required_args":[{"value":1}]}"#,
            r#"{"required_args":[{"value":"a","name":2}]}"#,
            r#"{"required_args":["a"]}"#,
        ] {
            assert_eq!(
                parse(&with_expected(body)),
                Err(MismatchType::BadType),
                "for {body}"
            );
        }
    }

    /// ACM-FR-12: the distinction the spec singles out for a unit test.
    #[test]
    fn absent_stdin_and_null_stdin_are_different_states() {
        let absent = parse(&with_expected(r#"{}"#)).unwrap();
        assert_eq!(absent.expected.stdin, StdinExpectation::Unchecked);

        let null = parse(&with_expected(r#"{"stdin":null}"#)).unwrap();
        assert_eq!(null.expected.stdin, StdinExpectation::Empty);

        let exact = parse(&with_expected(r#"{"stdin":"hi"}"#)).unwrap();
        assert_eq!(exact.expected.stdin, StdinExpectation::Exact(b"hi".to_vec()));

        assert_ne!(absent.expected.stdin, null.expected.stdin);
        assert_ne!(null.expected.stdin, exact.expected.stdin);
    }

    #[test]
    fn stdin_accepts_base64_bytes() {
        let scenario =
            parse(&with_expected(r#"{"stdin":{"encoding":"base64","value":"AAE="}}"#)).unwrap();
        assert_eq!(
            scenario.expected.stdin,
            StdinExpectation::Exact(vec![0x00, 0x01])
        );
    }

    // --- structural -------------------------------------------------------

    #[test]
    fn expected_and_response_are_both_required_objects() {
        for text in [
            r#"{"version":1,"response":{"stdout":"","stderr":"","exit_code":0}}"#,
            r#"{"version":1,"expected":{}}"#,
            r#"{"version":1,"expected":[],"response":{"stdout":"","stderr":"","exit_code":0}}"#,
            r#"{"version":1,"expected":{},"response":"x"}"#,
        ] {
            assert_eq!(parse(text), Err(MismatchType::BadType), "for {text}");
        }
    }

    #[test]
    fn a_document_that_is_not_an_object_is_rejected() {
        for text in ["[]", "1", "\"x\"", "null", "true"] {
            assert_eq!(parse(text), Err(MismatchType::BadType), "for {text}");
        }
    }
}
