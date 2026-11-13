//! The task progress of one change (OSM-FR-QLRW, OSM-FR-ODHE, OSM-FR-PEAK,
//! OSM-FR-WIQF, OSM-FR-VALR).

use super::*;
use crate::openspec::apply::parse_apply_instructions;

const NAME: &str = "add-logout-3f2a";

/// The shape OpenSpec 1.17 gives, with fields the seam does not read.
const READY: &str = r#"{ "changeName": "add-logout-3f2a", "changeDir": "/x/openspec/changes/add-logout-3f2a",
  "schemaName": "spec-driven", "contextFiles": { "proposal": "proposal.md" },
  "progress": { "total": 3, "complete": 1, "remaining": 2 },
  "tasks": [ { "id": "1", "description": "Add the button", "done": true, "sourcePath": "tasks.md", "line": 3 },
             { "id": "2", "description": "Wire the call", "done": false, "sourcePath": "tasks.md", "line": 4 },
             { "id": "3", "description": "Test it", "done": false, "sourcePath": "tasks.md", "line": 5 } ],
  "taskTrackingConfigured": true, "state": "ready", "instruction": "Implement the tasks.",
  "warnings": [] }"#;

fn project_with_change() -> (TempDir, RootFs) {
    let (dir, root) = openspec_project();
    write(&dir, &format!("openspec/changes/{NAME}/tasks.md"), "- [x] Add the button\n");
    (dir, root)
}

fn instructions_calls(runner: &FakeRunner) -> usize {
    runner.calls_of("instructions")
}

// OSM-FR-QLRW, OSM-FR-VALR: one `instructions apply` process with the exact
// arguments, in the working copy of the location, returns the state, the
// counts, the tasks in the CLI's order, and an empty list of missing
// artifacts where the CLI names none.
#[test]
fn the_apply_instructions_of_a_change_are_read_once() {
    let (_dir, root) = project_with_change();
    let (seam, runner, _) = seam_with(FakeRunner::default().json("instructions", READY));
    let read = seam.apply_instructions(&Silent, buffer(), Some(&root), &root, NAME).unwrap();
    assert_eq!(read.state, "ready");
    assert_eq!((read.total, read.complete), (3, 1));
    let tasks: Vec<_> = read.tasks.iter().map(|t| (t.description.as_str(), t.done)).collect();
    assert_eq!(tasks, [("Add the button", true), ("Wire the call", false), ("Test it", false)]);
    assert!(read.missing_artifacts.is_empty());
    let calls: Vec<_> =
        runner.calls().into_iter().filter(|c| c.args[0] == "instructions").collect();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].args, ["instructions", "apply", "--change", NAME, "--json"]);
    assert_eq!(calls[0].cwd, root.path());
}

// OSM-FR-PEAK: the progress names the first task that is not done, or none
// where every task is done, with the missing artifacts.
#[test]
fn the_progress_names_the_next_task() {
    let read = parse_apply_instructions(READY.as_bytes()).unwrap();
    let progress = read.progress();
    assert_eq!(progress.next_task.as_deref(), Some("Wire the call"));
    assert_eq!((progress.total, progress.complete), (3, 1));
    let wire = serde_json::to_value(&progress).unwrap();
    assert_eq!(wire["nextTask"], "Wire the call");
    assert_eq!(wire["missingArtifacts"], serde_json::json!([]));

    let done = parse_apply_instructions(
        br#"{ "state": "all_done", "progress": { "total": 1, "complete": 1 },
              "tasks": [ { "description": "x", "done": true } ] }"#,
    )
    .unwrap();
    assert_eq!(done.progress().next_task, None);

    let blocked = parse_apply_instructions(
        br#"{ "state": "blocked", "progress": { "total": 0, "complete": 0 }, "tasks": [],
              "missingArtifacts": ["design", "tasks"] }"#,
    )
    .unwrap();
    assert_eq!(blocked.progress().missing_artifacts, ["design", "tasks"]);
}

// OSM-FR-WIQF, OSM-FR-WWWK: a state outside the three, a count that is not an
// integer of 0 or more, and output that is not JSON fail with
// `cli_output_unreadable`.
#[test]
fn an_unknown_state_or_a_bad_count_is_unreadable() {
    for bad in [
        r#"{ "state": "waiting", "progress": { "total": 1, "complete": 0 }, "tasks": [] }"#,
        r#"{ "state": "ready", "progress": { "total": -1, "complete": 0 }, "tasks": [] }"#,
        r#"{ "state": "ready", "progress": { "total": 1.5, "complete": 0 }, "tasks": [] }"#,
        r#"{ "state": "ready", "tasks": [] }"#,
        "not json",
    ] {
        assert_eq!(
            parse_apply_instructions(bad.as_bytes()).unwrap_err(),
            OpenSpecReadError::CliOutputUnreadable,
            "{bad}"
        );
    }
}

// OSM-FR-ODHE, OSM-FR-XSOC: an invalid name and a missing change folder each
// fail without an `instructions` process; so does the mode off.
#[test]
fn refusals_of_the_apply_read_start_no_process() {
    let (_dir, root) = project_with_change();
    let (seam, runner, _) = seam_with(FakeRunner::default().json("instructions", READY));
    assert_eq!(
        seam.apply_instructions(&Silent, buffer(), Some(&root), &root, "Bad Name").unwrap_err(),
        OpenSpecReadError::InvalidChangeName
    );
    assert_eq!(
        seam.apply_instructions(&Silent, buffer(), Some(&root), &root, "absent").unwrap_err(),
        OpenSpecReadError::ChangeNotFound
    );
    assert_eq!(runner.calls().len(), 0, "not even the version check runs");

    let (_plain, plain) = plain_project();
    assert_eq!(
        seam.apply_instructions(&Silent, buffer(), Some(&plain), &root, NAME).unwrap_err(),
        OpenSpecReadError::OpenSpecModeInactive
    );
    assert_eq!(instructions_calls(&runner), 0);
}

// OSM-FR-WWWK, OSM-FR-IFWM: an exit status other than zero fails the read
// with `cli_failed`, and the failure is a `WARN` record with its code alone.
#[test]
fn a_failing_apply_read_is_cli_failed_and_logged_by_code() {
    let (_dir, root) = project_with_change();
    let (seam, _, _) = seam_with(FakeRunner::default().answering(
        "instructions",
        Ok(ProcessOutput { exit_status: Some(1), stdout: vec![], stderr: Vec::new() }),
    ));
    let log = buffer();
    assert_eq!(
        seam.apply_instructions(&Silent, log, Some(&root), &root, NAME).unwrap_err(),
        OpenSpecReadError::CliFailed { exit_status: Some(1), diagnostics: Vec::new() }
    );
    let warned = records(log)
        .into_iter()
        .find(|r| r.message == "openspec read failed")
        .expect("a warn record");
    assert_eq!(warned.level, LogLevel::Warn);
    assert_eq!(warned.fields.get("code"), Some(&serde_json::json!("cli_failed")));
    assert_eq!(warned.fields.get("read"), Some(&serde_json::json!("apply_instructions")));
}
