//! The pull requests of the project's GitHub repository
//! (`../../specifications/core/GTC-git.md`, GTC-FR-GXUB, GTC-FR-ZIHE,
//! GTC-FR-CKTM, GTC-FR-DWVY).
//!
//! Three read operations: the list of pull requests of one state, one pull
//! request's header and description, and one pull request's full timeline. Each
//! one finds the owner and the repository of the project's primary remote,
//! resolves the project's GitHub token (GTC-FR-09, GTC-FR-10) before any
//! request is made, and reads `api.github.com` through the `GithubPullRequests`
//! trait. None of them writes anything, and none of them keeps the token.
//!
//! The commands are `async` and run their blocking reads on a worker thread, so
//! a slow GitHub never holds the thread that serves `invoke` calls.

pub(crate) mod client;
pub(crate) mod create;
mod model;
mod timeline;

use std::path::Path;
use std::time::Instant;

use tauri::{Manager, State};

use crate::changes::{self};
use crate::github_publication::remotes::parse_github_remote;
use crate::github_tokens::{self, GithubTokens};
use crate::global_settings::GlobalSettingsStore;
use crate::log_fields;
use crate::logging::{self, Fields, LogBuffer, LogLevel, LogSink};
use crate::project::ProjectState;

use super::{duration_ms, ERR_NO_REMOTE_CONFIGURED, TRANSFER};
use client::{read_pages, GithubFailure, GithubPullRequests, HttpGithubPullRequests};
pub use create::{
    PullRequestHeadState,
    ERR_PULL_REQUEST_EXISTS, ERR_PULL_REQUEST_REJECTED, ERR_PULL_REQUEST_TITLE_REQUIRED,
};
pub use model::{
    CreatedPullRequest, PullRequestDetail, PullRequestSummary, PullRequestTimeline,
    PullRequestTimelineItem,
};

/// The remote is not on `github.com`, so there are no pull requests to read.
pub const ERR_NOT_A_GITHUB_REMOTE: &str = "not_a_github_remote";
/// GitHub has no pull request of that number.
pub const ERR_PULL_REQUEST_NOT_FOUND: &str = "pull_request_not_found";
/// GitHub refused the project's token.
pub const ERR_GITHUB_TOKEN_REJECTED: &str = "github_token_rejected";
/// `list_pull_requests` was asked for a state other than `open` or `closed`.
pub const ERR_INVALID_PULL_REQUEST_STATE: &str = "invalid_pull_request_state";

const MSG_READ_FAILED: &str = "pull request read failed";
const MSG_READ_DONE: &str = "pull request read finished";

/// The repository a read is about, and the credential it presents.
struct Target {
    owner: String,
    repo: String,
    secret: String,
}

