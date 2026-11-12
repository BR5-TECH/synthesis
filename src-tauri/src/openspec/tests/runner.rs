//! The production runner, with a shell script in place of the CLI
//! (OSM-FR-HHUD, OSM-FR-ITZI, OSM-FR-ZIMO).

use std::os::unix::fs::PermissionsExt;

use super::*;
use crate::openspec::cli::RealRunner;

fn script(dir: &TempDir, body: &str) -> PathBuf {
    let path = dir.path().join("bin").join("openspec");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

// OSM-FR-HHUD: the process runs in the given folder, with the fixed variables,
// the executable directory first on `PATH`, and its standard input closed.
#[test]
fn the_real_runner_sets_the_folder_the_variables_and_closed_input() {
    let dir = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let exe = script(
        &dir,
        "pwd\necho \"$OPENSPEC_TELEMETRY $DO_NOT_TRACK $NO_COLOR\"\necho \"$PATH\" | cut -d: -f1\n\
         if read line; then echo input; else echo closed; fi",
    );
    // The test process gets a pipe with a line in it as its standard input,
    // so a child that inherited it would read that line. nextest runs each
    // test in its own process, so no other test sees this.
    let mut fds = [0; 2];
    // SAFETY: `pipe` writes two descriptors into the array, `write` reads
    // three bytes of a static string, and `dup2` replaces descriptor 0.
    unsafe {
        assert_eq!(libc::pipe(fds.as_mut_ptr()), 0);
        assert_eq!(libc::write(fds[1], b"x\n".as_ptr().cast(), 2), 2);
        assert_eq!(libc::dup2(fds[0], 0), 0);
    }
    let out = RealRunner.run(&exe, &[], work.path(), Duration::from_secs(20)).unwrap();
    assert_eq!(out.exit_status, Some(0));
    let text = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    let cwd = std::fs::canonicalize(work.path()).unwrap();
    assert_eq!(std::fs::canonicalize(lines[0]).unwrap(), cwd);
    assert_eq!(lines[1], "0 1 1");
    assert_eq!(Path::new(lines[2]), exe.parent().unwrap());
    assert_eq!(lines[3], "closed");
}

// OSM-FR-WWWK: the exit status reaches the caller.
#[test]
fn the_real_runner_reports_the_exit_status() {
    let dir = TempDir::new().unwrap();
    let exe = script(&dir, "echo partial\nexit 4");
    let out = RealRunner.run(&exe, &[], dir.path(), Duration::from_secs(20)).unwrap();
    assert_eq!(out.exit_status, Some(4));
    assert_eq!(out.stdout, b"partial\n");
}

// OSM-FR-ZIMO: a process that runs past its limit is stopped.
#[test]
fn the_real_runner_stops_a_process_past_its_limit() {
    let dir = TempDir::new().unwrap();
    let exe = script(&dir, "sleep 30");
    let started = std::time::Instant::now();
    let out = RealRunner.run(&exe, &[], dir.path(), Duration::from_millis(200));
    assert_eq!(out, Err(ProcessError::TimedOut));
    assert!(started.elapsed() < Duration::from_secs(20), "the process was not stopped");
}

// OSM-FR-SKUP: a file that cannot be started is `not_executable`.
#[test]
fn the_real_runner_reports_a_file_it_cannot_start() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("openspec");
    std::fs::write(&path, "not a program").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        RealRunner.run(&path, &[], dir.path(), Duration::from_secs(5)),
        Err(ProcessError::NotExecutable)
    );
}

// OSM-FR-XKQB, OSM-FR-HHUD, OAR-FR-IAKL: the archive runs through the real
// runner on the terms of every other process: in the root of the working
// copy, with the fixed variables and the executable directory first on
// `PATH`.
#[test]
fn the_archive_runs_with_the_fixed_variables_through_the_real_runner() {
    let dir = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let exe = script(
        &dir,
        "echo \"$OPENSPEC_TELEMETRY $DO_NOT_TRACK $NO_COLOR\" > vars.txt\n\
         echo \"$PATH\" | cut -d: -f1 >> vars.txt\n\
         echo '{ \"archive\": { \"change\": \"add-logout\", \"archivedAs\": \"2026-11-11-add-logout\" } }'",
    );
    let probe = Arc::new(FakeProbe::default().with_executable(exe.to_str().unwrap()));
    let seam = OpenSpec::new(Arc::new(RealRunner), probe, env());
    let answer = seam.archive_change(&Silent, buffer(), &exe, work.path(), "add-logout").unwrap();
    assert_eq!(answer.archived_as, "2026-11-11-add-logout");
    let vars = std::fs::read_to_string(work.path().join("vars.txt")).unwrap();
    let lines: Vec<&str> = vars.lines().collect();
    assert_eq!(lines[0], "0 1 1");
    assert_eq!(Path::new(lines[1]), exe.parent().unwrap());
}
