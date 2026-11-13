//! The one HTTP path of the synchronous callers of the application.
//!
//! Specification: `specifications/core/AAP-ai-api-integrations.md`
//! (AAP-FR-DTJA, AAP-FR-UXUI, AAP-FR-TIZO, AAP-FR-GMVE, AAP-FR-JGYD,
//! AAP-FR-WMCX, AAP-FR-HZTB).
//!
//! Every request goes through `reqwest` 0.13, the one HTTP client library of
//! the application, on a client that `crate::tls` builds from the shared
//! roots. The callers stay synchronous: [`Request::send`] runs the request on a
//! current-thread runtime, on a thread that the call owns and joins. A bare
//! `block_on`, or the blocking client of `reqwest`, panics when the caller is
//! already on a Tokio worker, as a `#[tauri::command(async)]` body is.
//!
//! The rules this module keeps for each caller:
//!
//! - A 4xx or 5xx answer is the failure [`HttpError::Status`], unless the caller
//!   asks for every answer as a response (AAP-FR-UXUI).
//! - A body longer than [`MAX_BODY_BYTES`] is the failure
//!   [`HttpError::TooLarge`] (AAP-FR-TIZO).
//! - A redirect is followed here, not by `reqwest`, so that a credential
//!   header never goes to another host (AAP-FR-GMVE). `reqwest` removes
//!   `Authorization` on such a redirect, but it keeps a header of another name,
//!   such as `x-api-key`.
//! - A direct request uses no proxy, also when the system names one
//!   (AAP-FR-JGYD).
//! - The error holds a fixed kind and never the text of the client, because
//!   that text can echo the request it made.

use std::time::Duration;

use reqwest013::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_TYPE};
use reqwest013::{Method, StatusCode, Url};

use crate::tls::{TlsFailure, TlsRecord, TrustRoots};

/// AAP-FR-TIZO: the most bytes of one response body that a caller reads.
pub const MAX_BODY_BYTES: usize = 10 * 1024 * 1024;

/// The most redirects one request follows.
pub const MAX_REDIRECTS: u32 = 10;

/// The headers that carry a credential. A request removes them before it
/// follows a redirect to another host (AAP-FR-GMVE).
const CREDENTIAL_HEADERS: [&str; 4] =
    ["authorization", "proxy-authorization", "cookie", "x-api-key"];

/// Why a request did not give a response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HttpError {
    /// The server answered with a 4xx or 5xx status (AAP-FR-UXUI).
    Status(u16),
    /// The request did not end within its time limit, or the connection did
    /// not open within its own limit.
    Timeout,
    /// The TLS check refused the certificate of the server (AAP-FR-HZTB).
    Tls(TlsFailure),
    /// No connection, or the connection broke before an answer.
    Transport,
    /// Too many redirects, or a redirect that cannot be followed.
    Redirect,
    /// The body is longer than [`MAX_BODY_BYTES`] (AAP-FR-TIZO).
    TooLarge,
    /// The body broke off, or it is not text where text was read.
    Body,
    /// The HTTP client could not be built.
    Client,
}

impl HttpError {
    /// The status of a 4xx or 5xx answer, else `None`.
    pub fn status(&self) -> Option<u16> {
        match self {
            HttpError::Status(code) => Some(*code),
            _ => None,
        }
    }

    /// AAP-FR-PKWE: the wire text of a TLS refusal, else `None`.
    pub fn tls_wire(&self) -> Option<String> {
        match self {
            HttpError::Tls(failure) => Some(failure.wire()),
            _ => None,
        }
    }
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpError::Status(code) => write!(f, "the server answered {code}"),
            HttpError::Timeout => f.write_str("the request timed out"),
            HttpError::Tls(failure) => f.write_str(&failure.describe()),
            HttpError::Transport => f.write_str("the server could not be reached"),
            HttpError::Redirect => f.write_str("the redirect could not be followed"),
            HttpError::TooLarge => f.write_str("the response is too large"),
            HttpError::Body => f.write_str("the response body could not be read"),
            HttpError::Client => f.write_str("the HTTP client could not be built"),
        }
    }
}

