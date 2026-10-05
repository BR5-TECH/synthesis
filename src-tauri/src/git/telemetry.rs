//! How every git operation reports itself to the session buffer
//! (`../../specifications/core/LGC-logging.md`).

use std::time::Instant;


use crate::changes::{ERR_NOT_A_REPO, ERR_NO_MERGE_BASE, ERR_UNKNOWN_BRANCH};
use crate::github_tokens::{self};
use crate::log_fields;
use crate::logging::{self, Domain, Fields, LogBuffer, LogLevel, LogSink};
use crate::worktree::{ERR_BRANCH_ALREADY_CHECKED_OUT, ERR_CHECKOUT_BLOCKED};

use super::*;

// ---------------------------------------------------------------------------
// Logging (`../core/LGC-logging.md`)
// ---------------------------------------------------------------------------
//
// Every operation in this module reports itself through the session buffer,
// because the Logs panel is the only place an author can find out why a commit
// held nothing, why a checkout was refused, or how long a push spent on the
// wire. Two rules shape every emit site below:
//
// - **Shape, never content.** A path, a branch name, a count, a duration and a
//   boolean are what a reader filters on and none of them can leak. The diff
//   text, the file revisions, and the commit message are the author's own words
//   and never appear in a record.
// - **No credential, by construction** (GTC-FR-11). The one value in reach that
//   could carry one is a remote URL, so no record here carries a URL at all: a
//   transfer is described by its remote's *name*, and its failures are the fixed
//   vocabulary `classify_transfer_error` produces rather than libgit2's own
//   message. Whether a token was presented is a boolean.

/// A git operation that never leaves the machine.
pub(crate) const LOCAL: &[Domain] = &[Domain::Backend];

/// A transfer. `Remote` as well as `Backend`, because a reader filtering on
/// `remote` wants everything that crossed the network, whichever module made the
/// call.
pub(crate) const TRANSFER: &[Domain] = &[Domain::Backend, Domain::Remote];

pub(crate) const MSG_FETCH_FAILED: &str = "fetch failed";
pub(crate) const MSG_PUSH_FAILED: &str = "push failed";
pub(crate) const MSG_CHECKOUT_FAILED: &str = "checkout failed";
pub(crate) const MSG_COMMIT_FAILED: &str = "commit failed";
pub(crate) const MSG_DIFF_FAILED: &str = "diff failed";
pub(crate) const MSG_REVISIONS_FAILED: &str = "failed to read the file revisions";
pub(crate) const MSG_LISTING_FAILED: &str = "branch listing failed";

/// Whether a failure is a *refusal*: the operation declining before it did
/// anything, rather than attempting its work and failing at it.
///
/// A record's level is what a reader is meant to do about it, and the line falls
/// there for a reason that survives contact with edge cases. Nothing broke in a
/// refusal — no ref was written, no transport was opened, the working tree is
/// untouched — the surface that asked says so inline, and the author picks a
/// token, ticks a path, or stashes a change: a `WARN`. A credential the remote
/// *rejected* is on the other side of that line even though the author can also
/// act on it, because the operation ran and did not do what it was asked; so is
/// a keychain that could not be read, and so is any libgit2 failure.
pub(crate) fn is_refusal(error: &str) -> bool {
    // GTC-FR-31: nothing was read and nothing was written, and the surface that
    // asked says so — a refusal, whatever pair of paths it carries.
    if is_worktree_identity_changed(error) {
        return true;
    }
    // GTC-FR-WNZH: the refusals of a branch deletion and of a commit read.
    if is_deletion_refusal(error) {
        return true;
    }
    matches!(
        error,
        ERR_NOT_A_REPO
            | ERR_UNKNOWN_BRANCH
            | ERR_NO_MERGE_BASE
            | ERR_NO_COMPARISON_BASE
            | ERR_NO_COMPARISON_MERGE_BASE
            | ERR_PATH_NOT_IN_COMPARISON
            | ERR_SCOPE_NOT_A_FILE
            | ERR_NO_REMOTE_CONFIGURED
            | ERR_EMPTY_COMMIT_MESSAGE
            | ERR_NO_PATHS_SELECTED
            | ERR_NOTHING_TO_COMMIT
            | ERR_CHECKOUT_BLOCKED
            | ERR_BRANCH_ALREADY_CHECKED_OUT
            | ERR_NO_BRANCH_CHECKED_OUT
            | crate::worktree::ERR_CHECKOUT_NOT_IN_LINKED_WORKTREE
            | github_tokens::ERR_SELECTION_REQUIRED
            | github_tokens::ERR_TOKEN_MISSING
    )
}

/// Report an operation that produced its result.
pub(crate) fn log_ok<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    level: LogLevel,
    domains: &[Domain],
    message: &str,
    fields: Fields,
) {
    logging::log(sink, buffer, level, domains, message, fields);
}

/// Report one that did not, at the level its cause deserves ([`is_refusal`]),
/// with the typed cause in a field rather than interpolated into the message —
/// so a reader can filter on it and the message stays constant down the column.
pub(crate) fn log_failure<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    domains: &[Domain],
    message: &str,
    error: &str,
    mut fields: Fields,
) {
    fields.insert("error".to_string(), serde_json::json!(error));
    let level = if is_refusal(error) {
        LogLevel::Warn
    } else {
        LogLevel::Error
    };
    logging::log(sink, buffer, level, domains, message, fields);
}

/// What a read was asked about: which comparison, and the paths it names.
///
/// A project-relative path is safe in a record and is what a reader filters on;
/// the file's *content* appears in no record this module writes.
pub(crate) fn scope_fields(scope: &DiffScope) -> Fields {
    let (kind, path, previous, target) = match scope {
        DiffScope::Path {
            path,
            previous_path,
        } => ("path", Some(path), previous_path.as_deref(), None),
        DiffScope::Staged => ("staged", None, None, None),
        DiffScope::Branch {
            path,
            target_branch,
            previous_path,
        } => (
            "branch",
            Some(path),
            previous_path.as_deref(),
            Some(target_branch),
        ),
    };
    let mut fields = log_fields! { "scope" => kind };
    if let Some(path) = path {
        fields.insert("path".to_string(), serde_json::json!(path));
    }
    if let Some(previous) = previous {
        fields.insert("previousPath".to_string(), serde_json::json!(previous));
    }
    if let Some(target) = target {
        fields.insert("targetBranch".to_string(), serde_json::json!(target));
    }
    fields
}

/// Milliseconds since `started`, as the field every operation that can be slow
/// carries. Saturating rather than `as u64`: a duration this cannot hold is a
/// clock that moved, not a transfer that ran for six hundred thousand years.
pub(crate) fn duration_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}
