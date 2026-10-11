//! The Settings child window (`../../specifications/ui/SWN-settings-windows.md`).
//!
//! Global settings and Project settings are the two groups of **one native child
//! window** of the application, not tabs of the main viewport (SWN-FR-01). This
//! module owns that window as a window: how it is created, how large it is,
//! where it is placed, what it blocks, and how it is torn down. What the window
//! presents inside its frame is the frontend's, and is specified by
//! `GLS-global-settings.md` and `SET-project-settings.md`.
//!
//! Everything the author can ask for goes through one decision — [`decide`] —
//! whose whole input is the request, whether the window is open, and whether a
//! save sweep is already running. That keeps the one-window rule (SWN-FR-05),
//! the focus-instead-of-duplicate rule (SWN-FR-06), and the inertness of a
//! request made mid-sweep (SWN-FR-12) in a pure function a unit test can state
//! situations against, rather than spread across the I/O that performs them.
//!
//! **The sweep is the frontend's.** Only the window holds its sections' pending
//! changes, so this side never writes any of them: it asks the window to save
//! what it holds (`settings-window:save-and-close`) and waits for the answer
//! (`finish_settings_close`). A `true` answer performs the transition the
//! request recorded; a `false` answer cancels it and leaves the window exactly
//! where it was (SWN-FR-11). The one thing this side does write is the size the
//! author gave the window, once the window has closed (SWN-FR-MRBK).

use std::sync::Mutex;

use serde::Serialize;
use tauri::{Emitter, Manager};

use crate::log_fields;
use crate::logging::{self, Domain, BUFFER};

mod decision;
pub use decision::*;

/// The domains every record here carries. The settings window is a surface of
/// the application driven from this side, so it is `Backend` alone — nothing
/// about it is remote and nothing about it is AI.
const DOMAINS: [Domain; 1] = [Domain::Backend];

/// Label of the window the Settings window is a child of.
///
/// One label rather than two, because the application declares exactly one
/// ordinary window: the Project picker and the main shell are the same window
/// in two states (`window.rs`), which is precisely what SWN-FR-02 describes when
/// it names the main window and the picker as the two possible parents.
pub const PARENT_WINDOW_LABEL: &str = "main";

/// Label of the Settings child window.
pub const SETTINGS_WINDOW_LABEL: &str = "settings";
/// SWN-FR-17: the window's title.
pub const SETTINGS_WINDOW_TITLE: &str = "Settings";

/// SWN-FR-03: the outer size, in logical pixels, the window opens at when the
/// author never resized it.
pub const DEFAULT_WINDOW_SIZE: (u32, u32) = (800, 600);
/// SWN-FR-03: the smallest outer size, in logical pixels, the author can give
/// the window.
pub const MIN_WINDOW_SIZE: (u32, u32) = (800, 600);

/// Event asking the open window to save every pending change in every one of
/// its sections and report whether it may now go (SWN-FR-08, SWN-FR-09).
pub const EVENT_SAVE_AND_CLOSE: &str = "settings-window:save-and-close";
/// Event routing an already-open window to a named section (SWN-FR-06,
/// SWN-FR-HZWD).
pub const EVENT_ROUTE: &str = "settings-window:route";
/// Event announcing whether the window is open. The main window needs it for
/// the already-looking-at suppression of `NTF-notifications.md` NTF-FR-08 and
/// for returning focus to the control that invoked the window (SWN-FR-FCRT).
pub const EVENT_CHANGED: &str = "settings-window:changed";
/// Event announcing that the window took or lost OS focus, so the main window
/// can tell "no window of the application holds focus" from "another one of
/// ours does" (NTF-FR-08).
pub const EVENT_FOCUS: &str = "settings-window:focus";
/// Event telling the window to present the section whose save failed
/// (SWN-FR-11).
pub const EVENT_PRESENT_FAILURE: &str = "settings-window:present-failure";

