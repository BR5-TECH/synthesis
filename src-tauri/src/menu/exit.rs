//! The exit gate: how long a quit is held for the frontend to write its pending changes, and what releases it (SNV-FR-26 / EDT-FR-33).

use super::*;
use crate::project_scope::events::EmitToProject;

/// How long an unanswered quit stays held before a further quit request is let
/// through. Long enough for the frontend to write every dirty artifact, short
/// enough that a wedged frontend cannot make the application unquittable: the
/// user asks to quit a second time and it goes.
pub(super) const EXIT_FLUSH_GRACE: Duration = Duration::from_secs(10);

/// Whether a quit is currently being held for the frontend to answer
/// (SNV-FR-26), and since when. Managed state.
///
/// A quit is held while the frontend writes its pending Editor changes. Repeat
/// requests during that window are held too — a user asking to quit twice
/// because nothing visibly happened must not kill the process mid-write — but a
/// repeat never extends the deadline, so once the grace period since the FIRST
/// unanswered request has elapsed, the next request quits regardless.
#[derive(Default)]
pub struct ExitGate {
    held_since: Mutex<Option<Instant>>,
    /// PWN-FR-WVUO: the windows that have not answered the held quit yet.
    awaiting: Mutex<Awaiting>,
}

/// The windows that the held quit waits for.
#[derive(Default)]
enum Awaiting {
    /// No list: the first answer decides the quit.
    #[default]
    Nobody,
    /// These windows have not answered yet.
    Windows(std::collections::HashSet<String>),
    /// A window cancelled the held quit. A later answer to the same quit is
    /// ignored, so the application never quits after a cancel. A new quit
    /// replaces this state (`expect_windows`).
    Cancelled,
}

/// What a window's answer to a held quit decides.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum ExitAnswer {
    /// Every window wrote its changes: the application quits.
    Quit,
    /// A window could not write: the quit is cancelled.
    Cancel,
    /// Other windows still write their changes.
    Wait,
}

impl ExitGate {
    /// Decide a quit request made at `now`: `true` to hold it (the caller
    /// prevents the exit and waits for `finish_exit`), `false` to let it quit.
    /// Takes the clock so the policy is unit-testable.
    pub(super) fn claim_at(&self, now: Instant, grace: Duration) -> bool {
        let mut slot = self.held_since.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        match *slot {
            // A hold is outstanding: keep holding while the frontend still has
            // time to answer, and let the quit through once it has run out.
            Some(since) => now.duration_since(since) < grace,
            None => {
                *slot = Some(now);
                true
            }
        }
    }

    /// PWN-FR-WVUO: the windows whose answer the held quit waits for. A new
    /// quit starts here, so it forgets a cancel of the quit before.
    pub(super) fn expect_windows(&self, labels: impl IntoIterator<Item = String>) {
        *self.awaiting.lock().unwrap_or_else(|e| e.into_inner()) =
            Awaiting::Windows(labels.into_iter().collect());
    }

    /// Take the answer of one window to the held quit. After a cancel, an
    /// answer of any other window changes nothing.
    pub(super) fn answer(&self, label: &str, proceed: bool) -> ExitAnswer {
        let mut awaiting = self.awaiting.lock().unwrap_or_else(|e| e.into_inner());
        match &mut *awaiting {
            Awaiting::Cancelled => ExitAnswer::Wait,
            Awaiting::Nobody if proceed => ExitAnswer::Quit,
            Awaiting::Nobody => ExitAnswer::Cancel,
            Awaiting::Windows(_) if !proceed => {
                *awaiting = Awaiting::Cancelled;
                ExitAnswer::Cancel
            }
            Awaiting::Windows(set) => {
                set.remove(label);
                if set.is_empty() {
                    *awaiting = Awaiting::Nobody;
                    ExitAnswer::Quit
                } else {
                    ExitAnswer::Wait
                }
            }
        }
    }

    /// Release the hold, so a later quit is held (and flushed) again. Called
    /// whichever way the frontend answered.
    pub fn release(&self) {
        *self.held_since.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }
}

/// Hold a requested application quit while the frontend writes its pending
/// changes (SNV-FR-26 / EDT-FR-33). Returns whether the caller should prevent
/// whatever it was about to do — close the window, or exit. `code` is the exit
/// code the request carries: the quit `finish_exit` itself performs is
/// programmatic and carries one, so it passes straight through instead of being
/// held again.
pub fn begin_exit<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    gate: &ExitGate,
    code: Option<i32>,
) -> bool {
    begin_exit_at(app, gate, code, Instant::now())
}

