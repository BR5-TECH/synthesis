//! The CLI: resolution, the version check and its kept result, the state, the
//! child environment, and the slots (OSM-FR-EZKW, OSM-FR-ZNVZ, OSM-FR-SKUP,
//! OSM-FR-HHUD, OSM-FR-ITZI, OSM-FR-QHRV).

use super::*;
use crate::openspec::cli::{self as c, Slots};

// OSM-FR-EZKW: the directories of AIC-FR-03 come first, `PATH` before them,
// then the per-user Node directories in the order of the contract surface.
#[test]
fn the_search_order_is_path_then_conventional_then_the_node_directories() {
    let probe = FakeProbe::default();
    probe.folders.lock().unwrap().insert(
        PathBuf::from("/fake/home/.nvm/versions/node"),
        vec!["v9.0.0".into(), "v22.12.0".into(), "v18.1.0".into()],
    );
    let dirs = c::search_directories(&env(), &probe);
    let position = |p: &str| dirs.iter().position(|d| d == Path::new(p)).unwrap_or(usize::MAX);
    assert_eq!(dirs[0], PathBuf::from("/fake/bin"));
    assert!(position("/opt/homebrew/bin") < position("/fake/home/.local/share/pnpm"));
    assert!(position("/fake/home/.npm-global/bin") < position("/fake/home/.local/share/pnpm"));
    let v22 = position("/fake/home/.nvm/versions/node/v22.12.0/bin");
    let v18 = position("/fake/home/.nvm/versions/node/v18.1.0/bin");
    let v9 = position("/fake/home/.nvm/versions/node/v9.0.0/bin");
    assert!(position("/fake/home/.local/share/pnpm") < v22);
    assert!(v22 < v18 && v18 < v9, "highest version first: {dirs:?}");
}

// OSM-FR-EZKW: the first executable in the order wins, and a file that is not
// executable is passed over.
#[test]
fn the_first_executable_in_the_order_is_the_cli() {
    let probe = FakeProbe::default()
        .with_executable("/fake/home/.nvm/versions/node/v22.12.0/bin/openspec")
        .with_executable("/usr/local/bin/openspec");
    probe
        .folders
        .lock()
        .unwrap()
        .insert(PathBuf::from("/fake/home/.nvm/versions/node"), vec!["v22.12.0".into()]);
    assert_eq!(c::resolve(&env(), &probe), Some(PathBuf::from("/usr/local/bin/openspec")));

    let only_nvm =
        FakeProbe::default().with_executable("/fake/home/.nvm/versions/node/v22.12.0/bin/openspec");
    only_nvm
        .folders
        .lock()
        .unwrap()
        .insert(PathBuf::from("/fake/home/.nvm/versions/node"), vec!["v22.12.0".into()]);
    assert_eq!(
        c::resolve(&env(), &only_nvm),
        Some(PathBuf::from("/fake/home/.nvm/versions/node/v22.12.0/bin/openspec"))
    );
    assert_eq!(c::resolve(&env(), &FakeProbe::default()), None);
}

// OSM-FR-EZKW, OSM-FR-SKUP: each read resolves the CLI again, so a CLI that
// goes away is `not_found` at the next read.
#[test]
fn a_cli_that_goes_away_is_not_found_at_the_next_read() {
    let (seam, _, probe) = seam_with(FakeRunner::default());
    let buffer = buffer();
    assert!(seam.cli_state(&Silent, buffer).is_available());
    probe.executables.lock().unwrap().clear();
    assert_eq!(seam.cli_state(&Silent, buffer), OpenSpecCliState::NotFound);
}

// OSM-FR-ZNVZ: the version token is read from the output, with or without a
// leading `v` and around other words.
#[test]
fn the_version_is_the_first_dotted_number() {
    assert_eq!(c::parse_version("1.17.0\n"), Some(("1.17.0".to_string(), 1)));
    assert_eq!(c::parse_version("openspec v2.0.3"), Some(("2.0.3".to_string(), 2)));
    assert_eq!(c::parse_version("0.9.1"), Some(("0.9.1".to_string(), 0)));
    assert_eq!(c::parse_version("1.0.0-beta.2"), Some(("1.0.0-beta.2".to_string(), 1)));
    assert_eq!(c::parse_version("no version here"), None);
    assert_eq!(c::parse_version("1.2"), None);
}

// OSM-FR-ZNVZ, OSM-FR-SKUP: version 1 or more is `available`, and a lower one
// is `unsupported` with its path and version.
#[test]
fn a_version_below_one_is_unsupported() {
    let (seam, _, _) = seam_with(FakeRunner::default());
    assert_eq!(
        seam.cli_state(&Silent, buffer()),
        OpenSpecCliState::Available { path: CLI.into(), version: "1.17.0".into() }
    );

    let runner = Arc::new(FakeRunner::default().json("--version", "0.16.2\n"));
    let probe = Arc::new(FakeProbe::default().with_executable(CLI));
    let old = OpenSpec::new(runner, probe, env());
    assert_eq!(
        old.cli_state(&Silent, buffer()),
        OpenSpecCliState::Unsupported { path: CLI.into(), version: "0.16.2".into() }
    );
}

