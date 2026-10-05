//! The two settings child windows (`../../specifications/ui/SWN-settings-windows.md`).
//!
//! Global settings and Project settings are **native child windows** of the
//! application rather than tabs of the main viewport (SWN-FR-01). This module
//! owns them as windows: how one is created, where it is placed, what it
//! blocks, how it is torn down, and the order a switch between the two runs in.
//! What either window presents inside its frame is the frontend's, and is
//! specified by `GLS-global-settings.md` and `SET-project-settings.md`.
//!
//! Everything the author can ask for goes through one decision — [`decide`] —
//! whose whole input is the request, which settings window is open, and whether
//! a save sweep is already running. That keeps the one-window rule (SWN-FR-05),
//! the focus-instead-of-duplicate rule (SWN-FR-06), the ordered save-close-open
//! of a switch (SWN-FR-07), and the inertness of a request made mid-sweep
//! (SWN-FR-12) in a pure function a unit test can state situations against,
//! rather than spread across the I/O that performs them.
//!
//! **The sweep is the frontend's.** Only the settings window holds its sections'
//! pending changes, so this side never writes anything: it asks that window to
//! save what it holds (`settings-window:save-and-close`) and waits for the
//! answer (`finish_settings_close`). A `true` answer performs the transition the
//! request recorded; a `false` answer cancels it and leaves the window exactly
//! where it was (SWN-FR-11).

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

use crate::log_fields;
use crate::logging::{self, Domain, BUFFER};

/// The domains every record here carries. A settings window is a surface of the
/// application driven from this side, so it is `Backend` alone — nothing about
/// it is remote and nothing about it is AI.
const DOMAINS: [Domain; 1] = [Domain::Backend];

/// Label of the window both settings windows are children of.
///
/// One label rather than two, because the application declares exactly one
/// ordinary window: the Project picker and the main shell are the same window
/// in two states (`window.rs`), which is precisely what SWN-FR-02 describes when
/// it names the main window and the picker as the two possible parents.
pub const PARENT_WINDOW_LABEL: &str = "main";

/// Label of the Global settings child window.
pub const GLOBAL_WINDOW_LABEL: &str = "settings-global";
/// Label of the Project settings child window.
pub const PROJECT_WINDOW_LABEL: &str = "settings-project";

/// SWN-FR-03: the fixed **outer** size, in logical pixels, both windows open at
/// every time. Not user-resizable, not maximizable, and not sent to OS
/// full-screen.
pub const SETTINGS_WINDOW_SIZE: (u32, u32) = (800, 600);

/// Event asking the open settings window to save every pending change in every
/// one of its sections and report whether it may now go (SWN-FR-08, SWN-FR-09).
pub const EVENT_SAVE_AND_CLOSE: &str = "settings-window:save-and-close";
/// Event routing an already-open settings window to a named section
/// (SWN-FR-06, SWN-FR-13).
pub const EVENT_ROUTE: &str = "settings-window:route";
/// Event announcing which settings window is open, or that none is. The main
/// window needs it for the already-looking-at suppression of
/// `NTF-notifications.md` NTF-FR-08.
pub const EVENT_CHANGED: &str = "settings-window:changed";
/// Event announcing that a settings window took or lost OS focus, so the main
/// window can tell "no window of the application holds focus" from "another one
/// of ours does" (NTF-FR-08).
pub const EVENT_FOCUS: &str = "settings-window:focus";
/// Event telling the failed section's own window to present it (SWN-FR-11).
pub const EVENT_PRESENT_FAILURE: &str = "settings-window:present-failure";

/// Which settings window a request names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SettingsWindowKind {
    Global,
    Project,
}

impl SettingsWindowKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Global => GLOBAL_WINDOW_LABEL,
            Self::Project => PROJECT_WINDOW_LABEL,
        }
    }

    /// SWN-FR-17: the window's own title, so the two are told apart in the
    /// platform's window list as well as on screen.
    pub fn title(self) -> &'static str {
        match self {
            Self::Global => "Global settings",
            Self::Project => "Project settings",
        }
    }

    /// The `?settings=` value the webview boots on, which is how one HTML entry
    /// point renders two different windows.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Project => "project",
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            GLOBAL_WINDOW_LABEL => Some(Self::Global),
            PROJECT_WINDOW_LABEL => Some(Self::Project),
            _ => None,
        }
    }
}

