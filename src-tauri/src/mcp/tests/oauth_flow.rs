//! The whole OAuth sign-in against a fake authorization server on a loopback
//! port, then a connection with the token it stored.

use std::collections::HashMap;
use std::convert::Infallible;
use std::io::{Read, Write};

use bytes::Bytes;
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::{StreamableHttpServerConfig, StreamableHttpService};

use super::*;
use crate::mcp::connect::RealConnector;
use crate::mcp::manager::ServerInput;
use crate::mcp::oauth::{read_slot, sign_in, SignInRequest};
use crate::mcp::status::MachineFacts;

#[derive(Default)]
struct Seen {
    /// The token endpoint refuses a refresh with `invalid_grant`.
    refuse_refresh: bool,
    registrations: usize,
    token_requests: Vec<HashMap<String, String>>,
    authorize: Vec<HashMap<String, String>>,
}

struct AuthServer {
    base: String,
    seen: Arc<Mutex<Seen>>,
}

fn reply(
    status: u16,
    body: String,
    headers: &[(&'static str, String)],
) -> http::Response<BoxBody<Bytes, Infallible>> {
    let mut response = http::Response::new(Full::new(Bytes::from(body)).boxed());
    *response.status_mut() = http::StatusCode::from_u16(status).unwrap();
    response
        .headers_mut()
        .insert("content-type", http::HeaderValue::from_static("application/json"));
    for (name, value) in headers {
        response.headers_mut().insert(*name, http::HeaderValue::from_str(value).unwrap());
    }
    response
}

fn form(body: &[u8]) -> HashMap<String, String> {
    url::form_urlencoded::parse(body).into_owned().collect()
}

async fn auth_server() -> AuthServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let seen: Arc<Mutex<Seen>> = Arc::default();
    let mcp = StreamableHttpService::new(
        || Ok(TestServer::default()),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    );
    let (state, root) = (Arc::clone(&seen), base.clone());
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else { return };
            let (mcp, seen, base) = (mcp.clone(), Arc::clone(&state), root.clone());
            tokio::spawn(async move {
                let handler = hyper::service::service_fn(
                    move |request: http::Request<hyper::body::Incoming>| {
                        let (mut mcp, seen, base) = (mcp.clone(), Arc::clone(&seen), base.clone());
                        async move {
                            let path = request.uri().path().to_string();
                            if path == "/mcp" {
                                let token = request
                                    .headers()
                                    .get("authorization")
                                    .and_then(|v| v.to_str().ok());
                                if !matches!(token, Some("Bearer at-1") | Some("Bearer at-2")) {
                                    let challenge = format!(
                                    "Bearer resource_metadata=\"{base}/.well-known/oauth-protected-resource/mcp\""
                                );
                                    return Ok::<_, Infallible>(reply(
                                        401,
                                        String::new(),
                                        &[("www-authenticate", challenge)],
                                    ));
                                }
                                return tower_service::Service::call(&mut mcp, request).await;
                            }
                            let body = request
                                .into_body()
                                .collect()
                                .await
                                .map(|b| b.to_bytes())
                                .unwrap_or_default();
                            let json = match path.as_str() {
                                "/.well-known/oauth-protected-resource/mcp"
                                | "/.well-known/oauth-protected-resource" => {
                                    serde_json::json!({
                                        "resource": format!("{base}/mcp"),
                                        "authorization_servers": [base],
                                        "scopes_supported": ["tools"],
                                    })
                                }
                                "/.well-known/oauth-authorization-server" => serde_json::json!({
                                    "issuer": base,
                                    "authorization_endpoint": format!("{base}/authorize"),
                                    "token_endpoint": format!("{base}/token"),
                                    "registration_endpoint": format!("{base}/register"),
                                    "response_types_supported": ["code"],
                                    "grant_types_supported": ["authorization_code", "refresh_token"],
                                    "code_challenge_methods_supported": ["S256"],
                                    "token_endpoint_auth_methods_supported": ["none"],
                                }),
                                "/register" => {
                                    seen.lock().unwrap().registrations += 1;
                                    let request: serde_json::Value =
                                        serde_json::from_slice(&body).unwrap_or_default();
                                    let reply_body = serde_json::json!({
                                        "client_id": "client-1",
                                        "redirect_uris": request["redirect_uris"],
                                        "token_endpoint_auth_method": "none",
                                    });
                                    return Ok(reply(201, reply_body.to_string(), &[]));
                                }
                                "/token" => {
                                    let request = form(&body);
                                    let refresh = request.get("grant_type").map(String::as_str)
                                        == Some("refresh_token");
                                    let refuse = seen.lock().unwrap().refuse_refresh;
                                    seen.lock().unwrap().token_requests.push(request);
                                    if refresh && refuse {
                                        let error = serde_json::json!({ "error": "invalid_grant" });
                                        return Ok(reply(400, error.to_string(), &[]));
                                    }
                                    let token = if refresh { "at-2" } else { "at-1" };
                                    serde_json::json!({
                                        "access_token": token,
                                        "token_type": "Bearer",
                                        "expires_in": 3600,
                                        "refresh_token": "rt-1",
                                    })
                                }
                                _ => return Ok(reply(404, String::new(), &[])),
                            };
                            Ok(reply(200, json.to_string(), &[]))
                        }
                    },
                );
                let io = hyper_util::rt::TokioIo::new(stream);
                let _ =
                    hyper::server::conn::http1::Builder::new().serve_connection(io, handler).await;
            });
        }
    });
    AuthServer { base, seen }
}