/// `begin_exit` with the clock supplied, so the grace-period escape is testable
/// without waiting it out.
pub(super) fn begin_exit_at<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    gate: &ExitGate,
    code: Option<i32>,
    now: Instant,
) -> bool {
    // A hold is only ever released by the frontend calling `finish_exit`, so
    // holding with no webview left to receive the request would keep the process
    // alive forever with no window and no way to quit it. This is the state the
    // run loop's own exit request arrives in — it fires only after the last
    // window has been destroyed — which is why the window close is intercepted
    // earlier, at `CloseRequested`.
    if app.webview_windows().is_empty() {
        return false;
    }
    if !begin_exit_decision(code, gate, now, EXIT_FLUSH_GRACE) {
        return false;
    }
    // SWN-FR-19: an open settings window writes its pending changes first, and
    // a write that cannot proceed safely cancels the quit and leaves that
    // window open with its failed section presented. The hold stays claimed
    // throughout: the sweep either resumes this quit (`resume_held_exit`) or
    // releases the hold when it fails.
    if crate::settings_window::defer_quit_for_settings(app) {
        return true;
    }
    // If the event cannot be delivered there is nobody to flush or to answer, so
    // do not hold a quit that would never be released — and give the claim back,
    // since no answer is coming to release it.
    gate.expect_windows(quit_windows(app));
    if app.emit_project(MENU_EXIT_REQUESTED, ()).is_ok() {
        true
    } else {
        gate.release();
        false
    }
}

/// SWN-FR-19: carry on a quit that was held while the settings window wrote its
/// pending changes.
///
/// The exit gate is still claimed from the original request, so this only has to
/// deliver the request the settings sweep stood in front of. A delivery that
/// fails gives the claim back rather than leaving a hold nothing will release.
pub fn resume_held_exit<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    app.state::<ExitGate>().expect_windows(quit_windows(app));
    if app.emit_project(MENU_EXIT_REQUESTED, ()).is_err() {
        app.state::<ExitGate>().release();
    }
}

/// PWN-FR-WVUO: the windows that write their changes before a quit.
fn quit_windows<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Vec<String> {
    app.webview_windows()
        .into_keys()
        .filter(|label| crate::project_windows::is_project_window_label(label))
        .collect()
}

/// SNV-FR-26 / EDT-FR-33: the frontend's answer to a held quit. `proceed` is
/// true once its pending Editor changes are written, false when a write was
/// blocked and the quit is cancelled (EDT-FR-32). Either way the hold is
/// released, so a later quit is held and flushed again rather than slipping
/// through unflushed.
#[project_scope_macros::project_scoped]
#[tauri::command]
pub fn finish_exit<R: tauri::Runtime>(
    proceed: bool,
    window: tauri::Window<R>,
    app: tauri::AppHandle<R>,
    gate: tauri::State<'_, ExitGate>,
) {
    // PWN-FR-WVUO: every project window answers. A window that could not write
    // cancels the quit and comes to the front.
    if !finish_exit_for(window.label(), proceed, &app, &gate) {
        let _ = window.set_focus();
    }
}

/// The quit steps that act on the state of a project. They go through every open
/// project, each in the context of its project (PWN-FR-CTXT): the window that
/// answered last must not decide which project the steps reach.
pub(super) fn shut_down_projects<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    // `../core/GRD-graduation.md` GRD-FR-TWMA: a graduation the loop is
    // holding rests as `interrupted` before the process goes, so the run is
    // something to continue on the next launch rather than something to
    // notice missing. Written while the store is still reachable.
    crate::project_scope::for_each_project(app, || {
        crate::graduation::interrupt_running(
            app,
            crate::graduation::GraduationInterruptionReason::ApplicationShutdown,
        );
    });
    // `../core/DAS-draft-assets.md` DAS-FR-20: a housekeeping pass still
    // running is given the chance to finish and is then abandoned where it
    // stands. Nothing reports a completed sweep for it: the pass leaves
    // every asset it had not yet decided untouched and records itself
    // incomplete, and the next trigger simply runs it again.
    crate::project_scope::for_each_project(app, || crate::draft_assets::begin_shutdown(app));
}

/// The part of `finish_exit` that does not need a window. Returns `false` when
/// the quit is cancelled, so the caller can bring the window to the front.
pub(super) fn finish_exit_for<R: tauri::Runtime>(
    label: &str,
    proceed: bool,
    app: &tauri::AppHandle<R>,
    gate: &ExitGate,
) -> bool {
    let answer = gate.answer(label, proceed);
    if answer == ExitAnswer::Wait {
        return true;
    }
    if exit_decision(answer == ExitAnswer::Quit, gate) {
        shut_down_projects(app);
        // EAC-FR-ABHG: the runs are stopped, so every sandbox VM stops before
        // the process goes.
        crate::tools::agent_exec::sandbox::stop_all_before_exit(app);
        app.exit(0);
        return true;
    }
    answer == ExitAnswer::Quit
}

/// Whether a quit request should be held, given the exit code it carries and
/// the gate's state. The pure half of `begin_exit`, so the policy is testable
/// without a Tauri runtime.
pub(super) fn begin_exit_decision(
    code: Option<i32>,
    gate: &ExitGate,
    now: Instant,
    grace: Duration,
) -> bool {
    code.is_none() && gate.claim_at(now, grace)
}

/// The pure half of `finish_exit`: release the hold and report whether the
/// application should now quit. Separated so "released either way" and the
/// cancel path are unit-testable without a Tauri runtime.
pub(super) fn exit_decision(proceed: bool, gate: &ExitGate) -> bool {
    gate.release();
    proceed
}