/// Give back the exit hold a quit-sweep claimed, if this transition was one.
///
/// Called from every path a quit-sweep can end on without quitting — a reported
/// failure, a window destroyed under it, a sweep that could not be requested.
/// A hold is released only by an answer, so a path that forgets this leaves an
/// application that can never be quit.
fn release_exit_hold<R: tauri::Runtime>(app: &tauri::AppHandle<R>, next: &Transition) {
    if !releases_exit_gate(next) {
        return;
    }
    if let Some(gate) = app.try_state::<crate::menu::ExitGate>() {
        gate.release();
    }
}

/// The sweep in flight, if any, and the size the window was last placed at.
/// Managed state.
#[derive(Default)]
pub struct SettingsWindows {
    pending: Mutex<Option<PendingSweep>>,
    /// The outer size, in logical pixels, the window had once it was placed. A
    /// close compares the window against it to learn whether the author
    /// resized it (SWN-FR-MRBK).
    placed: Mutex<Option<(u32, u32)>>,
}

impl SettingsWindows {
    fn peek(&self) -> Option<PendingSweep> {
        self.pending.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    fn set(&self, sweep: Option<PendingSweep>) {
        *self.pending.lock().unwrap_or_else(|p| p.into_inner()) = sweep;
    }

    /// SWN-FR-19: a quit arriving while a sweep is already running does not
    /// start a second one — it upgrades what the running one will do when it
    /// lands. Returns whether there was a sweep to upgrade.
    fn upgrade_to_quit(&self) -> bool {
        let mut slot = self.pending.lock().unwrap_or_else(|p| p.into_inner());
        match slot.as_mut() {
            Some(sweep) => {
                sweep.next = Transition::Quit;
                true
            }
            None => false,
        }
    }

    /// Take the sweep, if the answer came from the window it was waiting on.
    fn take_for(&self, label: &str) -> Option<PendingSweep> {
        let mut slot = self.pending.lock().unwrap_or_else(|p| p.into_inner());
        match slot.as_ref() {
            Some(sweep) if sweep.label == label => slot.take(),
            _ => None,
        }
    }

    fn set_placed(&self, size: Option<(u32, u32)>) {
        *self.placed.lock().unwrap_or_else(|p| p.into_inner()) = size;
    }

    fn placed(&self) -> Option<(u32, u32)> {
        *self.placed.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// Whether the window exists right now.
fn is_open<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
    app.get_webview_window(SETTINGS_WINDOW_LABEL).is_some()
}

/// SWN-FR-02: the parent renders normally but takes no interaction while a
/// settings window is open, and takes it again the moment one closes. Enforced
/// by the platform's own window relationship — the parent is *blocked*, not
/// covered by a scrim — so a parent that is repainted, resized, or moved is
/// blocked throughout.
///
/// macOS does not have a disabled window, so the block is its own mechanism
/// there and this function is written twice. See `macos_modality` for what the
/// platform gives instead, and why Tauri's own `set_enabled` cannot be used.
#[cfg(not(target_os = "macos"))]
fn set_parent_enabled<R: tauri::Runtime>(app: &tauri::AppHandle<R>, enabled: bool) {
    let Some(parent) = app.get_webview_window(PARENT_WINDOW_LABEL) else {
        return;
    };
    if let Err(e) = parent.set_enabled(enabled) {
        logging::log_warn(
            app,
            &BUFFER,
            &DOMAINS,
            "settings window could not change parent interactivity",
            log_fields! { "enabled" => enabled, "reason" => e.to_string() },
        );
    }
}

/// SWN-FR-02 on macOS, where the block is a sheet rather than a disabled
/// window. See `macos_modality` for what the platform gives and what it costs.
#[cfg(target_os = "macos")]
fn set_parent_enabled<R: tauri::Runtime>(app: &tauri::AppHandle<R>, enabled: bool) {
    let Some(parent) = app.get_webview_window(PARENT_WINDOW_LABEL) else {
        return;
    };
    // AppKit is main-thread-only and a command runs on a worker thread, so the
    // block is posted rather than performed here. The post also fixes the order
    // it happens in: `create` blocks on building the settings window, which is
    // served by the same event loop, so a block posted first is in place before
    // that window exists rather than some frames after it.
    let handle = app.clone();
    let post = app.run_on_main_thread(move || {
        // The window went between the request and the main thread.
        let Ok(ns_window) = parent.ns_window() else {
            return;
        };
        let report = unsafe { macos_modality::set_window_blocked(ns_window, !enabled) };
        // SWN-FR-02 is invisible when it works and indistinguishable from a bug
        // when it does not, so each end of it is on the record with what it
        // actually installed and what it caught.
        logging::log_info(
            &handle,
            &BUFFER,
            &DOMAINS,
            if enabled {
                "settings window released the parent's input"
            } else {
                "settings window blocked the parent's input"
            },
            log_fields! {
                "monitor" => report.monitor,
                "overlay" => report.overlay,
                "dropped" => report.dropped
            },
        );
    });
    if let Err(e) = post {
        logging::log_warn(
            app,
            &BUFFER,
            &DOMAINS,
            "settings window could not change parent interactivity",
            log_fields! { "enabled" => enabled, "reason" => e.to_string() },
        );
    }
}

/// The macOS half of SWN-FR-02.
///
/// macOS has no enabled/disabled window the way Windows and GTK do, so the
/// block is built here from two pieces that cover each other:
///
/// * **A view over the parent's content.** A plain, empty `NSView` the size of
///   the content view, added last so it is the topmost one. AppKit hit-tests by
///   geometry, so every click, drag, and scroll over the parent lands on this
///   view; a view's own default is to hand the event up to its superview rather
///   than down to the webview beside it, so nothing in the parent ever sees it.
///   The window still takes the click, so the pointer never falls through to
///   whatever application is behind — the parent stops answering, it does not
///   become a hole. The view draws nothing at all.
/// * **A monitor on the application's event stream.** It drops every event
///   addressed to the parent window, which is what reaches the parts a view
///   cannot: the title bar, and any key press while the parent holds focus.
///
/// The parent's first responder is cleared as well, so a webview that held the
/// keyboard gives it up, and it is given back when the block lifts.
///
/// **Why not a sheet.** AppKit's own "this window is blocked" mechanism is the
/// sheet, and that is what Tauri's `set_enabled(false)` uses — at half opacity
/// over the parent's whole frame. Two things make it unusable here. It paints
/// the parent in a grey wash, against SWN-FR-02's "the parent renders normally";
/// and macOS additionally dims the host of an attached sheet on its own, which
/// reaches the settings window too, because a child window is composited with
/// the window it belongs to. An invisible sheet does not avoid the second one —
/// the dimming is the platform's, not the sheet's.
///
/// Everything installed here is taken down the moment the block lifts, and only
/// one of each is ever installed, so nothing can outlive the settings window
/// that asked for it and leave the parent deaf.
#[cfg(target_os = "macos")]
mod macos_modality;

/// Announce whether the window is open to every webview.
fn announce<R: tauri::Runtime>(app: &tauri::AppHandle<R>, open: bool) {
    let _ = app.emit(EVENT_CHANGED, serde_json::json!({ "open": open }));
}

/// Percent-encode a query-string value.
///
/// The section a request names travels in the window's boot URL (`create`), and
/// a section address can carry an item — an agent's id (`AGT-agents.md`
/// AGT-FR-06) — which this side does not get to choose the shape of. Anything
/// outside the unreserved set is escaped, so an id holding a `&`, a `#`, or a
/// space cannot truncate the query or invent a second parameter.
pub fn encode_query_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// The outer size of `window` in logical pixels, or `None` when the platform
/// cannot report it.
fn logical_outer_size<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) -> Option<(u32, u32)> {
    let outer = window.outer_size().ok()?;
    let scale = window.scale_factor().ok()?;
    Some((
        (outer.width as f64 / scale).round() as u32,
        (outer.height as f64 / scale).round() as u32,
    ))
}

/// The size the author last gave the window, read from the user-global store.
fn stored_size<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<(u32, u32)> {
    let store = app.try_state::<crate::global_settings::GlobalSettingsStore>()?;
    match store.load_settings_window_size() {
        Ok(size) => size.filter(|s| s.width > 0 && s.height > 0).map(|s| (s.width, s.height)),
        Err(e) => {
            logging::log_warn(
                app,
                &BUFFER,
                &DOMAINS,
                "settings window size could not be read",
                log_fields! { "reason" => e },
            );
            None
        }
    }
}

/// SWN-FR-MRBK: remember the size the author gave the window, when it differs
/// from the one the window was placed at. Called as the window goes, so one
/// write covers however many drags the author made.
fn remember_size<R: tauri::Runtime>(app: &tauri::AppHandle<R>, window: &tauri::WebviewWindow<R>) {
    let (Some(state), Some(store)) = (
        app.try_state::<SettingsWindows>(),
        app.try_state::<crate::global_settings::GlobalSettingsStore>(),
    ) else {
        return;
    };
    let (Some(placed), Some(now)) = (state.placed(), logical_outer_size(window)) else {
        return;
    };
    if placed == now {
        return;
    }
    match store.save_settings_window_size(now.0, now.1) {
        Ok(()) => logging::log_debug(
            app,
            &BUFFER,
            &DOMAINS,
            "settings window size stored",
            log_fields! { "width" => now.0, "height" => now.1 },
        ),
        Err(e) => logging::log_warn(
            app,
            &BUFFER,
            &DOMAINS,
            "settings window size could not be stored",
            log_fields! { "reason" => e },
        ),
    }
}

/// Where the window opens: the work area of the display the parent occupies, in
/// physical pixels, and that display's scale factor.
struct OpeningDisplay {
    work: crate::window::DisplayRect,
    scale: f64,
}

fn opening_display<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<OpeningDisplay> {
    let parent = app.get_webview_window(PARENT_WINDOW_LABEL)?;
    let (pos, size) = (parent.outer_position().ok()?, parent.outer_size().ok()?);
    let (work, scale) = crate::window::work_area_for_rect(
        app,
        crate::window::DisplayRect { x: pos.x, y: pos.y, width: size.width, height: size.height },
    )?;
    Some(OpeningDisplay { work, scale })
}

/// SWN-FR-03, SWN-FR-04, SWN-FR-CLMP: bring the window to the size it opens at
/// and into the centre of the display its parent occupies.
///
/// Best-effort: a platform that cannot report one of the measurements leaves the
/// window at the size it was built with, which is the same size minus the
/// chrome — wrong by a title bar rather than unusable. The window is placed
/// while still hidden, so it is never seen at another size or position first.
/// The position is never stored, so a window the author dragged elsewhere opens
/// centred again.
fn place<R: tauri::Runtime>(app: &tauri::AppHandle<R>, window: &tauri::WebviewWindow<R>) {
    let display = opening_display(app);
    let scale =
        display.as_ref().map(|d| d.scale).or_else(|| window.scale_factor().ok()).unwrap_or(1.0);
    let work_logical = display.as_ref().map(|d| {
        (
            (d.work.width as f64 / scale).floor() as u32,
            (d.work.height as f64 / scale).floor() as u32,
        )
    });
    let target = opening_size(stored_size(app), work_logical);
    if let (Ok(outer), Ok(inner)) = (window.outer_size(), window.inner_size()) {
        let decoration =
            (outer.width.saturating_sub(inner.width), outer.height.saturating_sub(inner.height));
        let physical = |logical: (u32, u32)| {
            ((logical.0 as f64 * scale).round() as u32, (logical.1 as f64 * scale).round() as u32)
        };
        let (w, h) = inner_size_for_outer(physical(target), decoration);
        let (min_w, min_h) = inner_size_for_outer(physical(MIN_WINDOW_SIZE), decoration);
        if let Err(e) = window.set_min_size(Some(tauri::PhysicalSize::new(min_w, min_h))) {
            logging::log_warn(
                app,
                &BUFFER,
                &DOMAINS,
                "settings window could not be given its minimum size",
                log_fields! { "reason" => e.to_string() },
            );
        }
        if let Err(e) = window.set_size(tauri::PhysicalSize::new(w, h)) {
            logging::log_warn(
                app,
                &BUFFER,
                &DOMAINS,
                "settings window could not be sized",
                log_fields! { "reason" => e.to_string() },
            );
        }
    }
    let (Some(display), Ok(own)) = (display, window.outer_size()) else {
        return;
    };
    // Centred in the work area rather than on the whole display, so a window
    // as tall as the work area does not slide under a menu bar or a taskbar.
    let (x, y) = crate::window::center_window_on_display(display.work, (own.width, own.height));
    if let Err(e) = window.set_position(tauri::PhysicalPosition::new(x, y)) {
        logging::log_warn(
            app,
            &BUFFER,
            &DOMAINS,
            "settings window could not be centred",
            log_fields! { "reason" => e.to_string() },
        );
    }
}

/// Build and show the window (SWN-FR-01 through SWN-FR-04).
///
/// The window is built hidden and shown only once it holds its final size in
/// the centre of the parent's display, so it is never seen at another size or
/// another position first.
fn create<R: tauri::Runtime>(app: &tauri::AppHandle<R>, section: Option<String>) {
    let url = match &section {
        Some(s) => format!("index.html?settings=1&section={}", encode_query_value(s)),
        None => "index.html?settings=1".to_string(),
    };
    // Built through a factory rather than a `mut` binding: `parent` consumes
    // the builder and gives it back only on success, so a platform that refuses
    // the relationship would otherwise leave nothing to build from.
    let base = || {
        tauri::WebviewWindowBuilder::new(
            app,
            SETTINGS_WINDOW_LABEL,
            tauri::WebviewUrl::App(url.clone().into()),
        )
        .title(SETTINGS_WINDOW_TITLE)
        .inner_size(DEFAULT_WINDOW_SIZE.0 as f64, DEFAULT_WINDOW_SIZE.1 as f64)
        // SWN-FR-03: the author can drag-resize the window, but it offers no
        // maximize. Disabling the maximize control also disables the macOS zoom
        // button, which is what keeps the window out of OS full-screen. The
        // minimum size is set in `place`, once the chrome can be measured.
        .resizable(true)
        .maximizable(false)
        .visible(false)
    };
    let builder = match app.get_webview_window(PARENT_WINDOW_LABEL) {
        Some(parent) => match base().parent(&parent) {
            Ok(b) => b,
            Err(e) => {
                logging::log_warn(
                    app,
                    &BUFFER,
                    &DOMAINS,
                    "settings window could not be parented",
                    log_fields! { "reason" => e.to_string() },
                );
                base()
            }
        },
        None => base(),
    };
    // Blocked before the window is built, not after: building a webview window
    // takes long enough to click through, and SWN-FR-02 gives the parent no
    // interaction from the moment the window is on its way.
    set_parent_enabled(app, false);
    let window = match builder.build() {
        Ok(w) => w,
        Err(e) => {
            logging::log_error(
                app,
                &BUFFER,
                &DOMAINS,
                "settings window could not be opened",
                log_fields! { "reason" => e.to_string() },
            );
            // Nothing opened, so nothing may go on blocking the parent.
            set_parent_enabled(app, true);
            return;
        }
    };
    place(app, &window);
    let _ = window.show();
    let _ = window.set_focus();
    // Measured once the window is on screen, so the size a close compares
    // against is the one the platform really gave it, frame included.
    let placed = logical_outer_size(&window);
    if let Some(state) = app.try_state::<SettingsWindows>() {
        state.set_placed(placed);
    }
    announce(app, true);
    logging::log_info(
        app,
        &BUFFER,
        &DOMAINS,
        "settings window opened",
        log_fields! {
            "width" => placed.map(|s| s.0),
            "height" => placed.map(|s| s.1),
            // The section is a fixed vocabulary of surface names, never author
            // content, so it is safe to record and is what explains where the
            // author landed.
            "section" => section,
        },
    );
}

/// SWN-FR-05 / SWN-FR-06: focus the window already open and route it to a named
/// section.
fn focus<R: tauri::Runtime>(app: &tauri::AppHandle<R>, section: Option<String>) {
    let Some(window) = app.get_webview_window(SETTINGS_WINDOW_LABEL) else {
        return;
    };
    let _ = window.set_focus();
    // Not reloaded, so its sections keep their state; a request naming a
    // section presents that section in the window it focuses.
    let _ = window.emit(EVENT_ROUTE, serde_json::json!({ "section": section }));
    logging::log_debug(
        app,
        &BUFFER,
        &DOMAINS,
        "settings window focused",
        log_fields! { "section" => section },
    );
}

/// SWN-FR-08: ask the open window to write every pending change it holds, and
/// remember what to do when it answers.
fn begin_sweep<R: tauri::Runtime>(app: &tauri::AppHandle<R>, sweep: PendingSweep) {
    let Some(state) = app.try_state::<SettingsWindows>() else {
        return;
    };
    let Some(window) = app.get_webview_window(&sweep.label) else {
        // Nothing to sweep — the window went between the decision and here.
        // Perform the transition directly so a quit does not strand.
        perform(app, sweep.next);
        return;
    };
    state.set(Some(sweep.clone()));
    if window.emit(EVENT_SAVE_AND_CLOSE, ()).is_err() {
        // No webview to answer means no sweep will ever land. Give the claim
        // back rather than leaving every later request inert (SWN-FR-12), and
        // the exit hold with it (SWN-FR-19).
        state.set(None);
        release_exit_hold(app, &sweep.next);
        logging::log_warn(
            app,
            &BUFFER,
            &DOMAINS,
            "settings window could not be asked to save before closing",
            log_fields! {
                "window" => sweep.label.clone(),
                "cancelled" => transition_name(&sweep.next),
            },
        );
        return;
    }
    logging::log_debug(
        app,
        &BUFFER,
        &DOMAINS,
        "settings window save sweep requested",
        log_fields! {
            "window" => sweep.label.clone(),
            "next" => transition_name(&sweep.next),
        },
    );
}

/// A short, stable name for a transition, for the log's structured fields.
fn transition_name(next: &Transition) -> &'static str {
    match next {
        Transition::Close => "close",
        Transition::Quit => "quit",
    }
}