/// The browser: it reads the authorization URL and follows the redirect with
/// a code and the state it was given (or another state).
fn browser(
    seen: Arc<Mutex<Seen>>,
    page: Arc<Mutex<String>>,
    wrong_state: bool,
) -> impl FnOnce(&str) -> Result<(), ()> {
    move |auth_url: &str| {
        let url = url::Url::parse(auth_url).map_err(|_| ())?;
        let params: HashMap<String, String> = url.query_pairs().into_owned().collect();
        seen.lock().unwrap().authorize.push(params.clone());
        let redirect = url::Url::parse(&params["redirect_uri"]).map_err(|_| ())?;
        let state = params["state"].clone();
        std::thread::spawn(move || {
            let send = |query: String| {
                let mut stream = std::net::TcpStream::connect((
                    redirect.host_str().unwrap(),
                    redirect.port().unwrap(),
                ))
                .unwrap();
                let request =
                    format!("GET {}?{query} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n", redirect.path());
                stream.write_all(request.as_bytes()).unwrap();
                let mut answer = String::new();
                let _ = stream.read_to_string(&mut answer);
                answer
            };
            if wrong_state {
                // A forged code first, then the real redirect with an error.
                *page.lock().unwrap() = send("code=forged-code&state=forged".to_string());
                send(format!("error=access_denied&state={state}"));
            } else {
                *page.lock().unwrap() = send(format!("code=code-1&state={state}"));
            }
        });
        Ok(())
    }
}

fn oauth_http(base: &str) -> HttpTransport {
    HttpTransport {
        url: format!("{base}/mcp"),
        headers: Default::default(),
        secret_headers: Vec::new(),
        auth: McpAuth::Oauth2,
        client_id: None,
        scopes: Vec::new(),
        callback_port: None,
    }
}

