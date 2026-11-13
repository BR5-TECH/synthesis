//! The GitHub transport of the pull request operations (GTC-FR-DWVY).
//!
//! Direct HTTPS against GitHub's REST API through `crate::http`, as the other
//! GitHub clients of this application do (AAP-FR-DTJA). The trait is what makes every pull request
//! operation testable without a network: the production implementation is the
//! only code in this module tree that opens a socket.

use std::time::Duration;

use serde_json::Value;

use crate::http::HttpError;

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
    /// HTTP 400 or 410 of a stacks endpoint: the host does not know the stacks
    /// API or its API version (GTS-FR-NWVR, GTC-FR-TBQW).
    Unsupported,
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
    /// HTTP 409: another request changes the same resource (GTC-FR-KQVA).
    Conflict,
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

    /// PATCH `body` as JSON to `path` (starting with `/`) and return the JSON
    /// body of a successful answer (GTC-FR-ZHVO). An implementation that cannot
    /// write answers `Unreachable`.
    fn patch_json(
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

/// The request of one call: the credential goes to no other host on a
/// redirect (AAP-FR-GMVE), and the connection opens within its own limit.
fn request(call: crate::http::Request, secret: &str, path: &str) -> crate::http::Request {
    call.connect_timeout(CONNECT_TIMEOUT)
        .bearer(secret)
        .credential_to_same_host()
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", crate::github_tokens::api_version_for_path(path))
        // GitHub rejects a request with no User-Agent, which would otherwise
        // read as a bad token.
        .header("User-Agent", "synthesis")
}

/// Map a transport error to a fixed failure. Never reads the error's text.
pub(crate) fn classify(error: &HttpError) -> GithubFailure {
    match error {
        HttpError::Status(401 | 403) => GithubFailure::Rejected,
        HttpError::Status(404 | 410) => GithubFailure::NotFound,
        HttpError::Tls(failure) => GithubFailure::TlsUntrusted(failure.clone()),
        _ => GithubFailure::Unreachable,
    }
}

impl GithubPullRequests for HttpGithubPullRequests {
    fn post_json(
        &self,
        secret: &str,
        path: &str,
        body: &Value,
    ) -> Result<Value, GithubWriteFailure> {
        write_at(API, WriteMethod::Post, secret, path, body)
    }

    fn patch_json(
        &self,
        secret: &str,
        path: &str,
        body: &Value,
    ) -> Result<Value, GithubWriteFailure> {
        write_at(API, WriteMethod::Patch, secret, path, body)
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
        write_at(&self.api, WriteMethod::Post, secret, path, body)
    }

    fn patch_json(
        &self,
        secret: &str,
        path: &str,
        body: &Value,
    ) -> Result<Value, GithubWriteFailure> {
        write_at(&self.api, WriteMethod::Patch, secret, path, body)
    }

    fn get_json(&self, secret: &str, path: &str) -> Result<Value, GithubFailure> {
        get_at(&self.api, secret, path)
    }
}

/// The HTTP method of one write.
#[derive(Clone, Copy)]
pub(crate) enum WriteMethod {
    Post,
    Patch,
}

pub(crate) fn write_at(
    api: &str,
    method: WriteMethod,
    secret: &str,
    path: &str,
    body: &Value,
) -> Result<Value, GithubWriteFailure> {
    if !path.starts_with('/') {
        return Err(GithubWriteFailure::Unreachable);
    }
    let url = format!("{api}{path}");
    let call = match method {
        WriteMethod::Post => crate::http::Request::post(&url, REQUEST_TIMEOUT),
        WriteMethod::Patch => crate::http::Request::patch(&url, REQUEST_TIMEOUT),
    };
    let response = request(call, secret, path)
        .header("Content-Type", "application/json")
        .json(body)
        // A non-2xx answer is a response, so the body of a 422 can be read for
        // the reason GitHub gave. Each status is classified below
        // (AAP-FR-UXUI).
        .statuses_as_responses()
        // A redirected POST may come back as a GET, whose answer is not the
        // pull request. A redirect is a failure to create, not a success.
        .max_redirects(0)
        .send()
        .map_err(|error| match error {
            HttpError::Tls(failure) => GithubWriteFailure::TlsUntrusted(failure),
            _ => GithubWriteFailure::Unreachable,
        })?;
    let status = response.status;
    let text = response.text().map_err(|_| GithubWriteFailure::Unreachable)?;
    match status {
        200..=299 => serde_json::from_str(text).map_err(|_| GithubWriteFailure::Unreachable),
        401 | 403 => Err(GithubWriteFailure::Rejected),
        404 | 410 => Err(GithubWriteFailure::NotFound),
        // GTC-FR-KQVA: a stacks endpoint that answers 400 does not know the
        // stacks API version of the host.
        400 if crate::github_tokens::is_stacks_path(path) => Err(GithubWriteFailure::NotFound),
        409 => Err(GithubWriteFailure::Conflict),
        422 => Err(GithubWriteFailure::Invalid(invalid_reason(text))),
        _ => Err(GithubWriteFailure::Unreachable),
    }
}

pub(crate) fn get_at(api: &str, secret: &str, path: &str) -> Result<Value, GithubFailure> {
    // The API base is fixed by the client, so the token cannot be sent
    // anywhere else.
    if !path.starts_with('/') {
        return Err(GithubFailure::Unreachable);
    }
    let url = format!("{api}{path}");
    let response = request(crate::http::Request::get(&url, REQUEST_TIMEOUT), secret, path)
        .send()
        .map_err(|error| match error {
        HttpError::Status(400 | 410) if crate::github_tokens::is_stacks_path(path) => {
            GithubFailure::Unsupported
        }
        other => classify(&other),
    })?;
    let body = response.text().map_err(|_| GithubFailure::Unreachable)?;
    serde_json::from_str(body).map_err(|_| GithubFailure::Unreachable)
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
