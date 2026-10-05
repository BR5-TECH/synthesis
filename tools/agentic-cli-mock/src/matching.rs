//! Generic expectation matching — the half of validation that knows nothing
//! about any particular tool (ACM-FR-28).
//!
//! Every comparison is over bytes. A received argument arrives as an `OsStr`
//! and is compared through `as_encoded_bytes`, so the mock never re-encodes,
//! normalises, or lossily converts what the operating system handed it.

use std::ffi::OsStr;

use crate::diagnostics::{Check, Mismatch, MismatchType};
use crate::scenario::{Requirement, StdinExpectation};

/// ACM-FR-10: the complete received vector — same count, same order, byte
/// identical values.
pub fn match_args(expected: &[String], received: &[&OsStr]) -> Option<Mismatch> {
    if expected.len() != received.len() {
        // ACM-FR-25: an arity failure reports element counts and no index —
        // there is no single position to blame.
        return Some(Mismatch {
            check: Check::Args,
            index: None,
            name: None,
            expected_length: Some(expected.len()),
            actual_length: Some(received.len()),
            mismatch_type: MismatchType::Arity,
        });
    }

    // ACM-FR-17: the lowest differing position is the one reported.
    for (index, (want, got)) in expected.iter().zip(received.iter()).enumerate() {
        let got = got.as_encoded_bytes();
        if want.as_bytes() != got {
            return Some(Mismatch {
                check: Check::Args,
                index: Some(index),
                name: None,
                // ACM-FR-24: lengths, never the values themselves.
                expected_length: Some(want.len()),
                actual_length: Some(got.len()),
                mismatch_type: MismatchType::ValueMismatch,
            });
        }
    }

    None
}

/// ACM-FR-11: each requirement must be satisfied by some element, in any
/// position. `name` travels into the record and is never compared.
pub fn match_required_args(expected: &[Requirement], received: &[&OsStr]) -> Option<Mismatch> {
    // ACM-FR-17: the first unsatisfied requirement, in the order the scenario
    // lists them.
    for (index, requirement) in expected.iter().enumerate() {
        let satisfied = received
            .iter()
            .any(|arg| arg.as_encoded_bytes() == requirement.value.as_bytes());

        if !satisfied {
            return Some(Mismatch {
                check: Check::RequiredArgs,
                index: Some(index),
                name: requirement.name.clone(),
                expected_length: Some(requirement.value.len()),
                // ACM-FR-25: nothing was found, so there is nothing to measure.
                actual_length: None,
                mismatch_type: MismatchType::Absent,
            });
        }
    }

    None
}