impl std::error::Error for HttpError {}

/// Whether a credential header goes with a redirect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialOnRedirect {
    /// No redirect carries a credential header.
    Never,
    /// A redirect to the same scheme, host, and port keeps the credential
    /// headers. A redirect to another one removes them.
    SameHost,
}

/// One answer, with its whole body.
#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

impl Response {
    /// The value of the header `name`, where it is text.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|value| value.to_str().ok())
    }

    /// The body as UTF-8 text.
    pub fn text(&self) -> Result<&str, HttpError> {
        std::str::from_utf8(&self.body).map_err(|_| HttpError::Body)
    }
}

/// One request and the rules it is sent with.
#[derive(Debug)]
pub struct Request {
    method: Method,
    url: String,
    headers: HeaderMap,
    /// Names of headers that the caller marked as a credential, in addition
    /// to [`CREDENTIAL_HEADERS`].
    credentials: Vec<HeaderName>,
    body: Option<Vec<u8>>,
    timeout: Duration,
    connect_timeout: Option<Duration>,
    max_redirects: u32,
    credential_on_redirect: CredentialOnRedirect,
    statuses_as_errors: bool,
    direct: bool,
    /// A header the caller gave is not valid in HTTP. Such a request is never
    /// sent, so that it cannot go out without a credential it was meant to
    /// carry.
    invalid_header: bool,
}

impl Request {
    fn new(method: Method, url: &str, timeout: Duration) -> Self {
        Self {
            method,
            url: url.to_string(),
            headers: HeaderMap::new(),
            credentials: Vec::new(),
            body: None,
            timeout,
            connect_timeout: None,
            max_redirects: MAX_REDIRECTS,
            credential_on_redirect: CredentialOnRedirect::Never,
            statuses_as_errors: true,
            direct: false,
            invalid_header: false,
        }
    }

    /// A `GET` that must end within `timeout`, body included.
    pub fn get(url: &str, timeout: Duration) -> Self {
        Self::new(Method::GET, url, timeout)
    }

    /// A `POST` that must end within `timeout`, body included.
    pub fn post(url: &str, timeout: Duration) -> Self {
        Self::new(Method::POST, url, timeout)
    }

    /// A `PATCH` that must end within `timeout`, body included.
    pub fn patch(url: &str, timeout: Duration) -> Self {
        Self::new(Method::PATCH, url, timeout)
    }

    /// A `DELETE` that must end within `timeout`, body included.
    pub fn delete(url: &str, timeout: Duration) -> Self {
        Self::new(Method::DELETE, url, timeout)
    }

    /// Add a header. A name or a value that is not valid in HTTP makes
    /// [`Request::send`] fail with [`HttpError::Client`], and nothing is sent.
    pub fn header(mut self, name: &str, value: &str) -> Self {
        match (HeaderName::from_bytes(name.as_bytes()), HeaderValue::from_str(value)) {
            (Ok(name), Ok(value)) => {
                self.headers.insert(name, value);
            }
            _ => self.invalid_header = true,
        }
        self
    }

    /// Add a header that carries a credential. It goes to no other host on a
    /// redirect (AAP-FR-GMVE).
    pub fn credential(mut self, name: &str, value: &str) -> Self {
        match (HeaderName::from_bytes(name.as_bytes()), HeaderValue::from_str(value)) {
            (Ok(name), Ok(mut value)) => {
                value.set_sensitive(true);
                self.headers.insert(name.clone(), value);
                self.credentials.push(name);
            }
            // A credential with a line break, as a pasted token can have, is a
            // local failure, never a request without it.
            _ => self.invalid_header = true,
        }
        self
    }

    /// `Authorization: Bearer <secret>`, as a credential.
    pub fn bearer(self, secret: &str) -> Self {
        self.credential("authorization", &format!("Bearer {secret}"))
    }