/// Tear the window down and give the parent its interaction back.
fn destroy<R: tauri::Runtime>(app: &tauri::AppHandle<R>, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        remember_size(app, &window);
        // `destroy` rather than `close`: `close` raises `CloseRequested`, which
        // this module intercepts to start a sweep — and the sweep has just
        // finished, so that would ask the window to save all over again.
        let _ = window.destroy();
    }
}

/// Perform the transition a completed sweep earned (SWN-FR-08, SWN-FR-19).
fn perform<R: tauri::Runtime>(app: &tauri::AppHandle<R>, next: Transition) {
    set_parent_enabled(app, true);
    announce(app, false);
    match next {
        Transition::Close => {
            if let Some(parent) = app.get_webview_window(PARENT_WINDOW_LABEL) {
                let _ = parent.set_focus();
            }
        }
        Transition::Quit => crate::menu::resume_held_exit(app),
    }
}

/// Route a request through [`decide`] and perform whatever it returns.
fn dispatch<R: tauri::Runtime>(app: &tauri::AppHandle<R>, request: Request) {
    // Every path below needs the sweep state. A handle that does not carry it
    // manages no settings window either, so there is nothing to decide.
    let Some(state) = app.try_state::<SettingsWindows>() else {
        return;
    };
    // SWN-FR-16: while no project is open, **no route anywhere** presents a
    // Project section. The Project group is absent from the window, but an
    // address a notification carries or a stale frontend call could still name
    // one, so the rule is enforced where every route meets.
    let project_open =
        app.try_state::<crate::project::ProjectState>().and_then(|p| p.root()).is_some();
    if !route_allowed(&request, project_open) {
        logging::log_warn(
            app,
            &BUFFER,
            &DOMAINS,
            "a Project settings section was requested with no project open",
            log_fields! {},
        );
        return;
    }
    let sweeping = state.peek().is_some();
    match decide(request, is_open(app), sweeping) {
        Action::Inert => {}
        Action::Focus { section } => focus(app, section),
        Action::Create { section } => create(app, section),
        Action::Sweep(sweep) => begin_sweep(app, sweep),
    }
}

