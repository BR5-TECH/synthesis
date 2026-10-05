//! Claude Code's wire protocol, transcribed from
//! `specifications/infra/CCP-claude-code-cli-protocol.md` and from nothing else.
//!
//! This module is the only place in the tree that knows how Claude Code is
//! invoked or how its answer is read. It names no flag, output format,
//! credential, or session term belonging to any other vendor, and it reads
//! nothing from the sibling module that transcribes one — which is what makes an
//! upgrade to either protocol a change to one file (EAC-FR-11, EAC-FR-12).

use std::borrow::Cow;

use serde_json::{Map, Value};

use super::{summary_line as one_line, ActivityKind, ExtractContext, Extracted, VendorActivity};
use crate::tools::agent_exec::protocol::{decode_envelope, EnvelopeInvalid, ResultContract};

/// The specification this module transcribes (EAC-FR-10's `protocol_spec`).
pub const SPEC: &str = "specifications/infra/CCP-claude-code-cli-protocol.md";

/// CCP-FR-01: the version every element here was asserted against. The manifest
/// remains the runtime record (AVI-FR-07); this is what that record has to agree
/// with, and `descriptor::tests` is where the two are compared.
pub const PINNED_CLI_VERSION: &str = "2.1.233";

/// CCP-FR-19: session state lives on a mount addressed by this variable. The
/// CLI writes transcripts beneath `<config-dir>/projects/`, so without it every
/// session dies with the container that created it (CCP-FR-18).
pub const SESSION_STATE_ENV: &str = "CLAUDE_CONFIG_DIR";

/// Where that mount is attached inside the container. Under the image's own
/// non-root home rather than at an invented path, so the CLI's defaults and this
/// mount describe the same place.
pub const SESSION_STATE_TARGET: &str = "/home/agent/.claude";

/// CCP-FR-05: the efforts the pinned CLI's `--effort` accepts. An identifier
/// outside this set is refused rather than passed through, because the CLI would
/// reject it after the container had already been created.
pub const EFFORT_LEVELS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

/// CCP-FR-07 / CCP-FR-08: the `AgentResponseEnvelope` schema, as fixed text
/// compiled into the binary.
///
/// It is never assembled per call, never derived from the task, and carries no
/// project material and no credential — which is the whole reason a schema may
/// ride on the argument vector while the task may not (EAC-FR-09).
///
/// It states every constraint `protocol.rs` enforces, and not a weaker set. A
/// field the schema offers more freely than the envelope rules accept is a trap
/// rather than a convenience: the agent fills it in, the CLI validates it, and
/// the turn is refused afterwards for something the agent was told it could do.
///
/// Draft-07, because that is the draft the pinned CLI validates against.
pub const ENVELOPE_SCHEMA: &str = concat!(
    r#"{"$schema":"http://json-schema.org/draft-07/schema#","type":"object","#,
    r#""properties":{"#,
    r#""protocol_version":{"type":"integer","enum":[1]},"#,
    r#""outcome":{"type":"string","enum":["success","failure","escalation_required"]},"#,
    r#""summary":{"type":"string","minLength":1},"#,
    r#""result":{"type":["object","null"]},"#,
    r#""failure":{"type":["object","null"],"additionalProperties":false,"properties":{"#,
    r#""code":{"type":"string"},"message":{"type":"string"},"retryable":{"type":"boolean"}},"#,
    r#""required":["code","message","retryable"]},"#,
    // EAC-FR-18 / EAC-FR-36: the escalation is stated to its full depth — the
    // question count, the option count each question allows, and the format each
    // proposed response's three fields keep. A question set composed to a shape
    // nobody stated is refused after a whole turn was spent on it, and an agent
    // that must guess how many questions it may ask asks one and loses the rest.
    r#""escalation":{"type":["object","null"],"additionalProperties":false,"properties":{"#,
    // `pattern` rather than `minLength` alone: the decoder trims before it
    // tests for emptiness, so a schema that accepted `"   "` would let an agent
    // answer inside the schema and be refused by the decoder afterwards — the
    // one failure CCP-FR-07 is about.
    //
    // No `maxLength` anywhere below. EAC-FR-08's limits are **byte** bounds and
    // JSON Schema counts characters, so a transcription of them would be the
    // looser of the two for any text outside ASCII — a rule stated more freely
    // here than the decoder enforces it, which is the trap rather than the
    // convenience. The byte bounds stay with the decoder (CCP-FR-07), together
    // with the readable word and sentence shapes, which this schema cannot
    // express faithfully either.
    r#""reason":{"type":"string","minLength":1,"pattern":"\\S"},"#,
    r#""questions":{"type":"array","minItems":1,"maxItems":8,"items":{"#,
    r#""type":"object","additionalProperties":false,"properties":{"#,
    r#""question":{"type":"string","minLength":1,"pattern":"\\S"},"#,
    // Not required: `AgentEscalationQuestion` defaults it, and a question with
    // no fixed set of answers leaves it out on purpose. Demanding it here would
    // re-prompt an agent that had answered correctly.
    r#""options":{"type":"array","maxItems":3,"items":{"#,
    r#""type":"object","additionalProperties":false,"properties":{"#,
    r#""answer":{"type":"string","minLength":1,"pattern":"\\S"},"#,
    r#""summary":{"type":"string","minLength":1,"pattern":"\\S"},"#,
    r#""description":{"type":"string","minLength":1,"pattern":"\\S"}},"#,
    r#""required":["answer","summary","description"]}}},"#,
    r#""required":["question"]}}},"#,
    r#""required":["reason","questions"]},"#,
    r#""session":{"type":["object","null"],"additionalProperties":false,"properties":{"#,
    r#""session_id":{"type":["string","null"]},"continuation_token":{"type":["string","null"]}}},"#,
    // EAC-FR-20: the v1 allowlist is empty, and the schema says so. Without
    // `maxProperties`, the schema offers a free-form object that the envelope
    // rules then refuse — so an agent fills the field in good faith, the CLI
    // validates it, and the turn is rejected after it has already been paid
    // for. Stating the bound here is what makes CCP-FR-07 correct the agent
    // inside the run instead.
    r#""metadata":{"type":["object","null"],"maxProperties":0}},"#,
    // `deny_unknown_fields` on the envelope and on each of its objects, said in
    // the schema's own words. Without it an agent's extra key passes this
    // validation and is refused by the decoder afterwards — the same shape of
    // loss the `metadata` bound above exists to prevent, at a different field.
    // `result` and `metadata` are deliberately not closed: `result` is
    // free-form by contract, and `metadata` is bounded to no keys at all.
    r#""additionalProperties":false,"#,
    r#""required":["protocol_version","outcome","summary"]}"#,
);

