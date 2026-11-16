//! The macOS half of the project tab group (`PWN-project-windows.md`
//! PWN-FR-XGTO).
//!
//! A window must not come to the front inside the callback of the event loop.
//! When a window joins a tab group, AppKit draws it at once. The draw goes back
//! into the event loop, which waits for the lock that its own callback holds, and
//! the main thread stops for good. So the calls here go on the main dispatch
//! queue: the queue runs them after the callback has ended.

use std::ffi::c_void;

use dispatch2::DispatchQueue;
use objc2::rc::Retained;
use objc2_app_kit::{NSWindow, NSWindowOrderingMode};

/// A window that goes to the main queue and is used there only.
struct MainQueueWindow(Retained<NSWindow>);

// SAFETY: the value is made on the main thread and is used only by a block on
// the main queue, which also runs on the main thread.
unsafe impl Send for MainQueueWindow {}

/// Hold a window for a block on the main queue.
///
/// # Safety
/// `ns_window` must be null or a live `NSWindow` pointer.
unsafe fn hold(ns_window: *mut c_void) -> Option<MainQueueWindow> {
    // SAFETY: the caller gives a live `NSWindow` or null.
    unsafe { Retained::retain(ns_window.cast::<NSWindow>()) }.map(MainQueueWindow)
}

/// Add `child` to the tab group of `parent` as the tab after it, and bring it to
/// the front, after the current callback of the event loop.
///
/// Returns false when a pointer is null, and nothing is scheduled.
///
/// # Safety
/// Each pointer must be null or a live `NSWindow` pointer.
pub unsafe fn add_tab_later(parent: *mut c_void, child: *mut c_void) -> bool {
    // SAFETY: the caller gives live `NSWindow` pointers or null.
    let (Some(parent), Some(child)) = (unsafe { hold(parent) }, unsafe { hold(child) }) else {
        return false;
    };
    DispatchQueue::main().exec_async(move || {
        let (parent, child) = (parent, child);
        parent.0.addTabbedWindow_ordered(&child.0, NSWindowOrderingMode::Above);
        child.0.makeKeyAndOrderFront(None);
    });
    true
}

/// Bring a window to the front with OS focus, after the current callback of the
/// event loop. A minimized window is restored first.
///
/// Returns false when the pointer is null, and nothing is scheduled.
///
/// # Safety
/// The pointer must be null or a live `NSWindow` pointer.
pub unsafe fn bring_to_front_later(ns_window: *mut c_void) -> bool {
    // SAFETY: the caller gives a live `NSWindow` pointer or null.
    let Some(window) = (unsafe { hold(ns_window) }) else {
        return false;
    };
    DispatchQueue::main().exec_async(move || {
        let window = window;
        if window.0.isMiniaturized() {
            window.0.deminiaturize(None);
        }
        window.0.makeKeyAndOrderFront(None);
    });
    true
}