/// Whether the window is on screen (SWN-FR-05).
pub fn a_settings_window_is_open<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
    is_open(app)
}

/// SWN-FR-13: the in-process entry point the application menu opens the window
/// through. Same rules as the command the frontend calls.
pub fn request_open<R: tauri::Runtime>(app: &tauri::AppHandle<R>, section: Option<String>) {
    dispatch(app, Request::Open { section });
}

/// SWN-FR-13: open the window, or focus it when it is already open (SWN-FR-05,
/// SWN-FR-06).
///
/// `section` is a section address naming its scope and its section
/// (SWN-FR-HZWD) — the Agents section the chrome roster routes to
/// (`AGT-agents.md` AGT-FR-06), the GitHub section a failed Git operation
/// offers, the section a notification's address carries. `None` opens the
/// window on Appearance (SWN-FR-MRNB).
#[tauri::command]
pub fn open_settings_window<R: tauri::Runtime>(section: Option<String>, app: tauri::AppHandle<R>) {
    dispatch(&app, Request::Open { section });
}

/// What the Settings window needs to know about the project behind it.
///
/// The window is its own webview, so it shares no state with the main window
/// and cannot read the open project off it. Every field is fixed for the
/// window's whole life — the parent is blocked, so neither the project nor its
/// active worktree can change while it is open (SWN-FR-02, `SET-FR-20`) —
/// which is what makes reading it once on mount correct.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsWindowContext {
    /// The project's display name, or empty when no project is open.
    pub project_name: String,
    /// The project's identity anchor, which is the key an address names
    /// (`NTF-notifications.md` NTF-FR-04). Empty when no project is open.
    pub project_key: String,
    /// The active worktree's directory. Empty when no project is open.
    pub content_root: String,
}