/// CCP-FR-28: the `result` position of [`ENVELOPE_SCHEMA`], as the schema states
/// it where a task names no result contract.
///
/// `concat!` takes literals alone, so the position is spelled inside the schema
/// and named again here. The two are held together by a test asserting this text
/// occurs in the envelope schema exactly once — which is what makes the
/// replacement below a placement rather than a search.
const RESULT_FREE_FORM: &str = r#""result":{"type":["object","null"]},"#;

/// CCP-FR-07 / CCP-FR-28: the schema this turn's `--json-schema` carries.
///
/// The envelope schema where the task names no result contract, and the same
/// schema with that contract's document at the `result` position where it names
/// one. Both halves are fixed text compiled into the binary: nothing here is
/// derived from the task, composed from its `input`, or varied by anything but
/// the identifier (CCP-FR-08).
pub fn schema_argument(result_contract: Option<ResultContract>) -> Cow<'static, str> {
    match result_contract {
        None => Cow::Borrowed(ENVELOPE_SCHEMA),
        // The null the position already admits stays admitted. It is the
        // envelope's own rule rather than the contract's — an outcome other
        // than `success` carries no result (EAC-FR-18) — and a schema that
        // dropped it would refuse a correctly reported failure inside the run,
        // leaving the agent no shape to move to but an invented verdict.
        Some(contract) => Cow::Owned(ENVELOPE_SCHEMA.replacen(
            RESULT_FREE_FORM,
            &format!(
                r#""result":{{"oneOf":[{{"type":"null"}},{}]}},"#,
                contract.schema_text()
            ),
            1,
        )),
    }
}

// ---------------------------------------------------------------------------
// The argument vector (CCP-FR-02)
// ---------------------------------------------------------------------------

