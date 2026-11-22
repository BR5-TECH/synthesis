//! The tmux server that keeps persistent sessions alive across application
//! processes (TRS-FR-OMYR .. TRS-FR-TCBS).
//!
//! Every call goes to the application's own socket (TRS-FR-ZOZO). The calls
//! go through [`TmuxRunner`] and the clients through [`ClientSpawner`], so a
//! test drives the module with a fake tmux or with a real tmux on a socket of
//! its own.

pub mod commands;
pub mod locate;
pub mod meta;
pub mod runner;
pub mod supervisor;

use std::sync::Arc;

pub use runner::{ClientSpawner, TmuxError, TmuxRunner};

/// TRS-FR-ZOZO: the socket name of the application's tmux server.
pub const SOCKET: &str = "synthesis";

/// A usable tmux: the calls and the clients.
pub struct Tmux {
    pub runner: Arc<dyn TmuxRunner>,
    pub clients: Arc<dyn ClientSpawner>,
}

/// TRS-FR-OMYR: where a usable tmux comes from.
pub trait TmuxSource: Send + Sync {
    /// The tmux to use now, or the cause why tmux is not usable.
    fn probe(&self) -> Result<Arc<Tmux>, &'static str>;

    /// The tmux to use now, or `None` where tmux is not usable.
    fn usable(&self) -> Option<Arc<Tmux>> {
        self.probe().ok()
    }
}

/// A source that a test or the application manages in place of the default.
pub struct ManagedTmuxSource(pub Arc<dyn TmuxSource>);

/// No tmux is usable.
pub struct NoTmux;

impl TmuxSource for NoTmux {
    fn probe(&self) -> Result<Arc<Tmux>, &'static str> {
        Err("tmux_not_found")
    }
}

/// The source of the running application. A test binary gets [`NoTmux`] unless
/// it manages a source, so no test reaches the user's own `synthesis` server.
pub fn source<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Arc<dyn TmuxSource> {
    use tauri::Manager;
    if let Some(managed) = app.try_state::<ManagedTmuxSource>() {
        return managed.0.clone();
    }
    default_source()
}

#[cfg(all(unix, not(test)))]
fn default_source() -> Arc<dyn TmuxSource> {
    Arc::new(locate::HostTmux)
}

#[cfg(any(not(unix), test))]
fn default_source() -> Arc<dyn TmuxSource> {
    Arc::new(NoTmux)
}

#[cfg(test)]
pub mod fake;

#[cfg(all(test, unix))]
mod real_tests;