/// Is `part` safe to place in a request path? GitHub names are letters, digits,
/// `-`, `_` and `.`. Anything else is not an owner or a repository of GitHub.
fn is_path_safe(part: &str) -> bool {
    !part.is_empty()
        && part
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Find the repository of the primary remote and resolve the token.
///
/// The checks run in the order of what they cost: nothing leaves the machine
/// until the remote is known to be on GitHub and the token is known to exist
/// (GTC-FR-05, GTC-FR-10).
fn resolve_target(
    root: &Path,
    store: &GlobalSettingsStore,
    tokens: &GithubTokens,
    project_key: &str,
) -> Result<Target, String> {
    let repo = changes::open_repo(root)?;
    let remote_name =
        changes::primary_remote_name(&repo).ok_or_else(|| ERR_NO_REMOTE_CONFIGURED.to_string())?;
    let url = repo
        .find_remote(&remote_name)
        .map(|remote| remote.url().unwrap_or_default().to_string())
        .map_err(|_| ERR_NO_REMOTE_CONFIGURED.to_string())?;
    let (owner, name) = parse_github_remote(&url)
        .filter(|(owner, name)| is_path_safe(owner) && is_path_safe(name))
        .ok_or_else(|| ERR_NOT_A_GITHUB_REMOTE.to_string())?;
    // The secret exists only for the length of this operation (GTC-FR-09).
    let secret = github_tokens::resolve_github_token_secret(store, tokens, project_key)?;
    Ok(Target { owner, repo: name, secret })
}

/// The typed error of a failed request. `missing` is what a 404 means to the
/// operation that asked.
fn failure_error(failure: GithubFailure, missing: &str) -> String {
    match failure {
        GithubFailure::NotFound => missing.to_string(),
        GithubFailure::Rejected => ERR_GITHUB_TOKEN_REJECTED.to_string(),
        GithubFailure::Unreachable => github_tokens::ERR_GITHUB_UNREACHABLE.to_string(),
    }
}

/// Whether an error is a refusal made before or without a request, or a failure
/// of the request itself. The first is a `WARN`, the second an `ERROR`.
fn is_refusal(error: &str) -> bool {
    super::is_refusal(error)
        || matches!(
            error,
            ERR_NOT_A_GITHUB_REMOTE
                | ERR_INVALID_PULL_REQUEST_STATE
                | ERR_PULL_REQUEST_NOT_FOUND
                | ERR_PULL_REQUEST_EXISTS
                | ERR_PULL_REQUEST_TITLE_REQUIRED
        ) || error.starts_with(ERR_PULL_REQUEST_REJECTED)
}

/// Run one read and report it. `work` gets the resolved target; `outcome` turns
/// its result into the counts the record carries. Records hold names, numbers,
/// and counts only: never the token, never text of a pull request.
fn run_reported<S, T>(
    sink: &S,
    buffer: &'static LogBuffer,
    operation: &str,
    mut fields: Fields,
    resolve: impl FnOnce() -> Result<Target, String>,
    work: impl FnOnce(&Target) -> Result<T, String>,
    outcome: impl FnOnce(&T) -> Fields,
) -> Result<T, String>
where
    S: LogSink + Clone + Send + 'static,
{
    let started = Instant::now();
    fields.insert("operation".to_string(), serde_json::json!(operation));
    logging::log_debug(sink, buffer, TRANSFER, "pull request read started", fields.clone());

    let result = resolve().and_then(|target| {
        fields.insert("owner".to_string(), serde_json::json!(target.owner));
        fields.insert("repo".to_string(), serde_json::json!(target.repo));
        work(&target)
    });
    fields.insert("durationMs".to_string(), serde_json::json!(duration_ms(started)));
    match &result {
        Ok(value) => {
            fields.extend(outcome(value));
            logging::log(sink, buffer, LogLevel::Info, TRANSFER, MSG_READ_DONE, fields);
        }
        Err(error) => {
            fields.insert("error".to_string(), serde_json::json!(error));
            let level = if is_refusal(error) { LogLevel::Warn } else { LogLevel::Error };
            logging::log(sink, buffer, level, TRANSFER, MSG_READ_FAILED, fields);
        }
    }
    result
}

/// GTC-FR-GXUB: the pull requests of `state`, newest update first.
pub(crate) fn list_pull_requests_reported<S>(
    sink: &S,
    buffer: &'static LogBuffer,
    root: &Path,
    store: &GlobalSettingsStore,
    tokens: &GithubTokens,
    project_key: &str,
    client: &dyn GithubPullRequests,
    state: &str,
) -> Result<Vec<PullRequestSummary>, String>
where
    S: LogSink + Clone + Send + 'static,
{
    // The state is checked first: a state GitHub would not take is no reason
    // to open the repository or the keychain.
    let state = match state {
        "open" | "closed" => state,
        other => {
            let error = ERR_INVALID_PULL_REQUEST_STATE;
            logging::log_warn(
                sink,
                buffer,
                TRANSFER,
                MSG_READ_FAILED,
                log_fields! {
                    "operation" => "list_pull_requests",
                    "error" => error,
                    "stateLength" => other.len(),
                },
            );
            return Err(error.to_string());
        }
    };
    let truncated = std::cell::Cell::new(false);
    run_reported(
        sink,
        buffer,
        "list_pull_requests",
        log_fields! { "state" => state },
        || resolve_target(root, store, tokens, project_key),
        |target| {
            let path = format!(
                "/repos/{}/{}/pulls?state={state}&sort=updated&direction=desc",
                target.owner, target.repo
            );
            // A repository the token cannot see answers 404 as well; for a list
            // that means the token, not a missing pull request.
            let pages = read_pages(client, &target.secret, &path)
                .map_err(|failure| failure_error(failure, ERR_GITHUB_TOKEN_REJECTED))?;
            truncated.set(pages.truncated);
            let mut rows = pages
                .items
                .iter()
                .map(model::summary_from_json)
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| github_tokens::ERR_GITHUB_UNREACHABLE.to_string())?;
            // GitHub already answers newest first. Sorting again, stably, keeps
            // the order the contract states whatever the answer was.
            rows.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
            Ok(rows)
        },
        |rows| log_fields! { "count" => rows.len(), "truncated" => truncated.get() },
    )
}

/// GTC-FR-ZIHE: the header fields and the description of pull request `id`.
pub(crate) fn get_pull_request_detail_reported<S>(
    sink: &S,
    buffer: &'static LogBuffer,
    root: &Path,
    store: &GlobalSettingsStore,
    tokens: &GithubTokens,
    project_key: &str,
    client: &dyn GithubPullRequests,
    id: u64,
) -> Result<PullRequestDetail, String>
where
    S: LogSink + Clone + Send + 'static,
{
    run_reported(
        sink,
        buffer,
        "get_pull_request_detail",
        log_fields! { "number" => id },
        || resolve_target(root, store, tokens, project_key),
        |target| {
            let path = format!("/repos/{}/{}/pulls/{id}", target.owner, target.repo);
            let body = client
                .get_json(&target.secret, &path)
                .map_err(|failure| failure_error(failure, ERR_PULL_REQUEST_NOT_FOUND))?;
            model::detail_from_json(&body)
                .ok_or_else(|| github_tokens::ERR_GITHUB_UNREACHABLE.to_string())
        },
        |detail| log_fields! { "state" => detail.state },
    )
}

