//! Closing a project (`PST-project-storage.md` PST-FR-14, SNV-FR-25,
//! `PWN-project-windows.md` PWN-FR-UHLU).

use super::{deactivate_project, ProjectState};
use crate::artifacts::ContentTracker;
use crate::progress;
use crate::project_scope::ProjectStores;
use crate::watcher::ProjectWatcher;

/// SNV-FR-25: close the open project. Invoked by the frontend as the last step
/// of File -> Close project, *after* it has written every artifact's pending
/// changes (EDT-FR-33) — a torn-down project has no root, so `save_artifact_contents`
/// would fail from here on. Writes no file itself, and is idempotent.
#[project_scope_macros::project_scoped]
#[tauri::command]
// Reason: the parameters of a Tauri command are its IPC contract with `invoke()`.
#[allow(clippy::too_many_arguments)]
pub fn close_project<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    project: crate::project_scope::Scoped<'_, ProjectState>,
    watcher: crate::project_scope::Scoped<'_, ProjectWatcher>,
    tracker: crate::project_scope::Scoped<'_, ContentTracker>,
    drafts_watcher: crate::project_scope::Scoped<'_, crate::draft_watcher::DraftsWatcher>,
    progress_registry: crate::project_scope::Scoped<'_, progress::ProgressRegistry>,
    candidates: crate::project_scope::Scoped<'_, crate::scanning::CandidateStore>,
    searches: crate::project_scope::Scoped<'_, crate::search::SearchRegistry>,
    turns: crate::project_scope::Scoped<'_, crate::agent_conversations::TurnRegistry>,
) {
    // PWN-FR-UHLU: the whole teardown runs in the context of the scope of this
    // call, the default scope included. With no context, a store lookup gives
    // the one open project (PWN-FR-CTXT), and a close that names no project
    // must not tear that project down.
    let project_id = project.project_id().to_string();
    crate::project_scope::context::run_in(&project_id, || {
        close_project_in_scope(
            app,
            project,
            watcher,
            tracker,
            drafts_watcher,
            progress_registry,
            candidates,
            searches,
            turns,
        )
    })
}

/// The teardown of [`close_project`], in the context that the command set.
// Reason: it takes the stores of the command one by one, as the command does.
#[allow(clippy::too_many_arguments)]
fn close_project_in_scope<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    project: crate::project_scope::Scoped<'_, ProjectState>,
    watcher: crate::project_scope::Scoped<'_, ProjectWatcher>,
    tracker: crate::project_scope::Scoped<'_, ContentTracker>,
    drafts_watcher: crate::project_scope::Scoped<'_, crate::draft_watcher::DraftsWatcher>,
    progress_registry: crate::project_scope::Scoped<'_, progress::ProgressRegistry>,
    candidates: crate::project_scope::Scoped<'_, crate::scanning::CandidateStore>,
    searches: crate::project_scope::Scoped<'_, crate::search::SearchRegistry>,
    turns: crate::project_scope::Scoped<'_, crate::agent_conversations::TurnRegistry>,
) {
    // LGC-FR-15: the session's diagnostic buffer is discarded in full when the
    // project closes. First, before anything else the close does, so every
    // record the teardown itself emits lands in the fresh buffer rather than
    // being discarded by a clear that followed it.
    // The buffer is shared by every open project window, so the clear waits
    // until no other project is open: closing one project keeps the records of
    // the others.
    if !crate::project_scope::other_project_open(&app, Some(project.project_id())) {
        crate::logging::clear_and_publish(&app, &crate::logging::BUFFER);
    }
    crate::logging::log(
        &app,
        &crate::logging::BUFFER,
        crate::logging::LogLevel::Info,
        &[crate::logging::Domain::Backend],
        "project closed",
        crate::logging::Fields::new(),
    );
    // SCC-FR-14: stop a search still sweeping the project being closed, so it
    // ends with its one terminal event instead of streaming into a closed
    // project's overlay.
    searches.cancel_running();
    // AGC-FR-20: a turn in flight when the project closes terminates
    // `cancelled` and appends nothing. Runs before the teardown, while the root
    // the reply would be written into is still knowable.
    crate::agent_conversations::cancel_all_turns(&app, &turns, &progress_registry);
    // `../core/GRD-graduation.md` GRD-FR-TWMA: a graduation the loop is holding
    // stops here and rests as `interrupted` — durable, still blocking its
    // queue, and offering Continue. Nothing is discarded: in Git mode the
    // worktree and its change set survive, in place the edits already made do.
    // Runs before the teardown, while the run's record is still readable.
    crate::graduation::interrupt_running(
        &app,
        crate::graduation::GraduationInterruptionReason::ProjectChanged,
    );
    // EAC-FR-ABHG: the runs are stopped, so the sandbox VMs of this project stop
    // with it. The VMs of the other open projects go on.
    crate::tools::agent_exec::sandbox::stop_project_in_background(&app, project.slot_key());
    // GPP-FR-DATH: the polling session ends with the project, so a project
    // opened again starts a new session with nothing reported.
    crate::github_polling::end_session(&app);
    // GHP-FR-ROBR: the publication metadata cache goes with the project.
    crate::github_publication::metadata_refresh::on_project_closed(&app);
    // PRG-FR-13: nothing may be left in flight against a root the application
    // has stopped reading. Runs before the teardown, while the root is still
    // knowable. Operations with no project scope (a plugin install, PRG-FR-15)
    // are deliberately untouched.
    if let Some(root) = project.root() {
        progress::terminate_scope_and_publish(&app, &progress_registry, &root);
    }
    // BMI-FR-25: the nine indexes go with the rest of the content root's
    // in-memory state, so a query after the close returns an empty list rather
    // than the closed project's chunks.
    if let Some(indexer) = crate::project_scope::ProjectStores::try_project_store::<
        crate::bm25_index::Bm25Indexer,
    >(&app)
    {
        indexer.clear();
    }
    // DCL-FR-XHSJ: the Documents collection stops its watch and discards its
    // snapshot, its documents instance, and its sources in memory. Its PDF text
    // cache stays for the application session.
    crate::documents::lifecycle::on_project_closed(&app);
    // EMB-FR-RMZH: stop every embedding job and close the store.
    crate::embeddings::on_project_closed(&app);
    // MCS-FR-QTSW: every MCP session of the project ends.
    crate::mcp::on_project_closed(&app);
    // PST-FR-30: closing a project schedules the draft-asset housekeeping pass
    // of `DAS-draft-assets.md` (DAS-FR-19), **before** the unmount below
    // releases the project's storage context — so it addresses drafts that
    // still resolve rather than drafts that have already gone. Each scheduled
    // pass owns the filesystem handle it was scheduled with, so the unmount
    // that follows cannot take the material out from under it. It is the one
    // write a close may perform, and it reaches draft-owned material
    // alone (PST-FR-14); the close itself completes at its ordinary speed
    // whatever the pass is doing.
    crate::draft_assets::sweep_project_draft_assets(&app);
    deactivate_project(
        &project,
        watcher.inner(),
        tracker.inner(),
        drafts_watcher.inner(),
        candidates.inner(),
        &app.project_store::<crate::scanning::AttributionBaseline>(),
    );
    // FSA-FR-21, FSA-FR-29, FSA-FR-TSHC: the instances of this project go with
    // it. The instances of the other open projects stay. The state is found by
    // the key of this command, as every store of the teardown is (PWN-FR-UHLU).
    if let Some(access) = crate::fs::project_fs_state_of(&app, project.project_id()) {
        access.discard_project();
    }
}