/// What happens once the open window's save sweep has succeeded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Transition {
    /// SWN-FR-08: the author closed the window and nothing follows it.
    Close,
    /// SWN-FR-07: the second half of a switch — the requested window opens only
    /// after the open one has closed.
    Open {
        kind: SettingsWindowKind,
        section: Option<String>,
    },
    /// SWN-FR-19: the application is quitting, and the settings window's pending
    /// changes are written first.
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
    /// SWN-FR-06: the window asked for is the one already open — focus it, and
    /// route it to the named section if the request carried one.
    Focus {
        kind: SettingsWindowKind,
        section: Option<String>,
    },
    /// No settings window is open: create the requested one.
    Create {
        kind: SettingsWindowKind,
        section: Option<String>,
    },
    /// Ask the open window to save what it holds, then perform `next`.
    Sweep(PendingSweep),
}

/// A request made of this module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// The author asked for a settings window (SWN-FR-13).
    Open {
        kind: SettingsWindowKind,
        section: Option<String>,
    },
    /// The author asked to close the settings window that is open.
    Close { kind: SettingsWindowKind },
    /// The application was asked to quit (SWN-FR-19).
    Quit,
}

/// SWN-FR-05 through SWN-FR-07 and SWN-FR-12, as one pure decision.
///
/// `open` is the settings window currently on screen; `sweeping` is whether a
/// save sweep is already running. Everything the module does to a window is one
/// of the four [`Action`]s this returns, so the rules are pinned by unit tests
/// that state a situation rather than by reading the I/O that follows.
pub fn decide(request: Request, open: Option<SettingsWindowKind>, sweeping: bool) -> Action {
    // SWN-FR-12: while a sweep runs, a second close, a request for the same
    // window, and a request for the other one are each inert. The exception is
    // a quit, which is not a *further* save but a stronger reason for the one
    // already running: it upgrades what the sweep in flight will do rather than
    // starting anything (handled by the caller, which rewrites `next`).
    if sweeping {
        return Action::Inert;
    }
    match request {
        Request::Open { kind, section } => match open {
            Some(current) if current == kind => Action::Focus { kind, section },
            Some(current) => Action::Sweep(PendingSweep {
                label: current.label().to_string(),
                next: Transition::Open { kind, section },
            }),
            None => Action::Create { kind, section },
        },
        Request::Close { kind } => match open {
            // A close named for a window that is not the open one is somebody
            // else's event; nothing to do.
            Some(current) if current == kind => Action::Sweep(PendingSweep {
                label: kind.label().to_string(),
                next: Transition::Close,
            }),
            _ => Action::Inert,
        },
        Request::Quit => match open {
            Some(current) => Action::Sweep(PendingSweep {
                label: current.label().to_string(),
                next: Transition::Quit,
            }),
            // Nothing of ours is open: the quit is the exit path's own.
            None => Action::Inert,
        },
    }
}

/// SWN-FR-16: whether a request may proceed at all, given whether a project is
/// open.
///
/// While no project is open, **no route anywhere** opens the Project settings
/// window — the menu entry is absent, but an address a notification carries or
/// a call from a frontend that has not caught up could still name it. The rule
/// therefore sits where every route meets rather than only in the menu.
///
/// Everything else is allowed in that state: Global settings is reachable
/// whether or not a project is open (SWN-FR-15), and a close or a quit is about
/// a window that exists rather than about a project.
pub fn route_allowed(request: &Request, project_open: bool) -> bool {
    match request {
        Request::Open {
            kind: SettingsWindowKind::Project,
            ..
        } => project_open,
        _ => true,
    }
}

