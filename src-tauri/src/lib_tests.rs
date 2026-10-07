//! The registration and naming guards for `lib.rs`.
//!
//! Split out of that file for the same reason `command_names.rs` was: they are
//! a third of its length and none of them is part of what it does. Mounted with
//! `#[path]` so the module is still `crate::tests` and every `super::` path in
//! it resolves exactly as it did in place.

use super::*;

/// Every event name the backend emits.
///
/// Tauri validates event names and accepts only alphanumerics, `-`, `/`,
/// `:` and `_`. A name it rejects fails at `emit` — which returns an error
/// rather than panicking, and every emit site discards it with `let _ =`
/// because a progress or watcher event must never take down the operation
/// it reports on. The result is a channel that is silently dead in
/// production while every test that stubs the sink or mocks `listen` stays
/// green. This list is what makes that catchable.
const EVENT_NAMES: &[&str] = &[
    progress::OPERATION_PROGRESS,
    changes::CHANGES_UPDATED,
    watcher::ARTIFACT_CHANGED_EXTERNALLY,
    watcher::PROJECT_TREE_CHANGED,
    worktree::WORKTREE_CONTEXT_CHANGED,
    worktree::BRANCHES_CHANGED,
    git::GIT_OUTPUT_LINE,
    git::GIT_OPERATION_FINISHED,
    search::SEARCH_RESULTS,
    search::SEARCH_ENDED,
    drafts::DRAFTS_CHANGED,
    draft_history::HISTORY_CHANGED,
    draft_proposals::PROPOSALS_CHANGED,
    prompt_proposals::PROMPT_PROPOSALS_CHANGED,
    agent_conversations::AGENT_TURN_STATE_CHANGED,
    comments::DISCUSSION_CHANGED,
    github_tokens::GITHUB_TOKENS_CHANGED,
    logging::LOG_RECORDS_APPENDED,
    notifications::NOTIFICATION_ACTIVATED,
    graduation::GRADUATION_QUEUE_CHANGED,
    streams::WORK_STREAMS_CHANGED,
    graduation::GRADUATION_RUN_CHANGED,
    draft_watcher::DRAFT_PROMPT_CHANGED,
    dashboard::DASHBOARD_RECENTLY_EDITED_CHANGED,
    dashboard::DASHBOARD_ACTIVE_DRAFTS_CHANGED,
    dashboard::DASHBOARD_AGENT_ACTIVITY_CHANGED,
    dashboard::DASHBOARD_PENDING_GIT_CHANGED,
    dashboard::DASHBOARD_REFRESH_FAILED,
];

#[test]
fn every_event_name_is_one_tauri_will_actually_deliver() {
    use tauri::Emitter;
    let app = tauri::test::mock_app();
    for name in EVENT_NAMES {
        assert!(
            app.emit(name, ()).is_ok(),
            "{name:?} is not a deliverable Tauri event name; the frontend \
             would simply never hear anything on this channel",
        );
    }
}

/// The body of this file, for the two registration guards below. Neither
/// `generate_handler!` nor the builder chain leaves anything inspectable at
/// runtime, and both failures are silent: an unregistered command rejects at
/// `invoke` time, an unregistered handler simply never runs.
const SOURCE: &str = include_str!("lib.rs");

#[test]
fn every_command_name_is_registered_in_generate_handler() {
    // COMMAND_NAMES is a hand-kept list; on its own it proves nothing, since
    // the test that reads it only compares it to another hand-kept list.
    // This reads the macro invocation itself. `finish_exit` is the one that
    // matters most: dropping it leaves the frontend unable to release a held
    // quit, and the application hangs with no window.
    let handler = SOURCE
        .split_once("generate_handler![")
        .expect("generate_handler! invocation")
        .1
        .split_once("])")
        .expect("end of generate_handler!")
        .0;
    for name in COMMAND_NAMES {
        assert!(
            handler.contains(name),
            "{name:?} is in COMMAND_NAMES but not in tauri::generate_handler!; \
             the frontend's invoke would fail at runtime",
        );
    }
}

