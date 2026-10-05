// Command probing for the build script.
//
// Specification: specifications/server/BMS-backend-microservice.md
// Requirement: BMS-FR-16.
//
// This file holds no inner doc comment and no `use` statement, because
// `build.rs` pulls it in with `include!` beside the crate's own modules. It is
// kept apart from `version_resolve.rs` so that the version rule there stays the
// pure function BMS-FR-26 specifies, and everything that touches the operating
// system lives here.
//
// Nothing in the running service calls this. The crate compiles it only so the
// build script's behaviour is covered by the crate's own tests.

/// Runs a command and returns its trimmed output.
///
/// BMS-FR-16: a program that is absent, or a command that reports a failure, is
/// not a build failure. Every such case returns `None`, and the caller falls to
/// the next source of BMS-FR-14. Output that is empty or only white space
/// counts as no value, because a command can succeed and print nothing.
pub fn capture(
    program: &str,
    arguments: &[&str],
    working_directory: &std::path::Path,
) -> Option<String> {
    let output = std::process::Command::new(program)
        .args(arguments)
        .current_dir(working_directory)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn here() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
    }

    // BMS-FR-16: the program is absent. The build script must fall to the next
    // source rather than fail the compilation, which means the probe returns
    // `None` instead of panicking or propagating an error.
    #[test]
    fn a_program_that_is_absent_returns_no_value() {
        let captured = capture(
            "synthesis-no-such-program-exists-here",
            &["--version"],
            here(),
        );
        assert_eq!(captured, None);
    }

    // BMS-FR-16: the program exists but the command reports a failure, which is
    // what `git describe --tags --exact-match` does at an untagged commit.
    #[cfg(unix)]
    #[test]
    fn a_command_that_reports_a_failure_returns_no_value() {
        assert_eq!(capture("false", &[], here()), None);
    }

    // A command can succeed and print nothing. An empty value must not become
    // the version, or the image would report a blank string.
    #[cfg(unix)]
    #[test]
    fn a_command_that_prints_nothing_returns_no_value() {
        assert_eq!(capture("true", &[], here()), None);
        assert_eq!(capture("echo", &["   "], here()), None);
    }

    // The happy path, with the trailing newline every command adds removed.
    #[cfg(unix)]
    #[test]
    fn output_is_returned_without_its_surrounding_white_space() {
        assert_eq!(
            capture("echo", &["  v1.4.0  "], here()),
            Some("v1.4.0".to_string())
        );
    }

    // A working directory that does not exist makes the spawn itself fail.
    #[test]
    fn a_working_directory_that_is_absent_returns_no_value() {
        let missing = here().join("no-such-directory-exists-here");
        assert_eq!(capture("echo", &["hello"], &missing), None);
    }
}
