//! The reads of the seam (OSM-FR-XSOC, OSM-FR-VALR, OSM-FR-ZIMO,
//! OSM-FR-WWWK, OSM-FR-JTOQ, OSM-FR-JAIC, OSM-FR-SIBT, OSM-FR-OPZF,
//! OSM-FR-FCMY, OSM-FR-HHUD, OSM-FR-IFWM).

use super::*;

const SPECS: &str = r#"{ "specs": [
  { "id": "ui/theme", "requirementCount": 2 },
  { "id": "auth", "requirementCount": 5, "extra": true }
] }"#;

const SHOW: &str = r#"{
  "id": "auth", "title": "Auth", "overview": "Sign in and out.", "requirementCount": 2,
  "requirements": [
    { "name": "Session Expiration", "text": "Sessions SHALL expire.",
      "scenarios": [ { "name": "Idle", "rawText": "- WHEN idle\n- THEN expire" } ] },
    { "name": "Logout", "text": "Users SHALL log out.", "scenarios": [] }
  ],
  "metadata": { "version": "1.0.0", "format": "openspec" }
}"#;

const CHANGES: &str = r#"{ "changes": [
  { "name": "add-logout", "completedTasks": 1, "totalTasks": 4,
    "lastModified": "2026-11-10T10:00:00.000Z", "status": "in-progress" }
] }"#;

const STATUS: &str = r#"{ "changes": [
  { "changeName": "add-logout", "schemaName": "spec-driven", "isComplete": false,
    "artifacts": [
      { "id": "proposal", "status": "done", "requires": [] },
      { "id": "specs", "status": "ready", "requires": ["proposal"] },
      { "id": "tasks", "status": "blocked", "requires": ["specs"], "missingDeps": ["specs"] }
    ] }
], "root": {} }"#;

fn full_runner() -> FakeRunner {
    FakeRunner::default().json("list", SPECS).json("show", SHOW).json("status", STATUS)
}

// OSM-FR-JTOQ: one entry per spec, ordered by id, extra fields ignored
// (OSM-FR-WWWK).
#[test]
fn capabilities_are_listed_in_id_order() {
    let (_dir, root) = openspec_project();
    let (seam, runner, _) = seam_with(full_runner());
    let list = seam.list_capabilities(&Silent, buffer(), Some(&root)).unwrap();
    assert_eq!(
        list,
        vec![
            Capability { id: "auth".into(), requirement_count: 5 },
            Capability { id: "ui/theme".into(), requirement_count: 2 },
        ]
    );
    let call = runner.calls().into_iter().find(|c| c.args[0] == "list").unwrap();
    assert_eq!(call.args, ["list", "--specs", "--json"]);
    assert_eq!(call.executable, PathBuf::from(CLI));
    // OSM-FR-HHUD: the process runs in the root of the active worktree.
    assert_eq!(call.cwd, root.path());
}

// OSM-FR-XSOC: an inactive mode fails the read and starts no process.
#[test]
fn a_read_while_the_mode_is_inactive_runs_nothing() {
    let (_dir, root) = plain_project();
    let (seam, runner, _) = seam_with(full_runner());
    assert_eq!(
        seam.list_capabilities(&Silent, buffer(), Some(&root)),
        Err(OpenSpecReadError::OpenSpecModeInactive)
    );
    assert_eq!(seam.list_changes(&Silent, buffer(), None), Err(OpenSpecReadError::NoOpenProject));
    assert!(runner.calls().is_empty());
}

// OSM-FR-XSOC: a CLI that is not usable fails the read with its state.
#[test]
fn a_read_with_no_usable_cli_carries_the_cli_state() {
    let (_dir, root) = openspec_project();
    let seam = OpenSpec::new(Arc::new(full_runner()), Arc::new(FakeProbe::default()), env());
    assert_eq!(
        seam.read_capability(&Silent, buffer(), Some(&root), "auth"),
        Err(OpenSpecReadError::CliUnavailable(OpenSpecCliState::NotFound))
    );
}