    /// A JSON body, with `Content-Type: application/json` where the caller set
    /// no content type.
    pub fn json(mut self, payload: &serde_json::Value) -> Self {
        self.body = Some(payload.to_string().into_bytes());
        if !self.headers.contains_key(CONTENT_TYPE) {
            self.headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        }
        self
    }

    /// The longest wait for a connection to open.
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = Some(timeout);
        self
    }

    /// The most redirects to follow. With `0`, a redirect answer is the
    /// response.
    pub fn max_redirects(mut self, count: u32) -> Self {
        self.max_redirects = count;
        self
    }

    /// Keep the credential headers on a redirect to the same host.
    pub fn credential_to_same_host(mut self) -> Self {
        self.credential_on_redirect = CredentialOnRedirect::SameHost;
        self
    }

    /// Give a 4xx or 5xx answer as a response, so that the caller can read its
    /// body. The caller then classifies the status itself (AAP-FR-UXUI).
    pub fn statuses_as_responses(mut self) -> Self {
        self.statuses_as_errors = false;
        self
    }

    /// AAP-FR-JGYD: with `true`, use no proxy, also when the system names one.
    pub fn direct(mut self, direct: bool) -> Self {
        self.direct = direct;
        self
    }

    /// Send the request with the shared roots of this run.
    pub fn send(self) -> Result<Response, HttpError> {
        let roots = crate::tls::roots();
        self.send_with_roots(&roots)
    }

    /// Send the request with `roots`, on a thread this call owns and joins.
    pub(crate) fn send_with_roots(self, roots: &TrustRoots) -> Result<Response, HttpError> {
        if self.invalid_header {
            return Err(HttpError::Client);
        }
        std::thread::scope(|scope| {
            let handle = scope.spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|_| HttpError::Client)?;
                let (client, record) = runtime.block_on(async { self.client(roots) })?;
                let timeout = self.timeout;
                let result = runtime.block_on(async move {
                    match tokio::time::timeout(timeout, self.run(&client, &record)).await {
                        Ok(result) => result,
                        Err(_elapsed) => Err(HttpError::Timeout),
                    }
                });
                // A name lookup that hangs runs on a blocking thread of this
                // runtime. It is left behind, so the call ends at its timeout.
                runtime.shutdown_background();
                result
            });
            handle.join().unwrap_or(Err(HttpError::Transport))
        })
    }

    /// AAP-FR-WMCX: the client of this request, from `roots`. It follows no
    /// redirect itself: [`Request::run`] does that.
    fn client(&self, roots: &TrustRoots) -> Result<(reqwest013::Client, TlsRecord), HttpError> {
        let record = TlsRecord::new();
        let mut builder = roots
            .reqwest013_builder(&record)
            .map_err(|_| HttpError::Client)?
            .redirect(reqwest013::redirect::Policy::none());
        if let Some(timeout) = self.connect_timeout {
            builder = builder.connect_timeout(timeout);
        }
        // AAP-FR-JGYD: a direct request, and a request to this computer, use no
        // proxy. A credential to a loopback host then never goes through a
        // proxy as plain text.
        if self.direct || is_loopback(&self.url) {
            builder = builder.no_proxy();
        }
        let client = builder.build().map_err(|_| HttpError::Client)?;
        Ok((client, record))
    }

    /// The request and each redirect it follows.
    async fn run(
        mut self,
        client: &reqwest013::Client,
        record: &TlsRecord,
    ) -> Result<Response, HttpError> {
        let mut url = Url::parse(&self.url).map_err(|_| HttpError::Transport)?;
        let mut followed = 0u32;
        loop {
            let mut call = client.request(self.method.clone(), url.clone());
            call = call.headers(self.headers.clone());
            if let Some(body) = &self.body {
                call = call.body(body.clone());
            }
            let mut response =
                call.send().await.map_err(|error| failure_of(&error, record, url.as_str()))?;
            let status = response.status();
            if self.max_redirects > 0 && is_redirect(status) {
                if followed >= self.max_redirects {
                    return Err(HttpError::Redirect);
                }
                followed += 1;
                url = self.follow(&url, &response)?;
                continue;
            }
            if self.statuses_as_errors && (status.is_client_error() || status.is_server_error()) {
                return Err(HttpError::Status(status.as_u16()));
            }
            let body = read_body(&mut response).await?;
            return Ok(Response {
                status: status.as_u16(),
                headers: response.headers().clone(),
                body,
            });
        }
    }

    /// Change this request into the request a redirect asks for, and give the
    /// address of that request. The rules are those of `curl`: a 307 or a 308
    /// repeats the method and the body, so it is not followed for a request
    /// that writes. A 301, a 302, or a 303 of a write becomes a `GET` with no
    /// body.
    fn follow(&mut self, from: &Url, response: &reqwest013::Response) -> Result<Url, HttpError> {
        let location = response
            .headers()
            .get(reqwest013::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .ok_or(HttpError::Redirect)?;
        let to = from.join(location).map_err(|_| HttpError::Redirect)?;
        let status = response.status();
        let keeps_method =
            status == StatusCode::TEMPORARY_REDIRECT || status == StatusCode::PERMANENT_REDIRECT;
        let reads = self.method == Method::GET || self.method == Method::HEAD;
        if keeps_method && !reads {
            return Err(HttpError::Redirect);
        }
        if !keeps_method && !reads {
            self.method = Method::GET;
            self.body = None;
            self.headers.remove(CONTENT_TYPE);
        }
        let keep =
            self.credential_on_redirect == CredentialOnRedirect::SameHost && same_origin(from, &to);
        if !keep {
            for name in CREDENTIAL_HEADERS {
                self.headers.remove(name);
            }
            for name in &self.credentials {
                self.headers.remove(name);
            }
        }
        Ok(to)
    }
}

