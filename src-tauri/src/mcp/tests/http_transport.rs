//! The Streamable HTTP transport against a real server on a loopback port.

use std::convert::Infallible;

use bytes::Bytes;
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::{StreamableHttpServerConfig, StreamableHttpService};

use super::*;
use crate::mcp::connect::RealConnector;
use crate::mcp::manager::ServerInput;
use crate::mcp::secrets::McpSecrets;
use crate::mcp::status::MachineFacts;

/// A local MCP server. With `bearer`, a request without that token gets 401.
/// Every request's headers are kept.
struct LocalServer {
    url: String,
    headers: Arc<Mutex<Vec<http::HeaderMap>>>,
}

async fn local_server(bearer: Option<&'static str>) -> LocalServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://127.0.0.1:{}/mcp", listener.local_addr().unwrap().port());
    let headers: Arc<Mutex<Vec<http::HeaderMap>>> = Arc::default();
    let service = StreamableHttpService::new(
        || Ok(TestServer::default()),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    );
    let seen = Arc::clone(&headers);
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else { return };
            let service = service.clone();
            let seen = Arc::clone(&seen);
            tokio::spawn(async move {
                let handler = hyper::service::service_fn(
                    move |request: http::Request<hyper::body::Incoming>| {
                        let mut service = service.clone();
                        let seen = Arc::clone(&seen);
                        async move {
                            seen.lock().unwrap().push(request.headers().clone());
                            if let Some(token) = bearer {
                                let expected = format!("Bearer {token}");
                                let given = request
                                    .headers()
                                    .get("authorization")
                                    .and_then(|v| v.to_str().ok());
                                if given != Some(expected.as_str()) {
                                    let body: BoxBody<Bytes, Infallible> =
                                        Full::new(Bytes::new()).boxed();
                                    let mut response = http::Response::new(body);
                                    *response.status_mut() = http::StatusCode::UNAUTHORIZED;
                                    return Ok::<_, Infallible>(response);
                                }
                            }
                            let response =
                                tower_service::Service::call(&mut service, request).await?;
                            Ok(response)
                        }
                    },
                );
                let io = hyper_util::rt::TokioIo::new(stream);
                let _ =
                    hyper::server::conn::http1::Builder::new().serve_connection(io, handler).await;
            });
        }
    });
    LocalServer { url, headers }
}

fn http_server(id: &str, url: &str, auth: McpAuth) -> McpServer {
    let mut record = http_record(id, auth);
    if let McpTransport::Http(http) = &mut record.transport {
        http.url = url.to_string();
        http.headers.insert("X-Team".to_string(), "core".to_string());
        http.secret_headers.push("X-Key".to_string());
    }
    record
}

fn input(record: McpServer) -> ServerInput {
    let facts = MachineFacts {
        trusted: true,
        secrets_present: true,
        signed_in: true,
        vault_unavailable: false,
    };
    ServerInput {
        owner: "/repo".into(),
        listed: crate::mcp::config::ListedRecord { id: record.id.clone(), record: Ok(record) },
        facts,
    }
}

fn manager_with(secrets: Arc<MemoryMcpSecrets>) -> (McpManager, Arc<RecordingObserver>) {
    let manager = McpManager::new(secrets, Arc::new(RealConnector), fast_timing());
    let observer = Arc::new(RecordingObserver::default());
    manager.install_observer(observer.clone());
    (manager, observer)
}

async fn settled(manager: &McpManager, id: &str) -> McpServerStatus {
    wait_until(|| {
        status_of(manager, id)
            .is_some_and(|s| !matches!(s, McpServerStatus::Connecting | McpServerStatus::Disabled))
    })
    .await;
    status_of(manager, id).unwrap()
}

