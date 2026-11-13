//! An archived change, and the one write the application runs
//! (OSM-FR-KWWM, OSM-FR-GMKV, OSM-FR-VALR, OSM-FR-XKQB; OAR-FR-IAKL,
//! OAR-FR-VAGH, OAR-FR-XTLB, OAR-FR-EUXZ, OAR-FR-NEEI).

use super::*;
use crate::openspec::archive_cli::parse_archive;
use crate::openspec::change::{archive_folder_of, ChangeLocation};

const NAME: &str = "add-logout";

fn archive_json(change: &str, archived_as: &str) -> String {
    format!(
        r#"{{ "archive": {{ "change": "{change}", "archivedAs": "{archived_as}", "specsUpdated": true }},
             "status": [] }}"#
    )
}

const REFUSED: &str = r#"{ "archive": null,
  "status": [ { "code": "change_has_incomplete_tasks", "message": "3 tasks are not done", "fix": "Finish them" },
              { "code": "second", "message": "x" } ] }"#;

// OSM-FR-KWWM, OSM-FR-GMKV: a change folder that moved below
// `openspec/changes/archive/` reads as `change_archived` with the newest
// archive folder by its date, in the active worktree and in a stream's
// working copy alike, and no process starts.
#[test]
fn an_archived_change_reads_as_archived_with_its_newest_folder() {
    let (seam, runner, _) = seam_with(FakeRunner::default());
    let (dir, root) = openspec_project();
    write(&dir, &format!("openspec/changes/archive/2026-01-02-{NAME}/proposal.md"), "x");
    write(&dir, &format!("openspec/changes/archive/2026-11-10-{NAME}/proposal.md"), "x");
    write(&dir, &format!("openspec/changes/archive/2026-12-01-{NAME}-two/proposal.md"), "x");
    write(&dir, "openspec/changes/archive/2027-01-01-other/proposal.md", "x");
    let before = runner.calls().len();
    let stream = ChangeLocation::Stream { stream_id: "w1".into() };
    for location in [ChangeLocation::ActiveWorktree, stream] {
        let read = seam.read_change(&Silent, buffer(), Some(&root), &root, location.clone(), NAME);
        let error = read.unwrap_err();
        assert_eq!(
            error,
            OpenSpecReadError::ChangeArchived { archived_as: format!("2026-11-10-{NAME}") },
            "{location:?}"
        );
        assert_eq!(error.code(), "change_archived");
        assert_eq!(error.to_failure().detail, Some(format!("2026-11-10-{NAME}")));
    }
    assert_eq!(runner.calls().len(), before, "no process started");
}

// OSM-FR-KWWM, OSM-FR-BCJO: a change that is neither a folder nor archived
// is not found; a file named like an archive folder is no archive folder.
#[test]
fn a_change_with_no_archive_folder_is_not_found() {
    let (seam, _, _) = seam_with(FakeRunner::default());
    let (dir, root) = openspec_project();
    write(&dir, &format!("openspec/changes/archive/2026-11-10-{NAME}"), "a file");
    write(&dir, &format!("openspec/changes/archive/26-11-10-{NAME}/x.md"), "x");
    let error = seam
        .read_change(&Silent, buffer(), Some(&root), &root, ChangeLocation::ActiveWorktree, NAME)
        .unwrap_err();
    assert_eq!(error, OpenSpecReadError::ChangeNotFound);
    assert_eq!(error.to_failure().code, "change_not_found");
}

// OAR-FR-XTLB, OSM-FR-KWWM: an archive folder is one path segment of the
// form `<YYYY-MM-DD>-<change>`.
#[test]
fn an_archive_folder_is_a_date_and_the_change_name() {
    assert!(archive_folder_of("2026-11-11-add-logout", "add-logout"));
    assert!(!archive_folder_of("2026-11-11-add-logout-2", "add-logout"));
    assert!(!archive_folder_of("2026-11-11add-logout", "add-logout"));
    assert!(!archive_folder_of("2026-1-11-add-logout", "add-logout"));
    assert!(!archive_folder_of("abcd-11-11-add-logout", "add-logout"));
    assert!(!archive_folder_of("x/2026-11-11-add-logout", "add-logout"));
    assert!(!archive_folder_of("add-logout", "add-logout"));
}

// OAR-FR-VAGH, OAR-FR-XTLB: an archive succeeds on exit status zero with an
// `archive` that names the change and a valid `archivedAs`.
#[test]
fn an_archive_succeeds_with_a_valid_archived_as() {
    let answer = parse_archive(NAME, 0, archive_json(NAME, "2026-11-11-add-logout").as_bytes());
    assert_eq!(answer.unwrap().archived_as, "2026-11-11-add-logout");
}

// OAR-FR-XTLB, OAR-FR-EUXZ: an `archivedAs` that is not one valid segment,
// and an `archive` of another change, do not succeed, with the code
// `cli_output_unreadable`.
#[test]
fn an_invalid_archived_as_is_unreadable_output() {
    for archived_as in [
        "../../escape",
        "2026-11-11-add-logout/x",
        "add-logout",
        "2026-11-11-other",
        "2026-11-11-add-logout-2",
    ] {
        let failure = parse_archive(NAME, 0, archive_json(NAME, archived_as).as_bytes());
        assert_eq!(failure.unwrap_err().code(), "cli_output_unreadable", "{archived_as}");
    }
    let other = parse_archive(NAME, 0, archive_json("other", "2026-11-11-other").as_bytes());
    assert_eq!(other.unwrap_err().code(), "cli_output_unreadable");
}

