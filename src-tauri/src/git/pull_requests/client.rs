//! The GitHub transport of the pull request operations (GTC-FR-DWVY).
//!
//! Direct HTTPS against GitHub's REST API through `ureq`, as the other GitHub
//! clients of this application do. The trait is what makes every pull request
//! operation testable without a network: the production implementation is the
//! only code in this module tree that opens a socket.

use std::time::Duration;

use serde_json::Value;

/// The API base of `github.com`. A request to another host goes through a client
/// that `GithubPullRequests::for_host` returns, which holds that host's base
/// (GTC-FR-DWVY, GTS-FR-PDWB).
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GithubFailure {
    /// HTTP 404 or 410: GitHub has no such resource for this token.
    NotFound,
    /// HTTP 401 or 403: GitHub refused the token.
    Rejected,
    /// Anything else: no answer in time, a transport error, another status, or
    /// a body that is not the JSON GitHub documents.
    Unreachable,
    /// The TLS check refused GitHub's certificate (AAP-FR-HZTB).
    TlsUntrusted(crate::tls::TlsFailure),
}

/// How a write to GitHub failed (GTC-FR-YQAE). Like [`GithubFailure`] it is a
/// fixed set, with one addition: the reason GitHub gave for refusing the
/// content of a request, which holds no credential because GitHub answers about
/// the content and never echoes the token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GithubWriteFailure {
    /// HTTP 404 or 410: GitHub has no such repository for this token.
    NotFound,
    /// HTTP 401 or 403: GitHub refused the token.
    Rejected,
    /// HTTP 422: GitHub refused the content. Carries the reasons GitHub gave.
    Invalid(String),
    /// Anything else.
    Unreachable,
    /// The TLS check refused GitHub's certificate (AAP-FR-HZTB).
    TlsUntrusted(crate::tls::TlsFailure),
}

/// One authenticated read of GitHub's REST API.
pub(crate) trait GithubPullRequests: Send + Sync {
    /// GET `path` (starting with `/`, query included) on `api.github.com` and
    /// return the JSON body of a successful answer.
    fn get_json(&self, secret: &str, path: &str) -> Result<Value, GithubFailure>;

    /// POST `body` as JSON to `path` (starting with `/`) on `api.github.com`
    /// and return the JSON body of a successful answer. An implementation that
    /// cannot write answers `Unreachable`.
    fn post_json(
        &self,
        _secret: &str,
        _path: &str,
        _body: &Value,
    ) -> Result<Value, GithubWriteFailure> {
        Err(GithubWriteFailure::Unreachable)
    }

    /// A client for the API of `host` (normalized), or `None` where this client
    /// serves every host itself, as a test double does.
    fn for_host(&self, _host: &str) -> Option<Box<dyn GithubPullRequests>> {
        None
    }
}

/// The production client for `github.com`. `for_host` gives the client of any
/// other host.
pub(crate) struct HttpGithubPullRequests;

/// The production client for the API base of one host.
pub(crate) struct HostedGithubPullRequests {
    api: String,
}

/// An agent that returns a non-2xx answer as a response, so the body of a 422
/// can be read for the reason GitHub gave.
fn write_agent() -> ureq::Agent {
    crate::tls::ureq_config()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .redirect_auth_headers(ureq::config::RedirectAuthHeaders::SameHost)
        .http_status_as_error(false)
        // A redirected POST may come back as a GET, whose answer is not the
        // pull request. A redirect is a failure to create, not a success.
        .max_redirects(0)
        .build()
        .into()
}

fn agent() -> ureq::Agent {
    crate::tls::ureq_config()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .timeout_connect(Some(CONNECT_TIMEOUT))
        // A redirect can only carry the token to the host that issued it.
        .redirect_auth_headers(ureq::config::RedirectAuthHeaders::SameHost)
        .build()
        .into()
}

/// Map a transport error to a fixed failure. Never reads the error's text.
#[cfg(test)]
pub(crate) fn classify(error: &ureq::Error) -> GithubFailure {
    classify_at(error, API)
}

/// [`classify`] for the API base `api`.
pub(crate) fn classify_at(error: &ureq::Error, api: &str) -> GithubFailure {
    match error {
        ureq::Error::StatusCode(401 | 403) => GithubFailure::Rejected,
        ureq::Error::StatusCode(404 | 410) => GithubFailure::NotFound,
        other => match crate::tls::ureq_failure(other, api) {
            Some(failure) => GithubFailure::TlsUntrusted(failure),
            None => GithubFailure::Unreachable,
        },
    }
}

