//! The real runner and the real file probe, against real processes
//! (AIC-FR-04, AIC-FR-05).

use super::*;

// -- AIC-FR-04: the real runner, against real processes ---------------
//
// Everything above fakes the child process, which tests the *rules* but
// leaves the production runner — the poll loop, the kill, the reaping wait,
// and the two output-drain threads — with no coverage at all. These three
// use only binaries every POSIX system has, so they stay hermetic: no vendor
// CLI is involved and CI needs nothing installed.

#[cfg(unix)]
#[test]
fn the_real_runner_terminates_a_binary_that_never_answers() {
    // AIC-FR-04 says "and no child process is left running" — a claim about
    // the real runner that a fake `RunError::TimedOut` cannot make.
    // Read off a marker the child would create on the far side of its
    // sleep, rather than off a stopwatch: "the child was killed" and "this
    // machine is fast" are different claims, and only the first is the
    // requirement. The marker's absence means the 30 seconds never elapsed,
    // at any speed and under any load.
    let marker = tempfile::tempdir().expect("dir");
    let survived = marker.path().join("survived");
    let script = format!("sleep 30; touch {}", survived.to_string_lossy());

    let runner = RealCliRunner;
    let result = runner.run(
        Path::new("/bin/sh"),
        &["-c", &script],
        Duration::from_millis(200),
    );
    assert_eq!(result.unwrap_err(), RunError::TimedOut);
    assert!(
        !survived.exists(),
        "the deadline must be enforced by killing the child, not by waiting it out"
    );
}

#[cfg(unix)]
#[test]
fn the_real_runner_drains_output_larger_than_a_pipe_buffer() {
    // The drain threads exist so a binary that fills the pipe cannot
    // deadlock against our own wait. Without them this blocks until the
    // timeout and comes back `TimedOut` instead of the output.
    let runner = RealCliRunner;
    let out = runner
        .run(
            Path::new("/bin/sh"),
            &["-c", "yes ab | head -c 200000"],
            Duration::from_secs(20),
        )
        .expect("a binary that merely produces a lot of output must succeed");
    assert!(
        out.stdout.len() >= 200_000,
        "expected the whole stream, got {} bytes",
        out.stdout.len()
    );
    assert!(out.success);
}

#[cfg(unix)]
#[test]
fn the_real_file_probe_tells_executable_from_merely_present() {
    // AIC-FR-05 rests on this distinction: a file that exists but cannot be
    // run is a permissions problem, not a wrong path.
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::TempDir::new().unwrap();
    let probe = RealFileProbe;

    let plain = dir.path().join("plain");
    std::fs::write(&plain, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(&plain, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(probe.exists(&plain));
    assert!(!probe.is_executable(&plain));

    let runnable = dir.path().join("runnable");
    std::fs::write(&runnable, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(&runnable, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(probe.is_executable(&runnable));

    // A directory is neither, however executable its mode bits look.
    assert!(!probe.exists(dir.path()));
    assert!(!probe.is_executable(dir.path()));

    assert!(!probe.exists(&dir.path().join("absent")));
}

// -- AIC-FR-05: identity, not mere existence --------------------------

#[test]
fn identity_is_what_verification_confirms_not_the_mere_fact_of_running() {
    // AIC-FR-05 is the rule that stops Claude Code being "configured" as
    // `/bin/ls`. Exercised directly, because the branch that makes it work
    // for a CLI printing nothing but a version number is subtle enough to
    // regress silently.
    let claude = vendor_descriptor("claude_code").unwrap();
    let codex = vendor_descriptor("codex").unwrap();
    let banner = |text: &str| CliOutput {
        stdout: text.into(),
        stderr: String::new(),
        success: true,
    };

    // Named in the banner: accepted however the file was renamed.
    assert!(identifies_vendor(claude, Path::new("/opt/x"), &banner("claude 2.1.4")));
    // Cross-vendor confusion is exactly what this must catch.
    assert!(!identifies_vendor(codex, Path::new("/usr/bin/codex-x"), &banner("claude 2.1.4")));
    assert!(!identifies_vendor(claude, Path::new("/bin/ls"), &banner("ls (GNU coreutils) 9.1")));

    // A bare version is accepted only when the file is named as the vendor's
    // executable — otherwise any program printing a version would pass.
    assert!(identifies_vendor(claude, Path::new("/usr/bin/claude"), &banner("0.4.12")));
    assert!(!identifies_vendor(claude, Path::new("/usr/bin/whatever"), &banner("0.4.12")));
    // And a bare version from a plausibly-named file is still refused when
    // the output is prose rather than a version.
    assert!(!identifies_vendor(
        claude,
        Path::new("/usr/bin/claude"),
        &banner("command not found")
    ));

    // The banner is read from stderr too, because CLIs disagree about which
    // stream a version goes to.
    assert!(identifies_vendor(
        claude,
        Path::new("/opt/x"),
        &CliOutput {
            stdout: String::new(),
            stderr: "claude 2.1.4".into(),
            success: true,
        }
    ));
}