// MCS-FR-SAQY, MCS-FR-PJNN, MCS-FR-RBCD, MCS-FR-NRXZ, MCS-FR-GTTD,
// MCS-FR-TGFR, MCS-FR-SZOY: discovery, registration, PKCE with a state and the
// resource, the code exchange, the stored tokens and client, the page, a
// connection with the token, and a second sign-in that registers nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_sign_in_registers_exchanges_and_connects() {
    let server = auth_server().await;
    let secrets = Arc::new(MemoryMcpSecrets::default());
    let page = Arc::new(Mutex::new(String::new()));
    let request = SignInRequest {
        project_key: "/repo".into(),
        server_id: "t".into(),
        http: oauth_http(&server.base),
        secrets: secrets.clone(),
        still_current: Box::new(|| true),
    };
    sign_in(request, browser(Arc::clone(&server.seen), Arc::clone(&page), false)).await.unwrap();

    let slot = read_slot(secrets.as_ref(), "/repo", "t").unwrap();
    assert!(slot.signed_in());
    let registration = slot.registration.unwrap();
    assert_eq!(registration.client_id, "client-1");
    assert!(registration.redirect_uri.starts_with("http://127.0.0.1:"));
    assert!(registration.redirect_uri.ends_with("/callback"));
    {
        let seen = server.seen.lock().unwrap();
        assert_eq!(seen.registrations, 1);
        let authorize = &seen.authorize[0];
        assert_eq!(authorize["response_type"], "code");
        assert_eq!(authorize["code_challenge_method"], "S256");
        assert!(!authorize["state"].is_empty());
        assert_eq!(authorize["resource"], format!("{}/mcp", server.base));
        assert!(
            authorize.get("scope").is_some_and(|s| s.contains("tools")),
            "scopes_supported is asked for"
        );
        let token = &seen.token_requests[0];
        assert_eq!(token["grant_type"], "authorization_code");
        assert_eq!(token["code"], "code-1");
        assert!(token.contains_key("code_verifier"));
    }
    wait_until(|| page.lock().unwrap().contains("Signed in")).await;

    let manager = McpManager::new(secrets.clone(), Arc::new(RealConnector), fast_timing());
    let mut record = http_record("t", McpAuth::Oauth2);
    record.transport = McpTransport::Http(oauth_http(&server.base));
    let mut trust = std::collections::BTreeMap::new();
    trust.insert("t".to_string(), crate::mcp::config::trust_fingerprint(&record).unwrap());
    let facts = crate::mcp::facts_for(&record, "/repo", &trust, secrets.as_ref());
    assert!(facts.signed_in);
    let input = ServerInput {
        listed: crate::mcp::config::ListedRecord { id: "t".into(), record: Ok(record) },
        facts,
    };
    manager.reconcile(
        crate::mcp::project_context("/repo", std::env::temp_dir()),
        vec![input],
        None,
    );
    wait_until(|| {
        status_of(&manager, "t")
            .is_some_and(|s| !matches!(s, McpServerStatus::Connecting | McpServerStatus::Disabled))
    })
    .await;
    assert_eq!(status_of(&manager, "t"), Some(McpServerStatus::Ready { tool_count: 5 }));
    manager.shutdown().await;

    let again = SignInRequest {
        project_key: "/repo".into(),
        server_id: "t".into(),
        http: oauth_http(&server.base),
        secrets: secrets.clone(),
        still_current: Box::new(|| true),
    };
    sign_in(again, browser(Arc::clone(&server.seen), Arc::new(Mutex::new(String::new())), false))
        .await
        .unwrap();
    assert_eq!(server.seen.lock().unwrap().registrations, 1, "the registered client is reused");
}

// MCS-FR-TGFR: a redirect with another state ends the sign-in as
// MCS-FR-TGFR: a redirect with a forged state gets the failure page and no
// exchange; the redirect with the sent state and an `error` ends the sign-in
// as `oauth_failed`, and no token is stored.
#[tokio::test(flavor = "multi_thread")]
async fn a_forged_state_is_refused_and_the_real_error_ends_the_sign_in() {
    let server = auth_server().await;
    let secrets = Arc::new(MemoryMcpSecrets::default());
    let page = Arc::new(Mutex::new(String::new()));
    let request = SignInRequest {
        project_key: "/repo".into(),
        server_id: "t".into(),
        http: oauth_http(&server.base),
        secrets: secrets.clone(),
        still_current: Box::new(|| true),
    };
    let outcome =
        sign_in(request, browser(Arc::clone(&server.seen), Arc::clone(&page), true)).await;
    assert_eq!(outcome, Err(McpFailure::OauthFailed));
    assert!(!read_slot(secrets.as_ref(), "/repo", "t").unwrap().signed_in());
    assert!(server.seen.lock().unwrap().token_requests.is_empty());
    wait_until(|| page.lock().unwrap().contains("Sign-in failed")).await;
}

