//! The OAuth 2 sign-in of an `http` MCP server (MCS-FR-MGCJ to MCS-FR-QRIB).
//!
//! `rmcp`'s `auth` module does the protocol: discovery (RFC 9728, RFC 8414),
//! dynamic registration (RFC 7591), PKCE, the `resource` parameter (RFC 8707),
//! and refresh. This module gives it a credential store in the vault, listens
//! for the redirect on a loopback port, and answers the browser.

use std::sync::Arc;
use std::time::Duration;

use rmcp::transport::auth::{AuthorizationManager, AuthorizationRequest, AuthorizationSession};
use rmcp::transport::{AuthError, CredentialStore, StoredCredentials};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use super::config::HttpTransport;
use super::connect::{is_loopback, ConnectError, ConnectRequest};
use super::secrets::{McpSecrets, OAUTH_CLIENT_SECRET_SLOT, OAUTH_SLOT};
use super::status::McpFailure;

/// MCS-FR-CMWV: how long a sign-in waits for its redirect.
pub const SIGN_IN_LIMIT: Duration = Duration::from_secs(5 * 60);
/// MCS-FR-RBCD: the path of the redirect.
pub const CALLBACK_PATH: &str = "/callback";
/// How long the listener waits for one connection to send its request.
const READ_LIMIT: Duration = Duration::from_secs(5);
/// The largest redirect request the listener reads.
const MAX_REQUEST_BYTES: usize = 16 * 1024;

/// The client a dynamic registration gave, kept for the next sign-in
/// (MCS-FR-PJNN, MCS-FR-SQWD, MCS-FR-QRIB).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Registration {
    pub client_id: String,
    pub redirect_uri: String,
}

/// MCS-FR-MGCJ: what the `oauth` slot holds, as one JSON string.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthSlot {
    #[serde(default)]
    pub registration: Option<Registration>,
    #[serde(default)]
    pub credentials: Option<StoredCredentials>,
}

impl OAuthSlot {
    /// MCS-FR-MGCJ: a slot with a token is signed in.
    pub fn signed_in(&self) -> bool {
        self.credentials.as_ref().is_some_and(|c| c.token_response.is_some())
    }
}

/// Read the `oauth` slot. A value that does not decode reads as empty.
pub fn read_slot(
    secrets: &dyn McpSecrets,
    project: &str,
    server: &str,
) -> Result<OAuthSlot, AuthError> {
    let text = secrets
        .get(project, server, OAUTH_SLOT)
        .map_err(|_| AuthError::CredentialStoreError("keychain_unavailable".into()))?;
    Ok(text.and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default())
}

fn write_slot(
    secrets: &dyn McpSecrets,
    project: &str,
    server: &str,
    slot: &OAuthSlot,
) -> Result<(), AuthError> {
    let text = serde_json::to_string(slot)
        .map_err(|_| AuthError::CredentialStoreError("encode".into()))?;
    secrets
        .set(project, server, OAUTH_SLOT, &text)
        .map_err(|_| AuthError::CredentialStoreError("keychain_unavailable".into()))
}

/// MCS-FR-SQWD, MCS-FR-QRIB: remove the tokens and keep the registered client.
pub fn drop_tokens(secrets: &dyn McpSecrets, project: &str, server: &str) -> Result<(), AuthError> {
    let mut slot = read_slot(secrets, project, server)?;
    slot.credentials = None;
    write_slot(secrets, project, server, &slot)
}

/// The credential store `rmcp` reads and writes the tokens through.
#[derive(Clone)]
pub struct VaultCredentialStore {
    pub secrets: Arc<dyn McpSecrets>,
    pub project: String,
    pub server: String,
}

#[async_trait::async_trait]
impl CredentialStore for VaultCredentialStore {
    async fn load(&self) -> Result<Option<StoredCredentials>, AuthError> {
        Ok(read_slot(self.secrets.as_ref(), &self.project, &self.server)?.credentials)
    }

    async fn save(&self, credentials: StoredCredentials) -> Result<(), AuthError> {
        let mut slot = read_slot(self.secrets.as_ref(), &self.project, &self.server)?;
        slot.credentials = Some(credentials);
        write_slot(self.secrets.as_ref(), &self.project, &self.server, &slot)
    }

    async fn clear(&self) -> Result<(), AuthError> {
        drop_tokens(self.secrets.as_ref(), &self.project, &self.server)
    }