// OAR-FR-EUXZ, OAR-FR-NEEI: a null `archive` gives the code of its first
// `status` entry alone; a status code that is not an identifier is
// unreadable output; exit 1 with CLI JSON and no status code is
// `cli_rejected`, exit 1 with other output `cli_failed`, and exit 0 with no
// status code `cli_output_unreadable`.
#[test]
fn a_refused_archive_gives_the_code_of_its_first_status_entry() {
    assert_eq!(
        parse_archive(NAME, 1, REFUSED.as_bytes()).unwrap_err().code(),
        "change_has_incomplete_tasks"
    );
    assert_eq!(
        parse_archive(NAME, 0, REFUSED.as_bytes()).unwrap_err().code(),
        "change_has_incomplete_tasks"
    );
    let prose = r#"{ "archive": null, "status": [ { "code": "a sentence with spaces" } ] }"#;
    assert_eq!(
        parse_archive(NAME, 1, prose.as_bytes()).unwrap_err().code(),
        "cli_output_unreadable"
    );
    let empty = r#"{ "archive": null, "status": [] }"#;
    assert_eq!(parse_archive(NAME, 1, empty.as_bytes()).unwrap_err().code(), "cli_rejected");
    assert_eq!(
        parse_archive(NAME, 0, empty.as_bytes()).unwrap_err().code(),
        "cli_output_unreadable"
    );
    assert_eq!(parse_archive(NAME, 1, b"Error: boom").unwrap_err().code(), "cli_failed");
    assert_eq!(parse_archive(NAME, 0, b"not json").unwrap_err().code(), "cli_output_unreadable");
    let late = archive_json(NAME, "2026-11-11-add-logout");
    assert_eq!(parse_archive(NAME, 1, late.as_bytes()).unwrap_err().code(), "cli_rejected");
    // OAR-FR-NEEI: 64 characters are an identifier, and 65 are not.
    for (length, expected) in [(64, "a".repeat(64)), (65, "cli_output_unreadable".into())] {
        let long = format!(
            r#"{{ "archive": null, "status": [ {{ "code": "{}" }} ] }}"#,
            "a".repeat(length)
        );
        assert_eq!(parse_archive(NAME, 1, long.as_bytes()).unwrap_err().code(), expected);
    }
    // A first entry with no code gives no code, so the exit status decides,
    // and a code of a later entry is not read.
    let first = r#"{ "archive": null, "status": [ { "message": "m" }, { "code": "second" } ] }"#;
    let rejected = parse_archive(NAME, 1, first.as_bytes()).unwrap_err();
    assert_eq!(rejected.code(), "cli_rejected");
    // OFP-FR-NJSO: every status entry is a diagnostic, in order.
    assert_eq!(rejected.diagnostics().len(), 2);
    assert_eq!(rejected.diagnostics()[0].message, "m");
    assert_eq!(
        parse_archive(NAME, 0, first.as_bytes()).unwrap_err().code(),
        "cli_output_unreadable"
    );
}

// OAR-FR-IAKL, OSM-FR-XKQB, OSM-FR-ZIMO, OSM-FR-IFWM: the archive runs
// `archive <change> --json --yes` with the resolved CLI in the root of the
// working copy, inside the time limit, and its records name no argument and
// no output. The variables of the process are checked with the real runner
// in `runner.rs`.
#[test]
fn the_archive_runs_on_the_process_terms_of_the_seam() {
    let runner =
        FakeRunner::default().json("archive", &archive_json(NAME, "2026-11-11-add-logout"));
    let (seam, runner, _) = seam_with(runner);
    let (_dir, root) = openspec_project();
    let buffer = buffer();
    let answer = seam.archive_change(&Silent, buffer, Path::new(CLI), root.path(), NAME).unwrap();
    assert_eq!(answer.archived_as, "2026-11-11-add-logout");
    let call = runner.calls().into_iter().find(|c| c.args[0] == "archive").expect("a call");
    assert_eq!(call.args, ["archive", NAME, "--json", "--yes"]);
    assert_eq!(call.executable, PathBuf::from(CLI));
    assert_eq!(call.cwd, root.path());
    assert_eq!(call.timeout, crate::openspec::cli::CLI_TIMEOUT);
    let records = records(buffer);
    assert!(!records.is_empty(), "the process is told");
    for record in records {
        let text = serde_json::to_string(&record).unwrap();
        assert!(!text.contains(NAME), "no argument in a record: {text}");
        assert!(!text.contains("specsUpdated"), "no output in a record: {text}");
    }
}

// OAR-FR-EUXZ: a process that times out is `cli_timed_out`, and one that
// exits with a status other than zero or one is `cli_failed`.
#[test]
fn an_archive_process_that_fails_gives_the_code_of_the_read() {
    let (seam, runner, _) =
        seam_with(FakeRunner::default().answering("archive", Err(ProcessError::timed_out())));
    let (_dir, root) = openspec_project();
    let at = root.path();
    let timed_out = seam.archive_change(&Silent, buffer(), Path::new(CLI), at, NAME);
    assert_eq!(timed_out.unwrap_err().code(), "cli_timed_out");
    runner.set(
        "archive",
        Ok(ProcessOutput { exit_status: Some(3), stdout: REFUSED.into(), stderr: Vec::new() }),
    );
    let failed = seam.archive_change(&Silent, buffer(), Path::new(CLI), at, NAME);
    assert_eq!(failed.unwrap_err().code(), "cli_failed");
    let invalid = seam.archive_change(&Silent, buffer(), Path::new(CLI), at, "Not A Name");
    assert_eq!(invalid.unwrap_err().code(), "cli_failed");
    assert_eq!(runner.calls_of("archive"), 2, "an invalid name starts no process");
}