fn is_redirect(status: StatusCode) -> bool {
    matches!(status.as_u16(), 301 | 302 | 303 | 307 | 308)
}

/// The same scheme, host, and port. A host name is compared without case, and
/// a port that is not written is the default port of the scheme.
pub(crate) fn same_origin(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str().map(str::to_ascii_lowercase) == b.host_str().map(str::to_ascii_lowercase)
        && a.port_or_known_default() == b.port_or_known_default()
}

/// AAP-FR-TIZO: the whole body, or [`HttpError::TooLarge`] when it is longer
/// than [`MAX_BODY_BYTES`].
async fn read_body(response: &mut reqwest013::Response) -> Result<Vec<u8>, HttpError> {
    if response.content_length().is_some_and(|length| length > MAX_BODY_BYTES as u64) {
        return Err(HttpError::TooLarge);
    }
    let mut body = Vec::new();
    loop {
        let chunk = response.chunk().await.map_err(|error| match error.is_timeout() {
            true => HttpError::Timeout,
            false => HttpError::Body,
        })?;
        let Some(chunk) = chunk else { break };
        if body.len() + chunk.len() > MAX_BODY_BYTES {
            return Err(HttpError::TooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// AAP-FR-HZTB: the kind of a failed send. A refusal that the verifier noted
/// comes first, then a TLS error in the error chain, then a time limit. The
/// text of the error is never read.
fn failure_of(error: &reqwest013::Error, record: &TlsRecord, url: &str) -> HttpError {
    if let Some(failure) = record.take() {
        return HttpError::Tls(failure);
    }
    if let Some(cause) = crate::tls::classify_error(error) {
        return HttpError::Tls(TlsFailure::new(crate::tls::host_of(url), cause));
    }
    if error.is_timeout() {
        return HttpError::Timeout;
    }
    HttpError::Transport
}

/// Whether a URL names this computer.
fn is_loopback(url: &str) -> bool {
    Url::parse(url).is_ok_and(|url| match url.host() {
        Some(url::Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(address)) => address.is_loopback(),
        Some(url::Host::Ipv6(address)) => address.is_loopback(),
        None => false,
    })
}

#[cfg(test)]
pub(crate) mod tests;