/// SWN-FR-11: what a failed sweep leaves behind.
///
/// The window stays, the transition does not happen, and — where the failure
/// arrived on the way to a quit — the held exit is released so the application
/// does not sit wedged waiting for an answer that is no longer coming.
pub fn releases_exit_gate(next: &Transition) -> bool {
    matches!(next, Transition::Quit)
}

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

/// The sweep in flight, if any. Managed state.
#[derive(Default)]
pub struct SettingsWindows {
    pending: Mutex<Option<PendingSweep>>,
}

impl SettingsWindows {
    fn peek(&self) -> Option<PendingSweep> {
        self.pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
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
}

/// Which settings window exists right now, or `None`.
fn open_kind<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<SettingsWindowKind> {
    for kind in [SettingsWindowKind::Global, SettingsWindowKind::Project] {
        if app.get_webview_window(kind.label()).is_some() {
            return Some(kind);
        }
    }
    None
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
mod macos_modality {
    use std::cell::{Cell, RefCell};
    use std::ptr::NonNull;

    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{MainThreadMarker, Message};
    use objc2_app_kit::{
        NSAutoresizingMaskOptions, NSEvent, NSEventMask, NSResponder, NSView, NSWindow,
    };

    /// What a block or a release actually did, for the log record that explains
    /// it. SWN-FR-02 is invisible when it works, so this is the only account of
    /// it anybody gets.
    pub struct BlockReport {
        /// Whether the event monitor is installed.
        pub monitor: bool,
        /// Whether the view over the parent's content is installed.
        pub overlay: bool,
        /// How many events the monitor dropped while the block was up. Read on
        /// release; zero on a block.
        pub dropped: u64,
    }

    /// The view over the parent's content, and the responder it displaced.
    struct Overlay {
        view: Retained<NSView>,
        /// What held the keyboard before the block, restored when it lifts. The
        /// webview, ordinarily.
        responder: Option<Retained<NSResponder>>,
        window: Retained<NSWindow>,
    }

    thread_local! {
        /// The installed monitor, held because removing one needs the object
        /// `addLocalMonitor…` returned. Thread-local rather than global: AppKit
        /// hands these out on the main thread and takes them back there.
        static MONITOR: RefCell<Option<Retained<AnyObject>>> = const { RefCell::new(None) };
        static OVERLAY: RefCell<Option<Overlay>> = const { RefCell::new(None) };
        static DROPPED: Cell<u64> = const { Cell::new(0) };
    }

    /// Everything an author does to a window with the pointer or the keyboard.
    ///
    /// Movement and scrolling are in the set as well as the presses: a blocked
    /// parent that still highlighted the row under the pointer, or still
    /// scrolled its editor, would be taking interaction in the way that counts.
    fn author_events() -> NSEventMask {
        NSEventMask::LeftMouseDown
            | NSEventMask::LeftMouseUp
            | NSEventMask::RightMouseDown
            | NSEventMask::RightMouseUp
            | NSEventMask::OtherMouseDown
            | NSEventMask::OtherMouseUp
            | NSEventMask::LeftMouseDragged
            | NSEventMask::RightMouseDragged
            | NSEventMask::OtherMouseDragged
            | NSEventMask::MouseMoved
            | NSEventMask::ScrollWheel
            | NSEventMask::KeyDown
            | NSEventMask::KeyUp
            | NSEventMask::FlagsChanged
    }

    /// # Safety
    /// `ns_window` must be a live `NSWindow` pointer, and the caller must be on
    /// the main thread.
    pub unsafe fn set_window_blocked(
        ns_window: *mut std::ffi::c_void,
        blocked: bool,
    ) -> BlockReport {
        let Some(mtm) = MainThreadMarker::new() else {
            return BlockReport {
                monitor: false,
                overlay: false,
                dropped: 0,
            };
        };
        // Whatever is installed goes first, so a second block request replaces
        // it rather than stacking on it.
        let dropped = release();
        if !blocked {
            return BlockReport {
                monitor: false,
                overlay: false,
                dropped,
            };
        }
        let window: &NSWindow = unsafe { &*ns_window.cast() };
        BlockReport {
            monitor: install_monitor(window, mtm),
            overlay: install_overlay(window, mtm),
            dropped: 0,
        }
    }

    fn install_monitor(window: &NSWindow, mtm: MainThreadMarker) -> bool {
        // The window's *number*, not the window: a number is a plain integer the
        // handler can hold without keeping the parent alive past its own end.
        let parent = window.windowNumber();
        DROPPED.with(|count| count.set(0));
        let handler = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
            let event = unsafe { event.as_ref() };
            let for_parent = event
                .window(mtm)
                .is_some_and(|window| window.windowNumber() == parent);
            if for_parent {
                // Dropped: nothing else sees it, so it reaches neither the
                // parent's webview nor a menu key equivalent behind it.
                DROPPED.with(|count| count.set(count.get().saturating_add(1)));
                std::ptr::null_mut()
            } else {
                // Passed on exactly as it arrived, which is what leaves the
                // settings window, and every other window, working normally.
                (event as *const NSEvent).cast_mut()
            }
        });
        let monitor = unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(author_events(), &handler)
        };
        let installed = monitor.is_some();
        MONITOR.with(|slot| *slot.borrow_mut() = monitor);
        installed
    }

    fn install_overlay(window: &NSWindow, mtm: MainThreadMarker) -> bool {
        let Some(content) = window.contentView() else {
            return false;
        };
        let view = NSView::initWithFrame(mtm.alloc::<NSView>(), content.bounds());
        // The parent is not resizable while it is blocked, but it can be zoomed
        // by the window server and it can change display; the view follows its
        // content rather than uncovering a strip of it.
        view.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        content.addSubview(&view);
        // The webview holds the keyboard until something takes it away, and a
        // view over it does not: a key press would still be typed into whatever
        // had the caret. Clearing the responder is what stops that, and the
        // window gives it back when the block lifts.
        let responder = window.firstResponder();
        window.makeFirstResponder(None);
        OVERLAY.with(|slot| {
            *slot.borrow_mut() = Some(Overlay {
                view,
                responder,
                window: window.retain(),
            })
        });
        true
    }

    /// Take down whatever is installed, and report what the monitor dropped.
    fn release() -> u64 {
        MONITOR.with(|slot| {
            if let Some(monitor) = slot.borrow_mut().take() {
                unsafe { NSEvent::removeMonitor(&monitor) };
            }
        });
        OVERLAY.with(|slot| {
            if let Some(overlay) = slot.borrow_mut().take() {
                overlay.view.removeFromSuperview();
                overlay
                    .window
                    .makeFirstResponder(overlay.responder.as_deref());
            }
        });
        DROPPED.with(|count| count.replace(0))
    }
}

