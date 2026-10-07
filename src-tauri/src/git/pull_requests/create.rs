//! Opening a pull request (`../../../specifications/core/GTC-git.md`,
//! GTC-FR-MMFM, GTC-FR-YQAE) and reading what stands between a branch and one
//! (GTC-FR-NEIW, GTC-FR-YWCP).
//!
//! Creation sends one request to GitHub. It commits nothing and pushes nothing:
//! a branch GitHub cannot see is a state the caller reports to the author, not
//! one this module repairs. The head state is read from local refs and the
//! checkout that holds the branch, and reaches no network.

use std::path::Path;

use git2::{BranchType, Oid, Repository};
use serde_json::json;

use super::client::{GithubPullRequests, GithubWriteFailure};
use super::model::CreatedPullRequest;
use super::{resolve_target, run_reported, ERR_GITHUB_TOKEN_REJECTED};
use crate::changes::{self, ERR_UNKNOWN_BRANCH};
use crate::github_tokens::{self, GithubTokens};
use crate::global_settings::GlobalSettingsStore;
use crate::log_fields;
use crate::logging::{self, LogBuffer, LogSink};

/// The title holds nothing but white space.
pub const ERR_PULL_REQUEST_TITLE_REQUIRED: &str = "pull_request_title_required";
/// GitHub already holds an open pull request for the same head and base.
pub const ERR_PULL_REQUEST_EXISTS: &str = "pull_request_exists";
/// GitHub refused the content of the request. The error text follows the code
/// after a colon.
pub const ERR_PULL_REQUEST_REJECTED: &str = "pull_request_rejected";

/// The words GitHub uses when a pull request for the same branches is open.
const ALREADY_EXISTS: &str = "pull request already exists";

/// The only host a created pull request's page may be on.
const PAGE_PREFIX: &str = "https://github.com/";

/// GTC-FR-YQAE: the typed error of a failed write.
fn write_error(failure: GithubWriteFailure) -> String {
    match failure {
        GithubWriteFailure::NotFound | GithubWriteFailure::Rejected => {
            ERR_GITHUB_TOKEN_REJECTED.to_string()
        }
        GithubWriteFailure::Invalid(reason) => {
            if reason.to_lowercase().contains(ALREADY_EXISTS) {
                ERR_PULL_REQUEST_EXISTS.to_string()
            } else if reason.trim().is_empty() {
                ERR_PULL_REQUEST_REJECTED.to_string()
            } else {
                format!("{ERR_PULL_REQUEST_REJECTED}: {}", reason.trim())
            }
        }
        GithubWriteFailure::Unreachable => github_tokens::ERR_GITHUB_UNREACHABLE.to_string(),
    }
}

/// GTC-FR-MMFM: open a pull request from `head` into `base`.
///
/// The arguments are checked before the repository is opened, so a refusal that
/// needs no network needs no keychain either. The title and the description are
/// sent as given and are never logged.
pub(crate) fn create_pull_request_reported<S>(
    sink: &S,
    buffer: &'static LogBuffer,
    root: &Path,
    store: &GlobalSettingsStore,
    tokens: &GithubTokens,
    project_key: &str,
    client: &dyn GithubPullRequests,
    title: &str,
    body: &str,
    base: &str,
    head: &str,
    draft: bool,
) -> Result<CreatedPullRequest, String>
where
    S: LogSink + Clone + Send + 'static,
{
    let (base, head) = (base.trim(), head.trim());
    if title.trim().is_empty() || head.is_empty() || base.is_empty() {
        let error = if title.trim().is_empty() {
            ERR_PULL_REQUEST_TITLE_REQUIRED
        } else {
            ERR_UNKNOWN_BRANCH
        };
        logging::log_warn(
            sink,
            buffer,
            super::super::TRANSFER,
            "pull request creation refused",
            log_fields! {
                "operation" => "create_pull_request",
                "error" => error,
            },
        );
        return Err(error.to_string());
    }
    run_reported(
        sink,
        buffer,
        "create_pull_request",
        log_fields! { "head" => head, "base" => base, "draft" => draft },
        || resolve_target(root, store, tokens, project_key),
        |target| {
            let path = format!("/repos/{}/{}/pulls", target.owner, target.repo);
            let request = json!({
                "title": title,
                "body": body,
                "head": head,
                "base": base,
                "draft": draft,
            });
            let answer = client
                .post_json(&target.secret, &path, &request)
                .map_err(write_error)?;
            created_from_json(&answer)
                .ok_or_else(|| github_tokens::ERR_GITHUB_UNREACHABLE.to_string())
        },
        |created| log_fields! { "number" => created.number },
    )
}

/// The number and the page of the pull request GitHub answered with. Nothing
/// else of the answer is kept.
fn created_from_json(value: &serde_json::Value) -> Option<CreatedPullRequest> {
    let number = value.get("number")?.as_u64()?;
    let url = value.get("html_url")?.as_str()?;
    url.starts_with(PAGE_PREFIX)
        .then(|| CreatedPullRequest { number, url: url.to_string() })
}