/// The open project behind the Settings window, for the surfaces inside it that
/// need to name it — the GitHub token picker's title (`GHA-FR-20`), the address
/// the Notifications rehearsal mints (`GLS-FR-27`), and the presence of the
/// Project group itself (SWN-FR-16).
///
/// Answers with empty strings rather than an error while no project is open:
/// the window is reachable in exactly that state (SWN-FR-15), and the window
/// then shows its Global group alone.
#[tauri::command]
pub fn get_settings_window_context<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> SettingsWindowContext {
    let project = app.state::<crate::project::ProjectState>();
    let project_key = project.anchor().unwrap_or_default();
    SettingsWindowContext {
        project_name: if project_key.is_empty() {
            String::new()
        } else {
            crate::project::basename(&project_key)
        },
        content_root: project.root().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
        project_key,
    }
}

/// The frontend's answer to a save sweep (SWN-FR-08 through SWN-FR-12).
///
/// `ok` is true once every pending change in every section has been written —
/// including a write that was already in flight, which the sweep awaits rather
/// than starting again (SWN-FR-09) — and false when one of them failed. A
/// failure cancels the close outright: the window stays open and a held quit is
/// released (SWN-FR-11, SWN-FR-19). `section` names the section address that
/// failed, so the window can present it.
#[tauri::command]
pub fn finish_settings_close<R: tauri::Runtime>(
    ok: bool,
    section: Option<String>,
    window: tauri::Window<R>,
    app: tauri::AppHandle<R>,
) {
    let label = window.label().to_string();
    let Some(sweep) = app.try_state::<SettingsWindows>().and_then(|s| s.take_for(&label)) else {
        // An answer nobody is waiting for — a duplicate, or one from a window
        // whose sweep was already settled. Ignoring it is what keeps a second
        // close request from performing the transition twice (SWN-FR-12).
        return;
    };
    if ok {
        destroy(&app, &label);
        perform(&app, sweep.next);
        logging::log_info(
            &app,
            &BUFFER,
            &DOMAINS,
            "settings window closed after saving its pending changes",
            log_fields! { "window" => label },
        );
        return;
    }
    // SWN-FR-11: the window stays open with its failed section presented, and
    // nothing else about the request happens.
    release_exit_hold(&app, &sweep.next);
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.emit(EVENT_PRESENT_FAILURE, serde_json::json!({ "section": section }));
        let _ = w.set_focus();
    }
    logging::log_warn(
        &app,
        &BUFFER,
        &DOMAINS,
        "settings window stayed open because a section save failed",
        log_fields! {
            "window" => label,
            "section" => section,
            "cancelled" => transition_name(&sweep.next),
        },
    );
}