/// Announce which settings window is open (or that none is) to every webview.
fn announce<R: tauri::Runtime>(app: &tauri::AppHandle<R>, open: Option<SettingsWindowKind>) {
    let _ = app.emit(
        EVENT_CHANGED,
        serde_json::json!({ "open": open.map(|k| k.slug()) }),
    );
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

/// SWN-FR-03: the inner size that lands the window on `target` **outer** pixels,
/// given the chrome the platform draws around it.
///
/// Pure, because the correction is the whole of what makes "800 × 600" the size
/// the author can measure rather than the size of the web view inside a title
/// bar. Never returns zero in either axis: a decoration larger than the target
/// would otherwise produce a window with no content at all.
pub fn inner_size_for_outer(target: (u32, u32), decoration: (u32, u32)) -> (u32, u32) {
    (
        target.0.saturating_sub(decoration.0).max(1),
        target.1.saturating_sub(decoration.1).max(1),
    )
}

/// Bring the window to exactly [`SETTINGS_WINDOW_SIZE`] outer pixels.
///
/// Best-effort: a platform that cannot report one of the three measurements
/// leaves the window at the size it was built with, which is the same size
/// minus the chrome — wrong by a title bar rather than unusable.
fn pin_outer_size<R: tauri::Runtime>(app: &tauri::AppHandle<R>, window: &tauri::WebviewWindow<R>) {
    let (Ok(outer), Ok(inner), Ok(scale)) = (
        window.outer_size(),
        window.inner_size(),
        window.scale_factor(),
    ) else {
        return;
    };
    let target = (
        (SETTINGS_WINDOW_SIZE.0 as f64 * scale).round() as u32,
        (SETTINGS_WINDOW_SIZE.1 as f64 * scale).round() as u32,
    );
    let decoration = (
        outer.width.saturating_sub(inner.width),
        outer.height.saturating_sub(inner.height),
    );
    let (w, h) = inner_size_for_outer(target, decoration);
    if let Err(e) = window.set_size(tauri::PhysicalSize::new(w, h)) {
        logging::log_warn(
            app,
            &BUFFER,
            &DOMAINS,
            "settings window could not be pinned to its fixed size",
            log_fields! { "reason" => e.to_string() },
        );
    }
}

/// SWN-FR-04: place the window in the centre of the display its parent occupies.
///
/// Recomputed on every opening and never persisted, so a window the author
/// dragged elsewhere before closing it opens centred again.
fn centre_on_parent_display<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    window: &tauri::WebviewWindow<R>,
) {
    let Some(parent) = app.get_webview_window(PARENT_WINDOW_LABEL) else {
        return;
    };
    let (Ok(pos), Ok(size), Ok(own)) = (
        parent.outer_position(),
        parent.outer_size(),
        window.outer_size(),
    ) else {
        return;
    };
    let parent_rect = crate::window::DisplayRect {
        x: pos.x,
        y: pos.y,
        width: size.width,
        height: size.height,
    };
    let Some(display) = crate::window::display_for_rect(app, parent_rect) else {
        return;
    };
    let (x, y) = crate::window::center_window_on_display(display, (own.width, own.height));
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

/// Build and show a settings window (SWN-FR-01 through SWN-FR-04).
///
/// The window is built hidden and shown only once it holds its fixed size in
/// the centre of the parent's display, so it is never seen at another size or
/// another position first.
fn create<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    kind: SettingsWindowKind,
    section: Option<String>,
) {
    let url = match &section {
        Some(s) => format!(
            "index.html?settings={}&section={}",
            kind.slug(),
            encode_query_value(s)
        ),
        None => format!("index.html?settings={}", kind.slug()),
    };
    // Built through a factory rather than a `mut` binding: `parent` consumes
    // the builder and gives it back only on success, so a platform that refuses
    // the relationship would otherwise leave nothing to build from.
    let base = || {
        tauri::WebviewWindowBuilder::new(
            app,
            kind.label(),
            tauri::WebviewUrl::App(url.clone().into()),
        )
        .title(kind.title())
        .inner_size(
            SETTINGS_WINDOW_SIZE.0 as f64,
            SETTINGS_WINDOW_SIZE.1 as f64,
        )
        // SWN-FR-03: no drag-to-resize and no maximize. `resizable(false)` also
        // disables the macOS zoom button's full-screen behaviour, which is what
        // keeps the window out of OS full-screen.
        .resizable(false)
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
                    log_fields! { "window" => kind.slug(), "reason" => e.to_string() },
                );
                base()
            }
        },
        None => base(),
    };
    // Blocked before the window is built, not after: building a webview window
    // takes long enough to click through, and SWN-FR-02 gives the parent no
    // interaction from the moment a settings window is on its way.
    set_parent_enabled(app, false);
    let window = match builder.build() {
        Ok(w) => w,
        Err(e) => {
            logging::log_error(
                app,
                &BUFFER,
                &DOMAINS,
                "settings window could not be opened",
                log_fields! { "window" => kind.slug(), "reason" => e.to_string() },
            );
            // Nothing opened, so nothing may go on blocking the parent — unless
            // the other settings window is still there, which a failed switch
            // leaves behind.
            if open_kind(app).is_none() {
                set_parent_enabled(app, true);
            }
            return;
        }
    };
    pin_outer_size(app, &window);
    centre_on_parent_display(app, &window);
    let _ = window.show();
    let _ = window.set_focus();
    announce(app, Some(kind));
    logging::log_info(
        app,
        &BUFFER,
        &DOMAINS,
        "settings window opened",
        log_fields! {
            "window" => kind.slug(),
            "width" => SETTINGS_WINDOW_SIZE.0,
            "height" => SETTINGS_WINDOW_SIZE.1,
            // The section is a fixed vocabulary of surface names, never author
            // content, so it is safe to record and is what explains where the
            // author landed.
            "section" => section,
        },
    );
}