/// Everything before the session argument, which is the only part a fresh turn
/// and a resumed one share.
fn leading(
    model_id: Option<&str>,
    effort_id: Option<&str>,
    result_contract: Option<ResultContract>,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        // CCP-FR-03: non-interactive, and machine-readable events rather than
        // prose.
        "-p".into(),
        // CCP-FR-25: one JSON event per line, written as the turn happens. The
        // single-document format answers only at exit, so a turn that takes a
        // quarter of an hour says nothing for a quarter of an hour and a turn
        // that never returns says nothing at all — and what the agent is doing
        // in that time is the whole of what a reader wants (EAC-FR-32). The
        // final event of the stream is the same result document the single
        // format would have printed, so nothing is given up for it.
        "--output-format".into(),
        "stream-json".into(),
        // CCP-FR-26: the CLI refuses `--print` with `--output-format
        // stream-json` unless this is present — verified against the pinned
        // version, which answers `When using --print,
        // --output-format=stream-json requires --verbose` and exits without
        // running. It is a precondition of the format rather than a request for
        // more output.
        "--verbose".into(),
        // CCP-FR-07: the CLI validates the final response against this and
        // re-prompts the agent on a mismatch, so a malformed envelope is
        // corrected inside the run rather than surfacing as a parse failure
        // outside it.
        "--json-schema".into(),
        schema_argument(result_contract).into_owned(),
        // CCP-FR-04: the container is already the isolation boundary. A weaker
        // mode leaves arbitrary shell commands needing an allow rule, which a
        // turn that has to build or test cannot complete under.
        "--permission-mode".into(),
        "bypassPermissions".into(),
    ];

    if let Some(model) = model_id.filter(|m| !m.is_empty()) {
        args.push("--model".into());
        args.push(model.to_string());
    }
    // CCP-FR-05: this CLI *does* have a reasoning-effort argument, so a resolved
    // effort reaches the command line instead of being discarded.
    if let Some(effort) = effort_id.filter(|e| !e.is_empty()) {
        args.push("--effort".into());
        args.push(effort.to_string());
    }
    args
}

/// CCP-FR-16: a fresh turn carries the identifier the executor generated, so the
/// session's identity is known before the CLI runs rather than recovered from
/// its output afterwards.
pub fn fresh_argv(
    model_id: Option<&str>,
    effort_id: Option<&str>,
    session_id: &str,
    result_contract: Option<ResultContract>,
) -> Vec<String> {
    let mut args = leading(model_id, effort_id, result_contract);
    args.push("--session-id".into());
    args.push(session_id.to_string());
    args
}

/// CCP-FR-17: a resumed turn carries `--resume` and no `--session-id`.
pub fn resume_argv(
    model_id: Option<&str>,
    effort_id: Option<&str>,
    resume_session_id: &str,
    result_contract: Option<ResultContract>,
) -> Vec<String> {
    let mut args = leading(model_id, effort_id, result_contract);
    args.push("--resume".into());
    args.push(resume_session_id.to_string());
    args
}

/// CCP-FR-05: an effort the pinned CLI would reject, refused before launch.
pub fn effort_supported(effort_id: &str) -> bool {
    EFFORT_LEVELS.contains(&effort_id)
}

// ---------------------------------------------------------------------------
// Extraction (CCP-FR-09 through CCP-FR-13)
// ---------------------------------------------------------------------------

/// The result document's own field names, read and no others.
const KEY_TYPE: &str = "type";
const EVENT_RESULT: &str = "result";
const KEY_SUBTYPE: &str = "subtype";
const KEY_IS_ERROR: &str = "is_error";
const KEY_SESSION_ID: &str = "session_id";
/// CCP-FR-09's first extraction position: the schema-validated object.
const KEY_STRUCTURED_OUTPUT: &str = "structured_output";
/// CCP-FR-09's second: the same document as text.
const KEY_RESULT: &str = "result";
const SUBTYPE_SUCCESS: &str = "success";

