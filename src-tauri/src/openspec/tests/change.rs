//! One change at a location, and its validation (OSM-FR-MFVN, OSM-FR-DDHL,
//! OSM-FR-IHUO, OSM-FR-BCJO, OSM-FR-LLIO, OSM-FR-NMHS, OSM-FR-PYHP,
//! OSM-FR-APVF, OSM-FR-XDRN), and the lifetime of a failed check
//! (OSM-FR-ZNVZ).

use super::*;
use crate::openspec::change::{valid_change_name, ChangeLocation};

const NAME: &str = "add-logout-3f2a";

const SPEC_AUTH: &str = r#"{ "id": "auth", "title": "Auth", "overview": "",
  "requirements": [
    { "name": "Session Expiration", "text": "Sessions SHALL expire after 60 minutes.", "scenarios": [] },
    { "name": "Remember Me", "text": "Users MAY stay signed in.", "scenarios": [] }
  ] }"#;

const SHOW_CHANGE: &str = r#"{ "id": "add-logout-3f2a", "title": "Add logout", "deltaCount": 5, "deltas": [
  { "spec": "auth", "operation": "MODIFIED", "description": "x",
    "requirement": { "name": "Session Expiration", "text": "Sessions SHALL expire after 30 minutes.",
      "scenarios": [ { "name": "Idle", "rawText": "- WHEN idle\n- THEN expire" } ] } },
  { "spec": "auth", "operation": "ADDED", "description": "x",
    "requirement": { "name": "Logout", "text": "Users SHALL log out.", "scenarios": [] } },
  { "spec": "auth", "operation": "REMOVED", "description": "x",
    "requirement": { "name": "Remember Me", "text": "", "scenarios": [] } },
  { "spec": "auth", "operation": "RENAMED", "description": "x",
    "rename": { "from": "Session Expiration", "to": "Session Timeout" } },
  { "spec": "billing", "operation": "ADDED", "description": "x",
    "requirement": { "name": "Invoices", "text": "The system SHALL send invoices.", "scenarios": [] } }
] }"#;

const LIST: &str = r#"{ "changes": [
  { "name": "add-logout-3f2a", "completedTasks": 1, "totalTasks": 4, "lastModified": "2026-11-10T10:00:00.000Z" }
] }"#;

/// The project with the change on disk, and the status output that names
/// its files by absolute path, as the CLI does.
fn change_project() -> (TempDir, RootFs, String) {
    let (dir, root) = openspec_project();
    write(&dir, "openspec/specs/auth/spec.md", "# Auth\n");
    write(&dir, &format!("openspec/changes/{NAME}/proposal.md"), "# Why\nLogout is missing.\n");
    write(
        &dir,
        &format!("openspec/changes/{NAME}/specs/auth/spec.md"),
        "## MODIFIED Requirements\n",
    );
    write(
        &dir,
        &format!("openspec/changes/{NAME}/specs/billing/spec.md"),
        "## ADDED Requirements\n",
    );
    let change_dir =
        crate::changes::canonicalize_lenient(&dir.path().join("openspec/changes").join(NAME));
    std::fs::write(change_dir.join("tasks.md"), [0xff_u8, 0xfe, 0x00]).unwrap();
    let path = |rel: &str| change_dir.join(rel).to_string_lossy().replace('\\', "\\\\");
    let status = format!(
        r#"{{ "changeName": "{NAME}", "schemaName": "spec-driven", "isComplete": false,
  "artifacts": [
    {{ "id": "proposal", "outputPath": "proposal.md", "status": "done", "requires": [] }},
    {{ "id": "specs", "outputPath": "specs/**/*.md", "status": "done", "requires": ["proposal"] }},
    {{ "id": "design", "outputPath": "design.md", "status": "ready", "requires": ["proposal"] }},
    {{ "id": "tasks", "outputPath": "tasks.md", "status": "done", "requires": ["specs"] }}
  ],
  "artifactPaths": {{
    "proposal": {{ "existingOutputPaths": ["{}"] }},
    "specs": {{ "existingOutputPaths": ["{}", "{}"] }},
    "design": {{ "existingOutputPaths": [] }},
    "tasks": {{ "existingOutputPaths": ["{}", "/elsewhere/outside.md"] }}
  }} }}"#,
        path("proposal.md"),
        path("specs/billing/spec.md"),
        path("specs/auth/spec.md"),
        path("tasks.md"),
    );
    (dir, root, status)
}