#[test]
fn every_registered_command_is_named_in_command_names() {
    // The converse of the check above, and the half that was missing. The
    // other direction proves a listed name is registered; this one proves a
    // registered name is listed, so a command added to the macro alone cannot
    // stay invisible to every reader that works from COMMAND_NAMES.
    let handler = SOURCE
        .split_once("generate_handler![")
        .expect("generate_handler! invocation")
        .1
        .split_once("])")
        .expect("end of generate_handler!")
        .0;
    for line in handler.lines() {
        let entry = line.trim().trim_end_matches(',');
        // Skip comments, blank lines, and the module path's own segments.
        if entry.is_empty() || entry.starts_with("//") {
            continue;
        }
        let Some(name) = entry.rsplit("::").next() else {
            continue;
        };
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit()) {
            continue;
        }
        assert!(
            COMMAND_NAMES.contains(&name),
            "{name:?} is registered in tauri::generate_handler! but is not in \
             COMMAND_NAMES; every reader that works from that list would miss it",
        );
    }
}

#[test]
fn no_recently_edited_command_is_registered_anywhere() {
    // PSS-FR-13: nothing records an artifact as recently edited
    // and nothing serves an ordered list of ids for that purpose, so there
    // is no operation for a caller to invoke and no second opinion anywhere
    // about which artifacts are recent. This is the ABSENCE half of the
    // guard above: every other check here asserts a name IS registered, so
    // re-adding one of these tomorrow would turn nothing else red.
    let handler = SOURCE
        .split_once("generate_handler![")
        .expect("generate_handler! invocation")
        .1
        .split_once("])")
        .expect("end of generate_handler!")
        .0;
    for gone in ["record_recently_edited_artifact", "list_recently_edited_ids"] {
        assert!(
            !handler.contains(gone),
            "{gone:?} must not be a registered command: the Dashboard's \
             Recently edited widget is served from the project's artifacts \
             and their source files (PST-FR-31), not from an MRU",
        );
    }
}

#[test]
fn no_secret_reading_function_is_reachable_from_the_frontend() {
    // GTS-FR-12 / GTS-FR-13. `github_tokens::resolve_github_token_secret` is
    // the one function in the application that returns a token's secret, and
    // the claim that no frontend `invoke` can reach it rests entirely on its
    // ABSENCE here. Every other guard in this file checks the forward
    // direction — that a name IS registered — so adding `#[tauri::command]`
    // to that function tomorrow would turn nothing else red.
    let handler = SOURCE
        .split_once("generate_handler![")
        .expect("generate_handler! invocation")
        .1
        .split_once("])")
        .expect("end of generate_handler!")
        .0;
    assert!(
        !handler.contains("resolve_github_token_secret"),
        "resolve_github_token_secret must never be a registered command: it \
         returns a token secret, and registering it would put one within \
         reach of any frontend invoke",
    );
    assert!(!COMMAND_NAMES.contains(&"resolve_github_token_secret"));
}

#[test]
fn no_vault_operation_is_reachable_from_the_frontend() {
    // ASV-FR-29. The application secret vault registers no
    // Tauri command at all: every one of its operations returns secret
    // material, or an object that holds it, and the claim that no frontend
    // `invoke` can reach one rests entirely on their ABSENCE here.
    let handler = SOURCE
        .split_once("generate_handler![")
        .expect("generate_handler! invocation")
        .1
        .split_once("])")
        .expect("end of generate_handler!")
        .0;
    for operation in [
        "read_secret",
        "secret_presence",
        "apply_secret_mutations",
        "migrate_legacy_secrets",
    ] {
        assert!(
            !handler.contains(operation),
            "{operation} must never be a registered command: the vault's \
             operations return secrets, and registering one would put a \
             credential within reach of any frontend invoke",
        );
        assert!(!COMMAND_NAMES.contains(&operation));
    }
    assert!(
        !handler.contains("secret_vault"),
        "no command of the secret vault may be registered (ASV-FR-29)",
    );
}