// OSM-FR-ZNVZ: the check is kept for one path and one modification time, and
// runs again when the time changes.
#[test]
fn the_version_check_is_kept_until_the_executable_changes() {
    let (seam, runner, probe) = seam_with(FakeRunner::default());
    let buffer = buffer();
    probe.touch(CLI, SystemTime::UNIX_EPOCH);
    seam.cli_state(&Silent, buffer);
    seam.cli_state(&Silent, buffer);
    assert_eq!(runner.calls_of("--version"), 1);

    probe.touch(CLI, SystemTime::UNIX_EPOCH + Duration::from_secs(60));
    seam.cli_state(&Silent, buffer);
    assert_eq!(runner.calls_of("--version"), 2);
}

// OSM-FR-SKUP: the three reasons of the `failed` state.
#[test]
fn each_failure_of_the_version_check_has_its_reason() {
    for (answer, reason) in [
        (Err(ProcessError::TimedOut), CliFailure::TimedOut),
        (Err(ProcessError::NotExecutable), CliFailure::NotExecutable),
        (
            Ok(ProcessOutput { exit_status: Some(3), stdout: b"1.17.0".to_vec() }),
            CliFailure::NoVersion,
        ),
        (Ok(ok("openspec")), CliFailure::NoVersion),
    ] {
        let runner = Arc::new(FakeRunner::default().answering("--version", answer));
        let probe = Arc::new(FakeProbe::default().with_executable(CLI));
        let seam = OpenSpec::new(runner, probe, env());
        assert_eq!(
            seam.cli_state(&Silent, buffer()),
            OpenSpecCliState::Failed { path: CLI.into(), reason }
        );
    }
}

// OSM-FR-SKUP: the states serialise with their tag and fields as the contract
// surface names them.
#[test]
fn the_cli_state_serialises_as_the_contract_names_it() {
    assert_eq!(
        serde_json::to_value(OpenSpecCliState::Available {
            path: "/x".into(),
            version: "1.0.0".into()
        })
        .unwrap(),
        serde_json::json!({ "state": "available", "path": "/x", "version": "1.0.0" })
    );
    assert_eq!(
        serde_json::to_value(OpenSpecCliState::NotFound).unwrap(),
        serde_json::json!({ "state": "not_found" })
    );
    assert_eq!(
        serde_json::to_value(OpenSpecCliState::Failed {
            path: "/x".into(),
            reason: CliFailure::NoVersion
        })
        .unwrap(),
        serde_json::json!({ "state": "failed", "path": "/x", "reason": "no_version" })
    );
}

// OSM-FR-HHUD: the directory of the executable comes first on `PATH`, before
// the inherited directories.
#[test]
fn the_child_path_starts_with_the_executable_directory() {
    let inherited = std::env::join_paths(["/usr/bin", "/bin"]).unwrap();
    let path = c::child_path(Path::new("/opt/node/bin/openspec"), Some(&inherited));
    let dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
    assert_eq!(dirs, vec![PathBuf::from("/opt/node/bin"), "/usr/bin".into(), "/bin".into()]);

    let alone = c::child_path(Path::new("/opt/node/bin/openspec"), None);
    assert_eq!(
        std::env::split_paths(&alone).collect::<Vec<_>>(),
        vec![PathBuf::from("/opt/node/bin")]
    );
}

// OSM-FR-HHUD, OSM-FR-ITZI: the child gets exactly the three fixed variables
// and `PATH` beside the application environment, and no credential.
#[test]
fn the_child_gets_exactly_the_fixed_variables_and_path() {
    let vars = c::child_variables(Path::new("/opt/node/bin/openspec"), None);
    let names: Vec<String> = vars.iter().map(|(k, _)| k.to_string_lossy().to_string()).collect();
    assert_eq!(names, ["OPENSPEC_TELEMETRY", "DO_NOT_TRACK", "NO_COLOR", "PATH"]);
    let value = |name: &str| {
        vars.iter().find(|(k, _)| k == name).map(|(_, v)| v.to_string_lossy().to_string()).unwrap()
    };
    assert_eq!(value("OPENSPEC_TELEMETRY"), "0");
    assert_eq!(value("DO_NOT_TRACK"), "1");
    assert_eq!(value("NO_COLOR"), "1");
}

// OSM-FR-QHRV: a fifth holder waits until one of four slots is freed.
#[test]
fn a_fifth_slot_waits_until_one_is_freed() {
    let slots = Arc::new(Slots::new(c::MAX_CONCURRENT));
    let held: Vec<_> = (0..4).map(|_| slots.acquire()).collect();
    assert_eq!(slots.in_use(), 4);

    let (sender, receiver) = std::sync::mpsc::channel();
    let waiting = Arc::clone(&slots);
    let fifth = std::thread::spawn(move || {
        let _slot = waiting.acquire();
        sender.send(()).unwrap();
    });
    // The fifth holder cannot have a slot while four are held.
    assert!(receiver.try_recv().is_err());
    assert_eq!(slots.in_use(), 4);

    drop(held);
    receiver.recv().unwrap();
    fifth.join().unwrap();
    assert_eq!(slots.in_use(), 0);
}
