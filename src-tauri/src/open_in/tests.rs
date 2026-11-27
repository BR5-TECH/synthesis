use super::*;

fn programs(plan: &[Launch]) -> Vec<&str> {
    plan.iter().map(|l| l.program.as_str()).collect()
}

// PST-FR-JJGI: macOS starts Terminal in the folder.
#[test]
fn macos_starts_terminal() {
    let dir = Path::new("/p/docs");
    let plan = launch_plan(Platform::MacOs, dir);
    assert_eq!(programs(&plan), ["open"]);
    assert_eq!(plan[0].args, ["-a", "Terminal", "/p/docs"].map(OsString::from));
    assert_eq!(plan[0].cwd, dir);
}

// PST-FR-JJGI: Windows starts Windows Terminal, else cmd.exe.
#[test]
fn windows_starts_windows_terminal_then_cmd() {
    let dir = Path::new("C:/p/docs");
    let plan = launch_plan(Platform::Windows, dir);
    assert_eq!(programs(&plan), ["wt.exe", "cmd.exe"]);
    assert_eq!(plan[0].args, ["-d", "C:/p/docs"].map(OsString::from));
    assert_eq!(plan[1].args, ["/c", "start", "", "cmd.exe"].map(OsString::from));
    assert!(plan.iter().all(|l| l.cwd == dir));
}

// PST-FR-JJGI: the folder is never an argument of `cmd.exe`, which reads `&`
// and `%` in it, and a folder with `;` skips Windows Terminal.
#[test]
fn a_windows_folder_never_reaches_a_command_line_that_reads_it() {
    let dir = Path::new("C:/p/x&calc;y%PATH%");
    let plan = launch_plan(Platform::Windows, dir);
    assert_eq!(programs(&plan), ["cmd.exe"]);
    assert!(plan[0].args.iter().all(|a| a != dir.as_os_str()));
    assert_eq!(plan[0].cwd, dir);
}

// PST-FR-JJGI: Linux tries the five terminals in order, each in the folder.
#[test]
fn linux_tries_five_terminals_in_order() {
    let dir = Path::new("/p/docs");
    let plan = launch_plan(Platform::Linux, dir);
    assert_eq!(
        programs(&plan),
        ["x-terminal-emulator", "gnome-terminal", "konsole", "xfce4-terminal", "xterm"]
    );
    assert!(plan.iter().all(|l| l.cwd == dir));
    assert_eq!(plan[1].args, [OsString::from("--working-directory=/p/docs")]);
    assert_eq!(plan[2].args, ["--workdir", "/p/docs"].map(OsString::from));
}

// PST-FR-JJGI: a program that is not installed moves on to the next one.
#[test]
fn a_missing_program_moves_on() {
    let plan = launch_plan(Platform::Linux, Path::new("/p"));
    let mut tried = Vec::new();
    let started = start_first(&plan, |l| {
        tried.push(l.program.clone());
        if l.program == "konsole" {
            Ok(())
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    });
    assert_eq!(started.unwrap(), "konsole");
    assert_eq!(tried, ["x-terminal-emulator", "gnome-terminal", "konsole"]);
}

// PST-FR-JJGI: no installed program is `no_terminal_found`.
#[test]
fn no_program_is_no_terminal_found() {
    let plan = launch_plan(Platform::Linux, Path::new("/p"));
    let err = start_first(&plan, |_| Err(io::ErrorKind::NotFound.into())).unwrap_err();
    assert_eq!(err, StartFailure { code: ERR_NO_TERMINAL_FOUND, last: None });
}

// PST-FR-JJGI: an installed program that fails, with none that starts, is
// `terminal_start_failed`.
#[test]
fn a_present_program_that_fails_is_start_failed() {
    let plan = launch_plan(Platform::Windows, Path::new("C:/p"));
    let err = start_first(&plan, |l| {
        if l.program == "wt.exe" {
            Err(io::ErrorKind::PermissionDenied.into())
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    })
    .unwrap_err();
    assert_eq!(err.code, ERR_TERMINAL_START_FAILED);
    assert_eq!(err.last, Some(("wt.exe".to_string(), io::ErrorKind::PermissionDenied)));
}

// PST-FR-JJGI: a folder is its own terminal folder, a file uses its parent,
// and the tree root is the worktree root.
#[test]
fn the_terminal_folder_of_a_folder_a_file_and_the_root() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("docs/specs")).unwrap();
    std::fs::write(tmp.path().join("docs/specs/a.md"), b"a").unwrap();
    let root = RootFs::for_root(tmp.path());
    assert_eq!(
        terminal_folder(&root, "docs/specs").unwrap(),
        tmp.path().join("docs").join("specs")
    );
    assert_eq!(
        terminal_folder(&root, "docs/specs/a.md").unwrap(),
        tmp.path().join("docs").join("specs")
    );
    assert_eq!(terminal_folder(&root, "").unwrap(), tmp.path());
}

// PST-FR-JJGI, PST-FR-ZGNE: a path that is not there is `path not found`.
#[test]
fn a_missing_path_is_not_found() {
    let tmp = tempfile::tempdir().unwrap();
    let root = RootFs::for_root(tmp.path());
    assert_eq!(terminal_folder(&root, "missing").unwrap_err(), ERR_PATH_NOT_FOUND);
    assert_eq!(entry_kind(&root, "missing/a.md").unwrap_err(), ERR_PATH_NOT_FOUND);
}

// PST-FR-JJGI: a file at the root uses the root folder.
#[test]
fn a_root_file_uses_the_root_folder() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.md"), b"a").unwrap();
    let root = RootFs::for_root(tmp.path());
    assert_eq!(terminal_folder(&root, "a.md").unwrap(), tmp.path());
}

// PST-FR-JJGI: one installed program that fails and the rest missing is
// `terminal_start_failed`, and every program is tried.
#[test]
fn a_failing_first_program_with_the_rest_missing_is_start_failed() {
    let plan = launch_plan(Platform::Linux, Path::new("/p"));
    let mut tried = 0;
    let err = start_first(&plan, |l| {
        tried += 1;
        if l.program == "x-terminal-emulator" {
            Err(io::ErrorKind::PermissionDenied.into())
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    })
    .unwrap_err();
    assert_eq!(err.code, ERR_TERMINAL_START_FAILED);
    assert_eq!(tried, plan.len());
}

// PST-FR-AGMX, PST-FR-26: a path through a symbolic link, and a path that
// leaves the root, are refused as an escape.
#[cfg(unix)]
#[test]
fn a_symlink_or_an_escape_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("docs")).unwrap();
    std::os::unix::fs::symlink(tmp.path().join("docs"), tmp.path().join("link")).unwrap();
    std::os::unix::fs::symlink(outside.path(), tmp.path().join("out")).unwrap();
    let root = RootFs::for_root(tmp.path());
    for path in ["link", "out", "../x"] {
        assert_eq!(terminal_folder(&root, path).unwrap_err(), ERR_PATH_ESCAPES_ROOT, "{path}");
    }
}
