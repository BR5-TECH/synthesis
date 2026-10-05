//! Parsing the mock's own command line.
//!
//! ```text
//! agentic-cli-mock --tool <claude|codex|docker> --scenario <path> -- <tool arguments...>
//! ```

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use crate::adapter::{self, ToolAdapter};
use crate::diagnostics::MismatchType;

const TOOL: &str = "--tool";
const SCENARIO: &str = "--scenario";
const TERMINATOR: &str = "--";

pub struct Invocation<'a> {
    pub adapter: &'a (dyn ToolAdapter + 'static),
    pub scenario_path: PathBuf,
    /// ACM-FR-04: exactly the tokens after `--`, in order, untouched.
    pub tool_args: Vec<OsString>,
}

/// A configuration error from the invocation itself. `tool` is `Some` only once
/// `--tool` resolved to a registered adapter, which is what ACM-FR-05 requires.
pub struct InvocationError {
    pub tool: Option<String>,
    pub mismatch_type: MismatchType,
}

/// ACM-FR-03 / ACM-FR-04 / ACM-FR-05.
///
/// `argv` excludes the program name. Problems are reported in a fixed order —
/// the tool is resolved first so that every later record can name it.
pub fn parse<'a>(
    argv: &[OsString],
    registry: &'a [&'a (dyn ToolAdapter + 'static)],
) -> Result<Invocation<'a>, InvocationError> {
    let mut tool: Option<OsString> = None;
    let mut scenario: Option<OsString> = None;
    let mut tool_args: Vec<OsString> = Vec::new();
    // ACM-FR-03: an option the mock does not define, and a repeated one, are
    // both invocations it does not define.
    let mut undefined_option = false;

    let mut index = 0;
    while index < argv.len() {
        let argument = argv[index].as_os_str();

        if argument == OsStr::new(TERMINATOR) {
            // ACM-FR-04: everything after the terminator is a received tool
            // argument, including tokens that look like the mock's own options.
            tool_args.extend_from_slice(&argv[index + 1..]);
            break;
        }

        let slot = if argument == OsStr::new(TOOL) {
            Some(&mut tool)
        } else if argument == OsStr::new(SCENARIO) {
            Some(&mut scenario)
        } else {
            None
        };

        if let Some(slot) = slot {
            let (consumed, advance) = take_value(argv, index, slot);
            if matches!(consumed, Consumed::Repeat) {
                undefined_option = true;
            }
            index += advance;
            continue;
        }

        undefined_option = true;
        index += 1;
    }

    // Resolve the tool before reporting anything else, so a record for a later
    // problem can carry it.
    let Some(tool) = tool else {
        return Err(InvocationError {
            tool: None,
            mismatch_type: MismatchType::MissingToolOption,
        });
    };
    let tool = tool.to_string_lossy().into_owned();

    let Some(adapter) = adapter::resolve(registry, &tool) else {
        return Err(InvocationError {
            tool: None,
            mismatch_type: MismatchType::UnknownTool,
        });
    };

    if undefined_option {
        return Err(InvocationError {
            tool: Some(tool),
            mismatch_type: MismatchType::UnknownOption,
        });
    }

    let Some(scenario_path) = scenario else {
        return Err(InvocationError {
            tool: Some(tool),
            mismatch_type: MismatchType::MissingScenarioOption,
        });
    };

    Ok(Invocation {
        adapter,
        scenario_path: PathBuf::from(scenario_path),
        tool_args,
    })
}

enum Consumed {
    Value,
    Repeat,
    NoValue,
}

