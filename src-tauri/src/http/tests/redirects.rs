//! AAP-FR-GMVE: a credential goes to no other host on a redirect, on the
//! synchronous path of `crate::http` and on the shared client of `rig`,
//! `rmcp`, and `openrouter-rs`.

use super::*;

/// A request with each kind of credential header, and one header that is no
/// credential.
fn credentialed(url: &str) -> Request {
    Request::get(url, LIMIT)
        .bearer("secret-bearer")
        .credential("x-api-key", "secret-key")
        .credential("x-custom-token", "secret-custom")
        .header("user-agent", "synthesis")
        .direct(true)
}

/// No credential of [`credentialed`] is in `request`.
fn assert_no_credential(request: &str) {
    for name in ["authorization", "x-api-key", "x-custom-token", "cookie"] {
        assert_eq!(header_of(request, name), None, "{name} went with the redirect: {request}");
    }
    assert!(!request.contains("secret-"), "{request}");
}

// AAP-FR-GMVE
#[test]
fn a_redirect_to_another_host_carries_no_credential() {
    for same_host_rule in [false, true] {
        let other = Loopback::serve(vec![json_reply(200, "{}")]);
        let target = other.url("/landing");
        let first = Loopback::serve(vec![reply(302, &[("location", &target)], b"")]);
        // `localhost` and `127.0.0.1` are two hosts that both reach a
        // loopback server.
        let mut call = credentialed(&format!("http://localhost:{}/start", first.port));
        if same_host_rule {
            call = call.credential_to_same_host();
        }
        let response = call.send().expect("the redirect is followed");
        assert_eq!(response.status, 200);

        let asked = first.requests();
        assert_eq!(header_of(&asked[0], "authorization").as_deref(), Some("Bearer secret-bearer"));
        assert_eq!(header_of(&asked[0], "x-api-key").as_deref(), Some("secret-key"));
        let landed = other.requests();
        assert_eq!(landed.len(), 1);
        assert!(landed[0].starts_with("GET /landing "), "{}", landed[0]);
        assert_no_credential(&landed[0]);
        assert_eq!(header_of(&landed[0], "user-agent").as_deref(), Some("synthesis"));
    }
}

// AAP-FR-GMVE: a redirect to another port of the same host name is another
// server, so the credential does not go there either.
#[test]
fn a_redirect_to_another_port_carries_no_credential() {
    let other = Loopback::serve(vec![json_reply(200, "{}")]);
    let first = Loopback::serve(vec![reply(301, &[("location", &other.url("/x"))], b"")]);
    credentialed(&first.url("/start")).credential_to_same_host().send().unwrap();
    first.requests();
    assert_no_credential(&other.requests()[0]);
}

// AAP-FR-GMVE, GHP-FR-SVXP: a redirect on the same host keeps the credential
// only where the caller asks for it.
#[test]
fn a_redirect_on_the_same_host_keeps_the_credential_only_where_asked() {
    let server = Loopback::serve(vec![
        reply(301, &[("location", "/moved")], b""),
        json_reply(200, "{}"),
        reply(301, &[("location", "/moved")], b""),
        json_reply(200, "{}"),
    ]);
    credentialed(&server.url("/start")).credential_to_same_host().send().unwrap();
    credentialed(&server.url("/start")).send().unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    assert!(requests[1].starts_with("GET /moved "), "{}", requests[1]);
    assert_eq!(header_of(&requests[1], "authorization").as_deref(), Some("Bearer secret-bearer"));
    assert_eq!(header_of(&requests[1], "x-api-key").as_deref(), Some("secret-key"));
    assert_no_credential(&requests[3]);
}

// A 303 of a write is read with a `GET` and no body. A 307 of a write repeats
// the write, so it is not followed.
#[test]
fn a_redirect_of_a_write_is_a_get_or_a_failure() {
    let server =
        Loopback::serve(vec![reply(303, &[("location", "/result")], b""), json_reply(200, "{}")]);
    Request::post(&server.url("/w"), LIMIT)
        .json(&serde_json::json!({ "a": 1 }))
        .direct(true)
        .send()
        .unwrap();
    let requests = server.requests();
    assert!(requests[1].starts_with("GET /result "), "{}", requests[1]);
    assert_eq!(header_of(&requests[1], "content-type"), None);
    assert!(requests[1].ends_with("\r\n\r\n"), "{}", requests[1]);

    let server = Loopback::serve(vec![reply(307, &[("location", "/again")], b"")]);
    let result = Request::post(&server.url("/w"), LIMIT)
        .json(&serde_json::json!({ "a": 1 }))
        .direct(true)
        .send();
    assert_eq!(result.unwrap_err(), HttpError::Redirect);
    assert_eq!(server.requests().len(), 1);
}

