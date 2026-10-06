//! Discussion write-identity resolution and the token-change event.
//!
//! Split from `github_tokens.rs` to keep that file under the line limit. The
//! parent re-exports the public items, so callers name them as before.

use super::{resolve_github_identity, GithubIdentity, ERR_TOKEN_MISSING};
use crate::global_settings::GlobalSettingsStore;

/// GTS-FR-AEQO: the event emitted after the token registry or a project's token
/// binding changed. It carries no payload.
pub const GITHUB_TOKENS_CHANGED: &str = "github-tokens-changed";

/// GTS-FR-ZKOD: like [`resolve_github_identity`], except that a registry with no
/// token at all answers `Ok(None)` instead of a refusal.
///
/// `None` is the only extra answer: a single token, a bound token, and the
/// several-tokens-without-a-binding refusal all come back exactly as
/// `resolve_github_identity` returns them, so no state is ever resolved to an
/// arbitrary or a latest token. `CMS-comments-storage.md` (CMS-FR-12) reads
/// `None` as "write as the local participant".
pub fn resolve_github_identity_if_stored(
    store: &GlobalSettingsStore,
    project_key: &str,
) -> Result<Option<GithubIdentity>, String> {
    match resolve_github_identity(store, project_key) {
        Ok(identity) => Ok(Some(identity)),
        Err(e) if e == ERR_TOKEN_MISSING => Ok(None),
        Err(e) => Err(e),
    }
}

/// GTS-FR-AEQO: tell every consumer of a resolved identity that it may be stale.
///
/// The emit result is discarded on purpose: an event must never fail the
/// operation it reports on, and a dropped event costs one stale label.
pub(super) fn emit_tokens_changed(app: &tauri::AppHandle) {
    use tauri::Emitter as _;
    let _ = app.emit(GITHUB_TOKENS_CHANGED, ());
}
