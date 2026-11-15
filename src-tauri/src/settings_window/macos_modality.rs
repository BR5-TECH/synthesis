//! The macOS half of the parent block (`SWN-settings-windows.md` SWN-FR-02).
//!
//! One part of `settings_window.rs`. See the doc comment on the `mod` line there
//! for what the platform gives and why a sheet is not used.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
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
    /// How many views over a window's content are installed, one for each
    /// blocked window.
    pub overlays: usize,
    /// How many events the monitor dropped while the block was up. Read on
    /// release; zero on a block.
    pub dropped: u64,
}

/// The view over one blocked window's content, and the responder it displaced.
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
    /// One overlay for each blocked window, keyed by the window number.
    static OVERLAYS: RefCell<HashMap<isize, Overlay>> = RefCell::new(HashMap::new());
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

/// Block or release every window in `ns_windows`: one event monitor that drops
/// the events of all of them, and one overlay for each.
///
/// # Safety
/// Each pointer in `ns_windows` must be a live `NSWindow` pointer, and the
/// caller must be on the main thread.
pub unsafe fn set_windows_blocked(
    ns_windows: &[*mut std::ffi::c_void],
    blocked: bool,
) -> BlockReport {
    let Some(mtm) = MainThreadMarker::new() else {
        return BlockReport { monitor: false, overlays: 0, dropped: 0 };
    };
    // Whatever is installed goes first, so a second block request replaces
    // it rather than stacking on it.
    let dropped = release();
    if !blocked || ns_windows.is_empty() {
        return BlockReport { monitor: false, overlays: 0, dropped };
    }
    let windows: Vec<&NSWindow> = ns_windows.iter().map(|w| unsafe { &*w.cast() }).collect();
    let numbers: Vec<isize> = windows.iter().map(|window| window.windowNumber()).collect();
    let overlays = windows.iter().filter(|window| install_overlay(window, mtm)).count();
    BlockReport { monitor: install_monitor(numbers, mtm), overlays, dropped: 0 }
}

fn install_monitor(blocked: Vec<isize>, mtm: MainThreadMarker) -> bool {
    // The window *numbers*, not the windows: a number is a plain integer the
    // handler can hold without keeping a window alive past its own end.
    DROPPED.with(|count| count.set(0));
    let handler = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
        let event = unsafe { event.as_ref() };
        let for_blocked = event
            .window(mtm)
            .is_some_and(|window| blocked.contains(&window.windowNumber()));
        if for_blocked {
            // Dropped: nothing else sees it, so it reaches neither the
            // blocked window's webview nor a menu key equivalent behind it.
            DROPPED.with(|count| count.set(count.get().saturating_add(1)));
            std::ptr::null_mut()
        } else {
            // Passed on exactly as it arrived, which is what leaves the
            // settings window, and every other window, working normally.
            (event as *const NSEvent).cast_mut()
        }
    });
    let monitor =
        unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(author_events(), &handler) };
    let installed = monitor.is_some();
    MONITOR.with(|slot| *slot.borrow_mut() = monitor);
    installed
}

fn install_overlay(window: &NSWindow, mtm: MainThreadMarker) -> bool {
    let Some(content) = window.contentView() else {
        return false;
    };
    let view = NSView::initWithFrame(mtm.alloc::<NSView>(), content.bounds());
    // A blocked window is not resizable, but it can be zoomed by the window
    // server and it can change display; the view follows its content rather
    // than uncovering a strip of it.
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
    OVERLAYS.with(|slot| {
        slot.borrow_mut()
            .insert(window.windowNumber(), Overlay { view, responder, window: window.retain() })
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
    OVERLAYS.with(|slot| {
        for (_, overlay) in slot.borrow_mut().drain() {
            overlay.view.removeFromSuperview();
            overlay.window.makeFirstResponder(overlay.responder.as_deref());
        }
    });
    DROPPED.with(|count| count.replace(0))
}