pub fn extract(context: &ExtractContext<'_>) -> Result<Extracted, EnvelopeInvalid> {
    // CCP-FR-13: stdout is one JSON object per line, and exactly one of them is
    // the result document. Every other line is progress — what the agent said,
    // which tool it reached for, what came back — and is read by the observer
    // rather than here.
    //
    // The *last* result event, so that a stream carrying more than one is read
    // as the turn ending rather than as the turn that ended first. A line that
    // is not JSON is skipped rather than failing extraction: the CLI writes its
    // own notices into this stream, and one of them appearing beside a
    // well-formed result is not a reason to discard the result.
    let mut document: Option<Value> = None;
    for line in context.stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(Value::Object(event)) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if event.get(KEY_TYPE).and_then(Value::as_str) == Some(EVENT_RESULT) {
            document = Some(Value::Object(event));
        }
    }
    // A stream carrying no result event is a turn that never reported one: the
    // CLI was killed, or it wrote something else entirely. What separates that
    // from an empty stream, for a reader, is the activity recorded beside it.
    let document = document.ok_or(EnvelopeInvalid::NotFound)?;
    let object = document.as_object().ok_or(EnvelopeInvalid::Malformed)?;

    // CCP-FR-12: a failed run, whatever the exit status. This is checked before
    // anything is extracted, because a failure arising inside the run — a
    // missing credential being the documented case — is printed as the result on
    // stdout under exit 0, so the exit status alone never establishes success.
    let subtype = object.get(KEY_SUBTYPE).and_then(Value::as_str);
    if subtype != Some(SUBTYPE_SUCCESS) {
        return Err(EnvelopeInvalid::VendorRunFailed);
    }
    if object.get(KEY_IS_ERROR).and_then(Value::as_bool) == Some(true) {
        return Err(EnvelopeInvalid::VendorRunFailed);
    }

    // CCP-FR-16: the identity the executor assigned is asserted back. A run that
    // reports a different one is not the session the caller will later resume,
    // so accepting it would hand back a reference that resumes the wrong
    // conversation.
    //
    // A *missing* identifier is the same failure rather than a lenient case: the
    // pinned CLI always reports one, so its absence means this is not the
    // document this protocol describes, and accepting it would leave the caller
    // with no resumable reference for a session the executor had already named.
    let reported = object
        .get(KEY_SESSION_ID)
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_string);
    if let Some(assigned) = context.assigned_session_id {
        if reported.as_deref() != Some(assigned) {
            return Err(EnvelopeInvalid::SessionMismatch);
        }
    }

    // CCP-FR-09: the ordered positions. The first that yields a document wins,
    // and a later one is never consulted once an earlier has produced one.
    let envelope = match object.get(KEY_STRUCTURED_OUTPUT) {
        Some(Value::Object(_)) => {
            let text = serde_json::to_string(&object[KEY_STRUCTURED_OUTPUT])
                .map_err(|_| EnvelopeInvalid::Malformed)?;
            decode_envelope(&text)?
        }
        // A present-but-wrong-typed `structured_output` is this position
        // failing, not this position being absent: falling through to the text
        // would accept a document the schema path had already rejected.
        Some(Value::Null) | None => match object.get(KEY_RESULT) {
            Some(Value::String(text)) => decode_envelope(text)?,
            Some(_) => return Err(EnvelopeInvalid::Malformed),
            None => return Err(EnvelopeInvalid::NotFound),
        },
        Some(_) => return Err(EnvelopeInvalid::Malformed),
    };

    Ok(Extracted {
        envelope,
        session_id: reported,
    })
}

// ---------------------------------------------------------------------------
// Observed activity (CCP-FR-27)
// ---------------------------------------------------------------------------

/// The event types this vendor writes on stdout under `stream-json`, and the
/// content-block types an assistant event carries. Read for a reader's benefit
/// alone: nothing here decides an outcome, and an event outside this vocabulary
/// is reported rather than dropped.
const EVENT_SYSTEM: &str = "system";
const EVENT_ASSISTANT: &str = "assistant";
const EVENT_USER: &str = "user";
const SUBTYPE_INIT: &str = "init";
const SUBTYPE_API_RETRY: &str = "api_retry";
const BLOCK_TEXT: &str = "text";
const BLOCK_THINKING: &str = "thinking";
const BLOCK_TOOL_USE: &str = "tool_use";
const BLOCK_TOOL_RESULT: &str = "tool_result";

/// CCP-FR-27: one line of the event stream, read as an activity.
pub fn activity(line: &str) -> VendorActivity {
    let Ok(Value::Object(event)) = serde_json::from_str::<Value>(line.trim()) else {
        return unrecognized(line);
    };
    let Some(event_type) = event.get(KEY_TYPE).and_then(Value::as_str) else {
        return unrecognized(line);
    };

    match event_type {
        EVENT_SYSTEM => system_activity(&event),
        EVENT_ASSISTANT | EVENT_USER => message_activity(&event, event_type),
        EVENT_RESULT => result_activity(&event),
        _ => unrecognized(line),
    }
}

fn system_activity(event: &Map<String, Value>) -> VendorActivity {
    match event.get(KEY_SUBTYPE).and_then(Value::as_str) {
        Some(SUBTYPE_INIT) => {
            let model = event.get("model").and_then(Value::as_str).unwrap_or("?");
            let tools = event
                .get("tools")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            VendorActivity {
                kind: ActivityKind::Started,
                summary: format!("session started · model {model} · {tools} tools"),
            }
        }
        // The single most useful line this CLI writes when a run is going
        // nowhere: a wrong or expired credential is ten of these and then a
        // failure, and without them the whole delay looks like thinking.
        Some(SUBTYPE_API_RETRY) => {
            let attempt = number(event.get("attempt"));
            let max = number(event.get("max_retries"));
            let status = number(event.get("error_status"));
            let error = event.get("error").and_then(Value::as_str).unwrap_or("");
            VendorActivity {
                kind: ActivityKind::Retry,
                summary: format!("API retry {attempt}/{max} · {status} {error}").trim_end().to_string(),
            }
        }
        Some(other) => VendorActivity {
            kind: ActivityKind::Diagnostic,
            summary: format!("system · {other}"),
        },
        None => VendorActivity {
            kind: ActivityKind::Diagnostic,
            summary: "system".to_string(),
        },
    }
}

