//! PST-FR-14 — the draft-asset housekeeping of PST-FR-30, and
//! the one write PST-FR-14 was amended to permit.

/// This file's production half — everything above the test modules, which
/// name the same functions and would otherwise be read as call sites.
fn production_half(file: &str) -> &str {
    &file[..file
        .find("mod asset_housekeeping_tests")
        .expect("this module")]
}

/// PST-FR-30 / DAS-FR-19: opening a project and closing one each schedule a
/// pass, and the close-triggered one is scheduled **before** the unmount
/// releases the project's storage context.
///
/// A source-level assertion because that is exactly what the requirement
/// is: an ordering between two calls on one path. A behavioural test could
/// only observe that a pass eventually ran, which is the part the async
/// contract deliberately leaves unpinned (DAS-FR-15) — the orderable fact
/// is that the schedule precedes the teardown, and it is orderable here.
#[test]
fn pst_ts34_a_close_schedules_housekeeping_before_it_unmounts() {
    const FILE: &str = include_str!("../../project.rs");
    let source = production_half(FILE);
    let close = source
        .find("pub fn close_project")
        .expect("the close command");
    let body = &source[close..];
    let scheduled = body
        .find("sweep_project_draft_assets")
        .expect("PST-FR-30: a close schedules the pass");
    let unmounted = body
        .find("deactivate_project(")
        .expect("the unmount");
    assert!(
        scheduled < unmounted,
        "PST-FR-30: the pass is scheduled before the unmount releases the \
         project's storage context, so it addresses drafts that still resolve",
    );

    // PST-FR-30 / DAS-FR-19: and an open schedules one too, together with
    // the application-start pass that runs once after the active project
    // has been loaded.
    let activate = source
        .find("fn activate_project")
        .expect("the activation");
    let after = &source[activate..close.max(activate)];
    assert!(
        after.contains("sweep_project_draft_assets"),
        "PST-FR-30: opening a project schedules the pass",
    );
    assert!(
        after.contains("sweep_at_application_start"),
        "DAS-FR-19: and the application's own start pass, once",
    );
}

/// PST-FR-CDYM / DRS-FR-ISPI: opening a project runs `ensure_gitignored`
/// against the active worktree's `.synthesis/` folder, and **changing the
/// active worktree does not**, a remount modifying no file of the project
/// (PST-FR-14).
///
/// A source-level assertion because the requirement is about which lifecycle
/// step performs the call: what the call itself does to the ignore file, and
/// that it reads and writes nothing else, is established where the primitive
/// lives (FSA-FR-08).
#[test]
fn pst_ts_cdym_an_open_migrates_the_ignore_file_and_a_remount_does_not() {
    const FILE: &str = include_str!("../../project.rs");
    let source = production_half(FILE);
    let remount = source
        .find("fn remount_content_root")
        .expect("the remount");
    let activate = source
        .find("fn activate_project")
        .expect("the activation");
    assert!(
        source[activate..].contains("ensure_gitignored"),
        "PST-FR-CDYM: opening a project brings the ignore file up to date, \
         which is what removes an existing project's `drafts/` entry",
    );
    assert!(
        !source[remount..activate].contains("ensure_gitignored"),
        "PST-FR-14: and a remount writes no file of the project, so changing \
         the active worktree does not perform it",
    );
    assert!(
        source[activate..].contains("drafts_are_ignored"),
        "DRS-FR-04: and an ignore rule this module does not own, still \
         covering the drafts root, is reported rather than left silent",
    );
}

/// DRS-FR-04: an ignore rule the author wrote, still covering the drafts
/// root, is what the open reports — and a project Git ignores nothing in
/// reports nothing.
#[test]
fn drs_ts_ispi_a_drafts_root_git_still_ignores_is_detected() {
    let dir = tempfile::TempDir::new().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    std::fs::create_dir_all(dir.path().join(".synthesis/drafts")).unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());
    assert!(
        !super::drafts_are_ignored(&root),
        "a repository with no rule against it ignores nothing",
    );

    // The author's own rule, in the project's own ignore file — which this
    // module neither reads nor rewrites.
    std::fs::write(dir.path().join(".gitignore"), ".synthesis/drafts/\n").unwrap();
    assert!(
        super::drafts_are_ignored(&root),
        "a rule anywhere Git reads one from is what decides it",
    );
    drop(repo);

    // A content root in no repository excludes nothing, and is not a state
    // to report.
    let plain = tempfile::TempDir::new().unwrap();
    assert!(!super::drafts_are_ignored(&crate::fs::RootFs::for_root(plain.path())));
}

/// PST-FR-14 / DAS-FR-21: a pass that fails leaves the open and the close
/// untouched, because neither waits on it and neither reads its answer.
#[test]
fn pst_ts34_neither_lifecycle_step_waits_on_the_pass_or_reads_its_answer() {
    const FILE: &str = include_str!("../../project.rs");
    // The production half alone: the tests below name the same function and
    // would otherwise be scanned as though they were call sites.
    let source = production_half(FILE);
    for call in source.match_indices("sweep_project_draft_assets") {
        let line_start = source[..call.0].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let line_end = source[call.0..]
            .find('\n')
            .map(|i| call.0 + i)
            .unwrap_or(source.len());
        let line = source[line_start..line_end].trim();
        // A statement, not an expression anything is bound from: the answer
        // is an acknowledgement and this path neither reads it nor waits on
        // it (DAS-FR-15).
        assert!(
            line.starts_with("crate::draft_assets::sweep_project_draft_assets("),
            "the pass is scheduled and nothing is taken from it: {line:?}",
        );
        assert!(
            line.ends_with(");"),
            "and nothing is bound from it: {line:?}",
        );
    }
}