fn change_runner(status: &str) -> FakeRunner {
    FakeRunner::default()
        .answering_prefix(&["status", "--change"], Ok(ok(status)))
        .answering_prefix(&["show", NAME], Ok(ok(SHOW_CHANGE)))
        .answering_prefix(&["show", "auth"], Ok(ok(SPEC_AUTH)))
        .json("list", LIST)
}

// OSM-FR-IHUO: lowercase groups joined by one `-`, 1 to 64 characters, and
// not `archive`.
#[test]
fn a_change_name_is_lowercase_groups_joined_by_single_dashes() {
    for good in ["a", "add-logout", "add-logout-3f2a", "v2", &"a".repeat(64)] {
        assert!(valid_change_name(good), "{good:?}");
    }
    for bad in [
        "",
        "archive",
        "Add",
        "add_logout",
        "add--logout",
        "-add",
        "add-",
        "a b",
        "../x",
        &"a".repeat(65),
    ] {
        assert!(!valid_change_name(bad), "{bad:?}");
    }
}

// OSM-FR-BCJO, OSM-FR-LLIO: the artifacts in the build order of the schema,
// each with its change-relative files and their whole text; a file outside
// the change folder is left out, and a file that is not text is unreadable.
#[test]
fn a_change_is_read_with_its_artifacts_and_files() {
    let (_dir, root, status) = change_project();
    let (seam, runner, _) = seam_with(change_runner(&status));
    let detail = seam
        .read_change(&Silent, buffer(), Some(&root), &root, ChangeLocation::ActiveWorktree, NAME)
        .unwrap();
    assert_eq!(detail.name, NAME);
    assert_eq!(detail.schema, "spec-driven");
    assert_eq!((detail.completed_tasks, detail.total_tasks), (1, 4));
    let ids: Vec<_> = detail.artifacts.iter().map(|a| (a.id.as_str(), a.status.as_str())).collect();
    assert_eq!(
        ids,
        [("proposal", "done"), ("specs", "done"), ("design", "ready"), ("tasks", "done")]
    );
    let proposal = &detail.artifacts[0].files;
    assert_eq!(proposal.len(), 1);
    assert_eq!(proposal[0].path, "proposal.md");
    assert_eq!(proposal[0].text.as_deref(), Some("# Why\nLogout is missing.\n"));
    let specs: Vec<_> = detail.artifacts[1].files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(specs, ["specs/auth/spec.md", "specs/billing/spec.md"]);
    assert!(detail.artifacts[2].files.is_empty());
    let tasks = &detail.artifacts[3].files;
    assert_eq!(tasks.len(), 1, "a path outside the change folder is left out");
    assert!(tasks[0].unreadable);
    assert_eq!(tasks[0].text, None);
    // OSM-FR-BCJO, OSM-FR-VALR: the exact arguments of every read, and
    // nothing outside the read subcommands.
    let args: Vec<Vec<String>> = runner.calls().into_iter().map(|c| c.args).collect();
    let owned = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert!(args.contains(&owned(&["status", "--change", NAME, "--json"])), "{args:?}");
    assert!(
        args.contains(&owned(&[
            "show",
            NAME,
            "--type",
            "change",
            "--json",
            "--deltas-only",
            "--no-interactive"
        ])),
        "{args:?}"
    );
    assert!(
        args.contains(&owned(&["show", "auth", "--type", "spec", "--json", "--no-interactive"])),
        "{args:?}"
    );
    for call in &args {
        assert!(
            ["--version", "list", "show", "status", "validate"].contains(&call[0].as_str()),
            "{call:?}"
        );
    }
    // OSM-FR-BCJO: status and show of the change run once each.
    assert_eq!(
        runner
            .calls()
            .iter()
            .filter(|c| c.args.starts_with(&["status".to_string(), "--change".to_string()]))
            .count(),
        1
    );
    assert_eq!(
        runner
            .calls()
            .iter()
            .filter(|c| c.args.starts_with(&["show".to_string(), NAME.to_string()]))
            .count(),
        1
    );
}