/// GTC-FR-NEIW, GTC-FR-YWCP: what stands between a local branch and a pull
/// request for it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHeadState {
    pub head: String,
    pub base: String,
    pub has_remote: bool,
    pub remote_branch_exists: bool,
    /// Commits the head holds that the remote branch lacks. Absent when the
    /// remote has no branch of that name.
    pub unpushed: Option<u32>,
    pub uncommitted_paths: Vec<String>,
    pub ahead_of_base: u32,
}

/// The tip a base name stands for: a local branch, else the same name on the
/// primary remote, else a remote-tracking name as written.
fn base_tip(repo: &Repository, base: &str, remote: Option<&str>) -> Result<Oid, String> {
    let mut names = vec![base.to_string()];
    if let Some(remote) = remote {
        names.push(format!("{remote}/{base}"));
    }
    for name in names {
        if let Ok(oid) = changes::resolve_branch_commit(repo, &name) {
            return Ok(oid);
        }
    }
    Err(ERR_UNKNOWN_BRANCH.to_string())
}

fn count(repo: &Repository, from: Oid, not_in: Oid) -> u32 {
    repo.graph_ahead_behind(from, not_in)
        .map(|(ahead, _)| ahead as u32)
        .unwrap_or(0)
}

/// The pure half of `get_pull_request_head_state`: everything but the paths of
/// the checkout, which the caller reads from wherever the branch is out.
pub(crate) fn head_state_in(
    repo: &Repository,
    head: &str,
    base: Option<&str>,
    uncommitted_paths: Vec<String>,
) -> Result<PullRequestHeadState, String> {
    let head = head.trim();
    let branch = repo
        .find_branch(head, BranchType::Local)
        .map_err(|_| ERR_UNKNOWN_BRANCH.to_string())?;
    let head_tip = branch
        .get()
        .peel_to_commit()
        .map_err(|_| ERR_UNKNOWN_BRANCH.to_string())?
        .id();
    let remote = changes::primary_remote_name(repo);
    let base = match base.map(str::trim).filter(|b| !b.is_empty()) {
        Some(named) => named.to_string(),
        None => changes::default_branch(repo)?,
    };
    let base_oid = base_tip(repo, &base, remote.as_deref())?;
    let remote_tip = remote.as_deref().and_then(|name| {
        repo.refname_to_id(&format!("refs/remotes/{name}/{head}")).ok()
    });
    Ok(PullRequestHeadState {
        head: head.to_string(),
        base,
        has_remote: remote.is_some(),
        remote_branch_exists: remote_tip.is_some(),
        unpushed: remote_tip.map(|tip| count(repo, head_tip, tip)),
        uncommitted_paths,
        ahead_of_base: count(repo, head_tip, base_oid),
    })
}

/// GTC-FR-YWCP: the directory of the checkout that has `head` out, whichever
/// worktree it is. A worktree whose directory is gone holds nothing.
pub(crate) fn checkout_holding(
    worktrees: &[crate::worktree::WorktreeEntry],
    head: &str,
) -> Option<std::path::PathBuf> {
    worktrees
        .iter()
        .find(|w| w.branch.as_deref() == Some(head) && !w.is_missing)
        .map(|w| std::path::PathBuf::from(&w.path))
}

/// GTC-FR-YWCP: the uncommitted paths of the checkout that holds `head`, in
/// whichever worktree it is out, or the working copy of the work stream whose
/// branch it is. Empty when nothing holds it.
fn uncommitted_paths_for<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &Path,
    head: &str,
) -> Vec<String> {
    let held = crate::worktree::context_for(root)
        .ok()
        .and_then(|context| checkout_holding(&context.worktrees, head));
    let tree = held.or_else(|| {
        crate::streams::stream_on_branch(app, head).map(|stream| stream.worktree())
    });
    tree.map(|path| crate::streams::uncommitted_paths_of(&path))
        .unwrap_or_default()
}

/// GTC-FR-NEIW: read the head state and report it.
pub(crate) fn head_state_reported<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
    root: &Path,
    head: &str,
    base: Option<&str>,
) -> Result<PullRequestHeadState, String> {
    let result = changes::open_repo(root).and_then(|repo| {
        let paths = uncommitted_paths_for(app, root, head.trim());
        head_state_in(&repo, head, base, paths)
    });
    match &result {
        Ok(state) => logging::log_debug(
            app,
            buffer,
            super::super::TRANSFER,
            "pull request head state read",
            log_fields! {
                "head" => state.head,
                "base" => state.base,
                "remoteBranchExists" => state.remote_branch_exists,
                "unpushed" => state.unpushed,
                "uncommittedCount" => state.uncommitted_paths.len(),
                "aheadOfBase" => state.ahead_of_base,
            },
        ),
        Err(error) => logging::log_warn(
            app,
            buffer,
            super::super::TRANSFER,
            "pull request head state refused",
            log_fields! { "head" => head, "error" => error },
        ),
    }
    result
}
