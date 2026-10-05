//! Where a log goes on disk.
//!
//! Every path here is resolved against the **repository machine store**
//! (`RMS-repository-machine-storage.md` RMS-FR-ZXHM), never against a worktree:
//! the `root` these helpers take is that store.

use super::*;

// ---------------------------------------------------------------------------
// Paths (CMS-FR-01 / CMS-FR-02)
// ---------------------------------------------------------------------------

pub(super) fn comments_dir(root: &Path) -> PathBuf {
    root.join(COMMENTS_REL)
}

/// CMS-FR-02: the log filename for an artifact, derived **deterministically**
/// from its project-relative path.
///
/// Determinism is the whole point rather than an implementation detail: one
/// artifact has one log whichever checkout it was commented on from, because
/// every worktree of a repository resolves to the same store (CMS-FR-XQBM). A
/// random id, or one seeded by a clock, would give two checkouts two unrelated
/// logs and neither of them would hold the conversation.
///
/// A hash rather than the path itself because a path contains separators and can
/// exceed a filename's length; 32 hex characters of SHA-256 is far past the point
/// where a collision within one project is a thing that happens.
pub fn log_id(artifact_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(artifact_id.as_bytes());
    let digest = hasher.finalize();
    digest.iter().take(16).map(|b| format!("{b:02x}")).collect()
}

pub(super) fn log_rel(artifact_id: &str) -> String {
    format!("{COMMENTS_REL}/{}.jsonl", log_id(artifact_id))
}

/// The absolute path of an artifact's log, behind the FSA-FR-10 escape gate.
///
/// `log_id` already yields a bare hex token, so no caller-supplied text reaches
/// the path — but the gate stays in the way regardless, because a primitive that
/// is only safe by virtue of its caller is one refactor from not being.
pub(super) fn log_path(root: &Path, artifact_id: &str) -> Result<PathBuf, String> {
    fsa::resolve_under(root, log_rel(artifact_id)).map_err(|e| e.to_string())
}