/// SWN-FR-06: focus the window already open and route it to a named section.
fn focus<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    kind: SettingsWindowKind,
    section: Option<String>,
) {
    let Some(window) = app.get_webview_window(kind.label()) else {
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
        log_fields! { "window" => kind.slug(), "section" => section },
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
        // Perform the transition directly so a switch does not strand.
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
        Transition::Open { .. } => "switch",
        Transition::Quit => "quit",
    }
}

/// Tear the window down and give the parent its interaction back.
fn destroy<R: tauri::Runtime>(app: &tauri::AppHandle<R>, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        // `destroy` rather than `close`: `close` raises `CloseRequested`, which
        // this module intercepts to start a sweep — and the sweep has just
        // finished, so that would ask the window to save all over again.
        let _ = window.destroy();
    }
}

/// Perform the transition a completed sweep earned (SWN-FR-07, SWN-FR-19).
fn perform<R: tauri::Runtime>(app: &tauri::AppHandle<R>, next: Transition) {
    match next {
        Transition::Close => {
            set_parent_enabled(app, true);
            announce(app, None);
            if let Some(parent) = app.get_webview_window(PARENT_WINDOW_LABEL) {
                let _ = parent.set_focus();
            }
        }
        // SWN-FR-07: the requested window opens only after the open one has
        // closed, so the switch is never two windows in flight at once.
        Transition::Open { kind, section } => create(app, kind, section),
        Transition::Quit => {
            set_parent_enabled(app, true);
            announce(app, None);
            crate::menu::resume_held_exit(app);
        }
    }
}