    /// MCS-FR-SQWD: a refused refresh removes the tokens.
    async fn on_refresh_token_rejected(
        &self,
        _credentials: &StoredCredentials,
    ) -> Result<(), AuthError> {
        drop_tokens(self.secrets.as_ref(), &self.project, &self.server)
    }
}

async fn new_manager(
    url: &str,
    client: reqwest013::Client,
    store: VaultCredentialStore,
) -> Result<AuthorizationManager, AuthError> {
    let mut manager = if is_loopback(url) {
        AuthorizationManager::new_allowing_private_addresses(url).await?
    } else {
        AuthorizationManager::new(url).await?
    };
    manager.with_client(client)?;
    manager.set_credential_store(store);
    Ok(manager)
}

/// MCS-FR-SZOY: the authorization of a connection, from the tokens in the vault.
/// `rmcp` refreshes a token that is about to expire, and refreshes one time
/// after a 401.
pub async fn authorized_manager(
    request: &ConnectRequest,
    http: &HttpTransport,
    client: reqwest013::Client,
) -> Result<AuthorizationManager, ConnectError> {
    let store = VaultCredentialStore {
        secrets: Arc::clone(&request.secrets),
        project: request.project_key.clone(),
        server: request.server.id.clone(),
    };
    let mut manager = new_manager(&http.url, client, store)
        .await
        .map_err(|_| ConnectError::Failure(McpFailure::OauthFailed))?;
    match manager.initialize_from_store().await {
        Ok(true) => {}
        Ok(false) => return Err(ConnectError::NeedsSignIn),
        Err(AuthError::HttpError(_)) => return Err(ConnectError::Failure(McpFailure::Unreachable)),
        Err(_) => return Err(ConnectError::Failure(McpFailure::OauthFailed)),
    }
    if let Some(client_id) = &http.client_id {
        let secret = request
            .secrets
            .get(&request.project_key, &request.server.id, OAUTH_CLIENT_SECRET_SLOT)
            .map_err(|_| ConnectError::Failure(McpFailure::KeychainUnavailable))?;
        if let Some(secret) = secret {
            let redirect =
                read_slot(request.secrets.as_ref(), &request.project_key, &request.server.id)
                    .ok()
                    .and_then(|slot| slot.registration.map(|r| r.redirect_uri))
                    .unwrap_or_else(|| format!("http://127.0.0.1{CALLBACK_PATH}"));
            let config = rmcp::transport::auth::OAuthClientConfig::new(client_id.clone(), redirect)
                .with_client_secret(secret)
                .with_scopes(http.scopes.clone());
            manager
                .configure_client(config)
                .map_err(|_| ConnectError::Failure(McpFailure::OauthFailed))?;
        }
    }
    Ok(manager)
}

// ---------------------------------------------------------------------------
// The redirect listener (MCS-FR-RBCD, MCS-FR-TGFR)
// ---------------------------------------------------------------------------

/// MCS-FR-RBCD: listen on `127.0.0.1` alone, at `port` or at a free port.
pub async fn bind_callback(port: Option<u16>) -> std::io::Result<TcpListener> {
    TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port.unwrap_or(0))).await
}

/// The redirect URI of a listener.
pub fn redirect_uri_of(listener: &TcpListener) -> std::io::Result<String> {
    Ok(format!("http://127.0.0.1:{}{CALLBACK_PATH}", listener.local_addr()?.port()))
}

/// What one redirect carried.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Redirect {
    /// A code and a state. `url` is the whole redirect URL.
    Code { url: String, state: String },
    /// MCS-FR-TGFR: an `error`, or no code. `state` is the state it carried.
    Error { state: Option<String> },
}

impl Redirect {
    fn state(&self) -> Option<&str> {
        match self {
            Redirect::Code { state, .. } => Some(state),
            Redirect::Error { state } => state.as_deref(),
        }
    }
}

/// Parse the request line of one HTTP request. `None` for a request that is
/// not a redirect to the callback path.
pub fn parse_redirect(request: &str, redirect_uri: &str) -> Option<Redirect> {
    let line = request.lines().next()?;
    let mut parts = line.split_whitespace();
    if parts.next()? != "GET" {
        return None;
    }
    let target = parts.next()?;
    let base = url::Url::parse(redirect_uri).ok()?;
    let url = base.join(target).ok()?;
    if url.path() != CALLBACK_PATH {
        return None;
    }
    let pairs: std::collections::HashMap<String, String> = url.query_pairs().into_owned().collect();
    let state = pairs.get("state").cloned();
    if pairs.contains_key("error") {
        return Some(Redirect::Error { state });
    }
    match (pairs.get("code"), state) {
        (Some(_), Some(state)) => Some(Redirect::Code { url: url.to_string(), state }),
        (_, state) => Some(Redirect::Error { state }),
    }
}

