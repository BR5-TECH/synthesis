//! Synthesis backend library crate (`synthesis_lib`).
//!
//! `lib.rs` is the Tauri-facing assembly layer: it declares the domain modules,
//! registers managed state and the command handler, installs the native menu,
//! and centers the picker on launch. The actual command logic lives in the
//! domain modules below, each of which owns its state, wire types, helpers, and
//! its own `#[cfg(test)]` block — mirroring the established `fs` / `scanning` /
//! `global_settings` convention.

use tauri::Manager;

pub mod agent_activity;
pub mod agent_conversations;
pub mod agentic;
pub mod agents;
pub mod ai_api;
pub mod ai_openrouter;
pub mod ai_shared;
pub mod artifacts;
pub mod bm25_index;
pub mod changes;
pub mod comments;
pub mod dashboard;
pub mod dialog;
pub mod docker;
pub mod documents;
pub mod draft_assets;
pub mod draft_history;
pub mod draft_proposals;
pub mod draft_watcher;
pub mod drafts;
pub mod flow_validation;
pub mod fonts;
pub mod fs;
pub mod git;
pub mod github_polling;
pub mod github_publication;
pub mod github_tokens;
pub mod global_settings;
pub mod graduation;
pub mod layout;
pub mod library;
pub mod logging;
pub mod menu;
pub mod notes;
pub mod notifications;
pub mod progress;
pub mod project;
pub mod project_settings;
pub mod prompt_proposals;
pub mod prompts;
pub mod relay_endpoint;
pub mod repository_store;
pub mod scanning;
pub mod search;
pub mod secret_vault;
pub mod settings;
pub mod settings_window;
pub mod skills;
pub mod statistics;
pub mod storage_floor;
pub mod streams;
pub mod tools;
pub mod watcher;
pub mod window;
pub mod worktree;