// OSM-FR-ZIMO, OSM-FR-WWWK: a timeout, an exit status other than zero, and
// output that is not the expected JSON each fail the read with their code.
#[test]
fn process_failures_fail_the_read_with_their_codes() {
    let (_dir, root) = openspec_project();
    for (answer, expected) in [
        (
            Err(ProcessError::timed_out()),
            OpenSpecReadError::CliTimedOut { diagnostics: Vec::new() },
        ),
        (
            Ok(ProcessOutput {
                exit_status: Some(2),
                stdout: SPECS.as_bytes().to_vec(),
                stderr: Vec::new(),
            }),
            OpenSpecReadError::CliFailed { exit_status: Some(2), diagnostics: Vec::new() },
        ),
        (Ok(ok("not json")), OpenSpecReadError::CliOutputUnreadable),
        (Ok(ok(r#"{ "specs": [ { "id": 3 } ] }"#)), OpenSpecReadError::CliOutputUnreadable),
    ] {
        let (seam, _, _) = seam_with(FakeRunner::default().answering("list", answer));
        assert_eq!(seam.list_capabilities(&Silent, buffer(), Some(&root)), Err(expected));
    }
}

// OSM-FR-JAIC: an id that is empty, absolute, climbs, or holds another
// character is refused, and no process starts for it.
#[test]
fn an_invalid_capability_id_runs_nothing() {
    let (_dir, root) = openspec_project();
    let (seam, runner, _) = seam_with(full_runner());
    for id in ["", "/auth", "../auth", "ui/../auth", "auth spec", "auth;rm", "ui//theme", "a\\b"] {
        assert_eq!(
            seam.read_capability(&Silent, buffer(), Some(&root), id),
            Err(OpenSpecReadError::InvalidCapabilityId),
            "{id:?}"
        );
    }
    assert_eq!(runner.calls_of("show"), 0);
    assert!(crate::openspec::reads::valid_capability_id("ui/theme-2_v1.0"));
}

// OSM-FR-SIBT: a capability with no spec file is not found, and no process
// starts for it.
#[test]
fn a_capability_with_no_spec_file_is_not_found() {
    let (_dir, root) = openspec_project();
    let (seam, runner, _) = seam_with(full_runner());
    assert_eq!(
        seam.read_capability(&Silent, buffer(), Some(&root), "auth"),
        Err(OpenSpecReadError::CapabilityNotFound)
    );
    assert_eq!(runner.calls_of("show"), 0);
}

// OSM-FR-SIBT: a capability is read through `show`, with its requirements and
// scenarios in file order.
#[test]
fn a_capability_is_read_whole_through_show() {
    let (dir, root) = openspec_project();
    write(&dir, "openspec/specs/auth/spec.md", "# Auth\n");
    let (seam, runner, _) = seam_with(full_runner());
    let spec = seam.read_capability(&Silent, buffer(), Some(&root), "auth").unwrap();
    assert_eq!(spec.id, "auth");
    assert_eq!(spec.title, "Auth");
    assert_eq!(spec.overview, "Sign in and out.");
    let names: Vec<_> = spec.requirements.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["Session Expiration", "Logout"]);
    assert_eq!(
        spec.requirements[0].scenarios,
        vec![Scenario { name: "Idle".into(), text: "- WHEN idle\n- THEN expire".into() }]
    );
    let call = runner.calls().into_iter().find(|c| c.args[0] == "show").unwrap();
    assert_eq!(call.args, ["show", "auth", "--type", "spec", "--json", "--no-interactive"]);
}

// OSM-FR-OPZF, OSM-FR-FCMY: `list` and `status` run once each, `list` with no
// flag that adds archived changes; the capabilities come from the delta
// folders.
#[test]
fn changes_join_list_and_status_and_take_capabilities_from_the_folders() {
    let (dir, root) = openspec_project();
    write(&dir, "openspec/changes/add-logout/specs/auth/spec.md", "## ADDED Requirements\n");
    write(&dir, "openspec/changes/add-logout/specs/ui/theme/spec.md", "delta\n");
    write(&dir, "openspec/changes/add-logout/specs/notes.md", "not a delta\n");
    std::fs::create_dir_all(dir.path().join("openspec/changes/add-logout/specs/empty")).unwrap();
    let (seam, runner, _) =
        seam_with(FakeRunner::default().json("list", CHANGES).json("status", STATUS));

    let changes = seam.list_changes(&Silent, buffer(), Some(&root)).unwrap();
    assert_eq!(
        changes,
        vec![ChangeSummary {
            name: "add-logout".into(),
            schema: "spec-driven".into(),
            artifacts: vec![
                ArtifactState { id: "proposal".into(), status: "done".into() },
                ArtifactState { id: "specs".into(), status: "ready".into() },
                ArtifactState { id: "tasks".into(), status: "blocked".into() },
            ],
            is_complete: false,
            completed_tasks: 1,
            total_tasks: 4,
            capabilities: vec!["auth".into(), "ui/theme".into()],
            last_modified: "2026-11-10T10:00:00.000Z".into(),
        }]
    );
    assert_eq!(runner.calls_of("list"), 1);
    assert_eq!(runner.calls_of("status"), 1);
    let args: Vec<Vec<String>> = runner.calls().into_iter().map(|c| c.args).collect();
    assert!(args.contains(&vec!["list".into(), "--changes".into(), "--json".into()]));
    assert!(args.contains(&vec!["status".into(), "--all".into(), "--json".into()]));
}

// OSM-FR-OPZF: a change that the status output does not name keeps an empty
// schema and no artifacts.
#[test]
fn a_change_missing_from_status_keeps_empty_state() {
    let (_dir, root) = openspec_project();
    let (seam, _, _) = seam_with(
        FakeRunner::default().json("list", CHANGES).json("status", r#"{ "changes": [] }"#),
    );
    let changes = seam.list_changes(&Silent, buffer(), Some(&root)).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].schema, "");
    assert!(changes[0].artifacts.is_empty());
    assert!(changes[0].capabilities.is_empty());
}

// OSM-FR-VALR: across every read, the seam runs only `--version`, `list`,
// `show`, and `status`.
#[test]
fn the_seam_runs_only_the_read_subcommands() {
    let (dir, root) = openspec_project();
    write(&dir, "openspec/specs/auth/spec.md", "# Auth\n");
    let (seam, runner, _) = seam_with(
        FakeRunner::default().json("list", SPECS).json("show", SHOW).json("status", STATUS),
    );
    let buffer = buffer();
    let _ = seam.status(&Silent, buffer, Some(&root));
    let _ = seam.list_capabilities(&Silent, buffer, Some(&root));
    let _ = seam.read_capability(&Silent, buffer, Some(&root), "auth");
    let _ = seam.list_changes(&Silent, buffer, Some(&root));
    for call in runner.calls() {
        assert!(
            ["--version", "list", "show", "status"].contains(&call.args[0].as_str()),
            "{:?}",
            call.args
        );
    }
}

// OSM-FR-IFWM: a run logs its subcommand, exit status, duration, and output
// size, and a failure logs its code. No record holds an argument value, a
// project path, or any output.
#[test]
fn records_name_the_subcommand_and_never_an_argument_path_or_output() {
    let (dir, root) = openspec_project();
    write(&dir, "openspec/specs/auth/spec.md", "# Auth\n");
    let buffer = buffer();
    let (seam, _, _) = seam_with(full_runner());
    seam.read_capability(&Silent, buffer, Some(&root), "auth").unwrap();
    let (failing, _, _) = seam_with(FakeRunner::default().answering(
        "list",
        Ok(ProcessOutput { exit_status: Some(7), stdout: vec![], stderr: Vec::new() }),
    ));
    let _ = failing.list_capabilities(&Silent, buffer, Some(&root));

    let all = records(buffer);
    let ran = all
        .iter()
        .find(|r| r.message == "openspec cli ran" && r.fields["subcommand"] == "show")
        .unwrap();
    assert_eq!(ran.fields["exitStatus"], 0);
    assert!(ran.fields.contains_key("durationMs"));
    assert_eq!(ran.fields["outputBytes"], SHOW.len());
    assert!(ran.domains.contains(&Domain::Backend));

    let failed = all.iter().find(|r| r.message == "openspec cli failed").unwrap();
    assert_eq!(failed.level, LogLevel::Warn);
    assert_eq!(failed.fields["subcommand"], "list");
    assert_eq!(failed.fields["exitStatus"], 7);
    let read = all.iter().find(|r| r.message == "openspec read failed").unwrap();
    assert_eq!(read.fields["code"], "cli_failed");

    let serialised = serde_json::to_string(&all).unwrap();
    let root_text = root.path().to_string_lossy().to_string();
    for leaked in ["\"auth\"", "Session Expiration", "Sign in and out", root_text.as_str()] {
        assert!(!serialised.contains(leaked), "a record must not carry {leaked:?}");
    }
}

// OSM-FR-OPZF: the archive folder itself is never a change, whatever the CLI
// lists.
#[test]
fn the_archive_folder_is_never_a_change() {
    let (_dir, root) = openspec_project();
    let list = r#"{ "changes": [
      { "name": "archive", "completedTasks": 0, "totalTasks": 0, "lastModified": "2026-01-01T00:00:00.000Z" },
      { "name": "add-logout", "completedTasks": 0, "totalTasks": 1, "lastModified": "2026-01-02T00:00:00.000Z" }
    ] }"#;
    let (seam, runner, _) =
        seam_with(FakeRunner::default().json("list", list).json("status", STATUS));
    let names: Vec<String> = seam
        .list_changes(&Silent, buffer(), Some(&root))
        .unwrap()
        .into_iter()
        .map(|c| c.name)
        .collect();
    assert_eq!(names, ["add-logout"]);
    for call in runner.calls() {
        assert!(!call.args.iter().any(|a| a == "--all" && call.args[0] == "list"));
        assert!(!call.args.iter().any(|a| a == "--archived"));
    }
}

