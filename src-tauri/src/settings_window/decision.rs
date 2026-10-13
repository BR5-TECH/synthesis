//! The pure decisions of the Settings window: what a request does, which
//! section address may be presented, and which size the window opens at
//! (`../../specifications/ui/SWN-settings-windows.md`).
//!
//! One part of `settings_window.rs`, which re-exports every item here. Nothing
//! in it touches a window, so a unit test can state a situation and read the
//! answer.

use super::{DEFAULT_WINDOW_SIZE, MIN_WINDOW_SIZE, SETTINGS_WINDOW_LABEL};

/// The two scopes a section address names (SWN-FR-HZWD).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsScope {
    Global,
    Project,
}

/// The scope a section address names, or `None` when it names none.
///
/// An address is `<scope>/<section>`, with an optional `:<item>` after the
/// section (`project/graduation`, `global/agents:new`). The scope is whatever
/// stands before the first `/`, so an item that carries a `/` of its own cannot
/// be mistaken for one.
pub fn address_scope(address: &str) -> Option<SettingsScope> {
    match address.split_once('/')?.0 {
        "global" => Some(SettingsScope::Global),
        "project" => Some(SettingsScope::Project),
        _ => None,
    }
}

/// What happens once the open window's save sweep has succeeded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Transition {
    /// SWN-FR-08: the author closed the window and nothing follows it.
    Close,
    /// SWN-FR-19: the application is quitting, and the window's pending changes
    /// are written first.
    Quit,
}

/// The sweep this side is waiting on, and what it is waiting to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingSweep {
    /// The window being swept. Kept so a stale answer from a window that is no
    /// longer the one under sweep cannot perform someone else's transition.
    pub label: String,
    pub next: Transition,
}

/// What a request should make happen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// SWN-FR-12: a sweep is running, so the request starts no further save,
    /// duplicates no write already in flight, and opens no second window.
    Inert,
    /// SWN-FR-05 / SWN-FR-06: the window is already open — focus it, and route
    /// it to the named section if the request carried one.
    Focus { section: Option<String> },
    /// The window is not open: create it.
    Create { section: Option<String> },
    /// Ask the open window to save what it holds, then perform `next`.
    Sweep(PendingSweep),
}

/// A request made of this module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// The author asked for the window (SWN-FR-13).
    Open { section: Option<String> },
    /// The author asked to close the window.
    Close,
    /// The application was asked to quit (SWN-FR-19).
    Quit,
}

/// SWN-FR-05, SWN-FR-06 and SWN-FR-12, as one pure decision.
///
/// `open` is whether the window is on screen; `sweeping` is whether a save sweep
/// is already running. Everything the module does to the window is one of the
/// four [`Action`]s this returns, so the rules are pinned by unit tests that
/// state a situation rather than by reading the I/O that follows.
pub fn decide(request: Request, open: bool, sweeping: bool) -> Action {
    // SWN-FR-12: while a sweep runs, a second close and a request for the
    // window are each inert. The exception is a quit, which is not a *further*
    // save but a stronger reason for the one already running: it upgrades what
    // the sweep in flight will do rather than starting anything (handled by the
    // caller, which rewrites `next`).
    if sweeping {
        return Action::Inert;
    }
    match request {
        Request::Open { section } if open => Action::Focus { section },
        Request::Open { section } => Action::Create { section },
        Request::Close if open => Action::Sweep(PendingSweep {
            label: SETTINGS_WINDOW_LABEL.to_string(),
            next: Transition::Close,
        }),
        Request::Quit if open => Action::Sweep(PendingSweep {
            label: SETTINGS_WINDOW_LABEL.to_string(),
            next: Transition::Quit,
        }),
        // A close or a quit with nothing open is somebody else's event; the
        // quit is the exit path's own.
        Request::Close | Request::Quit => Action::Inert,
    }
}

/// SWN-FR-16: whether a request may proceed at all, given whether a project is
/// open.
///
/// While no project is open, **no route anywhere** presents a Project section —
/// the Project group is absent from the window, but an address a notification
/// carries or a call from a frontend that has not caught up could still name
/// one. The rule therefore sits where every route meets rather than only in the
/// frontend. A request that names no section, or a Global one, is always
/// allowed (SWN-FR-15), and so is a close or a quit.
pub fn route_allowed(request: &Request, project_open: bool) -> bool {
    match request {
        Request::Open { section: Some(s) } => {
            address_scope(s) != Some(SettingsScope::Project) || project_open
        }
        _ => true,
    }
}