// OSM-FR-NMHS, OSM-FR-PYHP: each delta with the live requirement of its name,
// the old name for a rename, none for a new requirement or a new capability,
// and one `show` for each capability the working copy holds.
#[test]
fn each_delta_carries_the_live_requirement_it_changes() {
    let (_dir, root, status) = change_project();
    let (seam, runner, _) = seam_with(change_runner(&status));
    let detail = seam
        .read_change(&Silent, buffer(), Some(&root), &root, ChangeLocation::ActiveWorktree, NAME)
        .unwrap();
    let d = &detail.deltas;
    assert_eq!(d.len(), 5);
    assert_eq!(d[0].operation, "MODIFIED");
    assert_eq!(d[0].live.as_ref().unwrap().text, "Sessions SHALL expire after 60 minutes.");
    assert_eq!(d[0].requirement.as_ref().unwrap().scenarios[0].text, "- WHEN idle\n- THEN expire");
    assert!(d[0].capability_exists);
    assert_eq!(d[1].operation, "ADDED");
    assert!(d[1].live.is_none(), "no live requirement has that name");
    assert_eq!(d[2].operation, "REMOVED");
    assert_eq!(d[2].live.as_ref().unwrap().name, "Remember Me");
    assert_eq!(d[3].operation, "RENAMED");
    assert!(d[3].requirement.is_none());
    assert_eq!(d[3].rename.as_ref().unwrap().to, "Session Timeout");
    assert_eq!(d[3].live.as_ref().unwrap().name, "Session Expiration");
    assert_eq!(d[4].capability, "billing");
    assert!(!d[4].capability_exists);
    assert!(d[4].live.is_none());
    assert_eq!(
        runner
            .calls()
            .iter()
            .filter(|c| c.args.starts_with(&["show".to_string(), "auth".to_string()]))
            .count(),
        1
    );
    assert!(!runner.calls().iter().any(|c| c.args.len() > 1 && c.args[1] == "billing"));
}

// OSM-FR-BCJO, OSM-FR-IHUO: a change with no folder is not found, and an
// invalid name is refused; neither starts a read process.
#[test]
fn a_missing_change_and_an_invalid_name_run_no_read() {
    let (_dir, root, status) = change_project();
    let (seam, runner, _) = seam_with(change_runner(&status));
    let missing = seam.read_change(
        &Silent,
        buffer(),
        Some(&root),
        &root,
        ChangeLocation::ActiveWorktree,
        "other",
    );
    assert_eq!(missing, Err(OpenSpecReadError::ChangeNotFound));
    let invalid = seam.read_change(
        &Silent,
        buffer(),
        Some(&root),
        &root,
        ChangeLocation::ActiveWorktree,
        "../etc",
    );
    assert_eq!(invalid, Err(OpenSpecReadError::InvalidChangeName));
    assert!(runner.calls().iter().all(|c| c.args[0] == "--version"));
}

// OSM-FR-DDHL, OSM-FR-HHUD: the mode is decided on the active worktree, and
// the CLI runs in the working copy of the location.
#[test]
fn the_mode_comes_from_the_active_worktree_and_the_cli_runs_in_the_location() {
    let (_dir, stream_copy, status) = change_project();
    let (_plain_dir, plain) = plain_project();
    let (seam, runner, _) = seam_with(change_runner(&status));
    let location = ChangeLocation::Stream { stream_id: "s1".into() };
    let inactive =
        seam.read_change(&Silent, buffer(), Some(&plain), &stream_copy, location.clone(), NAME);
    assert_eq!(inactive, Err(OpenSpecReadError::OpenSpecModeInactive));

    let (_active_dir, active) = openspec_project();
    let detail = seam
        .read_change(&Silent, buffer(), Some(&active), &stream_copy, location.clone(), NAME)
        .unwrap();
    assert_eq!(detail.location, location);
    for call in runner.calls() {
        assert_eq!(call.cwd, stream_copy.path(), "{:?}", call.args);
    }
}