// MCS-FR-MGCJ, MCS-FR-SZOY: with no tokens, a connection to the protected
// endpoint needs a sign-in.
#[tokio::test(flavor = "multi_thread")]
async fn no_tokens_need_a_sign_in() {
    let server = auth_server().await;
    let secrets = Arc::new(MemoryMcpSecrets::default());
    let manager = McpManager::new(secrets, Arc::new(RealConnector), fast_timing());
    let mut record = http_record("t", McpAuth::Oauth2);
    record.transport = McpTransport::Http(oauth_http(&server.base));
    let facts = MachineFacts {
        trusted: true,
        secrets_present: true,
        signed_in: true,
        vault_unavailable: false,
    };
    let input = ServerInput {
        listed: crate::mcp::config::ListedRecord { id: "t".into(), record: Ok(record) },
        facts,
    };
    manager.reconcile(
        crate::mcp::project_context("/repo", std::env::temp_dir()),
        vec![input],
        None,
    );
    wait_until(|| status_of(&manager, "t") == Some(McpServerStatus::NeedsSignIn)).await;
}

/// The `oauth` slot of a server signed in with `at-1`, whose token expires in
/// `remaining` seconds.
fn signed_in_slot(remaining: u64) -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    serde_json::json!({
        "registration": { "clientId": "client-1", "redirectUri": "http://127.0.0.1:1/callback" },
        "credentials": {
            "client_id": "client-1",
            "token_response": {
                "access_token": "at-1",
                "token_type": "bearer",
                "expires_in": 3600,
                "refresh_token": "rt-1",
            },
            "token_received_at": now - (3600 - remaining),
        }
    })
    .to_string()
}

/// Connect one oauth2 server of the fake authorization server with this
/// `oauth` slot, and give back its settled status.
async fn connect_with(
    server: &AuthServer,
    slot: String,
) -> (McpServerStatus, Arc<MemoryMcpSecrets>) {
    use crate::mcp::secrets::McpSecrets;
    let secrets = Arc::new(MemoryMcpSecrets::default());
    secrets.set("/repo", "t", "oauth", &slot).unwrap();
    let manager = McpManager::new(secrets.clone(), Arc::new(RealConnector), fast_timing());
    let mut record = http_record("t", McpAuth::Oauth2);
    record.transport = McpTransport::Http(oauth_http(&server.base));
    let facts = MachineFacts {
        trusted: true,
        secrets_present: true,
        signed_in: true,
        vault_unavailable: false,
    };
    let input = ServerInput {
        listed: crate::mcp::config::ListedRecord { id: "t".into(), record: Ok(record) },
        facts,
    };
    manager.reconcile(
        crate::mcp::project_context("/repo", std::env::temp_dir()),
        vec![input],
        None,
    );
    wait_until(|| {
        status_of(&manager, "t")
            .is_some_and(|s| !matches!(s, McpServerStatus::Connecting | McpServerStatus::Disabled))
    })
    .await;
    let status = status_of(&manager, "t").unwrap();
    manager.shutdown().await;
    (status, secrets)
}

// MCS-FR-SZOY: a token that expires in less than 30 seconds is refreshed before
// the first request, and the refreshed token is stored and used.
#[tokio::test(flavor = "multi_thread")]
async fn a_token_about_to_expire_is_refreshed() {
    let server = auth_server().await;
    let (status, secrets) = connect_with(&server, signed_in_slot(10)).await;
    assert_eq!(status, McpServerStatus::Ready { tool_count: 5 });
    let seen = server.seen.lock().unwrap();
    assert!(seen
        .token_requests
        .iter()
        .any(|r| r.get("grant_type").map(String::as_str) == Some("refresh_token")
            && r.get("refresh_token").map(String::as_str) == Some("rt-1")));
    let stored = read_slot(secrets.as_ref(), "/repo", "t").unwrap();
    let text = serde_json::to_string(&stored.credentials).unwrap();
    assert!(text.contains("at-2"), "the refreshed token is stored");
}