/// MCS-FR-TGFR: the page the browser shows after the redirect.
pub fn callback_page(succeeded: bool) -> String {
    let (title, text) = if succeeded {
        ("Signed in", "The sign-in is complete. Go back to Synthesis.")
    } else {
        ("Sign-in failed", "The sign-in did not complete. Go back to Synthesis and try again.")
    };
    let body = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{title}</title></head>\
         <body style=\"font-family:sans-serif;margin:3em\"><h1>{title}</h1><p>{text}</p></body></html>"
    );
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

async fn read_request(stream: &mut TcpStream) -> String {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 2048];
    while buffer.len() < MAX_REQUEST_BYTES {
        match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                buffer.extend_from_slice(&chunk[..n]);
                if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
        }
    }
    String::from_utf8_lossy(&buffer).into_owned()
}

/// MCS-FR-TGFR, MCS-FR-GHOV: wait for the redirect that carries the sent
/// `state`. The listener serves each connection on a task of its own, so a
/// connection that sends nothing delays no other. A request to another path
/// gets 404; a redirect with no `state` or another `state` gets the failure
/// page, and the wait goes on.
pub async fn next_redirect(
    listener: TcpListener,
    redirect_uri: &str,
    expected_state: &str,
) -> (TcpStream, Redirect) {
    next_redirect_with(listener, redirect_uri, expected_state, READ_LIMIT).await
}

/// [`next_redirect`] with the time one connection may take to send its
/// request.
pub async fn next_redirect_with(
    listener: TcpListener,
    redirect_uri: &str,
    expected_state: &str,
    read_limit: Duration,
) -> (TcpStream, Redirect) {
    let (found, mut redirects) = tokio::sync::mpsc::channel::<(TcpStream, Redirect)>(8);
    let base = redirect_uri.to_string();
    // MCS-FR-CMWV: when the sign-in ends by any route, this future is dropped,
    // and the guard closes the listener with it.
    let _accept = AbortOnDrop(tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                // A failed accept is retried, after a short wait so it cannot spin.
                tokio::time::sleep(Duration::from_millis(50)).await;
                continue;
            };
            let (found, base) = (found.clone(), base.clone());
            tokio::spawn(async move {
                let Ok(request) = tokio::time::timeout(read_limit, read_request(&mut stream)).await
                else {
                    return;
                };
                match parse_redirect(&request, &base) {
                    Some(redirect) => {
                        let _ = found.send((stream, redirect)).await;
                    }
                    None => {
                        let _ = stream
                            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                            .await;
                    }
                }
            });
        }
    }));
    loop {
        // The accept task holds a sender for as long as this future lives.
        let Some((stream, redirect)) = redirects.recv().await else {
            std::future::pending::<()>().await;
            unreachable!();
        };
        if redirect.state() == Some(expected_state) {
            return (stream, redirect);
        }
        answer(stream, false).await;
    }
}

/// A task that ends when this value is dropped.
struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn answer(mut stream: TcpStream, succeeded: bool) {
    let _ = stream.write_all(callback_page(succeeded).as_bytes()).await;
    let _ = stream.shutdown().await;
}

/// The `state` an authorization URL carries.
pub fn state_of(auth_url: &str) -> Option<String> {
    url::Url::parse(auth_url)
        .ok()?
        .query_pairs()
        .find(|(key, _)| key == "state")
        .map(|(_, value)| value.into_owned())
}

// ---------------------------------------------------------------------------
// The sign-in (MCS-FR-SAQY to MCS-FR-CMWV)
// ---------------------------------------------------------------------------

/// What one sign-in is given.
pub struct SignInRequest {
    pub project_key: String,
    pub server_id: String,
    pub http: HttpTransport,
    pub secrets: Arc<dyn McpSecrets>,
    /// MCS-FR-ZCPK: whether the record still has the trust fingerprint the
    /// sign-in started with.
    pub still_current: Box<dyn Fn() -> bool + Send + Sync>,
}