/// Route a request through [`decide`] and perform whatever it returns.
fn dispatch<R: tauri::Runtime>(app: &tauri::AppHandle<R>, request: Request) {
    // Every path below needs the sweep state. A handle that does not carry it
    // manages no settings window either, so there is nothing to decide.
    let Some(state) = app.try_state::<SettingsWindows>() else {
        return;
    };
    // SWN-FR-16: while no project is open, **no route anywhere** opens the
    // Project settings window. The menu entry is absent, but an address a
    // notification carries or a stale frontend call could still name it, so the
    // rule is enforced where every route meets rather than only in the menu.
    let project_open = app
        .try_state::<crate::project::ProjectState>()
        .and_then(|p| p.root())
        .is_some();
    if !route_allowed(&request, project_open) {
        logging::log_warn(
            app,
            &BUFFER,
            &DOMAINS,
            "Project settings was requested with no project open",
            log_fields! {},
        );
        return;
    }
    let sweeping = state.peek().is_some();
    match decide(request, open_kind(app), sweeping) {
        Action::Inert => {}
        Action::Focus { kind, section } => focus(app, kind, section),
        Action::Create { kind, section } => create(app, kind, section),
        Action::Sweep(sweep) => begin_sweep(app, sweep),
    }
}

/// Whether either settings window is on screen (SWN-FR-05).
pub fn a_settings_window_is_open<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
    open_kind(app).is_some()
}

/// SWN-FR-13: the in-process entry point the application menu opens a settings
/// window through. Same rules as the command the frontend calls.
pub fn request_open<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    kind: SettingsWindowKind,
    section: Option<String>,
) {
    dispatch(app, Request::Open { kind, section });
}

