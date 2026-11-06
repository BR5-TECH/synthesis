//! Each write command runs the main branch write check (GTC-FR-EMLM,
//! DRS-FR-TMZX, PST-FR-ZHVE, WKS-FR-WUUH, GRD-FR-WJLO).
//!
//! The behaviour of the check is tested in `write_guard.rs`, and the refusal of
//! one command of each family in the tests of that family. This file reads the
//! source of every write command and asserts that it calls the check, so a new
//! route that skips it fails here.

/// The body of `fn name` in `source`: from its signature to the next line that
/// is a lone closing brace.
fn body_of<'a>(source: &'a str, name: &str) -> &'a str {
    let start = [format!("pub fn {name}"), format!("pub async fn {name}"), format!("fn {name}")]
        .iter()
        .find_map(|signature| {
            source.find(&format!("{signature}(")).or_else(|| source.find(&format!("{signature}<")))
        })
        .unwrap_or_else(|| panic!("no fn {name}"));
    let rest = &source[start..];
    let end = rest.find("\n}\n").unwrap_or(rest.len());
    &rest[..end]
}

/// The check comes before `write`, the first call of the body that writes.
fn assert_checked_before(source: &str, name: &str, call: &str, write: &str) {
    let body = body_of(source, name);
    let checked = body.find(call).unwrap_or_else(|| panic!("{name} does not call {call}"));
    let written = body.find(write).unwrap_or_else(|| panic!("{name} does not call {write}"));
    assert!(checked < written, "{name} calls {write} before {call}");
}

// GTC-FR-EMLM: the commit, the two pushes, and the pull reconcile run the
// check; the fast-forward pull does not.
#[test]
fn the_git_writes_run_the_check() {
    let git = include_str!("../../git.rs");
    let active = "require_active_branch_write";
    assert_checked_before(git, "commit_paths", active, "commit_paths_reported(");
    assert_checked_before(git, "push_current_branch", active, "push_branch_at(");
    let named = "require_branch_write(&app, &root, Some(&branch)";
    assert_checked_before(git, "push_branch", named, "push_named_branch_at(");
    let pull = include_str!("../pull.rs");
    let project = "require_project_branch_write";
    assert_checked_before(pull, "reconcile_pull", project, "reconcile_pull_blocking(");
    assert!(!body_of(pull, "pull_current_branch").contains("write_guard"));
    let fetch = include_str!("../fetch.rs");
    assert!(!fetch.contains("write_guard"), "a fetch runs no check");
}

// DRS-FR-TMZX: every draft command that writes into the active worktree runs
// the check.
#[test]
fn every_draft_write_runs_the_check() {
    let drafts = include_str!("../../drafts/commands.rs");
    for name in [
        "create_draft",
        "create_drafts_folder",
        "rename_drafts_folder",
        "delete_drafts_folder",
        "move_draft_to_folder",
        "move_drafts_folder",
        "rename_draft",
        "set_draft_status",
        "delete_draft",
        "save_draft_file_contents",
    ] {
        let call = format!("require_draft_write(&app, &root, \"{name}\")");
        assert_checked_before(drafts, name, &call, "_impl(");
    }
}

// PST-FR-ZHVE, WKS-FR-WUUH, GRD-FR-WJLO: the artifact save, the stream merge,
// and the three graduation starts run the check.
#[test]
fn the_save_the_merge_and_the_starts_run_the_check() {
    let artifacts = include_str!("../../artifacts.rs");
    let save = "save_validated_artifact(";
    let active = "require_active_branch_write";
    assert_checked_before(artifacts, "save_artifact_contents_blocking", active, save);
    let merge = include_str!("../../streams/merge_handoff.rs");
    let base = "Some(&stream.base_branch)";
    assert_checked_before(merge, "merge_work_stream_blocking", base, "begin_repository_update");
    let commands = include_str!("../../graduation/commands.rs");
    assert_checked_before(commands, "start_graduation", "require_start_write", "save_run(");
    assert_checked_before(commands, "restart_graduation_run", "require_restart_write", "save_run(");
    let direct = include_str!("../../graduation/direct.rs");
    assert_checked_before(direct, "start_direct_graduation", "require_start_write", "save_run(");
}

/// Whether `source` declares `name` as a command that does not run on the main
/// thread: an `async` command, or a sync one that asks for a thread of its own.
fn off_main_thread(source: &str, name: &str) -> bool {
    let generic = [format!("pub async fn {name}<"), format!("pub async fn {name}(")];
    let asked = format!("#[tauri::command(async)]\npub fn {name}<");
    generic.iter().any(|signature| source.contains(&format!("#[tauri::command]\n{signature}")))
        || source.contains(&asked)
}

// GTC-FR-JWOL, GRD-FR-WJLO: the check can read GitHub, so no command that runs
// it runs on the main thread.
#[test]
fn no_command_that_runs_the_check_runs_on_the_main_thread() {
    let git = include_str!("../../git.rs");
    for name in ["commit_paths", "push_current_branch", "push_branch"] {
        assert!(off_main_thread(git, name), "{name} runs on the main thread");
    }
    assert!(off_main_thread(include_str!("../pull.rs"), "reconcile_pull"));
    assert!(off_main_thread(include_str!("../../artifacts.rs"), "save_artifact_contents"));
    let drafts = include_str!("../../drafts/commands.rs");
    for name in [
        "create_draft",
        "create_drafts_folder",
        "rename_drafts_folder",
        "delete_drafts_folder",
        "move_draft_to_folder",
        "move_drafts_folder",
        "rename_draft",
        "set_draft_status",
        "delete_draft",
        "save_draft_file_contents",
    ] {
        assert!(off_main_thread(drafts, name), "{name} runs on the main thread");
    }
    let commands = include_str!("../../graduation/commands.rs");
    assert!(off_main_thread(commands, "start_graduation"));
    assert!(off_main_thread(commands, "restart_graduation_run"));
    assert!(off_main_thread(include_str!("../../graduation/direct.rs"), "start_direct_graduation"));
}

// DRS-FR-MIYJ: the production committer of the draft events checks each commit
// against the session the application installed, and the application installs
// the session of the guard it manages, so a confirmation of the author reaches
// the background writes.
#[test]
fn the_draft_event_committer_reads_the_installed_session() {
    let commit = include_str!("../../storage_floor/commit.rs");
    let run = body_of(commit, "run");
    assert!(
        run.contains(
            "commit_now(&job, last_reason, crate::git::write_guard::installed().as_deref())"
        ),
        "the committer thread does not pass the installed session"
    );
    let lib = include_str!("../../lib.rs");
    let guard = body_of(lib, "main_branch_write_guard");
    assert!(guard.contains("git::write_guard::install(state.session());"));
    assert!(lib.contains(".manage(main_branch_write_guard())"), "the installed guard is managed");
}