/// MCS-FR-SAQY to MCS-FR-GTTD: run one sign-in to its end. `open` shows the
/// authorization URL in the system browser. The caller bounds the whole sign-in
/// by [`SIGN_IN_LIMIT`] and by its cancel (MCS-FR-CMWV).
pub async fn sign_in(
    request: SignInRequest,
    open: impl FnOnce(&str) -> Result<(), ()>,
) -> Result<(), McpFailure> {
    let failed = |_| McpFailure::OauthFailed;
    let SignInRequest { project_key, server_id, http, secrets, still_current } = request;
    let slot = read_slot(secrets.as_ref(), &project_key, &server_id)
        .map_err(|_| McpFailure::KeychainUnavailable)?;

    // MCS-FR-PJNN: a registered client is reused on the port it was registered
    // with. When that port is taken, a new client is registered.
    let mut reuse = None;
    let listener = match (&http.client_id, http.callback_port, &slot.registration) {
        (None, None, Some(registration)) => {
            let port = url::Url::parse(&registration.redirect_uri).ok().and_then(|u| u.port());
            match bind_callback(port).await {
                Ok(listener) if port.is_some() => {
                    reuse = Some(registration.client_id.clone());
                    listener
                }
                _ => bind_callback(None).await.map_err(|_| McpFailure::OauthFailed)?,
            }
        }
        _ => bind_callback(http.callback_port).await.map_err(|_| McpFailure::OauthFailed)?,
    };
    let redirect_uri = redirect_uri_of(&listener).map_err(|_| McpFailure::OauthFailed)?;

    let (client, _tls) = crate::tls::rig_http_client().map_err(|_| McpFailure::OauthFailed)?;
    let store = VaultCredentialStore {
        secrets: Arc::clone(&secrets),
        project: project_key.clone(),
        server: server_id.clone(),
    };
    let mut manager = new_manager(&http.url, client, store).await.map_err(|e| match e {
        AuthError::HttpError(_) => McpFailure::Unreachable,
        _ => McpFailure::OauthFailed,
    })?;
    let resolution = manager.resolve_metadata().await.map_err(|e| match e {
        AuthError::HttpError(_) => McpFailure::Unreachable,
        _ => McpFailure::OauthFailed,
    })?;
    manager.set_metadata(resolution.metadata);

    let mut authorization =
        AuthorizationRequest::new(redirect_uri.clone()).with_client_name("Synthesis");
    if !http.scopes.is_empty() {
        authorization = authorization.with_scopes(http.scopes.iter().cloned());
    }
    if let Some(client_id) = &http.client_id {
        authorization = authorization.with_preregistered_client(client_id.clone());
        let secret = secrets
            .get(&project_key, &server_id, OAUTH_CLIENT_SECRET_SLOT)
            .map_err(|_| McpFailure::KeychainUnavailable)?;
        if let Some(secret) = secret {
            authorization = authorization.with_client_secret(secret);
        }
    } else if let Some(client_id) = &reuse {
        authorization = authorization.with_preregistered_client(client_id.clone());
    }
    let session =
        AuthorizationSession::new(manager, authorization).await.map_err(|(_, e)| failed(e))?;
    let expected_state = state_of(&session.auth_url).ok_or(McpFailure::OauthFailed)?;
    open(&session.auth_url).map_err(|_| McpFailure::OauthFailed)?;

    let (stream, redirect) = next_redirect(listener, &redirect_uri, &expected_state).await;
    let url = match redirect {
        Redirect::Code { url, .. } => url,
        Redirect::Error { .. } => {
            answer(stream, false).await;
            return Err(McpFailure::OauthFailed);
        }
    };
    // MCS-FR-ZCPK: a record that changed while the author signed in gets no
    // token. Checked before the exchange, and after it for a change during it.
    if !still_current() || session.handle_callback_url(&url).await.is_err() {
        answer(stream, false).await;
        return Err(McpFailure::OauthFailed);
    }
    if !still_current() {
        let _ = drop_tokens(secrets.as_ref(), &project_key, &server_id);
        answer(stream, false).await;
        return Err(McpFailure::OauthFailed);
    }
    // MCS-FR-PJNN: keep the client for the next sign-in. A pre-registered
    // client is the record's, so nothing is kept for it.
    if http.client_id.is_none() {
        let client_id = session.get_credentials().await.map(|(client_id, _)| client_id).ok();
        let mut slot = read_slot(secrets.as_ref(), &project_key, &server_id)
            .map_err(|_| McpFailure::KeychainUnavailable)?;
        if let Some(client_id) = client_id {
            slot.registration = Some(Registration { client_id, redirect_uri });
        }
        write_slot(secrets.as_ref(), &project_key, &server_id, &slot)
            .map_err(|_| McpFailure::KeychainUnavailable)?;
    }
    answer(stream, true).await;
    Ok(())
}