/// The canonical list of command names, split out to keep this file under the
/// project's thousand-line rule. `lib_tests.rs` is what reads it.
pub(crate) mod command_names;
#[cfg(test)]
pub(crate) use command_names::COMMAND_NAMES;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        // FSA-FR-21: the shared filesystem instance every backend module reaches
        // the disk through. Installed in `setup` below — the pre-project
        // instance first, replaced with a three-root one when a project opens.
        .manage(fs::FsAccessState::default())
        .manage(global_settings::GlobalSettingsStore::default())
        // GTS-FR-01 / GTS-FR-04: the OS credential store that holds every
        // GitHub token secret, and the verifier that checks one against GitHub.
        // The registry of descriptions is NOT here — it lives in the
        // user-global store above (GSS-FR-22).
        .manage(github_tokens::GithubTokens::default())
        // AIC-FR-03 / AIC-FR-04: the filesystem probe detection and
        // verification use, and the bounded child-process runner that asks a
        // binary what it is. The registry is NOT here — it lives in the
        // user-global store above (GSS-FR-14).
        .manage(agentic::AgenticIntegrations::default())
        // GSS-FR-37 / GSS-FR-38: the filesystem probe, the child-process
        // runner, and the Docker Engine connection the Docker backend commands
        // reach the outside world through, held in state so a test swaps any of
        // them without touching a call site.
        .manage(docker::DockerBackendSeams::default())
        // GSS-FR-HPWE: the probe the relay endpoint verification reads the
        // unauthenticated health answer through, held in state so a test swaps
        // it without touching a call site.
        .manage(relay_endpoint::RelayEndpointSeams::default())
        // PSS-FR-29: the one build a project may have in flight, and the seam
        // that performs it.
        .manage(project_settings::images::ImageBuildRegistry::default())
        .manage(project_settings::ImageBuilderSeam::default())
        .manage(ai_api::AiApiIntegrations::default())
        // AGC-FR-23: the turns in flight. Held in memory only, so nothing about
        // an interrupted conversation survives a relaunch — the reply delivered
        // into the conversation is the whole of the record. Also holds the
        // single completion seam (CVL-FR-11) and the concurrency bound of
        // AGC-FR-25.
        .manage(agent_conversations::TurnRegistry::default())
        // GRD-FR-BNTC / GRD-FR-TWMA: the lock that serialises writes to the
        // graduation store, and the cancellation of the one loop that may be
        // running. The queue itself is NOT here — it is on disk under
        // `short_data_dir()` (GSU-FR-RJRF), which is what makes an interrupted
        // run something to continue rather than something to notice missing.
        .manage(graduation::GraduationState::default())
        .manage(graduation::logs::LogRegistry::default())
        .manage(streams::StreamState::default())
        // AGV-FR-01: one store for every run's activity, alive for the process
        // rather than for a project — a run's narration outlives the project
        // switch that happens while it works.
        .manage(std::sync::Arc::new(agent_activity::ActivityStore::new()))
        // DAS-FR-14 / DAS-FR-15: the hold registry and the sweep scheduler are
        // memory of the running session — written to no file, surviving no
        // relaunch, and protecting nothing afterwards (DAS-FR-13).
        .manage(draft_assets::DraftAssetSweeper::default())
        .manage(github_publication::GithubIssuesSeam::default())
        // GPP-FR-DATH / GPP-FR-RAQP: the polling session and the per-issue
        // claim guards, and the seam the GitHub Projects client sits behind.
        .manage(github_polling::GithubPollingState::default())
        .manage(github_polling::GithubProjectsSeam::default())
        .manage(project::ProjectState::default())
        .manage(watcher::ProjectWatcher::default())
        .manage(artifacts::ContentTracker::default())
        // ASC-FR-20: the checksum of `.synthesis/library.toml` as this
        // application last wrote it, so the watcher can tell a teammate's pulled
        // curation from the echo of our own assignment.
        .manage(scanning::AttributionBaseline::default())
        // DRS-FR-42: the drafts-root watch, mounted with the content root and
        // torn down with it. `.synthesis/drafts/` is outside what the project
        // watcher above surfaces, so nothing else can observe a prompt write.
        .manage(draft_watcher::DraftsWatcher::default())
        // PST-FR-37: the per-widget refresh bookkeeping behind the timer —
        // one refresh in flight per widget, the ticks behind it coalesced, and
        // an overtaken result discarded rather than applied.
        .manage(dashboard::DashboardRefreshState::default())
        .manage(menu::ExitGate::default())
        // PRG-FR-02: the in-flight operation set. Held in memory only and
        // bounded by what is genuinely running (PRG-FR-10).
        .manage(progress::ProgressRegistry::default())
        // NTD-notification-delivery.md: what is showing in the OS notification
        // centre, and the platform it is shown through. Two slots rather than
        // one so a test can hold a different sink behind the same registry.
        .manage(notifications::NotificationRegistry::default())
        .manage(notifications::Notifier::default())
        // ASC-FR-17: the scan's file-node set as an enumerable candidate list,
        // mounted with the content root and torn down with it. Search consumes
        // it rather than walking the tree itself (SCC-FR-03 / SCC-FR-04).
        .manage(scanning::CandidateStore::default())
        // BMI-FR-02 / BMI-FR-14: the nine BM25 indexes and the pass scheduler
        // that keeps them in agreement with disk. Held in memory only —
        // nothing here is written to the project, to `.synthesis/cache/`, or to
        // `app_data_dir()`, and nothing is read back at startup (BMI-FR-13).
        .manage(bm25_index::Bm25Indexer::default())
        // DCL-FR-QGLH / DCL-FR-ZYQC: the Documents collection and the watch over
        // its selected paths. Held in memory only — the store keeps references
        // and nothing else (DCL-FR-VEVZ), and the PDF text cache lasts for the
        // application session (DCL-FR-QVYZ).
        .manage(documents::DocumentsCollection::default())
        .manage(documents::DocumentsWatcher::default())
        // SCC-FR-12: the at-most-one running search.
        .manage(search::SearchRegistry::default())
        // SNV-FR-28 / SNV-FR-30: handles on the Save / Save All items, whose
        // enabled state the frontend drives for the life of the application.
        .manage(menu::SaveMenuItems::<tauri::Wry>::default())
        // SNV-FR-43: handles on the Find / Find & Replace items, whose enabled
        // state the frontend drives from the active tab.
        .manage(menu::FindMenuItems::<tauri::Wry>::default())
        // SWN-FR-16: the Project settings menu entry, taken out of its menu
        // while no project is open and put back when one opens.
        .manage(menu::SettingsMenuItems::<tauri::Wry>::default())
        // SWN-FR-05 / SWN-FR-12: the save sweep a settings window is running,
        // and what it is running it for. Held in memory only — a settings
        // window survives nothing (SWN-FR-04 persists not even its position).
        .manage(settings_window::SettingsWindows::default())
        .setup(|app| {
            // FSA-FR-21: the pre-project instance — `app_data_dir()` and a
            // session temp directory, symlinks refused. It is what serves the
            // recent-projects list and the log buffer before any project root
            // exists; opening a project replaces it with a three-root one.
            if let Err(e) = app.state::<fs::FsAccessState>().install_pre_project() {
                // Nothing the user can do about this and nowhere to report it
                // yet — the log buffer's own export path depends on the very
                // instance that failed to build. Fail loudly at startup rather
                // than degrade into an application whose every write is refused.
                panic!("synthesis: could not establish filesystem access: {e}");
            }
            // ASV-FR-20: tell the vault where migration's candidates come
            // from. A keyring enumerates no entry, so the earlier
            // one-entry-per-secret layout can only be found by asking the
            // modules that own secrets what they stored. Installed here rather
            // than run here: migration happens on the first secret operation
            // the process performs, so a launch that never touches a secret
            // never asks the OS keychain to authenticate.
            let vault = secret_vault::global();
            vault.install_log_sink(app.handle().clone());
            let handle = app.handle().clone();
            vault.set_candidate_source(Box::new(move || {
                let store = handle.state::<global_settings::GlobalSettingsStore>();
                let mut candidates = Vec::new();
                if let Ok(records) = store.load_github_token_registry() {
                    candidates.extend(github_tokens::migration_candidates(&records));
                }
                if let Ok((records, _)) = store.load_ai_api_registry() {
                    candidates.extend(ai_api::migration_candidates(&records));
                }
                if let Ok((records, _)) = store.load_agentic_registry() {
                    candidates.extend(agentic::migration_candidates(&records));
                }
                candidates
            }));
            // PPK-FR-12: center the picker window on the active display BEFORE it
            // becomes interactive. The position is never persisted; this
            // computation runs unconditionally on every launch.
            window::center_picker_window(&app.handle());
            // SNV-FR-14: install the native Edit menu so OS-level edit
            // accelerators reach the focused webview (EDT-FR-15). Best-effort.
            if let Err(e) = menu::install_app_menu(app) {
                eprintln!("synthesis: failed to install application menu: {e}");
            }
            // SWN-FR-16 / PPK-FR-15: the application starts on the Project
            // picker with no project open, so the Project settings entry is
            // taken straight back out of the menu it was built into. Opening a
            // project puts it back (`project::activate_project`).
            menu::set_project_settings_present(&app.handle(), false);
            // PST-FR-36: the application-wide Dashboard refresh timer. Started
            // here rather than when a project opens, and never stopped until the
            // application quits: it runs whether or not a project is open and
            // whether or not a Dashboard tab exists anywhere, and a project
            // opened later is picked up by the next tick without a restart.
            dashboard::start_refresh_timer(app.handle());
            Ok(())
        })
        // SNV-FR-24 / SNV-FR-25 / SNV-FR-26: route menu activations. New folder,
        // New artifact and Close project reach the frontend via a Tauri event;
        // Exit and Quit hold the application open until the frontend has written
        // its pending changes. Predefined roles (Edit, Window) carry ids we do
        // not own and are ignored here — the OS dispatches them.
        .on_menu_event(|app, event| {
            menu::handle_menu_event(app, event.id().as_ref());
        })
        // SNV-FR-26 / EDT-FR-33: closing the only window quits the application,
        // so it is held here — while the webview still exists to write those
        // changes and to answer. The run loop's own exit request cannot serve:
        // it fires only once the last window is already gone.
        .on_window_event(menu::handle_window_event)
        .invoke_handler(tauri::generate_handler![
            settings::list_recent_projects,
            project::open_project_at_path,
            project::open_project_from_git_url,
            project::create_project,
            layout::load_layout_preferences,
            layout::save_layout_preferences,
            dialog::browse_for_folder,
            dialog::browse_for_file,
            dialog::browse_for_save_path,
            agent_activity::read_agent_activity,
            logging::append_log_records,
            logging::query_logs,
            logging::export_logs,
            window::center_picker,
            settings_window::open_settings_window,
            settings_window::finish_settings_close,
            settings_window::get_settings_window_context,
            settings::load_app_preferences,
            settings::save_app_preferences,
            fonts::list_system_fonts,
            settings::remove_recent_project,
            settings::clear_recent_projects,
            settings::pin_recent_project,
            settings::unpin_recent_project,
            settings::list_installed_plugins,
            settings::install_plugin,
            settings::uninstall_plugin,
            settings::list_agent_adapters,
            settings::install_adapter,
            library::load_project_tree,
            library::rescan_project_tree,
            library::assign_artifact_type,
            library::clear_artifact_type,
            library::delete_path,
            library::rename_path,
            library::copy_path_into_folder,
            library::create_file,
            library::create_folder,
            library::create_typed_file,
            drafts::list_drafts,
            drafts::create_draft,
            drafts::search_drafts,
            drafts::open_draft,
            drafts::rename_draft,
            drafts::set_draft_status,
            drafts::delete_draft,
            statistics::read_draft_statistics,
            statistics::record_draft_editing_interval,
            drafts::load_draft_file_contents,
            drafts::save_draft_file_contents,
            draft_assets::store_draft_image,
            draft_assets::read_draft_image,
            draft_assets::discard_draft_image,
            draft_assets::sweep_draft_assets,
            drafts::create_drafts_folder,
            drafts::rename_drafts_folder,
            drafts::delete_drafts_folder,
            drafts::move_draft_to_folder,
            drafts::move_drafts_folder,
            github_publication::get_draft_publication,
            github_publication::list_publication_remotes,
            github_publication::publish_draft_to_github,
            github_publication::load_publication_metadata,
            github_publication::get_github_publication_settings,
            github_publication::set_github_publication_settings,
            github_publication::list_github_issue_types,
            github_publication::retry_draft_publication,
            github_publication::resolve_draft_publication_conflict,
            github_publication::cancel_draft_publication_conflict,
            github_publication::cancel_draft_publication_attempt,
            github_publication::open_publication_issue,
            // GitHub polling (GPP-github-polling.md).
            github_polling::list_github_projects,
            github_polling::get_github_polling_state,
            github_polling::set_github_polling_settings,
            github_polling::poll_github_ready_tasks,
            github_polling::claim_github_task,
            github_polling::retry_github_claim,
            github_polling::acknowledge_github_claim,
            github_polling::open_github_task_issue,
            streams::create_work_stream,
            streams::list_work_streams,
            streams::get_work_stream,
            streams::merge_work_stream,
            streams::update_work_stream,
            streams::get_work_stream_update,
            streams::answer_work_stream_update_escalation,
            streams::retry_work_stream_update,
            streams::clear_work_stream_update,
            streams::cancel_work_stream_update,
            streams::delete_work_stream,
            streams::get_work_stream_uncommitted_paths,
            graduation::start_graduation,
            graduation::start_direct_graduation,
            graduation::preflight_direct_graduation,
            graduation::list_graduation_queue,
            graduation::get_graduation_run,
            graduation::get_graduation_capacity,
            graduation::read_graduation_logs,
            graduation::get_draft_graduation,
            graduation::answer_graduation_escalation,
            graduation::continue_graduation_run,
            graduation::pause_graduation_run,
            graduation::set_graduation_auto_start,
            graduation::reorder_graduation_run,
            graduation::discard_graduation_run,
            graduation::restart_graduation_run,
            graduation::revert_graduation_run,
            graduation::archive_graduation_run,
            graduation::unarchive_graduation_run,
            draft_history::list_draft_history,
            draft_history::load_draft_history_entry,
            draft_proposals::list_draft_change_proposals,
            draft_proposals::load_draft_change_proposal_hunks,
            draft_proposals::accept_draft_change_hunk,
            draft_proposals::reject_draft_change_hunk,
            draft_proposals::set_draft_change_hunk_discussing,
            draft_proposals::edit_draft_change_hunk,
            draft_proposals::decline_draft_change_proposal,
            prompt_proposals::list_prompt_change_proposals,
            prompt_proposals::load_prompt_change_proposal_content,
            prompt_proposals::save_prompt_change_proposal_candidate,
            prompt_proposals::apply_prompt_change_proposal,
            prompt_proposals::decline_prompt_change_proposal,
            prompt_proposals::complete_prompt_change_decision,
            artifacts::load_artifact_contents_by_id,
            artifacts::save_artifact_contents,
            flow_validation::validate_flow_document,
            project::close_project,
            menu::finish_exit,
            menu::set_save_menu_state,
            menu::set_find_menu_state,
            dashboard::list_recently_edited_artifacts,
            dashboard::list_active_drafts,
            dashboard::list_recent_agent_runs,
            dashboard::list_pending_git_activity,
            dashboard::list_due_reminders,
            dashboard::list_project_health_signals,
            changes::list_uncommitted_changes,
            changes::list_branch_changes,
            changes::get_default_branch,
            changes::list_comparison_branches,
            changes::get_uncommitted_diff_totals,
            git::get_diff,
            git::get_file_revisions,
            project_settings::load_changes_panel_state,
            project_settings::save_changes_panel_state,
            project_settings::load_library_panel_state,
            project_settings::save_library_panel_state,
            project_settings::load_notes_panel_state,
            project_settings::save_notes_panel_state,
            project_settings::load_drafts_panel_state,
            project_settings::save_drafts_panel_state,
            notes::list_notes_for_entity,
            notes::list_project_notes,
            notes::list_all_notes,
            notes::create_note,
            notes::update_note,
            notes::delete_note,
            comments::list_discussions,
            comments::list_all_discussions,
            comments::read_discussion,
            comments::open_discussion,
            comments::get_or_create_note_discussion,
            comments::add_comment,
            comments::set_discussion_lock,
            comments::set_discussion_resolution,
            comments::reanchor_discussion_fragment,
            comments::resolve_comment_author_identity,
            comments::read_comment_attachment,
            comments::read_discussion_question_set,
            comments::submit_discussion_question_answers,
            git::list_branches,
            git::list_commit_history,
            git::list_commit_files,
            git::get_commit_file_diff,
            git::list_branch_compare_files,
            git::get_branch_compare_file_diff,
            git::get_branch_information,
            git::inspect_branch_deletion,
            git::delete_branch,
            git::list_pull_requests,
            git::create_pull_request,
            git::get_pull_request_head_state,
            git::get_pull_request_detail,
            git::list_pull_request_timeline,
            git::get_working_tree_status,
            git::commit_paths,
            git::rollback_paths,
            git::get_upstream_sync_state,
            git::push_current_branch,
            worktree::list_worktrees_and_branches,
            worktree::get_active_worktree,
            worktree::activate_worktree,
            worktree::check_out_branch_in_active_worktree,
            worktree::propose_worktree_path,
            worktree::create_worktree,
            worktree::refresh_worktrees_and_branches,
            progress::list_in_flight_operations,
            notifications::get_notification_permission,
            notifications::request_notification_permission,
            notifications::post_notification,
            notifications::withdraw_notification,
            notifications::withdraw_all_notifications,
            project_settings::load_project_config,
            project_settings::save_project_config,
            search::start_search,
            search::cancel_search,
            artifacts::open_artifact_by_id,
            github_tokens::list_github_tokens,
            github_tokens::add_github_token,
            github_tokens::validate_github_token,
            github_tokens::rename_github_token,
            github_tokens::remove_github_token,
            github_tokens::open_github_token_creation_page,
            github_tokens::get_project_github_token_binding,
            github_tokens::set_project_github_token_binding,
            agentic::list_agentic_integrations,
            agentic::detect_agentic_cli_binary,
            agentic::verify_agentic_integration,
            agentic::set_agentic_integration_model,
            agentic::set_agentic_integration_effort,
            agentic::set_active_agentic_integration,
            agentic::clear_agentic_integration,
            agentic::get_project_agentic_integration,
            agentic::set_project_agentic_integration,
            project_settings::load_project_docker_images,
            project_settings::save_project_vendor_image,
            project_settings::build_project_vendor_image,
            project_settings::cancel_project_vendor_image_build,
            project_settings::load_project_image_build_in_flight,
            docker::load_docker_backend,
            docker::save_docker_backend,
            docker::detect_docker_cli_binary,
            docker::verify_docker_backend,
            // Remote connectivity (GSS-FR-ZKQT..GSS-FR-TXAO /
            // GLS-FR-KVNP..GLS-FR-XDUJ).
            relay_endpoint::load_relay_endpoint,
            relay_endpoint::save_relay_endpoint,
            relay_endpoint::verify_relay_endpoint,
            ai_api::list_ai_api_integrations,
            ai_api::list_ai_api_catalogs,
            ai_api::get_active_ai_api_catalog,
            ai_api::verify_ai_api_integration,
            ai_api::set_ai_api_model,
            ai_api::set_ai_api_reasoning,
            ai_api::set_ai_api_turn_timeout,
            ai_api::set_active_ai_api_integration,
            ai_api::clear_ai_api_integration,
            ai_api::get_project_ai_api_integration,
            ai_api::set_project_ai_api_integration,
            agents::list_agents,
            agents::create_agent,
            agents::update_agent,
            agents::delete_agent,
            agents::list_project_agents,
            agents::enrol_project_agent,
            agents::remove_project_agent,
            agent_conversations::dispatch_agent_turn,
            agent_conversations::cancel_agent_turn,
            agent_conversations::list_agent_turns,
            agent_conversations::list_recoverable_agent_turn_failures,
            agent_conversations::list_agent_turn_image_notices,
            agent_conversations::retry_agent_turn,
            // Documents (DCL-documents-collection.md).
            documents::commands::list_documents,
            documents::commands::pick_document_sources,
            documents::commands::remove_document_source,
            documents::commands::read_document,
            documents::commands::read_document_pdf,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        // SNV-FR-26 / EDT-FR-33: last-resort exit interception. Every quit the
        // user can ask for is caught earlier — at the menu item or at
        // `CloseRequested` — while the webview is still alive to write its
        // pending changes; by the time an exit request reaches the run loop the
        // last window is usually already destroyed, and `begin_exit` declines to
        // hold in that state rather than stranding a windowless process. The
        // exit `finish_exit` performs carries a code and passes straight through.
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, code, .. } = &event {
                if menu::begin_exit(app, &app.state::<menu::ExitGate>(), *code) {
                    api.prevent_exit();
                    return;
                }
                // NTD-FR-12: the exit is going ahead, so the notification centre
                // must not be left holding banners from a process that no longer
                // exists — one of those clicked later would raise a relaunched
                // application with an id it has never heard of (NTD-FR-13).
                // After `prevent_exit` deliberately: a held exit is not an exit,
                // and withdrawing there would clear notifications the author is
                // still able to act on.
                notifications::withdraw_all_on_exit(app);
            }
        });
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