// OSM-FR-ZIMO: every CLI process runs under the 30 second limit.
#[test]
fn every_process_runs_under_the_thirty_second_limit() {
    let (dir, root) = openspec_project();
    write(&dir, "openspec/specs/auth/spec.md", "# Auth\n");
    let (seam, runner, _) = seam_with(full_runner());
    let buffer = buffer();
    let _ = seam.list_capabilities(&Silent, buffer, Some(&root));
    let _ = seam.read_capability(&Silent, buffer, Some(&root), "auth");
    let _ = seam.list_changes(&Silent, buffer, Some(&root));
    let calls = runner.calls();
    assert!(calls.len() >= 4, "{calls:?}");
    for call in calls {
        assert_eq!(call.timeout, Duration::from_secs(30), "{:?}", call.args);
    }
}

// OSM-FR-HHUD: the version check of an open project runs in its worktree root.
#[test]
fn the_version_check_runs_in_the_worktree_root() {
    let (_dir, root) = openspec_project();
    let (seam, runner, _) = seam_with(full_runner());
    let _ = seam.list_capabilities(&Silent, buffer(), Some(&root));
    let version = runner.calls().into_iter().find(|c| c.args[0] == "--version").unwrap();
    assert_eq!(version.cwd, root.path());
}

/// A runner that holds every process until the test lets them go, and counts
/// how many it holds at once.
#[derive(Default)]
struct HeldRunner {
    state: Mutex<(usize, usize, bool)>,
    changed: std::sync::Condvar,
}