// OSM-FR-MFVN: a location serialises as the contract names it, and the review
// checkout never reaches the wire.
#[test]
fn a_location_reads_and_writes_as_the_contract_names_it() {
    let stream: ChangeLocation =
        serde_json::from_str(r#"{ "kind": "stream", "streamId": "s1" }"#).unwrap();
    assert_eq!(stream, ChangeLocation::Stream { stream_id: "s1".into() });
    let active: ChangeLocation = serde_json::from_str(r#"{ "kind": "active_worktree" }"#).unwrap();
    assert_eq!(active, ChangeLocation::ActiveWorktree);
    assert!(serde_json::from_str::<ChangeLocation>(
        r#"{ "kind": "review_checkout", "root": "/x" }"#
    )
    .is_err());
    assert_eq!(
        serde_json::to_value(ChangeLocation::Stream { stream_id: "s1".into() }).unwrap(),
        serde_json::json!({ "kind": "stream", "streamId": "s1" })
    );
}

// OSM-FR-APVF, OSM-FR-XDRN, OSM-FR-VALR: `validate` runs strict and as JSON;
// exit 0 is valid, exit 1 with readable output is not valid with its issues,
// and any other status fails.
#[test]
fn a_validation_reads_exit_one_as_an_answer() {
    let (_dir, root, _) = change_project();
    let valid = r#"{ "items": [ { "id": "add-logout-3f2a", "type": "change", "valid": true, "issues": [] } ] }"#;
    let invalid = r#"{ "items": [ { "id": "add-logout-3f2a", "type": "change", "valid": false,
      "issues": [ { "level": "ERROR", "path": "specs/auth/spec.md", "message": "A requirement has no scenario" } ] } ] }"#;

    let (seam, runner, _) = seam_with(FakeRunner::default().json("validate", valid));
    let ok_result = seam.validate_change(&Silent, buffer(), Some(&root), &root, NAME).unwrap();
    assert!(ok_result.valid);
    let call = runner.calls().into_iter().find(|c| c.args[0] == "validate").unwrap();
    assert_eq!(
        call.args,
        ["validate", NAME, "--type", "change", "--strict", "--json", "--no-interactive"]
    );

    let (seam, _, _) = seam_with(FakeRunner::default().answering(
        "validate",
        Ok(ProcessOutput { exit_status: Some(1), stdout: invalid.as_bytes().to_vec() }),
    ));
    let not_valid = seam.validate_change(&Silent, buffer(), Some(&root), &root, NAME).unwrap();
    assert!(!not_valid.valid);
    assert_eq!(not_valid.issues[0].level, "ERROR");
    assert_eq!(not_valid.issues[0].message, "A requirement has no scenario");

    let (seam, _, _) = seam_with(FakeRunner::default().answering(
        "validate",
        Ok(ProcessOutput { exit_status: Some(2), stdout: valid.as_bytes().to_vec() }),
    ));
    assert_eq!(
        seam.validate_change(&Silent, buffer(), Some(&root), &root, NAME),
        Err(OpenSpecReadError::CliFailed { exit_status: 2 })
    );
    let (seam, _, _) = seam_with(FakeRunner::default().answering(
        "validate",
        Ok(ProcessOutput { exit_status: Some(1), stdout: b"oops".to_vec() }),
    ));
    assert_eq!(
        seam.validate_change(&Silent, buffer(), Some(&root), &root, NAME),
        Err(OpenSpecReadError::CliOutputUnreadable)
    );
}

// OSM-FR-ZNVZ: a failed check is kept for 30 seconds at most; after that the
// next read checks again, and a usable result stays kept.
#[test]
fn a_failed_check_is_kept_for_thirty_seconds_at_most() {
    assert_eq!(crate::openspec::cli::FAILED_CHECK_TTL, Duration::from_secs(30));
    let failing =
        || Arc::new(FakeRunner::default().answering("--version", Err(ProcessError::TimedOut)));

    let runner = failing();
    let kept =
        OpenSpec::new(runner.clone(), Arc::new(FakeProbe::default().with_executable(CLI)), env());
    kept.cli_state(&Silent, buffer());
    kept.cli_state(&Silent, buffer());
    assert_eq!(runner.calls_of("--version"), 1, "inside 30 seconds the failure stands");

    let runner = failing();
    let expired =
        OpenSpec::new(runner.clone(), Arc::new(FakeProbe::default().with_executable(CLI)), env())
            .with_failed_ttl(Duration::ZERO);
    expired.cli_state(&Silent, buffer());
    assert_eq!(expired.kept_cli_state(), None, "an expired failure is no kept check");
    expired.cli_state(&Silent, buffer());
    assert_eq!(runner.calls_of("--version"), 2);

    let (usable, runner, _) = seam_with(FakeRunner::default());
    let usable = usable.with_failed_ttl(Duration::ZERO);
    usable.cli_state(&Silent, buffer());
    usable.cli_state(&Silent, buffer());
    assert_eq!(runner.calls_of("--version"), 1, "only a failure expires");
}
