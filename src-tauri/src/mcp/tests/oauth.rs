//! The OAuth sign-in parts that run without an authorization server.

use super::*;
use crate::mcp::oauth::*;
use crate::mcp::secrets::McpSecrets;
use rmcp::transport::CredentialStore;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const REDIRECT: &str = "http://127.0.0.1:5000/callback";

// MCS-FR-RBCD, MCS-FR-TGFR: a redirect to the callback path gives its code
// and state; an `error` parameter, or no code, is an error; another path or
// method is not a redirect.
#[test]
fn redirects_are_parsed() {
    let request = "GET /callback?code=abc&state=xyz HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
    let Some(Redirect::Code { url, state }) = parse_redirect(request, REDIRECT) else {
        panic!("a code")
    };
    assert_eq!(state, "xyz");
    assert_eq!(url, "http://127.0.0.1:5000/callback?code=abc&state=xyz");
    assert_eq!(
        parse_redirect("GET /callback?error=access_denied&state=xyz HTTP/1.1\r\n\r\n", REDIRECT),
        Some(Redirect::Error { state: Some("xyz".into()) })
    );
    assert_eq!(
        parse_redirect("GET /callback?state=xyz HTTP/1.1\r\n\r\n", REDIRECT),
        Some(Redirect::Error { state: Some("xyz".into()) })
    );
    assert_eq!(
        parse_redirect("GET /callback?code=abc HTTP/1.1\r\n\r\n", REDIRECT),
        Some(Redirect::Error { state: None })
    );
    assert_eq!(parse_redirect("GET /favicon.ico HTTP/1.1\r\n\r\n", REDIRECT), None);
    assert_eq!(parse_redirect("POST /callback?code=a&state=b HTTP/1.1\r\n\r\n", REDIRECT), None);
    assert_eq!(parse_redirect("", REDIRECT), None);
}

// MCS-FR-TGFR: the page says whether the sign-in succeeded and sends the author
// back to Synthesis.
#[test]
fn the_callback_page_names_the_outcome() {
    let ok = callback_page(true);
    assert!(ok.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(ok.contains("Signed in") && ok.contains("Go back to Synthesis"));
    let failed = callback_page(false);
    assert!(failed.contains("Sign-in failed") && failed.contains("Go back to Synthesis"));
    let body = failed.split("\r\n\r\n").nth(1).unwrap();
    assert!(failed.contains(&format!("Content-Length: {}", body.len())));
}

// MCS-FR-RBCD: the state of an authorization URL.
#[test]
fn the_state_is_read_from_the_authorization_url() {
    assert_eq!(
        state_of("https://as.example/authorize?client_id=c&state=s1&code_challenge=x").as_deref(),
        Some("s1")
    );
    assert_eq!(state_of("https://as.example/authorize?client_id=c"), None);
}

// MCS-FR-RBCD: the listener binds 127.0.0.1 alone, at a free port or at the
// port it is given; a request to another path gets 404 and the wait goes on.
#[tokio::test]
async fn the_listener_waits_for_the_callback_path() {
    let listener = bind_callback(None).await.unwrap();
    let address = listener.local_addr().unwrap();
    assert!(address.ip().is_loopback());
    let redirect = redirect_uri_of(&listener).unwrap();
    assert_eq!(redirect, format!("http://127.0.0.1:{}/callback", address.port()));

    let browser = tokio::spawn(async move {
        let mut other = tokio::net::TcpStream::connect(address).await.unwrap();
        other.write_all(b"GET /favicon.ico HTTP/1.1\r\n\r\n").await.unwrap();
        let mut answer = String::new();
        other.read_to_string(&mut answer).await.unwrap();
        assert!(answer.starts_with("HTTP/1.1 404"));
        let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
        stream.write_all(b"GET /callback?code=c1&state=s1 HTTP/1.1\r\n\r\n").await.unwrap();
        let mut page = String::new();
        stream.read_to_string(&mut page).await.unwrap();
        page
    });
    let (mut stream, got) = next_redirect(listener, &redirect, "s1").await;
    assert!(matches!(got, Redirect::Code { ref state, .. } if state == "s1"));
    stream.write_all(callback_page(true).as_bytes()).await.unwrap();
    drop(stream);
    assert!(browser.await.unwrap().contains("Signed in"));

    // MCS-FR-RBCD, MCS-FR-CMWV: the listener is closed with the wait, and a
    // fixed callback port binds that port.
    let mut fixed = bind_callback(Some(address.port())).await;
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while fixed.is_err() && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(5)).await;
        fixed = bind_callback(Some(address.port())).await;
    }
    assert_eq!(fixed.unwrap().local_addr().unwrap().port(), address.port());
}

