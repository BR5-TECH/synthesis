//! The validation of a whole project, and the changes of a location
//! (OSM-FR-BXIF, OSM-FR-DXHP, OSM-FR-OPZF, OSM-FR-DDHL).

use super::*;
use crate::openspec::project_validation::parse_project_validation;

const ALL: &str = r#"{ "items": [
  { "id": "add-logout", "type": "change", "valid": true, "issues": [], "durationMs": 3 },
  { "id": "auth", "type": "spec", "valid": false,
    "issues": [ { "level": "ERROR", "path": "requirements[1].scenarios", "message": "No scenario" } ] }
], "summary": { "totals": { "items": 2 } }, "version": "1.0" }"#;

// OSM-FR-BXIF: the scope `all` runs `validate --all --strict --json
// --no-interactive`, and the answer holds each item with its id, kind,
// validity, and issues, and whether every item is valid.
#[test]
fn a_project_validation_of_all_reads_each_item() {
    let runner = FakeRunner::default().answering(
        "validate",
        Ok(ProcessOutput { exit_status: Some(1), stdout: ALL.into(), stderr: Vec::new() }),
    );
    let (seam, runner, _) = seam_with(runner);
    let (_dir, root) = openspec_project();
    let validation =
        seam.validate_project(&Silent, buffer(), Some(&root), &root, ValidationScope::All).unwrap();
    assert!(!validation.valid);
    assert_eq!(validation.items.len(), 2);
    assert_eq!(validation.items[0].id, "add-logout");
    assert_eq!(validation.items[0].kind, "change");
    assert!(validation.items[0].valid);
    assert_eq!(validation.items[1].kind, "spec");
    assert_eq!(validation.items[1].issues[0].path, "requirements[1].scenarios");
    assert_eq!(validation.first_invalid_spec().map(|i| i.id.as_str()), Some("auth"));
    let call = runner.calls().into_iter().find(|c| c.args[0] == "validate").unwrap();
    assert_eq!(call.args, ["validate", "--all", "--strict", "--json", "--no-interactive"]);
    assert_eq!(call.cwd, root.path());
}

// OSM-FR-BXIF, OSM-FR-DXHP: the scope `specs` runs `validate --specs`, and an
// answer that holds no item is valid.
#[test]
fn a_project_validation_of_specs_with_no_item_is_valid() {
    let runner = FakeRunner::default().json("validate", r#"{ "items": [] }"#);
    let (seam, runner, _) = seam_with(runner);
    let (_dir, root) = openspec_project();
    let validation = seam
        .validate_project(&Silent, buffer(), Some(&root), &root, ValidationScope::Specs)
        .unwrap();
    assert_eq!(validation, ProjectValidation { valid: true, items: vec![] });
    let call = runner.calls().into_iter().find(|c| c.args[0] == "validate").unwrap();
    assert_eq!(call.args, ["validate", "--specs", "--strict", "--json", "--no-interactive"]);
}

// OSM-FR-DXHP, OSM-FR-WWWK: another exit status fails with `cli_failed`, and
// output of another shape, an item of another type included, is unreadable.
#[test]
fn a_project_validation_fails_on_other_statuses_and_shapes() {
    let runner = FakeRunner::default().answering(
        "validate",
        Ok(ProcessOutput { exit_status: Some(2), stdout: ALL.into(), stderr: Vec::new() }),
    );
    let (seam, runner, _) = seam_with(runner);
    let (_dir, root) = openspec_project();
    let scope = ValidationScope::All;
    assert_eq!(
        seam.validate_project(&Silent, buffer(), Some(&root), &root, scope),
        Err(OpenSpecReadError::CliFailed { exit_status: Some(2), diagnostics: Vec::new() })
    );
    runner.set(
        "validate",
        Ok(ok(r#"{ "items": [ { "id": "x", "type": "other", "valid": true } ] }"#)),
    );
    assert_eq!(
        seam.validate_project(&Silent, buffer(), Some(&root), &root, scope),
        Err(OpenSpecReadError::CliOutputUnreadable)
    );
    assert_eq!(parse_project_validation(b"{}"), Err(OpenSpecReadError::CliOutputUnreadable));
}

// OSM-FR-XSOC, OSM-FR-DDHL: a project validation while the mode is inactive
// on the active worktree starts no process.
#[test]
fn a_project_validation_outside_the_mode_starts_no_process() {
    let (seam, runner, _) = seam_with(FakeRunner::default());
    let (_dir, plain) = plain_project();
    let (_other, at) = openspec_project();
    assert_eq!(
        seam.validate_project(&Silent, buffer(), Some(&plain), &at, ValidationScope::All),
        Err(OpenSpecReadError::OpenSpecModeInactive)
    );
    assert!(runner.calls().is_empty());
}

const LIST: &str = r#"{ "changes": [
  { "name": "add-logout", "completedTasks": 0, "totalTasks": 2, "lastModified": "2026-11-10T10:00:00.000Z" }
] }"#;

// OSM-FR-OPZF, OSM-FR-FCMY, OSM-FR-DDHL: `list_changes(location)` runs its two
// processes in the working copy of the location and reads the delta folders
// there, while the mode is decided on the active worktree.
#[test]
fn the_changes_of_a_location_are_read_in_its_working_copy() {
    let runner = FakeRunner::default().json("list", LIST).json("status", r#"{ "changes": [] }"#);
    let (seam, runner, _) = seam_with(runner);
    let (_active_dir, active) = openspec_project();
    let (stream_dir, stream) = plain_project();
    write(&stream_dir, "openspec/changes/add-logout/specs/ui/theme/spec.md", "x");
    let changes = seam.list_changes_at(&Silent, buffer(), Some(&active), &stream).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].capabilities, ["ui/theme"]);
    let cwds: Vec<PathBuf> =
        runner.calls().into_iter().filter(|c| c.args[0] != "--version").map(|c| c.cwd).collect();
    assert_eq!(cwds, [stream.path().to_path_buf(), stream.path().to_path_buf()]);
    let (_plain_dir, plain) = plain_project();
    assert_eq!(
        seam.list_changes_at(&Silent, buffer(), Some(&plain), &stream),
        Err(OpenSpecReadError::OpenSpecModeInactive)
    );
}