/// SWN-FR-13: open a settings window, focus it when it is the one already open,
/// or switch to it when the other one is (SWN-FR-05 through SWN-FR-07).
///
/// `section` names a section within the window — the Agents section the chrome
/// roster routes to (`AGT-agents.md` AGT-FR-06), the GitHub section a failed Git
/// operation offers, the section a notification's address carries. `None` opens
/// the window on whichever section it presents by default.
#[tauri::command]
pub fn open_settings_window<R: tauri::Runtime>(
    kind: SettingsWindowKind,
    section: Option<String>,
    app: tauri::AppHandle<R>,
) {
    dispatch(&app, Request::Open { kind, section });
}

/// What a settings window needs to know about the project behind it.
///
/// A settings window is its own webview, so it shares no state with the main
/// window and cannot read the open project off it. Every field is fixed for the
/// window's whole life — the parent is blocked, so neither the project nor its
/// active worktree can change while one is open (SWN-FR-02, `SET-FR-20`) —
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

/// The open project behind the settings window, for the surfaces inside it that
/// need to name it — the GitHub token picker's title (`GHA-FR-20`) and the
/// address the Notifications rehearsal mints (`GLS-FR-27`).
///
/// Answers with empty strings rather than an error while no project is open:
/// Global settings is reachable in exactly that state (SWN-FR-15), and the
/// sections that need a project disable themselves rather than fail.
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
        content_root: project
            .root()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
        project_key,
    }
}

/// The frontend's answer to a save sweep (SWN-FR-08 through SWN-FR-12).
///
/// `ok` is true once every pending change in every section has been written —
/// including a write that was already in flight, which the sweep awaits rather
/// than starting again (SWN-FR-09) — and false when one of them failed. A
/// failure cancels the transition outright: the window stays open, no other
/// settings window opens, and a held quit is released (SWN-FR-11, SWN-FR-19).
/// `section` names the section that failed, so the window can present it.
#[tauri::command]
pub fn finish_settings_close<R: tauri::Runtime>(
    ok: bool,
    section: Option<String>,
    window: tauri::Window<R>,
    app: tauri::AppHandle<R>,
) {
    let label = window.label().to_string();
    let Some(sweep) = app
        .try_state::<SettingsWindows>()
        .and_then(|s| s.take_for(&label))
    else {
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
        let _ = w.emit(
            EVENT_PRESENT_FAILURE,
            serde_json::json!({ "section": section }),
        );
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

/// SWN-FR-19: hold a quit while the open settings window writes what it holds.
///
/// Returns whether the quit was taken over here. `false` means no settings
/// window is open and the ordinary exit path should proceed.
pub fn defer_quit_for_settings<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
    // Asked before the managed state is touched at all, so the ordinary quit
    // path costs nothing and needs nothing managed on a handle that never opens
    // a settings window.
    if open_kind(app).is_none() {
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

/// Intercept what happens to a settings window itself.
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
    let Some(kind) = SettingsWindowKind::from_label(window.label()) else {
        return false;
    };
    let app = window.app_handle();
    match event {
        tauri::WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            dispatch(app, Request::Close { kind });
        }
        tauri::WindowEvent::Destroyed => {
            // A sweep this window was under can never land now, so give the
            // claim back — but only its own: a switch destroys one window while
            // the other's sweep may already be outstanding.
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
            // Whatever destroyed it, the parent must not stay disabled — unless
            // a settings window is still up. A switch destroys the outgoing
            // window and opens the incoming one, and this event is delivered
            // through the platform's event loop rather than inline, so it can
            // arrive AFTER the incoming window has already blocked the parent.
            // Re-enabling unconditionally would hand the author a live main
            // window underneath an open settings window (SWN-FR-02).
            if open_kind(app).is_none() {
                set_parent_enabled(app, true);
                announce(app, None);
            }
        }
        // NTF-FR-08: "no window of the application holds OS focus" is what
        // decides whether a raise is posted, and this window is one of ours.
        tauri::WindowEvent::Focused(focused) => {
            let _ = app.emit(
                EVENT_FOCUS,
                serde_json::json!({ "focused": *focused, "window": kind.slug() }),
            );
        }
        _ => {}
    }
    true
}

#[cfg(test)]
mod tests;
