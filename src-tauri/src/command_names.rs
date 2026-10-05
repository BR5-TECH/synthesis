//! The canonical list of Tauri command names exposed to the frontend.
//!
//! Split out of `lib.rs`, which the project's own rule keeps under a thousand
//! lines: the list grows with every command and is the one part of that file
//! that grows without the module gaining any behaviour. Nothing else moved with
//! it — the builder, the handler registration, and the setup chain all stay
//! where they were.

/// Canonical list of Tauri command names exposed to the frontend.
///
/// Keep this list in lock-step with the entries passed to
/// `tauri::generate_handler!` in `run()`. Per CLAUDE.md, an unregistered command
/// silently fails at runtime — the unit test below asserts each expected name is
/// present so removing a command from `generate_handler!` without updating this
/// list is caught locally (`lib_tests.rs`). (Each domain module additionally pins its own command
/// fns in scope via `*_are_in_scope` tests, so a rename fails to compile there.)
#[allow(dead_code)]
pub(crate) const COMMAND_NAMES: &[&str] = &[
    "list_recent_projects",
    "open_project_at_path",
    "open_project_from_git_url",
    "create_project",
    "load_layout_preferences",
    "save_layout_preferences",
    "browse_for_folder",
    // AII-FR-05: point at a CLI binary detection could not find.
    "browse_for_file",
    // LOG-FR-17 / FSA-FR-16: where an export is written.
    "browse_for_save_path",
    // What an agent CLI did, per run (AGV-agent-activity.md).
    "read_agent_activity",
    // Session logging (LGC-logging.md / LOG-logs.md).
    "append_log_records",
    "query_logs",
    "export_logs",
    // Picker window re-centering on re-show (PPK-FR-12).
    "center_picker",
    // The two settings child windows (SWN-settings-windows.md SWN-FR-13,
    // SWN-FR-08).
    "open_settings_window",
    "finish_settings_close",
    "get_settings_window_context",
    // User-global settings (GSS-global-settings-storage.md).
    "load_app_preferences",
    "save_app_preferences",
    // Installed font families for the Appearance typography controls
    // (FNT-font-enumeration.md FNT-FR-01).
    "list_system_fonts",
    "remove_recent_project",
    "clear_recent_projects",
    "pin_recent_project",
    "unpin_recent_project",
    "list_installed_plugins",
    "install_plugin",
    "uninstall_plugin",
    "list_agent_adapters",
    "install_adapter",
    // Library tree (PST-project-storage.md / ASC-artifact-scanning.md).
    "load_project_tree",
    "rescan_project_tree",
    "assign_artifact_type",
    "clear_artifact_type",
    // Library file operations (PST-project-storage.md / LIB-library.md).
    "delete_path",
    "rename_path",
    "copy_path_into_folder",
    // Plain-file creation (PST-project-storage.md / NFI-new-file.md).
    "create_file",
    // Folder creation (PST-project-storage.md / NFW-new-folder.md).
    "create_folder",
    // Typed-artifact creation (PST-project-storage.md /
    // NTA-new-typed-artifact.md).
    "create_typed_file",
    // Drafts — the New Artifact workspace and the Drafts panel
    // (NAW-new-artifact.md / DRP-drafts-panel.md).
    "list_drafts",
    "create_draft",
    "search_drafts",
    "open_draft",
    "rename_draft",
    "set_draft_status",
    "delete_draft",
    // Drafts folders (DRS-draft-storage.md / DRP-drafts-panel.md).
    "create_drafts_folder",
    "rename_drafts_folder",
    "delete_drafts_folder",
    "move_draft_to_folder",
    "move_drafts_folder",
    "read_draft_statistics",
    "record_draft_editing_interval",
    "load_draft_file_contents",
    "save_draft_file_contents",
    // Draft assets (DAS-draft-assets.md). `read_prompt_images` and
    // `sweep_project_draft_assets` are deliberately absent: the first is the one
    // path by which asset bytes reach a conversation and is registered as no
    // command so no frontend call reaches it (DAS-FR-22), and the second is the
    // application's own lifecycle trigger (DAS-FR-19).
    "store_draft_image",
    "read_draft_image",
    "discard_draft_image",
    "sweep_draft_assets",
    // GitHub publication (GHP-github-publication.md).
    "get_draft_publication",
    "list_publication_remotes",
    "publish_draft_to_github",
    "load_publication_metadata",
    "get_github_publication_settings",
    "set_github_publication_settings",
    "list_github_issue_types",
    "retry_draft_publication",
    "resolve_draft_publication_conflict",
    "cancel_draft_publication_conflict",
    "cancel_draft_publication_attempt",
    "open_publication_issue",
    // GitHub polling (GPP-github-polling.md).
    "list_github_projects",
    "get_github_polling_state",
    "set_github_polling_settings",
    "poll_github_ready_tasks",
    "claim_github_task",
    "retry_github_claim",
    "acknowledge_github_claim",
    "open_github_task_issue",
    // Graduation (GRD-graduation.md / GRU-graduation-runs.md /
    // GRV-graduation-review.md). `dispatch_graduation_turn`,
    // `recompute_manifest`, `execution_directory`, `record_escalation`,
    // `record_checkpoint`, and `publish_change_set` are deliberately absent:
    // each is an internal seam that launches an agent, computes the manifest,
    // or writes into a worktree the author is not looking at (GXD-FR-DRFF,
    // GRB-FR-CYWM).
    // Work streams (WKS-work-streams.md / WSS-work-stream-selector.md).
    "create_work_stream",
    "list_work_streams",
    "get_work_stream",
    "merge_work_stream",
    "update_work_stream",
    "get_work_stream_update",
    "answer_work_stream_update_escalation",
    "retry_work_stream_update",
    "clear_work_stream_update",
    "cancel_work_stream_update",
    "delete_work_stream",
    "get_work_stream_uncommitted_paths",
    "start_graduation",
    "start_direct_graduation",
    "preflight_direct_graduation",
    "list_graduation_queue",
    "get_graduation_run",
    "get_graduation_capacity",
    "read_graduation_logs",
    "get_draft_graduation",
    "answer_graduation_escalation",
    "continue_graduation_run",
    "pause_graduation_run",
    "set_graduation_auto_start",
    "reorder_graduation_run",
    "discard_graduation_run",
    "restart_graduation_run",
    "revert_graduation_run",
    "archive_graduation_run",
    "unarchive_graduation_run",
    "list_draft_history",
    "load_draft_history_entry",
    "list_draft_change_proposals",
    "load_draft_change_proposal_hunks",
    "accept_draft_change_hunk",
    "reject_draft_change_hunk",
    "set_draft_change_hunk_discussing",
    "edit_draft_change_hunk",
    "decline_draft_change_proposal",
    "list_prompt_change_proposals",
    "load_prompt_change_proposal_content",
    "save_prompt_change_proposal_candidate",
    "apply_prompt_change_proposal",
    "decline_prompt_change_proposal",
    "complete_prompt_change_decision",
    // Artifact contents (PST-project-storage.md / EDT-editor.md).
    "load_artifact_contents_by_id",
    "save_artifact_contents",
    "validate_flow_document",
    // Held project close / application quit (SNV-FR-25 / SNV-FR-26 / EDT-FR-33).
    "close_project",
    "finish_exit",
    // File -> Save / Save All enablement (SNV-FR-28 / SNV-FR-30).
    "set_save_menu_state",
    // Edit -> Find / Find & Replace enablement (SNV-FR-43).
    "set_find_menu_state",
    // Dashboard widget loaders (PST-project-storage.md / DSH-dashboard.md).
    "list_recently_edited_artifacts",
    "list_active_drafts",
    "list_recent_agent_runs",
    "list_pending_git_activity",
    "list_due_reminders",
    "list_project_health_signals",
    // Changes panel (CHC-changes.md / CHG-changes.md).
    "list_uncommitted_changes",
    "list_branch_changes",
    "get_default_branch",
    "list_comparison_branches",
    // Uncommitted diff totals for the status bar (CHC-FR-21 / STB-FR-25).
    "get_uncommitted_diff_totals",
    // Diff payload for the Changes panel's Diff tab (GTC-git.md).
    "get_diff",
    // Whole-file revisions for the Diff tab's side-by-side, final, and rich
    // modes (GTC-FR-16 / DFV-FR-25).
    "get_file_revisions",
    // Changes panel state, project-local scope (PSS-project-settings-storage.md).
    "load_changes_panel_state",
    "save_changes_panel_state",
    // Library panel state, project-local scope (PSS-FR-18 / LIB-FR-14).
    "load_library_panel_state",
    "save_library_panel_state",
    // Notes panel state, project-local scope (PSS-FR-19 / NTS-FR-09 / NTS-FR-13).
    "load_drafts_panel_state",
    "save_drafts_panel_state",
    "load_notes_panel_state",
    "save_notes_panel_state",
    // Notes (NTC-notes-storage.md / NTS-notes.md).
    "list_notes_for_entity",
    "list_project_notes",
    "list_all_notes",
    "create_note",
    "update_note",
    "delete_note",
    // Comments (CMS-comments-storage.md / CMT-comments.md).
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
    // The discussion question set an agent records and the author answers
    // (CMS-FR-JWVH / CMS-FR-TXRB). `reserve_discussion_question_set` is
    // deliberately absent: no frontend call creates a set (CMS-FR-EKUP).
    "read_discussion_question_set",
    "submit_discussion_question_answers",
    // Git branches (GTC-git.md GTC-FR-07).
    "list_branches",
    "list_commit_history",
    "list_commit_files",
    "get_commit_file_diff",
    // GTC-git.md GTC-FR-YQVD / GTC-FR-PYVV.
    "list_branch_compare_files",
    "get_branch_compare_file_diff",
    "get_branch_information",
    "inspect_branch_deletion",
    "delete_branch",
    "list_pull_requests",
    "get_pull_request_detail",
    "list_pull_request_timeline",
    // The single working-tree status primitive, which the graduation start
    // preflight is read from (GTC-FR-29 / GSU-FR-MLEJ).
    "get_working_tree_status",
    // The path-scoped commit the application makes (GTC-FR-19 / CMW-FR-07).
    "commit_paths",
    // The path-scoped rollback the Changes panel performs (GTC-FR-23 /
    // CHG-FR-62).
    "rollback_paths",
    // Whether the branch holds commits the remote does not (GTC-FR-21 /
    // CHG-FR-37).
    "get_upstream_sync_state",
    // The push both the Git panel and the Changes panel perform (GTC-FR-22).
    "push_current_branch",
    // Worktree context (WTC-worktree-context.md / WTS-worktree-selector.md).
    "list_worktrees_and_branches",
    "get_active_worktree",
    "activate_worktree",
    "check_out_branch_in_active_worktree",
    "propose_worktree_path",
    "create_worktree",
    "refresh_worktrees_and_branches",
    // Progress reporting (PRG-progress-reporting.md / STB-status-bar.md).
    "list_in_flight_operations",
    // OS notification delivery (NTD-notification-delivery.md / NTF-notifications.md).
    "get_notification_permission",
    "request_notification_permission",
    "post_notification",
    "withdraw_notification",
    "withdraw_all_notifications",
    // Project-public config, incl. the line-ending convention (PSS-FR-17).
    "load_project_config",
    "save_project_config",
    // Universal search (SCC-search.md / SCH-search.md).
    "start_search",
    "cancel_search",
    // Routing hint for an id the UI is about to open (PST-FR-08 / PST-FR-24).
    "open_artifact_by_id",
    // GitHub tokens (GTS-github-token-storage.md / GHA-github-authentication.md).
    "list_github_tokens",
    "add_github_token",
    "validate_github_token",
    "rename_github_token",
    "remove_github_token",
    "open_github_token_creation_page",
    "get_project_github_token_binding",
    "set_project_github_token_binding",
    // Agentic integrations (AIC-agentic-integrations.md / AII-ai-integrations.md).
    // `resolve_agentic_invocation` is deliberately absent: it is the read path
    // for an actual invocation and must stay unreachable from the frontend
    // (AIC-FR-19).
    "list_agentic_integrations",
    "detect_agentic_cli_binary",
    "verify_agentic_integration",
    "set_agentic_integration_model",
    "set_agentic_integration_effort",
    "set_active_agentic_integration",
    "clear_agentic_integration",
    "get_project_agentic_integration",
    "set_project_agentic_integration",
    // Docker backend (GSS-global-settings-storage.md GSS-FR-35..GSS-FR-40 /
    // GLS-global-settings.md GLS-FR-29..GLS-FR-31). `resolve_docker_backend` is
    // deliberately absent for the same reason `resolve_agentic_invocation` is:
    // it is the read path for an actual Docker operation and must stay
    // unreachable from the frontend (GSS-FR-40).
    "load_project_docker_images",
    "save_project_vendor_image",
    "build_project_vendor_image",
    "cancel_project_vendor_image_build",
    "load_project_image_build_in_flight",
    "load_docker_backend",
    "save_docker_backend",
    "detect_docker_cli_binary",
    "verify_docker_backend",
    // Remote connectivity (GSS-global-settings-storage.md GSS-FR-ZKQT..GSS-FR-TXAO
    // / GLS-global-settings.md GLS-FR-KVNP..GLS-FR-XDUJ). The store holds no
    // `SYNTHESIS_SERVER_TOKEN` and no private key, so no command here reads or
    // writes one (GSS-FR-VMRB).
    "load_relay_endpoint",
    "save_relay_endpoint",
    "verify_relay_endpoint",
    // AI API integrations (AAP-ai-api-integrations.md / AII-ai-integrations.md).
    // `resolve_ai_api_call` is deliberately absent for the same reason: it
    // hands out an endpoint and the key to call it with (AAP-FR-19).
    "list_ai_api_integrations",
    "list_ai_api_catalogs",
    "get_active_ai_api_catalog",
    "verify_ai_api_integration",
    "set_ai_api_model",
    "set_ai_api_reasoning",
    "set_ai_api_turn_timeout",
    "set_active_ai_api_integration",
    "clear_ai_api_integration",
    "get_project_ai_api_integration",
    "set_project_ai_api_integration",
    // Conversational agents (AGR-agent-registry.md / AGT-agents.md).
    // `resolve_project_agent` is deliberately absent: it is the read path a
    // dispatch takes and must stay unreachable from the frontend (AGR-FR-18).
    "list_agents",
    "create_agent",
    "update_agent",
    "delete_agent",
    "list_project_agents",
    "enrol_project_agent",
    "remove_project_agent",
    // Agent conversations (AGC-agent-conversations.md / CMT-comments.md).
    // `build_input` and the completion seam are likewise absent: one reads the
    // material under discussion, the other reaches a model with a key
    // (CVL-FR-11).
    "dispatch_agent_turn",
    "cancel_agent_turn",
    "list_agent_turns",
    "list_recoverable_agent_turn_failures",
    "list_agent_turn_image_notices",
    "retry_agent_turn",
    // Documents (DCL-documents-collection.md). Document tools are absent: they
    // are agent tools and have no Tauri reach (SDT-FR-LNNL, GDT-FR-LITM).
    "list_documents",
    "pick_document_sources",
    "remove_document_source",
    "read_document",
    "read_document_pdf",
];