/// GTC-FR-CKTM: the conversation and activity of pull request `id`, oldest
/// first.
///
/// Two reads feed it: the issue timeline, and the pull request's review
/// comments. The timeline carries review comments inside its `line-commented`
/// events when GitHub includes them, and the second read adds any it left out;
/// each comment appears once, by its id.
pub(crate) fn list_pull_request_timeline_reported<S>(
    sink: &S,
    buffer: &'static LogBuffer,
    root: &Path,
    store: &GlobalSettingsStore,
    tokens: &GithubTokens,
    project_key: &str,
    client: &dyn GithubPullRequests,
    id: u64,
) -> Result<PullRequestTimeline, String>
where
    S: LogSink + Clone + Send + 'static,
{
    run_reported(
        sink,
        buffer,
        "list_pull_request_timeline",
        log_fields! { "number" => id },
        || resolve_target(root, store, tokens, project_key),
        |target| {
            let base = format!("/repos/{}/{}", target.owner, target.repo);
            let read = |path: String| {
                read_pages(client, &target.secret, &path)
                    .map_err(|failure| failure_error(failure, ERR_PULL_REQUEST_NOT_FOUND))
            };
            // Either read failing, on any page, fails the operation: a timeline
            // missing a part of itself is not reported as complete (GTC-FR-DWVY).
            let events = read(format!("{base}/issues/{id}/timeline"))?;
            let review_comments = read(format!("{base}/pulls/{id}/comments"))?;
            Ok(PullRequestTimeline {
                items: timeline::build_timeline(&events.items, &review_comments.items),
                truncated: events.truncated || review_comments.truncated,
            })
        },
        |timeline| {
            log_fields! { "itemCount" => timeline.items.len(), "truncated" => timeline.truncated }
        },
    )
}

/// The wait of a command that could not run its work.
fn join_error(error: tauri::Error) -> String {
    format!("{MSG_READ_FAILED}: {error}")
}

/// GTC-FR-GXUB: the pull requests of the primary GitHub remote, in one state.
#[tauri::command]
pub async fn list_pull_requests(
    state: String,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<Vec<PullRequestSummary>, String> {
    let root = project.require_root()?;
    let project_key = project.slot_key();
    // The managed state is resolved inside the closure: a `State` borrows the
    // invoke context and cannot cross onto another thread.
    tauri::async_runtime::spawn_blocking(move || {
        let store = app.state::<GlobalSettingsStore>();
        let tokens = app.state::<GithubTokens>();
        list_pull_requests_reported(
            &app,
            &logging::BUFFER,
            &root,
            &store,
            &tokens,
            &project_key,
            &HttpGithubPullRequests,
            &state,
        )
    })
    .await
    .map_err(join_error)?
}

/// GTC-FR-ZIHE: one pull request's header fields and description.
#[tauri::command]
pub async fn get_pull_request_detail(
    id: u64,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<PullRequestDetail, String> {
    let root = project.require_root()?;
    let project_key = project.slot_key();
    tauri::async_runtime::spawn_blocking(move || {
        let store = app.state::<GlobalSettingsStore>();
        let tokens = app.state::<GithubTokens>();
        get_pull_request_detail_reported(
            &app,
            &logging::BUFFER,
            &root,
            &store,
            &tokens,
            &project_key,
            &HttpGithubPullRequests,
            id,
        )
    })
    .await
    .map_err(join_error)?
}

/// GTC-FR-CKTM: one pull request's full conversation and activity.
#[tauri::command]
pub async fn list_pull_request_timeline(
    id: u64,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<PullRequestTimeline, String> {
    let root = project.require_root()?;
    let project_key = project.slot_key();
    tauri::async_runtime::spawn_blocking(move || {
        let store = app.state::<GlobalSettingsStore>();
        let tokens = app.state::<GithubTokens>();
        list_pull_request_timeline_reported(
            &app,
            &logging::BUFFER,
            &root,
            &store,
            &tokens,
            &project_key,
            &HttpGithubPullRequests,
            id,
        )
    })
    .await
    .map_err(join_error)?
}

/// GTC-FR-MMFM: open a pull request on the primary GitHub remote.
#[tauri::command]
pub async fn create_pull_request(
    title: String,
    body: String,
    base: String,
    head: String,
    draft: bool,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<CreatedPullRequest, String> {
    let root = project.require_root()?;
    let project_key = project.slot_key();
    tauri::async_runtime::spawn_blocking(move || {
        let store = app.state::<GlobalSettingsStore>();
        let tokens = app.state::<GithubTokens>();
        create::create_pull_request_reported(
            &app,
            &logging::BUFFER,
            &root,
            &store,
            &tokens,
            &project_key,
            &HttpGithubPullRequests,
            &title,
            &body,
            &base,
            &head,
            draft,
        )
    })
    .await
    .map_err(join_error)?
}

/// GTC-FR-NEIW: what stands between a local branch and a pull request for it.
#[tauri::command]
pub async fn get_pull_request_head_state(
    head: String,
    base: Option<String>,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<create::PullRequestHeadState, String> {
    let root = project.require_root()?;
    tauri::async_runtime::spawn_blocking(move || {
        create::head_state_reported(&app, &logging::BUFFER, &root, &head, base.as_deref())
    })
    .await
    .map_err(join_error)?
}
