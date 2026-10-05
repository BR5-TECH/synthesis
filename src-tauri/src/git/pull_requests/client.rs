//! The GitHub transport of the pull request operations (GTC-FR-DWVY).
//!
//! Direct HTTPS against GitHub's REST API through `ureq`, as the other GitHub
//! clients of this application do. The trait is what makes every pull request
//! operation testable without a network: the production implementation is the
//! only code in this module tree that opens a socket.

use std::time::Duration;

use serde_json::Value;

/// The one host a token is ever sent to (GTC-FR-DWVY).
pub(crate) const API: &str = "https://api.github.com";

/// Rows GitHub returns on one page. Its largest allowed value.
pub(crate) const PAGE_SIZE: usize = 100;

/// Pages one read follows: `MAX_PAGES * PAGE_SIZE` is the 1000-row limit of
/// GTC-FR-GXUB and GTC-FR-CKTM.
pub(crate) const MAX_PAGES: usize = 10;

/// Bound for one whole request, including its connection and its body.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// Bound for opening one connection.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// How a request to GitHub failed. A fixed set, so no transport message (which
/// can echo the request, and so the credential) ever reaches a caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GithubFailure {
    /// HTTP 404 or 410: GitHub has no such resource for this token.
    NotFound,
    /// HTTP 401 or 403: GitHub refused the token.
    Rejected,
    /// Anything else: no answer in time, a transport error, another status, or
    /// a body that is not the JSON GitHub documents.
    Unreachable,
}

/// One authenticated read of GitHub's REST API.
pub(crate) trait GithubPullRequests: Send + Sync {
    /// GET `path` (starting with `/`, query included) on `api.github.com` and
    /// return the JSON body of a successful answer.
    fn get_json(&self, secret: &str, path: &str) -> Result<Value, GithubFailure>;
}

/// The production client.
pub(crate) struct HttpGithubPullRequests;

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .timeout_connect(Some(CONNECT_TIMEOUT))
        // A redirect can only carry the token to the host that issued it.
        .redirect_auth_headers(ureq::config::RedirectAuthHeaders::SameHost)
        .build()
        .into()
}

/// Map a transport error to a fixed failure. Never reads the error's text.
pub(crate) fn classify(error: &ureq::Error) -> GithubFailure {
    match error {
        ureq::Error::StatusCode(401 | 403) => GithubFailure::Rejected,
        ureq::Error::StatusCode(404 | 410) => GithubFailure::NotFound,
        _ => GithubFailure::Unreachable,
    }
}

impl GithubPullRequests for HttpGithubPullRequests {
    fn get_json(&self, secret: &str, path: &str) -> Result<Value, GithubFailure> {
        // The host is fixed here, so the token cannot be sent anywhere else.
        if !path.starts_with('/') {
            return Err(GithubFailure::Unreachable);
        }
        let url = format!("{API}{path}");
        let mut response = agent()
            .get(&url)
            .header("Authorization", &format!("Bearer {secret}"))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            // GitHub rejects a request with no User-Agent, which would
            // otherwise read as a bad token.
            .header("User-Agent", "synthesis")
            .call()
            .map_err(|error| classify(&error))?;
        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|_| GithubFailure::Unreachable)?;
        serde_json::from_str(&body).map_err(|_| GithubFailure::Unreachable)
    }
}

/// What a paginated read found.
pub(crate) struct Pages {
    pub items: Vec<Value>,
    /// GitHub held more rows than the read limit allows.
    pub truncated: bool,
}

/// Read a JSON array list page by page, `PAGE_SIZE` rows at a time, up to
/// `MAX_PAGES` pages.
///
/// `path` may already hold a query string. A failing page, the first or a later
/// one, fails the whole read: a partial list is never returned as if it were
/// complete (GTC-FR-DWVY). `truncated` is true when a page past the limit still
/// held rows; that probe is one more request, made only when the limit was
/// reached with every page full.
pub(crate) fn read_pages(
    client: &dyn GithubPullRequests,
    secret: &str,
    path: &str,
) -> Result<Pages, GithubFailure> {
    let separator = if path.contains('?') { '&' } else { '?' };
    let mut items: Vec<Value> = Vec::new();
    for page in 1..=MAX_PAGES + 1 {
        let url = format!("{path}{separator}per_page={PAGE_SIZE}&page={page}");
        let body = client.get_json(secret, &url)?;
        let Value::Array(rows) = body else {
            return Err(GithubFailure::Unreachable);
        };
        if page > MAX_PAGES {
            return Ok(Pages { items, truncated: !rows.is_empty() });
        }
        let count = rows.len();
        items.extend(rows);
        if count < PAGE_SIZE {
            break;
        }
    }
    Ok(Pages { items, truncated: false })
}