#[test]
fn no_agent_invocation_path_is_reachable_from_the_frontend() {
    // AIC-FR-19 / AAP-FR-19. `agentic::resolve_agentic_invocation` hands
    // out a binary path or an endpoint-and-key to run an agent with, and
    // `ai_api::resolve_ai_api_call` hands out an endpoint and the key to
    // call it with. The claim that no frontend `invoke` can reach either
    // rests entirely on their ABSENCE here, exactly as it does for the
    // token secret above.
    let handler = SOURCE
        .split_once("generate_handler![")
        .expect("generate_handler! invocation")
        .1
        .split_once("])")
        .expect("end of generate_handler!")
        .0;
    assert!(
        !handler.contains("resolve_agentic_invocation"),
        "resolve_agentic_invocation must never be a registered command: it \
         resolves an agent backend to run, and registering it would put an \
         agent invocation within reach of any frontend invoke",
    );
    assert!(!COMMAND_NAMES.contains(&"resolve_agentic_invocation"));
    assert!(
        !handler.contains("resolve_ai_api_call"),
        "resolve_ai_api_call must never be a registered command: it returns \
         a provider API key, and registering it would put one within reach \
         of any frontend invoke",
    );
    assert!(!COMMAND_NAMES.contains(&"resolve_ai_api_call"));
}

#[test]
fn no_agent_conversation_internal_is_reachable_from_the_frontend() {
    // AAP-FR-16, AAP-FR-19 / AGR-FR-18 / CVL-FR-11. Four internals, each of which the
    // spec claims no frontend `invoke` can reach, and each claim resting
    // entirely on its ABSENCE here:
    //
    // - `resolve_ai_api_endpoint` hands out a base URL and a cleartext key
    //   for a provider the caller names (AAP-FR-34).
    // - `validate_ai_api_selection` is the rule an agent's stored selection
    //   is checked against, not something a frontend decides (AAP-FR-33).
    // - `resolve_project_agent` resolves a nickname against a project's
    //   enrolment; registering it would let the frontend enumerate agents a
    //   project never enrolled (AGR-FR-18).
    // - `build_input` reads the material under discussion and assembles
    //   what an agent is shown (AGC-FR-05).
    // - `append_as` / `append_as_scoped` / `append_agent_comment` are the
    //   comment log's writers, and they take a `Participant`. Registering
    //   any of them would let a frontend call attribute a comment to an
    //   agent, or to another human (CMS-FR-41 / CMS-FR-11).
    let handler = SOURCE
        .split_once("generate_handler![")
        .expect("generate_handler! invocation")
        .1
        .split_once("])")
        .expect("end of generate_handler!")
        .0;
    for internal in [
        "resolve_ai_api_endpoint",
        "validate_ai_api_selection",
        "resolve_project_agent",
        "build_input",
        "append_as",
        "append_as_scoped",
        "append_agent_comment",
        // CMS-FR-EKUP: a question set is created by an agent tool and by
        // nothing the frontend can reach.
        "reserve_discussion_question_set",
    ] {
        assert!(
            !handler.contains(internal),
            "{internal} must never be a registered command",
        );
        assert!(!COMMAND_NAMES.contains(&internal));
    }
}

#[test]
fn the_quit_interceptions_are_registered_on_the_builder() {
    // SNV-FR-26 / EDT-FR-33. Without the window-event handler a window close
    // is never held, so the pending Editor writes never happen — and the
    // run-loop exit request that follows arrives too late to help. Without
    // the menu handler, no File-menu item does anything at all.
    // `SOURCE` is `lib.rs` alone, and these tests now live beside it rather
    // than inside it, so the whole of it is the production half and no literal
    // below can match this test's own source.
    let production = SOURCE;
    for registration in [
        ".on_window_event(menu::handle_window_event)",
        "menu::handle_menu_event(app, event.id().as_ref())",
        "menu::begin_exit(app, &app.state::<menu::ExitGate>(), *code)",
    ] {
        assert!(
            production.contains(registration),
            "{registration} must stay registered on the builder",
        );
    }
}

