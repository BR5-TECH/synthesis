//! The caller-held cancellation flag for one execution (EAC-FR-25).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A caller-held cancellation flag (EAC-FR-25).
///
/// Deliberately a flag rather than a channel: the executor polls it beside the
/// child it is already waiting on, and a caller that drops every clone has not
/// thereby cancelled anything — cancelling is something a caller decides, not
/// something that happens when it stops paying attention.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    own: Arc<AtomicBool>,
    /// The caller's flag, where this token was made by [`Self::linked`].
    parent: Option<Arc<CancellationToken>>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    /// EAC-FR-CPEP: a token that is cancelled when this one is, and that can be
    /// cancelled on its own without touching this one.
    pub fn linked(&self) -> Self {
        Self {
            own: Arc::new(AtomicBool::new(false)),
            parent: Some(Arc::new(self.clone())),
        }
    }

    pub fn cancel(&self) {
        self.own.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.own.load(Ordering::SeqCst)
            || self
                .parent
                .as_ref()
                .is_some_and(|parent| parent.is_cancelled())
    }
}
