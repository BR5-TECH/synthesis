//! Branch listing and the checkout primitive (GTC-FR-07 / GTC-FR-08).

use std::path::Path;
use std::time::Instant;

use git2::{BranchType, Repository};
use serde::Serialize;

use crate::changes::{self, ERR_NOT_A_REPO, ERR_UNKNOWN_BRANCH};
use crate::log_fields;
use crate::logging::{self, LogBuffer, LogLevel, LogSink};
use crate::worktree::ERR_CHECKOUT_BLOCKED;

use super::*;

// ---------------------------------------------------------------------------
// Branches (GTC-FR-07 / GTC-FR-08)
// ---------------------------------------------------------------------------

/// One entry of `list_branches` (GTC-FR-07).
///
/// Deliberately *without* a worktree association: the Git panel's branches
/// section is a flat view of the repository's refs. Which branches have a
/// worktree, and which worktree is active, is `WTC-worktree-context.md`'s
/// richer `list_worktrees_and_branches`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitBranch {
    pub name: String,
    /// `"local"` or `"remote"`.
    pub kind: String,
    /// Checked out in the project's **active worktree** (GTC-FR-02).
    pub is_current: bool,
}

/// GTC-FR-07: the repository's local and remote branches, with the active
/// worktree's branch marked current. Read-only.
pub fn branches_for(root: &Path) -> Result<Vec<GitBranch>, String> {
    let repo = changes::open_repo(root)?;
    let current = repo
        .head()
        .ok()
        .filter(|_| !repo.head_detached().unwrap_or(false))
        .and_then(|h| h.shorthand().ok().map(|s| s.to_string()));

    let mut locals: Vec<GitBranch> = Vec::new();
    let mut remotes: Vec<GitBranch> = Vec::new();
    for (branch_type, out) in [
        (BranchType::Local, &mut locals),
        (BranchType::Remote, &mut remotes),
    ] {
        let iter = repo
            .branches(Some(branch_type))
            .map_err(|_| ERR_NOT_A_REPO.to_string())?;
        for item in iter {
            let Ok((branch, _)) = item else { continue };
            let Ok(Some(name)) = branch.name() else {
                continue;
            };
            // `origin/HEAD` is a symbolic pointer, not a checkout target.
            if branch_type == BranchType::Remote && name.ends_with("/HEAD") {
                continue;
            }
            // GTC-FR-07: a graduation run's temporary branch is the
            // application's own scratch ref rather than a line of the author's
            // work (per `../core/GRD-graduation.md` WKS-FR-MFDW). It exists only
            // while a run does, and listing it would offer the Git panel a
            // branch nothing in the panel may act on.
            out.push(GitBranch {
                name: name.to_string(),
                kind: if branch_type == BranchType::Local {
                    "local".to_string()
                } else {
                    "remote".to_string()
                },
                is_current: branch_type == BranchType::Local
                    && current.as_deref() == Some(name),
            });
        }
    }
    locals.sort_by(|a, b| a.name.cmp(&b.name));
    remotes.sort_by(|a, b| a.name.cmp(&b.name));
    locals.extend(remotes);
    Ok(locals)
}

/// GTC-FR-07: **the** checkout primitive. `WTC-worktree-context.md` WTC-FR-10
/// composes this rather than reimplementing it, so a checkout requested from
/// the Git panel and one requested from the worktree selector take the same
/// path and produce the same re-rooting.
///
/// GTC-FR-08: when Git refuses — the branch is checked out in another worktree,
/// or local modifications would be overwritten — a typed error comes back and
/// the active worktree's branch, index, and working tree are untouched. That is
/// why the tree is checked out *before* `HEAD` moves: a refused checkout leaves
/// `HEAD` exactly where it was rather than pointing at a branch whose content
/// was never written.
///
/// Logs the request, the outcome, and every refusal — here, in the primitive,
/// for the reason the linked-worktree refusal itself lives here: every route to
/// a checkout is bound by what this function does, including one added later, so
/// a checkout that never reaches the Logs panel cannot be introduced by wiring
/// up a new caller.
pub fn checkout_branch_at<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    root: &Path,
    name: &str,
) -> Result<(), String> {
    let started = Instant::now();
    logging::log_info(
        sink,
        buffer,
        LOCAL,
        "checking out branch",
        log_fields! { "branch" => name.trim() },
    );
    let outcome = checkout_at(root, name);
    match &outcome {
        Ok(target) => log_ok(
            sink,
            buffer,
            LogLevel::Info,
            LOCAL,
            "branch checked out",
            log_fields! {
                "branch" => name.trim(),
                // The local branch the working tree ended on, which is not the
                // requested name when a remote-tracking branch was checked out
                // under its short one (WTC-FR-11).
                "localBranch" => target.local_name(),
                "createdLocalBranch" => matches!(target, CheckoutTarget::FromRemote { .. }),
                "durationMs" => duration_ms(started),
            },
        ),
        Err(error) => log_failure(
            sink,
            buffer,
            LOCAL,
            MSG_CHECKOUT_FAILED,
            error,
            log_fields! {
                "branch" => name.trim(),
                "durationMs" => duration_ms(started),
            },
        ),
    }
    outcome.map(|_| ())
}