// MCS-FR-TGFR: a redirect with no state or another state gets the failure
// page, and the wait goes on to the redirect with the sent state.
#[tokio::test]
async fn a_redirect_with_another_state_does_not_end_the_wait() {
    let listener = bind_callback(None).await.unwrap();
    let address = listener.local_addr().unwrap();
    let redirect = redirect_uri_of(&listener).unwrap();
    let browser = tokio::spawn(async move {
        let mut pages = Vec::new();
        for target in
            ["/callback?code=c&state=forged", "/callback?code=c", "/callback?error=x&state=forged"]
        {
            let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
            stream.write_all(format!("GET {target} HTTP/1.1\r\n\r\n").as_bytes()).await.unwrap();
            let mut page = String::new();
            let _ = stream.read_to_string(&mut page).await;
            pages.push(page);
        }
        let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
        stream.write_all(b"GET /callback?error=denied&state=real HTTP/1.1\r\n\r\n").await.unwrap();
        pages
    });
    let (_stream, got) = next_redirect(listener, &redirect, "real").await;
    assert_eq!(got, Redirect::Error { state: Some("real".into()) });
    let pages = browser.await.unwrap();
    assert!(pages.iter().all(|page| page.contains("Sign-in failed")), "{pages:?}");
}

fn credentials(client: &str, token: &str) -> rmcp::transport::StoredCredentials {
    serde_json::from_value(serde_json::json!({
        "client_id": client,
        "token_response": { "access_token": token, "token_type": "bearer" }
    }))
    .unwrap()
}

// MCS-FR-MGCJ, MCS-FR-SBND, MCS-FR-SQWD: the credential store keeps the tokens
// in the `oauth` slot beside the registered client, and a clear or a refused
// refresh removes the tokens alone.
#[tokio::test]
async fn the_credential_store_keeps_the_registration() {
    let secrets = Arc::new(MemoryMcpSecrets::default());
    let store = VaultCredentialStore {
        secrets: secrets.clone(),
        project: "/repo".into(),
        server: "t".into(),
    };
    assert!(store.load().await.unwrap().is_none());
    assert!(!read_slot(secrets.as_ref(), "/repo", "t").unwrap().signed_in());

    let slot = OAuthSlot {
        registration: Some(Registration { client_id: "c".into(), redirect_uri: REDIRECT.into() }),
        ..Default::default()
    };
    secrets.set("/repo", "t", "oauth", &serde_json::to_string(&slot).unwrap()).unwrap();
    store.save(credentials("c", "secret-token")).await.unwrap();
    let read = read_slot(secrets.as_ref(), "/repo", "t").unwrap();
    assert!(read.signed_in());
    assert_eq!(read.registration.as_ref().unwrap().client_id, "c");
    assert_eq!(store.load().await.unwrap().unwrap().client_id, "c");

    store.on_refresh_token_rejected(&credentials("c", "secret-token")).await.unwrap();
    let read = read_slot(secrets.as_ref(), "/repo", "t").unwrap();
    assert!(!read.signed_in());
    assert!(read.registration.is_some(), "the registered client stays");

    store.save(credentials("c", "again")).await.unwrap();
    store.clear().await.unwrap();
    assert!(!read_slot(secrets.as_ref(), "/repo", "t").unwrap().signed_in());

    secrets.set("/repo", "t", "oauth", "not json").unwrap();
    assert!(
        !read_slot(secrets.as_ref(), "/repo", "t").unwrap().signed_in(),
        "a damaged slot reads as empty"
    );
    secrets.unavailable.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(read_slot(secrets.as_ref(), "/repo", "t").is_err());
}