#[test]
fn command_names_list_contains_layout_preferences_commands() {
    // Guard against silently dropping a command from
    // `tauri::generate_handler!`. If the list below changes, the
    // `generate_handler!` invocation in `run()` must match.
    for required in [
        "list_recent_projects",
        "open_project_at_path",
        "open_project_from_git_url",
        "create_project",
        "load_layout_preferences",
        "save_layout_preferences",
        "browse_for_folder",
        "center_picker",
        "load_app_preferences",
        "save_app_preferences",
        "remove_recent_project",
        "clear_recent_projects",
        "pin_recent_project",
        "unpin_recent_project",
        "list_installed_plugins",
        "install_plugin",
        "uninstall_plugin",
        "list_agent_adapters",
        "install_adapter",
        "load_project_tree",
        "rescan_project_tree",
        "assign_artifact_type",
        "clear_artifact_type",
        "delete_path",
        "rename_path",
        "copy_path_into_folder",
        "create_file",
        "create_folder",
        "create_typed_file",
        "load_artifact_contents_by_id",
        "save_artifact_contents",
        "close_project",
        "finish_exit",
        "list_recently_edited_artifacts",
        "list_active_drafts",
        "list_recent_agent_runs",
        "list_pending_git_activity",
        "list_due_reminders",
        "list_project_health_signals",
        "list_uncommitted_changes",
        "list_branch_changes",
        "get_default_branch",
        "list_comparison_branches",
        "get_diff",
        "get_file_revisions",
        "load_changes_panel_state",
        "save_changes_panel_state",
        "load_library_panel_state",
        "save_library_panel_state",
        "load_notes_panel_state",
        "save_notes_panel_state",
        "list_notes_for_entity",
        "list_project_notes",
        "list_all_notes",
        "create_note",
        "update_note",
        "delete_note",
        "list_discussions",
        "list_all_discussions",
        "read_discussion",
        "open_discussion",
        "get_or_create_note_discussion",
        "add_comment",
        "set_discussion_lock",
        "set_discussion_resolution",
        "reanchor_discussion_fragment",
        "resolve_comment_author_identity",
        "read_comment_attachment",
        "list_branches",
        "list_commit_history",
        "list_commit_files",
        "get_commit_file_diff",
        "get_branch_information",
        "inspect_branch_deletion",
        "delete_branch",
        "list_pull_requests",
        "create_pull_request",
        "get_pull_request_head_state",
        "get_pull_request_detail",
        "list_pull_request_timeline",
        "list_worktrees_and_branches",
        "get_active_worktree",
        "activate_worktree",
        "check_out_branch_in_active_worktree",
        "propose_worktree_path",
        "create_worktree",
        "refresh_worktrees_and_branches",
        "get_uncommitted_diff_totals",
        "list_in_flight_operations",
        "load_project_config",
        "save_project_config",
        "start_search",
        "cancel_search",
        "open_artifact_by_id",
        "list_github_tokens",
        "add_github_token",
        "validate_github_token",
        "rename_github_token",
        "remove_github_token",
        "open_github_token_creation_page",
        "get_project_github_token_binding",
        "set_project_github_token_binding",
        // FNT-FR-01: without this entry the command could be dropped from
        // both `COMMAND_NAMES` and `generate_handler!` and stay green — the
        // subset check above is one-directional, and this list is the other
        // direction.
        "list_system_fonts",
    ] {
        assert!(
            COMMAND_NAMES.contains(&required),
            "COMMAND_NAMES is missing required command {required:?}; \
             ensure it is also registered in tauri::generate_handler!",
        );
    }
}
