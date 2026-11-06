//! Linking a pull request to the issues it resolves
//! (`../../../specifications/core/GTC-git.md`, GTC-FR-ZHVO, GTC-FR-QJFC,
//! GTC-FR-WJUG, GTC-FR-GNKQ).
//!
//! GitHub has no public operation that links a pull request to an issue in its
//! Development sidebar. A closing keyword in the description is the one link
//! GitHub reads, so this module adds one line `* Resolves <ref>` for each issue
//! and writes the description back with one update request. It changes no issue
//! and no other field of the pull request. Its records hold the number and the
//! counts, and never the description text.

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::client::GithubPullRequests;
use super::{
    failure_error, is_path_safe, run_reported, PullRequestAccess, Target,
    ERR_PULL_REQUEST_NOT_FOUND,
};
use crate::log_fields;
use crate::logging::{self, LogBuffer, LogSink};

/// GTC-FR-WJUG: the one way this module links an issue.
pub const LINK_WAY_CLOSING_KEYWORD: &str = "closing_keyword";

/// An issue reference whose owner or repository is not a GitHub name. It is
/// never written into a description.
pub const ERR_INVALID_ISSUE_REFERENCE: &str = "invalid_issue_reference";

/// GTC-FR-QJFC: one issue a pull request resolves.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestIssueRef {
    pub owner: String,
    pub repository: String,
    pub number: u64,
}

impl PullRequestIssueRef {
    /// Two references name the same issue. GitHub names are case-insensitive.
    fn same_issue(&self, other: &PullRequestIssueRef) -> bool {
        self.number == other.number
            && self.owner.eq_ignore_ascii_case(&other.owner)
            && self.repository.eq_ignore_ascii_case(&other.repository)
    }

    /// The issue is in the repository `owner/repo`.
    fn is_in(&self, owner: &str, repo: &str) -> bool {
        self.owner.eq_ignore_ascii_case(owner) && self.repository.eq_ignore_ascii_case(repo)
    }
}

/// GTC-FR-WJUG: one issue the pull request is linked to, and how.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkedPullRequestIssue {
    pub issue: PullRequestIssueRef,
    pub way: String,
}

/// GTC-FR-WJUG: one issue the pull request could not be linked to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FailedPullRequestIssue {
    pub issue: PullRequestIssueRef,
    pub error: String,
}

/// GTC-FR-ZHVO: what the link operation did.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestLinkOutcome {
    pub linked: Vec<LinkedPullRequestIssue>,
    pub failed: Vec<FailedPullRequestIssue>,
    /// GTC-FR-GNKQ: the base of the pull request is the default branch.
    pub closes_on_merge: bool,
}

/// GTC-FR-QJFC: each issue once, in the order of `issues`.
fn distinct(issues: &[PullRequestIssueRef]) -> Vec<PullRequestIssueRef> {
    let mut out: Vec<PullRequestIssueRef> = Vec::new();
    for issue in issues {
        if !out.iter().any(|seen| seen.same_issue(issue)) {
            out.push(issue.clone());
        }
    }
    out
}

/// GTC-FR-QJFC: `#<number>` in the pull request's own repository, else
/// `<owner>/<repository>#<number>`.
pub(crate) fn issue_ref_text(issue: &PullRequestIssueRef, owner: &str, repo: &str) -> String {
    if issue.is_in(owner, repo) {
        format!("#{}", issue.number)
    } else {
        format!("{}/{}#{}", issue.owner, issue.repository, issue.number)
    }
}

/// GTC-FR-ZHVO: the description already resolves `issue` with a closing
/// keyword, in any of the forms GitHub reads: `#N` (own repository alone),
/// `owner/repo#N`, or the address of the issue's page.
pub(crate) fn already_resolves(
    body: &str,
    issue: &PullRequestIssueRef,
    target: (&str, &str, &str),
) -> bool {
    let (host, owner, repo) = target;
    let full = format!(
        "{}/{}#{}",
        regex::escape(&issue.owner),
        regex::escape(&issue.repository),
        issue.number
    );
    let page = format!(
        "https://{}/{}/{}/issues/{}",
        regex::escape(host),
        regex::escape(&issue.owner),
        regex::escape(&issue.repository),
        issue.number
    );
    let mut forms = vec![full, page];
    if issue.is_in(owner, repo) {
        forms.push(format!("#{}", issue.number));
    }
    let pattern =
        format!(r"(?i)\b(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?):?\s+(?:{})\b", forms.join("|"));
    Regex::new(&pattern).map(|re| re.is_match(body)).unwrap_or(false)
}

/// GTC-FR-ZHVO: the description with one block of `* Resolves <ref>` lines
/// after a blank line. An empty description holds the block alone.
pub(crate) fn body_with_block(body: &str, lines: &[String]) -> String {
    let block = lines.join("\n");
    let kept = body.trim_end();
    if kept.is_empty() {
        block
    } else {
        format!("{kept}\n\n{block}")
    }
}

