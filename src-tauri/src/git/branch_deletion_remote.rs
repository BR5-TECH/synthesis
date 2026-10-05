//! The remote part of a branch deletion
//! (`../../specifications/core/GTC-git.md` GTC-FR-FSRQ).
//!
//! The associated remote branch is the upstream of the local branch when that
//! upstream is on the primary remote, and otherwise the branch of the same name
//! on the primary remote. The deletion is a libgit2 push of a
//! `:refs/heads/<name>` refspec through the credential chain every transfer of
//! this module uses, followed by the removal of the remote-tracking ref.

use std::cell::RefCell;
use std::time::Instant;

use git2::{BranchType, Repository};

use crate::changes;
use crate::github_tokens;
use crate::log_fields;
use crate::logging::{self, LogBuffer, LogLevel};

use super::branch_deletion::{
    RemoteDeletionOutcome, REMOTE_DELETED, REMOTE_FAILED,
};
use super::*;

/// The remote branch a deletion would delete.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RemoteTarget {
    /// The remote's name.
    pub remote: String,
    /// The branch's name on the remote, without the remote prefix.
    pub branch: String,
    /// The remote-tracking branch's short name, such as `origin/feature`.
    pub tracking: String,
    /// The remote's URL. Never logged and never returned: it can carry a
    /// credential (GTC-FR-11).
    pub url: String,
}

/// GTC-FR-FSRQ: the remote branch associated with a local branch, if one
/// exists.
pub(crate) fn remote_target(repo: &Repository, local_name: &str) -> Option<RemoteTarget> {
    let remote = changes::primary_remote_name(repo)?;
    let prefix = format!("{remote}/");
    let local = repo.find_branch(local_name, BranchType::Local).ok()?;

    let from_upstream = local
        .upstream()
        .ok()
        .filter(|upstream| upstream.get().is_remote())
        .and_then(|upstream| upstream.name().ok().flatten().map(str::to_string))
        .and_then(|tracking| {
            let branch = tracking.strip_prefix(&prefix)?.to_string();
            Some((tracking, branch))
        });
    let (tracking, branch) = from_upstream.or_else(|| {
        let tracking = format!("{prefix}{local_name}");
        repo.find_branch(&tracking, BranchType::Remote)
            .ok()
            .map(|_| (tracking, local_name.to_string()))
    })?;
    let url = repo
        .find_remote(&remote)
        .ok()
        .and_then(|r| r.url().ok().map(str::to_string))
        .unwrap_or_default();
    Some(RemoteTarget {
        remote,
        branch,
        tracking,
        url,
    })
}

/// Delete the branch on the remote, then its remote-tracking ref.
fn push_deletion(
    repo: &Repository,
    target: &RemoteTarget,
    token: Option<&str>,
) -> Result<(), String> {
    let mut remote = repo
        .find_remote(&target.remote)
        .map_err(|_| ERR_NO_REMOTE_CONFIGURED.to_string())?;
    // The remote's verdict for the ref. A push can succeed at the transport and
    // still be refused for the ref, which libgit2 reports only here.
    let rejection = RefCell::new(None::<String>);
    let mut callbacks = git2::RemoteCallbacks::new();
    install_credentials(&mut callbacks, token);
    callbacks.push_update_reference(|_reference, status| {
        if let Some(reason) = status {
            *rejection.borrow_mut() = Some(redact_credentials(reason));
        }
        Ok(())
    });
    let mut opts = git2::PushOptions::new();
    opts.remote_callbacks(callbacks);
    let refspec = format!(":refs/heads/{}", target.branch);
    remote
        .push(&[refspec.as_str()], Some(&mut opts))
        .map_err(|e| classify_transfer_error(&e))?;
    if let Some(reason) = rejection.borrow().clone() {
        return Err(reason);
    }

    // GTC-FR-FSRQ: the remote-tracking ref goes with the branch it tracked.
    if let Ok(mut tracking) = repo.find_reference(&format!("refs/remotes/{}", target.tracking)) {
        tracking
            .delete()
            .map_err(|_| github_tokens::ERR_GITHUB_UNREACHABLE.to_string())?;
    }
    Ok(())
}

/// GTC-FR-FSRQ: the remote leg, reported rather than raised.
///
/// A failure is `state = "failed"` with its typed error, because the local
/// results already stand and the caller must be told both. The error is either
/// one of the fixed transfer vocabulary or the remote's own reason with any
/// credential removed.
pub(crate) fn delete_remote_branch<S: logging::LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    repo: &Repository,
    target: &RemoteTarget,
    token: Option<&str>,
) -> RemoteDeletionOutcome {
    let started = Instant::now();
    logging::log_info(
        sink,
        buffer,
        TRANSFER,
        "deleting remote branch",
        log_fields! {
            "remote" => target.remote.as_str(),
            "branch" => target.branch.as_str(),
            "authenticated" => token.is_some(),
        },
    );
    let result = push_deletion(repo, target, token);
    let fields = log_fields! {
        "remote" => target.remote.as_str(),
        "branch" => target.branch.as_str(),
        "durationMs" => duration_ms(started),
    };
    match result {
        Ok(()) => {
            log_ok(sink, buffer, LogLevel::Info, TRANSFER, "remote branch deleted", fields);
            RemoteDeletionOutcome {
                requested: true,
                state: REMOTE_DELETED.to_string(),
                branch: Some(target.tracking.clone()),
                error: None,
            }
        }
        Err(error) => {
            let error = redact_credentials(&error);
            log_failure(sink, buffer, TRANSFER, "remote branch deletion failed", &error, fields);
            RemoteDeletionOutcome {
                requested: true,
                state: REMOTE_FAILED.to_string(),
                branch: Some(target.tracking.clone()),
                error: Some(error),
            }
        }
    }
}
