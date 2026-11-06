//! The GitHub reads of the main branch write guard (`GTC-git.md` GTC-FR-UOUS,
//! GTC-FR-VZJR).
//!
//! Two reads: the repository, for the default branch that GitHub reports, and
//! one branch, for whether GitHub protects it. Both resolve the project's token
//! as the pull request reads do (GTC-FR-09), and neither keeps it.

use serde_json::Value;

use super::{failure_error, resolve_target, PullRequestAccess, ERR_GITHUB_TOKEN_REJECTED};
use super::{ERR_NOT_A_GITHUB_REMOTE, ERR_NO_REMOTE_CONFIGURED};
use crate::changes::ERR_NOT_A_REPO;
use crate::github_tokens;

use super::client::GithubFailure;

/// What GitHub says about the repository of the primary remote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GithubView {
    /// The primary remote is not a GitHub remote, or there is no remote.
    NotApplicable,
    /// The read failed, with its typed cause.
    Failed(String),
    /// GitHub answered. `default_branch` is the default branch it reports.
    Repository { default_branch: Option<String> },
}

/// GTC-FR-UOUS: read the repository of the primary remote.
pub(crate) fn read_github_view(access: &PullRequestAccess<'_>) -> GithubView {
    let target = match resolve_target(access.project) {
        Ok(target) => target,
        Err(error) if is_not_github(&error) => return GithubView::NotApplicable,
        Err(error) => return GithubView::Failed(error),
    };
    let scoped = access.client.for_host(&target.host);
    let client = scoped.as_deref().unwrap_or(access.client);
    let path = format!("/repos/{}/{}", target.owner, target.repo);
    match client.get_json(&target.secret, &path) {
        Ok(body) => GithubView::Repository {
            default_branch: body
                .get("default_branch")
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
                .map(str::to_string),
        },
        // A repository the token cannot see answers 404 as well, which means
        // the token here.
        Err(failure) => GithubView::Failed(failure_error(failure, ERR_GITHUB_TOKEN_REJECTED)),
    }
}

/// GTC-FR-VZJR: whether GitHub protects `branch`. A branch that GitHub does not
/// hold is not protected.
pub(crate) fn read_protection(
    access: &PullRequestAccess<'_>,
    branch: &str,
) -> Result<bool, String> {
    // A name that is not a valid branch name is never put in a request path.
    if !git2::Reference::is_valid_name(&format!("refs/heads/{branch}")) {
        return Err(crate::changes::ERR_UNKNOWN_BRANCH.to_string());
    }
    let target = resolve_target(access.project)?;
    let scoped = access.client.for_host(&target.host);
    let client = scoped.as_deref().unwrap_or(access.client);
    let path =
        format!("/repos/{}/{}/branches/{}", target.owner, target.repo, encode_branch(branch));
    match client.get_json(&target.secret, &path) {
        Ok(body) => body
            .get("protected")
            .and_then(Value::as_bool)
            .ok_or_else(|| github_tokens::ERR_GITHUB_UNREACHABLE.to_string()),
        Err(GithubFailure::NotFound) => Ok(false),
        Err(failure) => Err(failure_error(failure, ERR_GITHUB_TOKEN_REJECTED)),
    }
}

/// Whether a refusal of the target means there is no GitHub repository to ask.
fn is_not_github(error: &str) -> bool {
    matches!(error, ERR_NO_REMOTE_CONFIGURED | ERR_NOT_A_GITHUB_REMOTE | ERR_NOT_A_REPO)
}

/// A branch name as one part of a request path. Each byte that is not an
/// unreserved character or `/` is percent-encoded, so a name cannot change the
/// request it is put in.
pub(crate) fn encode_branch(branch: &str) -> String {
    let mut out = String::with_capacity(branch.len());
    for byte in branch.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'/') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}