// The most redirects one request follows is ten. With none allowed, the
// redirect answer is the response.
#[test]
fn redirects_have_a_limit_and_none_can_be_asked_for() {
    let loop_reply = || reply(302, &[("location", "/again")], b"");
    let server = Loopback::serve((0..=MAX_REDIRECTS).map(|_| loop_reply()).collect());
    let result = Request::get(&server.url("/x"), LIMIT).direct(true).send();
    assert_eq!(result.unwrap_err(), HttpError::Redirect);
    assert_eq!(server.requests().len(), MAX_REDIRECTS as usize + 1);

    let server = Loopback::serve(vec![loop_reply()]);
    let response =
        Request::get(&server.url("/x"), LIMIT).max_redirects(0).direct(true).send().unwrap();
    assert_eq!(response.status, 302);
    assert_eq!(server.requests().len(), 1);
}

// AAP-FR-GMVE: a redirect that changes the host alone, to the same port, is
// another host. The credential does not go there, also with the same-host rule.
#[test]
fn a_redirect_that_changes_only_the_host_carries_no_credential() {
    let server = Loopback::serve_with(|port| {
        let target = format!("http://127.0.0.1:{port}/landing");
        vec![reply(302, &[("location", &target)], b""), json_reply(200, "{}")]
    });
    credentialed(&format!("http://localhost:{}/start", server.port))
        .credential_to_same_host()
        .send()
        .expect("the redirect is followed");
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(header_of(&requests[0], "x-api-key").as_deref(), Some("secret-key"));
    assert!(requests[1].starts_with("GET /landing "), "{}", requests[1]);
    let host = header_of(&requests[1], "host").unwrap_or_default();
    assert!(host.starts_with("127.0.0.1:"), "the second request went to {host}");
    assert_no_credential(&requests[1]);
    assert_eq!(header_of(&requests[1], "user-agent").as_deref(), Some("synthesis"));
}

// AAP-FR-GMVE: a header with the name of a credential is removed on a
// redirect to another host also where the caller did not mark it.
#[test]
fn a_credential_header_given_as_a_plain_header_is_removed_on_a_redirect() {
    let other = Loopback::serve(vec![json_reply(200, "{}")]);
    let first = Loopback::serve(vec![reply(302, &[("location", &other.url("/x"))], b"")]);
    Request::get(&format!("http://localhost:{}/start", first.port), LIMIT)
        .header("x-api-key", "secret-key")
        .header("cookie", "session=secret-cookie")
        .header("authorization", "Bearer secret-bearer")
        .credential_to_same_host()
        .direct(true)
        .send()
        .unwrap();
    let asked = first.requests();
    assert_eq!(header_of(&asked[0], "cookie").as_deref(), Some("session=secret-cookie"));
    assert_no_credential(&other.requests()[0]);
}

/// One `GET` with an `x-api-key` through the shared client of `rig`, `rmcp`,
/// and `openrouter-rs`. The status of the answer.
fn shared_client_get(url: &str) -> u16 {
    let (client, _) = crate::tls::rig_http_client().expect("the shared client");
    let runtime =
        tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime");
    runtime.block_on(async {
        let response = client
            .get(url)
            .header("x-api-key", "secret-key")
            .header("user-agent", "synthesis")
            .send()
            .await
            .expect("an answer");
        response.status().as_u16()
    })
}

// AAP-FR-GMVE: the shared client follows no redirect to another origin, so
// an `x-api-key` (rig to Anthropic) or the secret header of an MCP server
// never reaches it. The redirect answer is the response.
#[test]
fn the_shared_client_follows_no_redirect_to_another_origin() {
    let other = Loopback::serve(vec![json_reply(200, "{}")]);
    let first = Loopback::serve(vec![reply(302, &[("location", &other.url("/landing"))], b"")]);
    assert_eq!(shared_client_get(&format!("http://localhost:{}/start", first.port)), 302);
    assert_eq!(first.requests().len(), 1);
    assert!(other.requests().is_empty(), "the other server got a request");

    // Only the host changes, to the same port.
    let server = Loopback::serve_with(|port| {
        vec![reply(302, &[("location", &format!("http://127.0.0.1:{port}/landing"))], b"")]
    });
    assert_eq!(shared_client_get(&format!("http://localhost:{}/start", server.port)), 302);
    let requests = server.requests();
    assert_eq!(requests.len(), 1, "{requests:?}");
}

// AAP-FR-GMVE: the shared client follows a redirect to the same origin, at
// most ten of them.
#[test]
fn the_shared_client_follows_a_redirect_to_the_same_origin() {
    let server =
        Loopback::serve(vec![reply(301, &[("location", "/moved")], b""), json_reply(200, "{}")]);
    assert_eq!(shared_client_get(&server.url("/start")), 200);
    let requests = server.requests();
    assert!(requests[1].starts_with("GET /moved "), "{}", requests[1]);
    assert_eq!(header_of(&requests[1], "x-api-key").as_deref(), Some("secret-key"));

    let again = || reply(302, &[("location", "/again")], b"");
    let server = Loopback::serve((0..=MAX_REDIRECTS).map(|_| again()).collect());
    let (client, _) = crate::tls::rig_http_client().expect("the shared client");
    let runtime =
        tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime");
    let result = runtime.block_on(client.get(server.url("/x")).send());
    assert!(result.expect_err("too many redirects").is_redirect());
    assert_eq!(server.requests().len(), MAX_REDIRECTS as usize + 1);
}