/// SWN-FR-19: hold a quit while the open window writes what it holds.
///
/// Returns whether the quit was taken over here. `false` means the window is not
/// open and the ordinary exit path should proceed.
pub fn defer_quit_for_settings<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
    // Asked before the managed state is touched at all, so the ordinary quit
    // path costs nothing and needs nothing managed on a handle that never opens
    // a settings window.
    if !is_open(app) {
        return false;
    }
    // A quit arriving mid-sweep does not start a second one: it upgrades what
    // the running sweep will do when it lands (SWN-FR-12).
    let Some(state) = app.try_state::<SettingsWindows>() else {
        return false;
    };
    if state.upgrade_to_quit() {
        return true;
    }
    dispatch(app, Request::Quit);
    // Taken over either way. A sweep that started will resume the quit when it
    // answers; a window that vanished between the decision and the request has
    // already had `perform(Quit)` resume it. Reporting `false` on that second
    // path would have the exit request emitted twice.
    true
}

/// Intercept what happens to the window itself.
///
/// A close the author requests — the title-bar button, ⌘W, the Window menu — is
/// **held**: the window's sections may hold pending changes, and SWN-FR-08
/// requires those written before it goes. A destruction that happens anyway
/// (the parent going, a platform teardown) releases the parent so the
/// application cannot be left permanently uninteractive.
///
/// Returns whether the event was ours, so the shell's own window handling only
/// runs for windows this module does not own.
pub fn handle_window_event<R: tauri::Runtime>(
    window: &tauri::Window<R>,
    event: &tauri::WindowEvent,
) -> bool {
    if window.label() != SETTINGS_WINDOW_LABEL {
        return false;
    }
    let app = window.app_handle();
    match event {
        tauri::WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            dispatch(app, Request::Close);
        }
        tauri::WindowEvent::Destroyed => {
            // A sweep this window was under can never land now, so give the
            // claim back.
            if let Some(state) = app.try_state::<SettingsWindows>() {
                if let Some(dropped) = state.take_for(window.label()) {
                    // SWN-FR-19: and a quit held for a sweep that will never
                    // answer must be released, or the application becomes
                    // unquittable — the gate is only ever given back by an
                    // answer, and no answer is coming from a window that is
                    // gone.
                    release_exit_hold(app, &dropped.next);
                }
            }
            // Whatever destroyed it, the parent must not stay disabled.
            set_parent_enabled(app, true);
            announce(app, false);
        }
        // NTF-FR-08: "no window of the application holds OS focus" is what
        // decides whether a raise is posted, and this window is one of ours.
        tauri::WindowEvent::Focused(focused) => {
            let _ = app.emit(EVENT_FOCUS, serde_json::json!({ "focused": *focused }));
        }
        _ => {}
    }
    true
}

#[cfg(test)]
mod tests;