impl GithubPullRequests for HttpGithubPullRequests {
    fn post_json(
        &self,
        secret: &str,
        path: &str,
        body: &Value,
    ) -> Result<Value, GithubWriteFailure> {
        post_at(API, secret, path, body)
    }

    fn get_json(&self, secret: &str, path: &str) -> Result<Value, GithubFailure> {
        get_at(API, secret, path)
    }

    fn for_host(&self, host: &str) -> Option<Box<dyn GithubPullRequests>> {
        Some(Box::new(HostedGithubPullRequests {
            api: crate::github_tokens::github_api_base(host),
        }))
    }
}

impl GithubPullRequests for HostedGithubPullRequests {
    fn post_json(
        &self,
        secret: &str,
        path: &str,
        body: &Value,
    ) -> Result<Value, GithubWriteFailure> {
        post_at(&self.api, secret, path, body)
    }

    fn get_json(&self, secret: &str, path: &str) -> Result<Value, GithubFailure> {
        get_at(&self.api, secret, path)
    }
}

fn post_at(api: &str, secret: &str, path: &str, body: &Value) -> Result<Value, GithubWriteFailure> {
    if !path.starts_with('/') {
        return Err(GithubWriteFailure::Unreachable);
    }
    let url = format!("{api}{path}");
    let payload = serde_json::to_string(body).map_err(|_| GithubWriteFailure::Unreachable)?;
    let mut response = write_agent()
        .post(&url)
        .header("Authorization", &format!("Bearer {secret}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "synthesis")
        .header("Content-Type", "application/json")
        .send(payload)
        .map_err(|error| match crate::tls::ureq_failure(&error, api) {
            Some(failure) => GithubWriteFailure::TlsUntrusted(failure),
            None => GithubWriteFailure::Unreachable,
        })?;
    let status = response.status().as_u16();
    let text = response
        .body_mut()
        .read_to_string()
        .map_err(|_| GithubWriteFailure::Unreachable)?;
    match status {
        200..=299 => serde_json::from_str(&text).map_err(|_| GithubWriteFailure::Unreachable),
        401 | 403 => Err(GithubWriteFailure::Rejected),
        404 | 410 => Err(GithubWriteFailure::NotFound),
        422 => Err(GithubWriteFailure::Invalid(invalid_reason(&text))),
        _ => Err(GithubWriteFailure::Unreachable),
    }
}

fn get_at(api: &str, secret: &str, path: &str) -> Result<Value, GithubFailure> {
    // The API base is fixed by the client, so the token cannot be sent
    // anywhere else.
    if !path.starts_with('/') {
        return Err(GithubFailure::Unreachable);
    }
    let url = format!("{api}{path}");
    let mut response = agent()
        .get(&url)
        .header("Authorization", &format!("Bearer {secret}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        // GitHub rejects a request with no User-Agent, which would
        // otherwise read as a bad token.
        .header("User-Agent", "synthesis")
        .call()
        .map_err(|error| classify_at(&error, api))?;
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|_| GithubFailure::Unreachable)?;
    serde_json::from_str(&body).map_err(|_| GithubFailure::Unreachable)
}

/// The reasons of a 422 answer, joined: the `message` of GitHub's body and each
/// error's own `message`. Text GitHub wrote about the request, never the token.
pub(crate) fn invalid_reason(body: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return String::new();
    };
    let mut parts: Vec<String> = Vec::new();
    if let Some(message) = value.get("message").and_then(Value::as_str) {
        parts.push(message.to_string());
    }
    if let Some(errors) = value.get("errors").and_then(Value::as_array) {
        for error in errors {
            if let Some(message) = error.get("message").and_then(Value::as_str) {
                parts.push(message.to_string());
            } else if let (Some(field), Some(code)) = (
                error.get("field").and_then(Value::as_str),
                error.get("code").and_then(Value::as_str),
            ) {
                // GitHub names a branch it does not hold by field and code,
                // for example `head` and `invalid`.
                parts.push(format!("{field} {code}"));
            }
        }
    }
    parts.join("; ")
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