// MCS-FR-MGCJ, MCS-FR-PXRJ: the facts of an oauth2 record read the slot, and a
// vault that cannot be read is a fact of its own.
#[test]
fn the_facts_read_the_vault() {
    let secrets = MemoryMcpSecrets::default();
    let record = http_record("t", McpAuth::Oauth2);
    let McpTransport::Http(http) = &record.transport else { unreachable!() };
    let mut trust = std::collections::BTreeMap::new();
    let untrusted = crate::mcp::facts_for(&record, "/repo", &trust, &secrets);
    assert!(!untrusted.trusted, "MCS-FR-KXYO: an oauth2 record needs trust");
    // MCS-FR-HQJQ: an untrusted record reads nothing of the vault, so an
    // unavailable vault is not seen either.
    secrets.unavailable.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(!crate::mcp::facts_for(&record, "/repo", &trust, &secrets).vault_unavailable);
    secrets.unavailable.store(false, std::sync::atomic::Ordering::SeqCst);
    trust.insert("t".to_string(), crate::mcp::config::credential_fingerprint(http));
    let facts = crate::mcp::facts_for(&record, "/repo", &trust, &secrets);
    assert!(!facts.signed_in && facts.secrets_present && facts.trusted && !facts.vault_unavailable);
    let slot = OAuthSlot { credentials: Some(credentials("c", "t")), ..Default::default() };
    secrets.set("/repo", "t", "oauth", &serde_json::to_string(&slot).unwrap()).unwrap();
    assert!(crate::mcp::facts_for(&record, "/repo", &trust, &secrets).signed_in);

    let bearer = http_record("b", McpAuth::Bearer);
    trust.insert("b".to_string(), crate::mcp::config::trust_fingerprint(&bearer).unwrap());
    assert!(!crate::mcp::facts_for(&bearer, "/repo", &trust, &secrets).secrets_present);
    secrets.unavailable.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(crate::mcp::facts_for(&bearer, "/repo", &trust, &secrets).vault_unavailable);

    let local = stdio_record("docs");
    let McpTransport::Stdio(stdio) = &local.transport else { unreachable!() };
    let mut trusted = std::collections::BTreeMap::new();
    trusted.insert("docs".to_string(), crate::mcp::config::command_fingerprint(stdio));
    let fresh = MemoryMcpSecrets::default();
    assert!(crate::mcp::facts_for(&local, "/repo", &trusted, &fresh).trusted);
    trusted.insert("docs".to_string(), "old".to_string());
    assert!(!crate::mcp::facts_for(&local, "/repo", &trusted, &fresh).trusted);
}

// MCS-FR-RBCD: a connection that sends nothing does not hold back the redirect
// on a second connection.
#[tokio::test]
async fn a_silent_connection_does_not_block_the_redirect() {
    let listener = bind_callback(None).await.unwrap();
    let address = listener.local_addr().unwrap();
    let redirect = redirect_uri_of(&listener).unwrap();
    let browser = tokio::spawn(async move {
        let _silent = tokio::net::TcpStream::connect(address).await.unwrap();
        let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
        stream.write_all(b"GET /callback?code=c&state=s HTTP/1.1\r\n\r\n").await.unwrap();
        let mut page = String::new();
        let _ = stream.read_to_string(&mut page).await;
        drop(_silent);
        page
    });
    // MCS-FR-GHOV: with a read limit of an hour, only a listener that serves
    // connections at the same time can see the second connection.
    let (stream, got) =
        next_redirect_with(listener, &redirect, "s", Duration::from_secs(3600)).await;
    assert!(matches!(got, Redirect::Code { ref state, .. } if state == "s"));
    drop(stream);
    browser.await.unwrap();
}
