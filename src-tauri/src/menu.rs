//! Native application menu (SNV-shell-navigation.md SNV-FR-14 / SNV-FR-23..26).
//!
//! A native Edit menu with predefined roles so OS-level edit accelerators
//! (undo/redo/cut/copy/paste/select-all) reach the focused webview — notably the
//! Editor (EDT-FR-15). Predefined roles dispatch to the focused surface
//! natively, so no new Tauri command and no ACL entry is required. Undo and Redo
//! arrive in the webview as `beforeinput` (historyUndo/historyRedo); inside an
//! Editor tab the frontend claims them for its tab-wide history (EDT-FR-22),
//! which needs nothing from this side. A minimal app submenu is kept first so
//! macOS's application menu carries the Quit item users expect to find there.
//!
//! A native File menu (SNV-FR-23) sits beside Edit with seven items: New File,
//! New Artifact, New Folder, Save, Save All, Close Project, and Exit — all custom
//! items, whose activation reaches `handle_menu_event` through `on_menu_event`.
//! The three creation items lead the menu in the same order the Library's folder
//! context menu presents them (LCM-FR-01), and each only *initiates* its creation
//! flow: it hands off to the modal that owns it (SNV-FR-24), so the backend just
//! relays the intent as an event.
//!
//! Save and Save All relay likewise: the buffers they write live only in the
//! frontend (EDT-FR-34 / EDT-FR-35 / FLO-FR-27), so this side contributes the
//! menu items, their accelerators, and their *enabled* state. Both start
//! disabled — at startup no tab is open and nothing is unsaved — and the
//! frontend drives them from there through `set_save_menu_state` as the active
//! tab and the session's unsaved set change (SNV-FR-28 / SNV-FR-30). A disabled
//! native item swallows its accelerator, which is what makes ⌘S a no-op on a
//! Dashboard tab rather than a write of something else.
//!
//! Close Project and Exit are *held*: both must write the Editor's pending
//! changes before anything is torn down (EDT-FR-33), and a write that cannot
//! proceed safely cancels the teardown outright (EDT-FR-32). Only the frontend
//! holds those buffers, so this side relays the intent and waits — Close Project
//! emits its event and leaves the project alive until the frontend calls
//! `close_project`; Exit emits `menu:exit-requested` and quits only once the
//! frontend answers with `finish_exit`.
//!
//! A quit therefore has to be caught while the webview is still alive, because
//! the webview is the only thing that can flush and the only thing that can
//! answer. Both of the ways the application itself offers are:
//!
//!  - **Exit / Quit** are custom menu items rather than the predefined quit
//!    role. The native role invokes the platform's own terminate path (on macOS,
//!    `NSApp terminate:`), which never surfaces as a run-loop exit request — the
//!    process is simply gone, and the pending writes with it.
//!  - **Closing the window** (title-bar button, ⌘W, Window → Close) is
//!    intercepted at `CloseRequested` (`handle_window_event`), *before* the
//!    window is destroyed. Waiting for the run loop's exit request is too late:
//!    it only arrives once the last window has already been destroyed, so
//!    holding it would leave a running process with no UI, no flush and no way
//!    to answer. `begin_exit` refuses to hold in exactly that state.
//!
//! One route stays outside all of this and loses unsaved edits: a termination
//! the OS initiates rather than the application — on macOS the Dock icon's own
//! Quit item, a logout, or a shutdown. Those go straight to `NSApp terminate:`,
//! and the only hook that could defer them (`applicationShouldTerminate:`) is
//! not implemented by tao, so nothing here is consulted. Intercepting it needs
//! an upstream change or an Objective-C shim over the app delegate.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{Emitter, Manager};

#[cfg(test)]
use crate::artifacts::ContentTracker;
#[cfg(test)]
use crate::project::ProjectState;
#[cfg(test)]
use crate::watcher::ProjectWatcher;

mod build;
mod events;
mod exit;
mod items;
mod state;

pub use build::*;
pub use events::*;
pub use exit::*;
use items::*;
pub use state::*;

#[cfg(test)]
mod tests;
