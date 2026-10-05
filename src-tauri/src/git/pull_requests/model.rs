//! The payload shapes of the pull request operations, and their mapping from
//! GitHub's JSON (GTC-FR-GXUB, GTC-FR-ZIHE, GTC-FR-CKTM).
//!
//! Every payload is built field by field from the answer, so nothing GitHub
//! sends that is not named here — a URL that carries a query token, for one —
//! can reach the frontend.

use serde::Serialize;
use serde_json::Value;

/// The name GitHub shows for an account that no longer exists.
const GHOST: &str = "ghost";

/// One row of the pull request list (GTC-FR-GXUB).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestSummary {
    pub number: u64,
    pub title: String,
    /// `"open"`, `"closed"`, or `"merged"`.
    pub state: String,
    pub is_draft: bool,
    pub author: String,
    pub head_branch: String,
    pub base_branch: String,
    pub created_at: String,
    pub updated_at: String,
}

/// One pull request's header fields and description (GTC-FR-ZIHE).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestDetail {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub is_draft: bool,
    pub author: String,
    /// The description as written. Empty when the author wrote none.
    pub body: String,
    pub url: String,
    pub head_branch: String,
    pub base_branch: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merged_at: Option<String>,
}

/// One pull request's conversation and activity, oldest first (GTC-FR-CKTM).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestTimeline {
    pub items: Vec<PullRequestTimelineItem>,
    /// True when GitHub held more than this module reads.
    pub truncated: bool,
}

/// One entry of the timeline.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestTimelineItem {
    pub id: String,
    /// `"comment"`, `"review"`, `"review_comment"`, `"commit"`, or `"event"`.
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
}

/// A string field, or `None` when it is absent or not text.
pub(super) fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

/// A string field of a nested object, such as `head.ref`.
pub(super) fn nested_text(value: &Value, object: &str, key: &str) -> Option<String> {
    value.get(object).and_then(|inner| text(inner, key))
}

/// The state a pull request is shown in: `merged` when GitHub holds a merge
/// time, whatever its own `state` says (GTC-FR-GXUB).
fn pull_request_state(value: &Value) -> String {
    if value.get("merged_at").is_some_and(|merged| !merged.is_null()) {
        return "merged".to_string();
    }
    match value.get("state").and_then(Value::as_str) {
        Some("open") => "open",
        _ => "closed",
    }
    .to_string()
}

fn author(value: &Value) -> String {
    nested_text(value, "user", "login").unwrap_or_else(|| GHOST.to_string())
}

/// The row of one pull request in a list. `None` for an entry with no number,
/// which is not a pull request this module can name.
pub(super) fn summary_from_json(value: &Value) -> Option<PullRequestSummary> {
    Some(PullRequestSummary {
        number: value.get("number")?.as_u64()?,
        title: text(value, "title").unwrap_or_default(),
        state: pull_request_state(value),
        is_draft: value.get("draft").and_then(Value::as_bool).unwrap_or(false),
        author: author(value),
        head_branch: nested_text(value, "head", "ref").unwrap_or_default(),
        base_branch: nested_text(value, "base", "ref").unwrap_or_default(),
        created_at: text(value, "created_at").unwrap_or_default(),
        updated_at: text(value, "updated_at").unwrap_or_default(),
    })
}

/// One pull request's detail. `None` for an answer with no number.
pub(super) fn detail_from_json(value: &Value) -> Option<PullRequestDetail> {
    let summary = summary_from_json(value)?;
    Some(PullRequestDetail {
        number: summary.number,
        title: summary.title,
        state: summary.state,
        is_draft: summary.is_draft,
        author: summary.author,
        // `null` is an absent description; it is returned as an empty one.
        body: text(value, "body").unwrap_or_default(),
        url: text(value, "html_url").unwrap_or_default(),
        head_branch: summary.head_branch,
        base_branch: summary.base_branch,
        created_at: summary.created_at,
        updated_at: summary.updated_at,
        closed_at: text(value, "closed_at"),
        merged_at: text(value, "merged_at"),
    })
}