/// ACM-FR-12: absent validates nothing; `null` requires zero bytes; a payload
/// requires those exact bytes.
pub fn match_stdin(expected: &StdinExpectation, received: &[u8]) -> Option<Mismatch> {
    let want: &[u8] = match expected {
        StdinExpectation::Unchecked => return None,
        StdinExpectation::Empty => &[],
        StdinExpectation::Exact(bytes) => bytes,
    };

    if want.len() != received.len() {
        return Some(Mismatch {
            check: Check::Stdin,
            index: None,
            name: None,
            expected_length: Some(want.len()),
            actual_length: Some(received.len()),
            mismatch_type: MismatchType::Length,
        });
    }

    if want != received {
        return Some(Mismatch {
            check: Check::Stdin,
            index: None,
            name: None,
            expected_length: Some(want.len()),
            actual_length: Some(received.len()),
            mismatch_type: MismatchType::Content,
        });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    fn refs(args: &[OsString]) -> Vec<&OsStr> {
        args.iter().map(OsString::as_os_str).collect()
    }

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| (*s).to_owned()).collect()
    }

    // --- ACM-FR-10 ---------------------------------------------------------

    #[test]
    fn an_identical_vector_matches() {
        let received = os(&["-p", "hello world", ""]);
        assert!(match_args(&strings(&["-p", "hello world", ""]), &refs(&received)).is_none());
    }

    #[test]
    fn an_empty_expectation_requires_an_empty_vector() {
        assert!(match_args(&[], &[]).is_none());

        let received = os(&["-p"]);
        let mismatch = match_args(&[], &refs(&received)).expect("should fail");
        assert_eq!(mismatch.mismatch_type, MismatchType::Arity);
        assert_eq!(mismatch.expected_length, Some(0));
        assert_eq!(mismatch.actual_length, Some(1));
    }

    #[test]
    fn a_differing_count_reports_arity_with_element_counts_and_no_index() {
        let received = os(&["-p"]);
        let mismatch = match_args(&strings(&["-p", "hi"]), &refs(&received)).expect("should fail");
        assert_eq!(mismatch.check, Check::Args);
        assert_eq!(mismatch.mismatch_type, MismatchType::Arity);
        assert_eq!(mismatch.index, None);
        assert_eq!(mismatch.expected_length, Some(2));
        assert_eq!(mismatch.actual_length, Some(1));
    }

    #[test]
    fn a_differing_value_reports_the_lowest_index_and_byte_lengths() {
        let received = os(&["-p", "wrong", "also wrong"]);
        let mismatch = match_args(
            &strings(&["-p", "sk-ant-oat01-REDACTEDVALUE", "nope"]),
            &refs(&received),
        )
        .expect("should fail");

        assert_eq!(mismatch.check, Check::Args);
        assert_eq!(mismatch.mismatch_type, MismatchType::ValueMismatch);
        assert_eq!(mismatch.index, Some(1));
        assert_eq!(mismatch.expected_length, Some(26));
        assert_eq!(mismatch.actual_length, Some(5));
        assert_eq!(mismatch.name, None);
    }

    #[test]
    fn order_is_part_of_the_expectation() {
        let received = os(&["hi", "-p"]);
        let mismatch = match_args(&strings(&["-p", "hi"]), &refs(&received)).expect("should fail");
        assert_eq!(mismatch.index, Some(0));
    }

    #[test]
    fn lengths_are_byte_counts_not_character_counts() {
        let received = os(&["a"]);
        let mismatch = match_args(&strings(&["é"]), &refs(&received)).expect("should fail");
        assert_eq!(mismatch.expected_length, Some(2));
        assert_eq!(mismatch.actual_length, Some(1));
    }

    // --- ACM-FR-11 ---------------------------------------------------------

    #[test]
    fn a_requirement_is_satisfied_from_any_position() {
        let received = os(&["hello", "--model", "opus", "-p"]);
        let expected = vec![
            Requirement {
                name: None,
                value: "-p".into(),
            },
            Requirement {
                name: Some("prompt".into()),
                value: "hello".into(),
            },
        ];
        assert!(match_required_args(&expected, &refs(&received)).is_none());
    }

    #[test]
    fn an_empty_requirement_list_requires_nothing() {
        let received = os(&["anything"]);
        assert!(match_required_args(&[], &refs(&received)).is_none());
        assert!(match_required_args(&[], &[]).is_none());
    }

    #[test]
    fn an_unsatisfied_requirement_reports_index_name_and_expected_length_only() {
        let received = os(&["-p", "goodbye"]);
        let expected = vec![
            Requirement {
                name: None,
                value: "-p".into(),
            },
            Requirement {
                name: Some("prompt".into()),
                value: "hello".into(),
            },
        ];
        let mismatch = match_required_args(&expected, &refs(&received)).expect("should fail");

        assert_eq!(mismatch.check, Check::RequiredArgs);
        assert_eq!(mismatch.mismatch_type, MismatchType::Absent);
        assert_eq!(mismatch.index, Some(1));
        assert_eq!(mismatch.name.as_deref(), Some("prompt"));
        assert_eq!(mismatch.expected_length, Some(5));
        assert_eq!(mismatch.actual_length, None);
    }

    #[test]
    fn an_unnamed_requirement_reports_no_name() {
        let received = os(&["x"]);
        let expected = vec![Requirement {
            name: None,
            value: "-p".into(),
        }];
        let mismatch = match_required_args(&expected, &refs(&received)).expect("should fail");
        assert_eq!(mismatch.index, Some(0));
        assert_eq!(mismatch.name, None);
    }

    /// ACM-FR-11: a name is metadata. A vector containing another
    /// requirement's *name* does not satisfy a requirement.
    #[test]
    fn a_name_is_never_matched_against_the_vector() {
        let received = os(&["prompt"]);
        let expected = vec![Requirement {
            name: Some("prompt".into()),
            value: "hello".into(),
        }];
        assert!(match_required_args(&expected, &refs(&received)).is_some());

        // And the value alone satisfies it, whatever the name says.
        let received = os(&["hello"]);
        assert!(match_required_args(&expected, &refs(&received)).is_none());
    }

    #[test]
    fn the_first_unsatisfied_requirement_in_scenario_order_is_reported() {
        let received = os(&["c"]);
        let expected = vec![
            Requirement {
                name: None,
                value: "a".into(),
            },
            Requirement {
                name: None,
                value: "b".into(),
            },
        ];
        let mismatch = match_required_args(&expected, &refs(&received)).expect("should fail");
        assert_eq!(mismatch.index, Some(0));
    }

    // --- ACM-FR-12 ---------------------------------------------------------

    #[test]
    fn unchecked_stdin_accepts_anything() {
        assert!(match_stdin(&StdinExpectation::Unchecked, b"").is_none());
        assert!(match_stdin(&StdinExpectation::Unchecked, b"anything at all").is_none());
    }

    #[test]
    fn null_stdin_requires_zero_bytes() {
        assert!(match_stdin(&StdinExpectation::Empty, b"").is_none());

        let mismatch = match_stdin(&StdinExpectation::Empty, b"hi").expect("should fail");
        assert_eq!(mismatch.check, Check::Stdin);
        assert_eq!(mismatch.mismatch_type, MismatchType::Length);
        assert_eq!(mismatch.expected_length, Some(0));
        assert_eq!(mismatch.actual_length, Some(2));
        assert_eq!(mismatch.index, None);
        assert_eq!(mismatch.name, None);
    }

    #[test]
    fn exact_stdin_compares_every_byte() {
        let expected = StdinExpectation::Exact(b"hello\n".to_vec());
        assert!(match_stdin(&expected, b"hello\n").is_none());

        // Same length, different bytes.
        let mismatch = match_stdin(&expected, b"hellp\n").expect("should fail");
        assert_eq!(mismatch.mismatch_type, MismatchType::Content);
        assert_eq!(mismatch.expected_length, Some(6));
        assert_eq!(mismatch.actual_length, Some(6));

        // Differing length.
        let mismatch = match_stdin(&expected, b"hello").expect("should fail");
        assert_eq!(mismatch.mismatch_type, MismatchType::Length);
        assert_eq!(mismatch.expected_length, Some(6));
        assert_eq!(mismatch.actual_length, Some(5));
    }

    /// No normalisation of any kind: a trailing newline is a byte like any other.
    #[test]
    fn stdin_comparison_does_not_normalise_line_endings_or_unicode() {
        assert!(match_stdin(&StdinExpectation::Exact(b"a\r\n".to_vec()), b"a\n").is_some());
        assert!(match_stdin(&StdinExpectation::Exact(b"a".to_vec()), b"a\n").is_some());

        // NFC vs NFD spellings of the same character stay different.
        let composed = "é".as_bytes().to_vec();
        let decomposed = "e\u{0301}".as_bytes();
        assert!(match_stdin(&StdinExpectation::Exact(composed), decomposed).is_some());
    }

    #[test]
    fn exact_stdin_accepts_bytes_that_are_not_valid_utf8() {
        let expected = StdinExpectation::Exact(vec![0xff, 0x00, 0xfe]);
        assert!(match_stdin(&expected, &[0xff, 0x00, 0xfe]).is_none());
        assert!(match_stdin(&expected, &[0xff, 0x00, 0xff]).is_some());
    }
}