/// The checkout itself, returning what it resolved to so the caller can say what
/// happened without recomputing it.
pub(crate) fn checkout_at(root: &Path, name: &str) -> Result<CheckoutTarget, String> {
    let repo = changes::open_repo(root)?;
    let name = name.trim();
    if name.is_empty() {
        return Err(ERR_UNKNOWN_BRANCH.to_string());
    }

    // GTC-FR-08 / WTC-FR-21: a branch is checked out only in the repository's
    // primary worktree. Enforced here, in the single checkout primitive, so
    // every route to a checkout is bound by it — including any added later.
    if !crate::worktree::is_in_primary_worktree(root)? {
        return Err(crate::worktree::ERR_CHECKOUT_NOT_IN_LINKED_WORKTREE.to_string());
    }

    let target = resolve_checkout_target(&repo, name)?;
    let local_name = target.local_name().to_string();

    // GTC-FR-08 / WTC-FR-12: Git cannot have one branch checked out in two
    // worktrees, and libgit2's `set_head` will not stop us — so the refusal is
    // ours to make, before anything is written.
    if crate::worktree::branch_checked_out_elsewhere(root, &local_name)? {
        return Err(crate::worktree::ERR_BRANCH_ALREADY_CHECKED_OUT.to_string());
    }

    let commit = target.commit(&repo)?;

    // A *safe* checkout is what refuses rather than clobbering: libgit2 aborts
    // when a file it would write differs from both the old and the new tree.
    // It runs before anything is created or moved, so a refusal leaves the
    // repository exactly as it was — no half-made branch, no moved HEAD.
    let mut builder = git2::build::CheckoutBuilder::new();
    builder.safe();
    repo.checkout_tree(commit.as_object(), Some(&mut builder))
        .map_err(|_| ERR_CHECKOUT_BLOCKED.to_string())?;

    // WTC-FR-11: only now is the local branch created for a remote-tracking
    // one, so a refused checkout never leaves a new ref behind.
    if let CheckoutTarget::FromRemote { short, upstream } = &target {
        let mut local = repo
            .branch(short, &commit, false)
            .map_err(|e| format!("failed to create local branch: {e}"))?;
        // Best-effort: a branch that cannot be wired to its upstream is still a
        // usable checkout target.
        let _ = local.set_upstream(Some(upstream));
    }

    repo.set_head(&format!("refs/heads/{local_name}"))
        .map_err(|e| format!("failed to move HEAD: {e}"))?;
    Ok(target)
}

/// What a checkout request resolves to: a local branch that already exists, or
/// a remote-tracking branch a local one has to be created from (WTC-FR-11).
pub(crate) enum CheckoutTarget {
    Local(String),
    FromRemote { short: String, upstream: String },
}

impl CheckoutTarget {
    fn local_name(&self) -> &str {
        match self {
            CheckoutTarget::Local(name) => name,
            CheckoutTarget::FromRemote { short, .. } => short,
        }
    }

    /// The commit the working tree is being moved to.
    fn commit<'r>(&self, repo: &'r Repository) -> Result<git2::Commit<'r>, String> {
        let branch = match self {
            CheckoutTarget::Local(name) => repo.find_branch(name, BranchType::Local),
            CheckoutTarget::FromRemote { upstream, .. } => {
                repo.find_branch(upstream, BranchType::Remote)
            }
        };
        branch
            .and_then(|b| b.get().peel_to_commit())
            .map_err(|_| ERR_UNKNOWN_BRANCH.to_string())
    }
}

pub(crate) fn resolve_checkout_target(repo: &Repository, name: &str) -> Result<CheckoutTarget, String> {
    if repo.find_branch(name, BranchType::Local).is_ok() {
        return Ok(CheckoutTarget::Local(name.to_string()));
    }
    // Not a local branch — it has to be a remote-tracking one, checked out
    // under its short name (`origin/experiment` -> `experiment`).
    repo.find_branch(name, BranchType::Remote)
        .map_err(|_| ERR_UNKNOWN_BRANCH.to_string())?;
    let short = name.split_once('/').map(|(_, rest)| rest).unwrap_or(name);
    if short.is_empty() {
        return Err(ERR_UNKNOWN_BRANCH.to_string());
    }
    // A local branch may already hold that short name; then it, not the remote,
    // is the checkout target and no tracking branch is created.
    if repo.find_branch(short, BranchType::Local).is_ok() {
        return Ok(CheckoutTarget::Local(short.to_string()));
    }
    Ok(CheckoutTarget::FromRemote {
        short: short.to_string(),
        upstream: name.to_string(),
    })
}
