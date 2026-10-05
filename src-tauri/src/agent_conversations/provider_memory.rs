//! What the process remembers about a provider between calls.
//!
//! Two process-wide memories, both of them hints rather than state: how many
//! provider-native entries a model refused (CVL-FR-32), and which upstream
//! carried a model last (CVL-FR-31). Both are forgotten on restart, and the
//! affinity is cleared by a test through the seam this file exposes for it.

use super::*;

/// CVL-FR-36: a refusal of the request *for the sake of a server tool it
/// offered*, told apart from every other `400` by the provider's own message.
///
/// Narrowing on any `400` would strip an agent's capabilities over a malformed
/// message or an oversized context, and would do it silently on every turn.
pub(super) fn refuses_a_server_tool(failure: &CallFailure) -> bool {
    failure.status == Some(400)
        && failure
            .provider_message
            .as_deref()
            .is_some_and(|message| message.to_lowercase().contains("server tool"))
}

/// CVL-FR-36: how many trailing entries a model is known to refuse, for the life
/// of the process.
///
/// Per model rather than per provider: this is the model's own limitation and
/// two models of one provider differ in it. Not persisted — a provider that
/// fixes its own service should not have to wait for an author to clear a cache
/// this application wrote about it.
fn native_refusals() -> &'static std::sync::Mutex<std::collections::HashMap<String, usize>> {
    static REFUSALS: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, usize>>,
    > = std::sync::OnceLock::new();
    REFUSALS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

pub(super) fn native_entries_refused_by(model: &str) -> usize {
    native_refusals()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(model)
        .copied()
        .unwrap_or(0)
}

pub(super) fn remember_native_entries_refused(model: &str, dropped: usize) {
    let mut guard = native_refusals().lock().unwrap_or_else(|e| e.into_inner());
    let entry = guard.entry(model.to_string()).or_insert(0);
    *entry = (*entry).max(dropped);
}

/// CVL-FR-40: the upstream each model was last routed to, for the life of the
/// process.
///
/// Per model rather than per provider, per turn, or per loop: two models of one
/// provider are served by different upstreams, and one model's next turn
/// benefits from what its last turn wrote. Not persisted, for the reason
/// [`native_refusals`] is not — a routing fact an author cannot see is not one
/// they should have to clear.
fn upstream_affinity() -> &'static std::sync::Mutex<std::collections::HashMap<String, String>> {
    static AFFINITY: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, String>>,
    > = std::sync::OnceLock::new();
    AFFINITY.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// CVL-FR-40: the upstream to ask for, or `None` before anything is known of
/// this model.
pub(super) fn upstream_for(model: &str) -> Option<String> {
    upstream_affinity()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(model)
        .cloned()
}

/// CVL-FR-40: remember where a call was actually served.
///
/// Written from every answered call rather than from the first alone, so a call
/// that fell back re-pins to wherever it landed and the affinity heals itself.
pub(super) fn remember_upstream(model: &str, upstream: &str) {
    if upstream.is_empty() {
        return;
    }
    upstream_affinity()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(model.to_string(), upstream.to_string());
}

/// Forget one model's upstream. A test clears only the model ids it chose
/// itself: the affinity is process-wide, so a clear of all of it would remove
/// the pin of a test that runs at the same time.
#[cfg(test)]
pub(crate) fn forget_upstream(model: &str) {
    upstream_affinity()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(model);
}