// MCS-FR-SZOY: a token with time left is not refreshed.
#[tokio::test(flavor = "multi_thread")]
async fn a_token_with_time_left_is_used_as_it_is() {
    let server = auth_server().await;
    let (status, _secrets) = connect_with(&server, signed_in_slot(3000)).await;
    assert_eq!(status, McpServerStatus::Ready { tool_count: 5 });
    assert!(server.seen.lock().unwrap().token_requests.is_empty());
}

// MCS-FR-SQWD: a refresh that the authorization server refuses removes the
// tokens, keeps the registered client, and the server needs a sign-in.
#[tokio::test(flavor = "multi_thread")]
async fn a_refused_refresh_needs_a_new_sign_in() {
    let server = auth_server().await;
    server.seen.lock().unwrap().refuse_refresh = true;
    let (status, secrets) = connect_with(&server, signed_in_slot(10)).await;
    assert_eq!(status, McpServerStatus::NeedsSignIn);
    let stored = read_slot(secrets.as_ref(), "/repo", "t").unwrap();
    assert!(!stored.signed_in());
    assert_eq!(stored.registration.unwrap().client_id, "client-1");
}

// MCS-FR-PJNN, MCS-FR-NRXZ: a record with a client id registers nothing and
// signs in as that client; a record with scopes asks for them.
#[tokio::test(flavor = "multi_thread")]
async fn a_pre_registered_client_and_record_scopes_are_used() {
    let server = auth_server().await;
    let secrets = Arc::new(MemoryMcpSecrets::default());
    let mut http = oauth_http(&server.base);
    http.client_id = Some("pre-1".into());
    http.scopes = vec!["read".into(), "write".into()];
    let request = SignInRequest {
        project_key: "/repo".into(),
        server_id: "t".into(),
        http,
        secrets: secrets.clone(),
        still_current: Box::new(|| true),
    };
    sign_in(request, browser(Arc::clone(&server.seen), Arc::new(Mutex::new(String::new())), false))
        .await
        .unwrap();
    let seen = server.seen.lock().unwrap();
    assert_eq!(seen.registrations, 0);
    assert_eq!(seen.authorize[0]["client_id"], "pre-1");
    let scopes: Vec<&str> = seen.authorize[0]["scope"].split(' ').collect();
    assert!(scopes.contains(&"read") && scopes.contains(&"write"), "{scopes:?}");
    assert!(!scopes.contains(&"tools"), "the record's scopes, not scopes_supported");
    assert_eq!(seen.token_requests[0]["client_id"], "pre-1");
    let stored = read_slot(secrets.as_ref(), "/repo", "t").unwrap();
    assert!(stored.signed_in());
    assert!(stored.registration.is_none(), "a pre-registered client is not kept as a registration");
}

// MCS-FR-ZCPK: a sign-in whose record changed while it ran exchanges no code
// and stores no token.
#[tokio::test(flavor = "multi_thread")]
async fn a_record_that_changed_during_the_sign_in_gets_no_token() {
    let server = auth_server().await;
    let secrets = Arc::new(MemoryMcpSecrets::default());
    let page = Arc::new(Mutex::new(String::new()));
    let request = SignInRequest {
        project_key: "/repo".into(),
        server_id: "t".into(),
        http: oauth_http(&server.base),
        secrets: secrets.clone(),
        still_current: Box::new(|| false),
    };
    let outcome =
        sign_in(request, browser(Arc::clone(&server.seen), Arc::clone(&page), false)).await;
    assert_eq!(outcome, Err(McpFailure::OauthFailed));
    assert!(server.seen.lock().unwrap().token_requests.is_empty());
    assert!(!read_slot(secrets.as_ref(), "/repo", "t").unwrap().signed_in());
    wait_until(|| page.lock().unwrap().contains("Sign-in failed")).await;
}