/// An assistant or user event, read through its content blocks.
///
/// One event carries several blocks — prose and a tool call together, most
/// commonly — and the summary names each in order, because a turn where the
/// agent explained itself *and* reached for a file is two different things a
/// reader is watching for.
fn message_activity(event: &Map<String, Value>, event_type: &str) -> VendorActivity {
    let blocks = event
        .get("message")
        .and_then(Value::as_object)
        .and_then(|message| message.get("content"))
        .and_then(Value::as_array);
    let Some(blocks) = blocks else {
        return VendorActivity {
            kind: ActivityKind::Message,
            summary: event_type.to_string(),
        };
    };

    let mut kind = ActivityKind::Message;
    let mut parts: Vec<String> = Vec::new();
    for block in blocks {
        let Some(block) = block.as_object() else {
            continue;
        };
        match block.get(KEY_TYPE).and_then(Value::as_str) {
            Some(BLOCK_TEXT) => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    parts.push(one_line(text));
                }
            }
            Some(BLOCK_THINKING) => {
                kind = ActivityKind::Reasoning;
                if let Some(text) = block.get(BLOCK_THINKING).and_then(Value::as_str) {
                    parts.push(one_line(text));
                }
            }
            Some(BLOCK_TOOL_USE) => {
                kind = ActivityKind::ToolCall;
                let name = block.get("name").and_then(Value::as_str).unwrap_or("tool");
                parts.push(format!("{name}({})", tool_target(block.get("input"))));
            }
            Some(BLOCK_TOOL_RESULT) => {
                kind = ActivityKind::ToolResult;
                let failed = block.get("is_error").and_then(Value::as_bool) == Some(true);
                let marker = if failed { "failed" } else { "ok" };
                parts.push(format!("← {marker}"));
            }
            _ => {}
        }
    }

    if parts.is_empty() {
        parts.push(event_type.to_string());
    }
    VendorActivity {
        kind,
        summary: parts.join(" · "),
    }
}

/// The turn's own ending, which is also the document extraction reads.
fn result_activity(event: &Map<String, Value>) -> VendorActivity {
    let subtype = event
        .get(KEY_SUBTYPE)
        .and_then(Value::as_str)
        .unwrap_or("?");
    let failed = event.get(KEY_IS_ERROR).and_then(Value::as_bool) == Some(true);
    let turns = number(event.get("num_turns"));
    let duration = number(event.get("duration_ms"));
    let mut summary = format!("turn ended · {subtype} · {turns} turns · {duration} ms");
    // CCP-FR-12: a failure inside the run is printed as the result under exit
    // zero, so the flag that says so is the only thing that separates this from
    // a turn that worked.
    if failed {
        summary.push_str(" · is_error");
        if let Some(status) = event.get("api_error_status").and_then(Value::as_i64) {
            summary.push_str(&format!(" · API {status}"));
        }
    }
    VendorActivity {
        kind: if failed {
            ActivityKind::Error
        } else {
            ActivityKind::Finished
        },
        summary,
    }
}

/// What a tool call is *about*, which is the part a reader is following.
///
/// The named field where the vendor's own tools carry one, and otherwise
/// nothing: an arbitrary tool's whole argument object belongs in the verbatim
/// line beside this rather than in a one-line summary.
fn tool_target(input: Option<&Value>) -> String {
    let Some(input) = input.and_then(Value::as_object) else {
        return String::new();
    };
    for key in ["file_path", "path", "command", "pattern", "url", "query", "prompt"] {
        if let Some(value) = input.get(key).and_then(Value::as_str) {
            return one_line(value);
        }
    }
    String::new()
}

fn unrecognized(line: &str) -> VendorActivity {
    VendorActivity {
        kind: ActivityKind::Unrecognized,
        summary: one_line(line),
    }
}

/// A number as text, whatever JSON shape it arrived in.
fn number(value: Option<&Value>) -> String {
    match value {
        Some(Value::Number(number)) => number.to_string(),
        _ => "?".to_string(),
    }
}