/// GTC-FR-GNKQ: the base branch of the pull request is the default branch that
/// GitHub reports. A read that fails makes it false.
fn base_is_default<S>(
    sink: &S,
    buffer: &'static LogBuffer,
    client: &dyn GithubPullRequests,
    target: &Target,
    base: &str,
    number: u64,
) -> bool
where
    S: LogSink + Clone + Send + 'static,
{
    let path = format!("/repos/{}/{}", target.owner, target.repo);
    match client.get_json(&target.secret, &path) {
        Ok(repo) => repo.get("default_branch").and_then(Value::as_str) == Some(base),
        Err(failure) => {
            logging::log_warn(
                sink,
                buffer,
                super::super::TRANSFER,
                "pull request default branch read failed",
                log_fields! {
                    "operation" => "link_pull_request_issues",
                    "number" => number,
                    "error" => failure_error(failure, ERR_PULL_REQUEST_NOT_FOUND),
                },
            );
            false
        }
    }
}

/// GTC-FR-ZHVO … GTC-FR-GNKQ: link pull request `number` to each of `issues`
/// by a closing keyword in its description.
///
/// A failure to resolve the repository or the token, or to read the pull
/// request, is the typed error of the pull request operations. A failure of the
/// update puts every issue in `failed` with the typed cause, and the
/// description stays as it was.
pub(crate) fn link_pull_request_issues_reported<S>(
    sink: &S,
    buffer: &'static LogBuffer,
    access: &PullRequestAccess<'_>,
    number: u64,
    issues: &[PullRequestIssueRef],
) -> Result<PullRequestLinkOutcome, String>
where
    S: LogSink + Clone + Send + 'static,
{
    let issues = distinct(issues);
    // GTC-FR-WJUG: an empty list makes no request.
    if issues.is_empty() {
        logging::log_debug(
            sink,
            buffer,
            super::super::TRANSFER,
            "pull request issue link skipped",
            log_fields! { "number" => number, "issueCount" => 0 },
        );
        return Ok(PullRequestLinkOutcome::default());
    }
    let (valid, invalid): (Vec<_>, Vec<_>) = issues
        .into_iter()
        .partition(|issue| is_path_safe(&issue.owner) && is_path_safe(&issue.repository));
    let invalid: Vec<FailedPullRequestIssue> = invalid
        .into_iter()
        .map(|issue| FailedPullRequestIssue {
            issue,
            error: ERR_INVALID_ISSUE_REFERENCE.to_string(),
        })
        .collect();
    let issue_count = valid.len() + invalid.len();
    let invalid_count = invalid.len();
    run_reported(
        sink,
        buffer,
        "link_pull_request_issues",
        log_fields! { "number" => number, "issueCount" => issue_count },
        access,
        |target, client| {
            let path = format!("/repos/{}/{}/pulls/{number}", target.owner, target.repo);
            let pull = client
                .get_json(&target.secret, &path)
                .map_err(|failure| failure_error(failure, ERR_PULL_REQUEST_NOT_FOUND))?;
            let body = pull.get("body").and_then(Value::as_str).unwrap_or_default();
            let base = pull.get("base").and_then(|b| b.get("ref")).and_then(Value::as_str);
            let closes_on_merge = match base {
                Some(base) => base_is_default(sink, buffer, client, target, base, number),
                None => false,
            };
            let scope = (target.host.as_str(), target.owner.as_str(), target.repo.as_str());
            let lines: Vec<String> = valid
                .iter()
                .filter(|issue| !already_resolves(body, issue, scope))
                .map(|issue| {
                    format!("* Resolves {}", issue_ref_text(issue, &target.owner, &target.repo))
                })
                .collect();
            let mut outcome =
                PullRequestLinkOutcome { linked: Vec::new(), failed: invalid, closes_on_merge };
            let written = if lines.is_empty() {
                Ok(())
            } else {
                client
                    .patch_json(
                        &target.secret,
                        &path,
                        &json!({ "body": body_with_block(body, &lines) }),
                    )
                    .map(|_| ())
                    .map_err(super::create::write_error)
            };
            match written {
                Ok(()) => {
                    outcome.linked = valid
                        .into_iter()
                        .map(|issue| LinkedPullRequestIssue {
                            issue,
                            way: LINK_WAY_CLOSING_KEYWORD.to_string(),
                        })
                        .collect();
                }
                Err(error) => {
                    // The code alone: a refusal of GitHub carries its own words
                    // after the colon, which can quote the description.
                    let code = error.split(':').next().unwrap_or_default().trim().to_string();
                    logging::log_warn(
                        sink,
                        buffer,
                        super::super::TRANSFER,
                        "pull request description update failed",
                        log_fields! {
                            "operation" => "link_pull_request_issues",
                            "number" => number,
                            "issueCount" => issue_count,
                            "error" => code,
                        },
                    );
                    outcome.failed.extend(
                        valid
                            .into_iter()
                            .map(|issue| FailedPullRequestIssue { issue, error: error.clone() }),
                    );
                }
            }
            Ok(outcome)
        },
        |outcome| {
            log_fields! {
                "invalidCount" => invalid_count,
                "linkedCount" => outcome.linked.len(),
                "failedCount" => outcome.failed.len(),
                "closesOnMerge" => outcome.closes_on_merge,
            }
        },
    )
}