impl OpenSpecRunner for HeldRunner {
    fn run(
        &self,
        _e: &Path,
        args: &[&str],
        _c: &Path,
        _t: Duration,
    ) -> Result<ProcessOutput, ProcessError> {
        if args.first() == Some(&"--version") {
            return Ok(ok("1.17.0"));
        }
        let mut s = self.state.lock().unwrap();
        s.0 += 1;
        s.1 = s.1.max(s.0);
        self.changed.notify_all();
        while !s.2 {
            s = self.changed.wait(s).unwrap();
        }
        s.0 -= 1;
        Ok(ok(SPECS))
    }
}

// OSM-FR-QHRV: of six reads at once, four run a process and two wait for a
// slot; no more than four ever run together.
#[test]
fn at_most_four_cli_processes_run_at_once() {
    let (_dir, root) = openspec_project();
    let runner = Arc::new(HeldRunner::default());
    let seam = Arc::new(OpenSpec::new(
        runner.clone(),
        Arc::new(FakeProbe::default().with_executable(CLI)),
        env(),
    ));
    seam.cli_state(&Silent, buffer());
    let readers: Vec<_> = (0..6)
        .map(|_| {
            let seam = Arc::clone(&seam);
            let root = root.clone();
            std::thread::spawn(move || {
                seam.list_capabilities(&Silent, buffer(), Some(&root)).unwrap()
            })
        })
        .collect();
    // Wait for the state, not for time: four inside the runner and two at the
    // gate. A gate that let a fifth through would never reach this state.
    {
        let mut s = runner.state.lock().unwrap();
        while !(s.0 == 4 && seam.slots.waiting() == 2) {
            s = runner.changed.wait_timeout(s, Duration::from_millis(20)).unwrap().0;
        }
        assert_eq!(s.0, 4);
        s.2 = true;
        runner.changed.notify_all();
    }
    for reader in readers {
        assert_eq!(reader.join().unwrap().len(), 2);
    }
    assert_eq!(runner.state.lock().unwrap().1, 4);
    assert_eq!(seam.slots.in_use(), 0);
}
