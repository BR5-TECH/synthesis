//! The thread on which a synchronous command body runs
//! (`CEX-command-execution.md`).
//!
//! `#[project_scope_macros::blocking_command]` registers a synchronous command
//! to run on a worker of the async runtime (CEX-FR-JBTB) and wraps its body in
//! [`run_blocking`] (CEX-FR-KFVT).

use tokio::runtime::{Handle, RuntimeFlavor};

/// CEX-FR-KFVT, CEX-FR-VKTJ, CEX-FR-VYEQ: run a blocking command body.
///
/// On a worker of a multi-threaded runtime, the runtime first moves the other
/// tasks of that worker to other workers, so the body starves no task. The
/// body runs whole on the calling thread, so a thread-local project context
/// stays visible to it. With no runtime, or in a single-threaded runtime,
/// where the runtime cannot move tasks, the body runs as a plain call.
pub fn run_blocking<T>(work: impl FnOnce() -> T) -> T {
    match Handle::try_current() {
        Ok(handle) if handle.runtime_flavor() == RuntimeFlavor::MultiThread => {
            tokio::task::block_in_place(work)
        }
        _ => work(),
    }
}

#[cfg(test)]
mod tests;
