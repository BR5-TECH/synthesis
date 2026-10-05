//! Scheduling a housekeeping pass off the path that triggered it.

use super::*;

// ---------------------------------------------------------------------------
// Scheduling (DAS-FR-15, DAS-FR-19)
// ---------------------------------------------------------------------------

/// DAS-FR-15 / DAS-FR-19: schedule one housekeeping pass over one draft and
/// return the acknowledgement at once, before the pass has read anything.
///
/// The pass runs asynchronously and off the path that triggered it: it blocks no
/// keystroke, no prompt render, no project open or close, and no draft open or
/// close. It owns the `RootFs` it was scheduled with rather than reading one
/// when it runs, which is what lets a close-triggered pass address material that
/// still resolves after the project's storage context has been released (per
/// `PST-project-storage.md` PST-FR-30).
pub fn schedule_draft_sweep<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    sweeper: &DraftAssetSweeper,
    root: fs::RootFs,
    draft_id: &str,
    locked: bool,
) -> DraftAssetSweepScheduled {
    let (answer, run_now) = sweeper.claim(draft_id);
    if !run_now {
        log_debug(
            app,
            &BUFFER,
            &[Domain::Backend],
            "draft asset sweep coalesced",
            log_fields! { "draftId" => draft_id },
        );
        return answer;
    }
    let app = app.clone();
    let draft = draft_id.to_string();
    // The worker reaches the shared state through the app handle rather than
    // borrowing it from here, so the thread outlives this call and the caller
    // returns without waiting on anything.
    std::thread::spawn(move || {
        run_sweep_worker(app, root, draft, locked);
    });
    answer
}

/// The body of one scheduled worker: run the pass, then run it again while a
/// further request arrived while it was running (DAS-FR-15).
pub(super) fn run_sweep_worker<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    root: fs::RootFs,
    draft_id: String,
    locked: bool,
) {
    use tauri::Manager as _;
    let Some(sweeper) = app.try_state::<DraftAssetSweeper>() else {
        return;
    };
    let cancelled = sweeper.shutdown.clone();
    // DAS-FR-15 / DAS-FR-21: the draft leaves the running set however this
    // thread ends. A pass that panicked part-way through would otherwise leave
    // the draft marked running for the life of the process, and every later
    // request would be coalesced into a pass that is not there — while
    // `sweep_draft_assets` went on answering `scheduled` true, which is exactly
    // the "reports success for work it did not do" DAS-FR-27 forbids. A guard
    // that runs on unwind is what makes the next trigger able to run the pass
    // again (DAS-FR-20).
    struct Running<'a>(&'a DraftAssetSweeper, &'a str);
    impl Drop for Running<'_> {
        fn drop(&mut self) {
            self.0.finish(self.1);
        }
    }
    let _running = Running(&sweeper, &draft_id);
    while sweeper.take(&draft_id) {
        // DAS-FR-14: the registry itself, asked per candidate, rather than a
        // snapshot of it taken when the pass began.
        let held = |name: &str| sweeper.is_held(&draft_id, name);
        let sweep = sweep_draft_impl(&root, &draft_id, locked, &held, &cancelled);
        report_sweep(&app, &draft_id, &sweep);
    }
}

/// DAS-FR-12 / DAS-FR-21: the tally reaches the log and reaches no command
/// return value and no event.
///
/// A pass that decided everything is a milestone of normal operation; one that
/// did not is worth knowing about, and is reported at `WARN` and through no
/// other route — routine cleanup raises no blocking error, opens no dialog, and
/// puts nothing in front of the author.
pub(super) fn report_sweep<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    draft_id: &str,
    sweep: &DraftAssetSweep,
) {
    let fields = log_fields! {
        "draftId" => draft_id,
        "scanned" => sweep.scanned,
        "removed" => sweep.removed,
        "retained" => sweep.retained,
        "failed" => sweep.failed,
        "complete" => sweep.complete,
    };
    if sweep.complete {
        log_info(app, &BUFFER, &[Domain::Backend], "draft asset sweep", fields);
    } else {
        log_warn(
            app,
            &BUFFER,
            &[Domain::Backend],
            "draft asset sweep incomplete",
            fields,
        );
    }
}

/// DAS-FR-20: begin abandoning whatever housekeeping is still running, because
/// the application is quitting.
///
/// Signalling rather than waiting: the pass checks between candidates and stops
/// where it stands, so a quit is never held on a directory walk, and the tally
/// it records carries `complete` false — nothing reports a completed sweep for
/// work it did not complete.
pub fn begin_shutdown<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use tauri::Manager as _;
    if let Some(sweeper) = app.try_state::<DraftAssetSweeper>() {
        sweeper.begin_shutdown();
    }
}

/// DAS-FR-19: the **application-start** trigger — one pass over every draft of
/// the active worktree, once, after the active project has been loaded.
///
/// Distinct from the project-open trigger beside it and taken at most once for
/// the life of the process. Where the two coincide — the usual case, the shell
/// reopening the last project at launch — the second request is coalesced into
/// the first (DAS-FR-15) rather than running a duplicate pass.
pub fn sweep_at_application_start<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use tauri::Manager as _;
    let Some(sweeper) = app.try_state::<DraftAssetSweeper>() else {
        return;
    };
    if sweeper.started.swap(true, Ordering::SeqCst) {
        return;
    }
    log_info(
        app,
        &BUFFER,
        &[Domain::Backend],
        "draft asset housekeeping at application start",
        log_fields! {},
    );
    sweep_project_draft_assets(app);
}

/// DAS-FR-12 / DAS-FR-19: one pass over **every** draft of the active worktree,
/// on the terms one pass over one draft runs.
///
/// What the application-start and project-lifecycle triggers schedule (per
/// `PST-project-storage.md` PST-FR-30). Each draft gets its own pass and its own
/// tally, so one draft whose prompt cannot be read costs the others nothing.
pub fn sweep_project_draft_assets<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use tauri::Manager as _;
    let Some(project) = app.try_state::<ProjectState>() else {
        return;
    };
    let Ok(root) = project.require_root() else {
        return;
    };
    let Some(sweeper) = app.try_state::<DraftAssetSweeper>() else {
        return;
    };
    let hierarchy = crate::drafts::list_drafts_impl(&root);
    log_debug(
        app,
        &BUFFER,
        &[Domain::Backend],
        "draft asset housekeeping scheduled for project",
        log_fields! { "drafts" => hierarchy.drafts.len() },
    );
    for summary in &hierarchy.drafts {
        let locked = summary
            .graduation
            .as_ref()
            .is_some_and(|g| g.locked || g.graduated);
        schedule_draft_sweep(app, &sweeper, root.clone(), &summary.id, locked);
    }
}