/// Returns what happened and how far to advance. The advance is part of the
/// result because an option with no value available must not step over the
/// token that follows it — that token may be the terminator.
fn take_value(
    argv: &[OsString],
    index: usize,
    slot: &mut Option<OsString>,
) -> (Consumed, usize) {
    // ACM-FR-04: `--` terminates the mock's own options, so it is never
    // swallowed as one of their values.
    let value = argv
        .get(index + 1)
        .filter(|value| value.as_os_str() != OsStr::new(TERMINATOR));

    let Some(value) = value else {
        // No value available. A repeat is still a repeat — ACM-FR-03 makes
        // repeating an option a configuration error whether or not the second
        // occurrence carries a value.
        let consumed = if slot.is_some() {
            Consumed::Repeat
        } else {
            // A value-less first occurrence leaves the slot empty, so it reads
            // as the option having been omitted.
            Consumed::NoValue
        };
        return (consumed, 1);
    };

    if slot.is_some() {
        return (Consumed::Repeat, 2);
    }

    *slot = Some(value.clone());
    (Consumed::Value, 2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::REGISTRY;

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    fn parse_ok(args: &[&str]) -> Invocation<'static> {
        parse(&os(args), REGISTRY).unwrap_or_else(|e| {
            panic!("expected success, got {:?}", e.mismatch_type);
        })
    }

    fn parse_err(args: &[&str]) -> InvocationError {
        match parse(&os(args), REGISTRY) {
            Ok(_) => panic!("expected failure for {args:?}"),
            Err(error) => error,
        }
    }

    #[test]
    fn a_well_formed_invocation_resolves_the_adapter_path_and_vector() {
        let invocation = parse_ok(&["--tool", "claude", "--scenario", "s.json", "--", "-p", "hi"]);
        assert_eq!(invocation.adapter.id(), "claude");
        assert_eq!(invocation.scenario_path, PathBuf::from("s.json"));
        assert_eq!(invocation.tool_args, os(&["-p", "hi"]));
    }

    #[test]
    fn the_options_may_appear_in_either_order() {
        let invocation = parse_ok(&["--scenario", "s.json", "--tool", "codex", "--", "exec"]);
        assert_eq!(invocation.adapter.id(), "codex");
        assert_eq!(invocation.tool_args, os(&["exec"]));
    }

    /// ACM-FR-04: the terminator is absolute — nothing after it is interpreted.
    #[test]
    fn every_token_after_the_terminator_is_a_received_argument() {
        let invocation = parse_ok(&[
            "--tool", "claude", "--scenario", "s.json", "--", "-p", "", "--tool", "-p", "--",
            "--scenario",
        ]);
        assert_eq!(
            invocation.tool_args,
            os(&["-p", "", "--tool", "-p", "--", "--scenario"])
        );
    }

    #[test]
    fn an_invocation_without_a_terminator_has_an_empty_vector() {
        let invocation = parse_ok(&["--tool", "claude", "--scenario", "s.json"]);
        assert!(invocation.tool_args.is_empty());
    }

    #[test]
    fn a_terminator_with_nothing_after_it_has_an_empty_vector() {
        let invocation = parse_ok(&["--tool", "claude", "--scenario", "s.json", "--"]);
        assert!(invocation.tool_args.is_empty());
    }

    // --- ACM-FR-03 / ACM-FR-05: configuration errors -----------------------

    #[test]
    fn a_missing_tool_option_reports_no_tool() {
        let error = parse_err(&["--scenario", "s.json", "--", "-p"]);
        assert_eq!(error.mismatch_type, MismatchType::MissingToolOption);
        assert_eq!(error.tool, None);
    }

    #[test]
    fn an_unregistered_tool_reports_no_tool() {
        let error = parse_err(&["--tool", "opencode", "--scenario", "s.json"]);
        assert_eq!(error.mismatch_type, MismatchType::UnknownTool);
        assert_eq!(error.tool, None);
    }

    /// The tool is resolved before the scenario is missed, so this record can
    /// name it.
    #[test]
    fn a_missing_scenario_option_reports_the_resolved_tool() {
        let error = parse_err(&["--tool", "claude", "--", "-p"]);
        assert_eq!(error.mismatch_type, MismatchType::MissingScenarioOption);
        assert_eq!(error.tool.as_deref(), Some("claude"));
    }

    #[test]
    fn an_option_the_mock_does_not_define_is_a_configuration_error() {
        for args in [
            vec!["--tool", "claude", "--scenario", "s.json", "--verbose"],
            vec!["--tool", "claude", "--scenario", "s.json", "-v"],
            vec!["stray", "--tool", "claude", "--scenario", "s.json"],
        ] {
            let error = parse_err(&args);
            assert_eq!(
                error.mismatch_type,
                MismatchType::UnknownOption,
                "for {args:?}"
            );
        }
    }

    /// Only the space-separated form is defined, so an `=` spelling supplies no
    /// option at all. Which category that lands on follows from the reporting
    /// precedence — tool, then undefined option, then scenario — so `--tool=`
    /// reads as a missing tool while `--scenario=` is reported as the undefined
    /// option it is, the tool having already resolved.
    #[test]
    fn the_equals_form_is_not_the_defined_spelling_of_an_option() {
        let error = parse_err(&["--tool=claude", "--scenario", "s.json"]);
        assert_eq!(error.mismatch_type, MismatchType::MissingToolOption);
        assert_eq!(error.tool, None);

        let error = parse_err(&["--tool", "claude", "--scenario=s.json"]);
        assert_eq!(error.mismatch_type, MismatchType::UnknownOption);
        assert_eq!(error.tool.as_deref(), Some("claude"));
    }

    #[test]
    fn a_repeated_option_is_a_configuration_error() {
        let error = parse_err(&[
            "--tool", "claude", "--tool", "codex", "--scenario", "s.json",
        ]);
        assert_eq!(error.mismatch_type, MismatchType::UnknownOption);
        // The first occurrence is what resolved, so the record can still name it.
        assert_eq!(error.tool.as_deref(), Some("claude"));

        let error = parse_err(&[
            "--tool",
            "claude",
            "--scenario",
            "a.json",
            "--scenario",
            "b.json",
        ]);
        assert_eq!(error.mismatch_type, MismatchType::UnknownOption);
    }

    #[test]
    fn a_value_less_trailing_option_reads_as_omitted() {
        let error = parse_err(&["--scenario", "s.json", "--tool"]);
        assert_eq!(error.mismatch_type, MismatchType::MissingToolOption);

        let error = parse_err(&["--tool", "claude", "--scenario"]);
        assert_eq!(error.mismatch_type, MismatchType::MissingScenarioOption);
    }

    /// A repeat is a repeat whether or not the second occurrence carries a
    /// value — including as the very last token, where there is no value left
    /// to take.
    #[test]
    fn a_repeated_option_with_no_value_is_still_a_repeat() {
        for args in [
            vec!["--tool", "claude", "--scenario", "s.json", "--scenario"],
            vec!["--tool", "claude", "--scenario", "s.json", "--tool"],
            vec!["--scenario", "s.json", "--tool", "claude", "--tool"],
        ] {
            let error = parse_err(&args);
            assert_eq!(
                error.mismatch_type,
                MismatchType::UnknownOption,
                "for {args:?}"
            );
        }
    }

    /// And a repeat immediately before the terminator, where the token that
    /// follows must not be stepped over.
    #[test]
    fn a_repeat_just_before_the_terminator_does_not_swallow_it() {
        let error = parse_err(&[
            "--tool", "claude", "--scenario", "s.json", "--scenario", "--", "-p",
        ]);
        assert_eq!(error.mismatch_type, MismatchType::UnknownOption);
    }

    /// ACM-FR-04: the terminator is never taken as an option's value, so an
    /// option that runs into it has no value and reads as omitted.
    #[test]
    fn the_terminator_is_never_consumed_as_an_option_value() {
        let error = parse_err(&["--tool", "claude", "--scenario", "--", "-p", "hi"]);
        assert_eq!(error.mismatch_type, MismatchType::MissingScenarioOption);
        assert_eq!(error.tool.as_deref(), Some("claude"));

        let error = parse_err(&["--scenario", "s.json", "--tool", "--", "-p"]);
        assert_eq!(error.mismatch_type, MismatchType::MissingToolOption);

        // And the vector after it is still delivered intact when the rest of
        // the invocation is well formed.
        let invocation = parse_ok(&["--tool", "claude", "--scenario", "s.json", "--", "-p"]);
        assert_eq!(invocation.tool_args, os(&["-p"]));
    }

    /// A repeated option *after* the terminator is just an argument.
    #[test]
    fn options_repeated_after_the_terminator_are_not_repeats() {
        let invocation = parse_ok(&[
            "--tool", "claude", "--scenario", "s.json", "--", "--tool", "codex",
        ]);
        assert_eq!(invocation.adapter.id(), "claude");
        assert_eq!(invocation.tool_args, os(&["--tool", "codex"]));
    }
}