// MCS-FR-MDBI, MCS-FR-PBFI, MCS-FR-BYBA, MCT-FR-OSXG: a bearer server gets the
// token, the plain and the secret headers on each request, and its tools work.
#[tokio::test(flavor = "multi_thread")]
async fn a_bearer_server_gets_its_token_and_headers() {
    let server = local_server(Some("tok-123")).await;
    let secrets = Arc::new(MemoryMcpSecrets::default());
    secrets.set(&"/repo".into(), "remote", "bearer", "tok-123").unwrap();
    secrets.set(&"/repo".into(), "remote", "header:X-Key", "key-456").unwrap();
    let (manager, observer) = manager_with(secrets);
    let record = http_server("remote", &server.url, McpAuth::Bearer);
    manager.reconcile(
        crate::mcp::project_context("/repo", std::env::temp_dir()),
        vec![input(record)],
        None,
    );
    assert_eq!(settled(&manager, "remote").await, McpServerStatus::Ready { tool_count: 5 });

    let result = manager
        .call_mcp_tool("remote", "echo", Default::default(), Duration::from_secs(20))
        .await
        .unwrap();
    assert_eq!(crate::mcp::tool::result_text(&result), "echo: ");
    let headers = server.headers.lock().unwrap().clone();
    assert!(!headers.is_empty());
    for map in &headers {
        assert_eq!(map.get("authorization").unwrap(), "Bearer tok-123");
        assert_eq!(map.get("x-team").unwrap(), "core");
        assert_eq!(map.get("x-key").unwrap(), "key-456");
    }
    let logs = observer.log_text();
    assert!(!logs.contains("tok-123") && !logs.contains("key-456"), "no credential is logged");
    assert!(!logs.contains("127.0.0.1"), "no URL is logged");
    manager.shutdown().await;
}

// MCS-FR-JNXV: a 401 the server keeps sending is `rejected`; a port with no
// server is `unreachable`.
#[tokio::test(flavor = "multi_thread")]
async fn refused_and_unreachable_servers_are_classified() {
    let server = local_server(Some("right")).await;
    let secrets = Arc::new(MemoryMcpSecrets::default());
    secrets.set(&"/repo".into(), "remote", "bearer", "wrong").unwrap();
    secrets.set(&"/repo".into(), "remote", "header:X-Key", "k").unwrap();
    let (manager, _observer) = manager_with(secrets.clone());
    let project = crate::mcp::project_context("/repo", std::env::temp_dir());
    manager.reconcile(
        project.clone(),
        vec![input(http_server("remote", &server.url, McpAuth::Bearer))],
        None,
    );
    assert_eq!(
        settled(&manager, "remote").await,
        McpServerStatus::Failed { failure: McpFailure::Rejected }
    );

    let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://127.0.0.1:{}/mcp", closed.local_addr().unwrap().port());
    drop(closed);
    secrets.set(&"/repo".into(), "gone", "header:X-Key", "k").unwrap();
    manager.reconcile(project, vec![input(http_server("gone", &url, McpAuth::None))], None);
    assert_eq!(
        settled(&manager, "gone").await,
        McpServerStatus::Failed { failure: McpFailure::Unreachable }
    );
}

// MCS-FR-MGCJ, MCS-FR-SZOY: an oauth2 server with no tokens in the vault needs
// a sign-in, and the connector sends nothing with a token.
#[tokio::test(flavor = "multi_thread")]
async fn an_oauth2_server_without_tokens_needs_a_sign_in() {
    let server = local_server(None).await;
    let secrets = Arc::new(MemoryMcpSecrets::default());
    secrets.set(&"/repo".into(), "oauth", "header:X-Key", "k").unwrap();
    let (manager, _observer) = manager_with(secrets);
    let record = http_server("oauth", &server.url, McpAuth::Oauth2);
    manager.reconcile(
        crate::mcp::project_context("/repo", std::env::temp_dir()),
        vec![input(record)],
        None,
    );
    assert_eq!(settled(&manager, "oauth").await, McpServerStatus::NeedsSignIn);
    assert!(server.headers.lock().unwrap().iter().all(|h| h.get("authorization").is_none()));
}

// MCS-FR-LZBV, MCS-FR-JNXV: a refusal that the TLS check noted is
// `tls_untrusted`, whatever error the transport gave.
#[test]
fn a_tls_refusal_is_tls_untrusted() {
    use crate::mcp::connect::{classify, ConnectError};
    let record = crate::tls::TlsRecord::new();
    record.set_for_test(crate::tls::TlsFailure::new(
        "mcp.example.com",
        crate::tls::TlsCause::UnknownIssuer,
    ));
    let error = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
    assert_eq!(
        classify(&error, false, McpAuth::Bearer, Some(&record)),
        ConnectError::Failure(McpFailure::TlsUntrusted)
    );
    let plain = crate::tls::TlsRecord::new();
    assert_eq!(
        classify(&error, false, McpAuth::Bearer, Some(&plain)),
        ConnectError::Failure(McpFailure::Unreachable)
    );
}