/// SWN-FR-CLMP: the outer size, in logical pixels, the window opens at.
///
/// `stored` is the size the author last gave the window and `work_area` the
/// logical size of the work area of the display it opens on. A size nobody
/// stored is the default. A stored size is clamped to the work area and never
/// goes below the minimum; when the work area cannot hold the minimum at all,
/// the window falls back to the default size. An unknown work area clamps
/// nothing, since there is nothing to clamp against.
pub fn opening_size(stored: Option<(u32, u32)>, work_area: Option<(u32, u32)>) -> (u32, u32) {
    let Some((width, height)) = stored else {
        return DEFAULT_WINDOW_SIZE;
    };
    let Some((work_w, work_h)) = work_area else {
        return (width.max(MIN_WINDOW_SIZE.0), height.max(MIN_WINDOW_SIZE.1));
    };
    if work_w < MIN_WINDOW_SIZE.0 || work_h < MIN_WINDOW_SIZE.1 {
        return DEFAULT_WINDOW_SIZE;
    }
    (width.clamp(MIN_WINDOW_SIZE.0, work_w), height.clamp(MIN_WINDOW_SIZE.1, work_h))
}

/// SWN-FR-11: what a failed sweep leaves behind.
///
/// The window stays, the transition does not happen, and — where the failure
/// arrived on the way to a quit — the held exit is released so the application
/// does not sit wedged waiting for an answer that is no longer coming.
pub fn releases_exit_gate(next: &Transition) -> bool {
    matches!(next, Transition::Quit)
}

/// SWN-FR-03: the inner size that lands the window on `target` **outer** pixels,
/// given the chrome the platform draws around it.
///
/// Pure, because the correction is the whole of what makes "800 × 600" the size
/// the author can measure rather than the size of the web view inside a title
/// bar. Never returns zero in either axis: a decoration larger than the target
/// would otherwise produce a window with no content at all.
pub fn inner_size_for_outer(target: (u32, u32), decoration: (u32, u32)) -> (u32, u32) {
    (target.0.saturating_sub(decoration.0).max(1), target.1.saturating_sub(decoration.1).max(1))
}

/// The physical geometry the window is given as it opens (SWN-FR-03,
/// SWN-FR-04).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    /// The inner size to set, so that the outer size is `outer`.
    pub inner: (u32, u32),
    /// The smallest inner size the author can drag the window to.
    pub min_inner: (u32, u32),
    /// The outer size the window has once `inner` is applied.
    pub outer: (u32, u32),
    /// The top-left point that centres `outer` in the work area, or `None`
    /// when the work area is unknown.
    pub position: Option<(i32, i32)>,
}

/// SWN-FR-03, SWN-FR-04: the geometry that brings the window to `target` outer
/// logical pixels, centred in `work` (physical pixels).
///
/// The position is computed from the outer size the window is given, never
/// from a size read back from the window. On macOS a resize is applied after
/// the call that asks for it returns, so a read-back still gives the size the
/// window was built with, and the window would be centred at that size.
pub fn placement(
    target: (u32, u32),
    scale: f64,
    decoration: (u32, u32),
    work: Option<crate::window::DisplayRect>,
) -> Placement {
    let physical = |logical: (u32, u32)| {
        ((logical.0 as f64 * scale).round() as u32, (logical.1 as f64 * scale).round() as u32)
    };
    let inner = inner_size_for_outer(physical(target), decoration);
    let min_inner = inner_size_for_outer(physical(MIN_WINDOW_SIZE), decoration);
    let outer = (inner.0.saturating_add(decoration.0), inner.1.saturating_add(decoration.1));
    let position = work.map(|w| crate::window::center_window_on_display(w, outer));
    Placement { inner, min_inner, outer, position }
}

/// SWN-FR-MRBK: a physical size in logical pixels, the measure a close
/// compares against the size the window was placed at.
pub fn logical_size(physical: (u32, u32), scale: f64) -> (u32, u32) {
    ((physical.0 as f64 / scale).round() as u32, (physical.1 as f64 / scale).round() as u32)
}
