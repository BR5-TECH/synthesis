//! The endpoints, the envelope, and the boundaries between the three protocols.
//!
//! Specifications: `specifications/server/WSK-websocket.md` and
//! `specifications/server/RSN-remote-session.md`.

use super::*;

// WSK-FR-HGVU, RSN-FR-XBRC, RSN-FR-TWQJ, RSN-FR-QHVA: every upgrade needs the
// bearer token, the token is validated before any frame is read, and the
// unauthenticated health answer advertises the two capabilities of V1.
#[tokio::test]
async fn every_upgrade_needs_the_bearer_token() {
    let relay = Relay::start().await;
    for path in ["/v1/worker", "/v1/client", "/v1/registration"] {
        assert_eq!(
            relay.open_with(path, None).await.expect_err("no token"),
            StatusCode::UNAUTHORIZED,
            "path {path}"
        );
        assert_eq!(
            relay
                .open_with(path, Some("another-token-0123456789"))
                .await
                .expect_err("the wrong token"),
            StatusCode::UNAUTHORIZED,
            "path {path}"
        );
        assert!(relay.open_with(path, Some(TEST_TOKEN)).await.is_ok());
    }

    // BMS-FR-11: the health route is the one route that needs no credential.
    let (status, body) = relay.get("/v1/health", &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["capabilities"], json!(["remote_session", "websocket"]));
}

// WSK-FR-NAXC: a request to one of the three paths that is not an upgrade is
// answered `400 Bad Request` rather than routed.
#[tokio::test]
async fn a_request_that_is_not_an_upgrade_is_refused() {
    let relay = Relay::start().await;
    for path in ["/v1/worker", "/v1/client", "/v1/registration"] {
        let (status, _) = relay.get(path, &[]).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "path {path}");
    }
}

// WSK-FR-TDKM, WSK-FR-XRVO: the first worker frame is `worker_hello`, and a
// malformed identifier registers no route.
#[tokio::test]
async fn the_worker_endpoint_registers_a_route_only_from_a_valid_hello() {
    let relay = Relay::start().await;

    let mut early = relay.open("/v1/worker").await;
    send(
        &mut early,
        "worker",
        "projects_changed",
        None,
        json!({"projects": projects(), "selected_project_id": null}),
    )
    .await;
    let failure = expect(&mut early).await;
    assert_eq!(failure["type"], "relay_failure");
    assert_eq!(failure["body"]["reason"], "missing_worker_authentication");
    assert!(next(&mut early).await.is_none(), "the connection closes");

    let mut empty = relay.open("/v1/worker").await;
    send(
        &mut empty,
        "worker",
        "worker_hello",
        Some("w-1"),
        json!({"instance_id": "", "projects": projects(), "selected_project_id": null}),
    )
    .await;
    let failure = expect(&mut empty).await;
    assert_eq!(failure["body"]["reason"], "invalid_worker_authentication");

    let mut duplicate = relay.open("/v1/worker").await;
    send(
        &mut duplicate,
        "worker",
        "worker_hello",
        Some("w-1"),
        json!({
            "instance_id": "instance-a",
            "projects": [
                {"project_id": "p1", "display_name": "One"},
                {"project_id": "p1", "display_name": "Two"}
            ],
            "selected_project_id": null
        }),
    )
    .await;
    let failure = expect(&mut duplicate).await;
    assert_eq!(failure["body"]["reason"], "invalid_frame_shape");

    // RSN-FR-VTKA: a selection outside the announcement is refused too.
    let mut unexposed = relay.open("/v1/worker").await;
    send(
        &mut unexposed,
        "worker",
        "worker_hello",
        Some("w-1"),
        json!({"instance_id": "instance-a", "projects": projects(), "selected_project_id": "p9"}),
    )
    .await;
    let failure = expect(&mut unexposed).await;
    assert_eq!(failure["body"]["reason"], "invalid_frame_shape");

    assert_eq!(relay.state.http.relay.workers().len(), 0);
}

// WSK-FR-ZWOE, WSK-FR-CLYA: a frame of one protocol is never accepted on
// another endpoint, and the relay never reads a registration frame as a worker
// frame.
#[tokio::test]
async fn the_three_protocols_never_read_each_other() {
    let relay = Relay::start().await;

    let cases = [
        ("/v1/worker", "registration", "registration_start"),
        ("/v1/client", "worker", "worker_hello"),
        ("/v1/registration", "client", "client_attach"),
    ];
    for (path, protocol, frame_type) in cases {
        let mut socket = relay.open(path).await;
        send(&mut socket, protocol, frame_type, Some("x-1"), json!({})).await;
        let failure = expect(&mut socket).await;
        assert_eq!(
            failure["body"]["reason"], "protocol_mismatch",
            "path {path}"
        );
        assert!(
            next(&mut socket).await.is_none(),
            "path {path} kept the connection open"
        );
    }

    // A type of another protocol, stamped with the right protocol, is refused
    // by the endpoint's own frame table.
    let mut worker_socket = worker(&relay).await;
    send(
        &mut worker_socket,
        "worker",
        "client_attach",
        Some("x-1"),
        json!({"handle": "h", "instance_id": "instance-a"}),
    )
    .await;
    let failure = expect(&mut worker_socket).await;
    assert_eq!(failure["body"]["reason"], "invalid_frame_shape");
}

// WSK-FR-RUKN, WSK-FR-DMJT: an unsupported version and a frame that is not one
// JSON object are refused over the wire.
#[tokio::test]
async fn a_bad_envelope_is_refused_over_the_wire() {
    let relay = Relay::start().await;

    let mut socket = relay.open("/v1/worker").await;
    socket
        .send(Message::Text(
            r#"{"protocol":"worker","version":2,"type":"worker_hello","body":{}}"#.into(),
        ))
        .await
        .expect("the frame is written");
    let failure = expect(&mut socket).await;
    assert_eq!(failure["body"]["reason"], "unsupported_version");

    let mut binary = relay.open("/v1/worker").await;
    binary
        .send(Message::Binary(vec![1, 2, 3].into()))
        .await
        .expect("the frame is written");
    let failure = expect(&mut binary).await;
    assert_eq!(failure["body"]["reason"], "invalid_frame_shape");
}

// RSN-FR-TGSE: the relay adds no frame type for an application operation. Every
// remote operation travels inside an `application_message`.
#[tokio::test]
async fn no_frame_type_names_an_application_operation() {
    let relay = Relay::start().await;
    let mut worker_socket = worker(&relay).await;
    let handle = accepted_handle(&relay, &mut worker_socket, "reg-1").await;

    for operation in [
        "create_draft",
        "archive_draft",
        "move_draft",
        "create_folder",
        "start_graduation",
        "answer_escalation",
        "merge_work_stream",
    ] {
        let mut client = relay.open("/v1/client").await;
        send(
            &mut client,
            "client",
            "client_attach",
            Some("c-1"),
            json!({"handle": handle, "instance_id": "instance-a"}),
        )
        .await;
        let _ = expect(&mut client).await;
        send(&mut client, "client", operation, Some("o-1"), json!({})).await;
        let failure = expect(&mut client).await;
        assert_eq!(
            failure["body"]["reason"], "invalid_frame_shape",
            "the relay routed a frame named {operation}"
        );
    }
}

// WSK-FR-OGLM: the relay closes an unrecoverable protocol error with the
// WebSocket close code 1008 and the reason of WSK-FR-BWZT as the close reason.
#[tokio::test]
async fn every_unrecoverable_failure_closes_with_its_own_reason() {
    let relay = Relay::start().await;

    let cases: Vec<(&str, Value, &str)> = vec![
        (
            "/v1/worker",
            json!({"protocol":"worker","version":2,"type":"worker_hello","body":{}}),
            "unsupported_version",
        ),
        (
            "/v1/worker",
            json!({"protocol":"client","version":1,"type":"client_attach","body":{}}),
            "protocol_mismatch",
        ),
        (
            "/v1/worker",
            json!({"protocol":"worker","version":1,"type":"projects_changed","body":{"projects":[]}}),
            "missing_worker_authentication",
        ),
        (
            "/v1/client",
            json!({"protocol":"client","version":1,"type":"application_message","body":{"payload":"YQ"}}),
            "invalid_frame_shape",
        ),
    ];

    for (path, frame, expected) in cases {
        let mut socket = relay.open(path).await;
        socket
            .send(Message::Text(frame.to_string().into()))
            .await
            .expect("the frame is written");
        let failure = expect(&mut socket).await;
        assert_eq!(failure["body"]["reason"], expected, "path {path}");

        let closed = expect_close(&mut socket).await;
        assert_eq!(closed.code, 1008, "path {path}");
        assert_eq!(closed.reason, expected, "path {path}");
    }
}

// WSK-FR-WVLC: a frame larger than the bound is refused with
// `invalid_frame_shape` and the connection closes.
#[tokio::test]
async fn a_frame_over_the_bound_is_refused_over_the_wire() {
    let relay = Relay::start().await;
    let mut socket = relay.open("/v1/worker").await;

    // One byte over the bound, which the transport still reads, so the relay
    // answers with its own reason rather than dropping the connection mute.
    let payload = "a".repeat(1024 * 1024);
    let frame = json!({
        "protocol": "worker",
        "version": 1,
        "type": "worker_hello",
        "body": {"instance_id": payload, "projects": []},
    });
    assert!(frame.to_string().len() > 1024 * 1024);
    socket
        .send(Message::Text(frame.to_string().into()))
        .await
        .expect("the frame is written");

    let failure = expect(&mut socket).await;
    assert_eq!(failure["body"]["reason"], "invalid_frame_shape");
    assert_eq!(
        expect_close(&mut socket).await.reason,
        "invalid_frame_shape"
    );
}

// WSK-FR-NAXC: the relay serves no method other than the upgrade on the three
// paths.
#[tokio::test]
async fn no_other_method_is_served_on_the_three_paths() {
    let relay = Relay::start().await;
    for path in ["/v1/worker", "/v1/client", "/v1/registration"] {
        for method in ["POST", "PUT", "PATCH", "DELETE"] {
            let request = Request::builder()
                .method(method)
                .uri(path)
                .header("authorization", format!("Bearer {TEST_TOKEN}"))
                .body(Body::empty())
                .expect("the request is well formed");
            let response = tower::ServiceExt::oneshot(build_router(relay.state.clone()), request)
                .await
                .expect("the router answers");
            assert_eq!(
                response.status(),
                StatusCode::METHOD_NOT_ALLOWED,
                "{method} {path}"
            );
        }
    }
}

// RSN-FR-XNUH: a later announcement carrying two descriptors with one
// identifier is refused, and the exposed set does not change.
#[tokio::test]
async fn a_duplicate_identifier_in_a_later_announcement_changes_nothing() {
    let relay = Relay::start().await;
    let mut worker_socket = worker(&relay).await;
    let handle = accepted_handle(&relay, &mut worker_socket, "reg-1").await;

    send(
        &mut worker_socket,
        "worker",
        "projects_changed",
        Some("a-1"),
        json!({
            "projects": [
                {"project_id": "p9", "display_name": "Nine"},
                {"project_id": "p9", "display_name": "Nine again"}
            ],
            "selected_project_id": null
        }),
    )
    .await;
    let failure = expect(&mut worker_socket).await;
    assert_eq!(failure["body"]["reason"], "invalid_frame_shape");
    assert_eq!(failure["request_id"], "a-1");

    // The refusal is unrecoverable, so the connection closes and its route goes
    // with it. What the route must never hold is the announcement that was
    // refused.
    assert_eq!(
        expect_close(&mut worker_socket).await.reason,
        "invalid_frame_shape"
    );
    for route in relay.state.http.relay.workers() {
        assert!(
            !route
                .projects
                .iter()
                .any(|project| project.project_id == "p9"),
            "the refused announcement reached the route"
        );
    }

    // RSN-FR-ADXL: the handle outlives the worker connection, and a later read
    // reports that its worker holds no route.
    let (status, body) = relay
        .get(
            "/v1/relay/projects",
            &[(HANDLE_HEADER, &handle), (INSTANCE_HEADER, "instance-a")],
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "worker_not_found");
}

// WSK-FR-YMOR: the relay answers a ping with a pong, so a connection that no
// longer carries traffic is still found to be closed.
#[tokio::test]
async fn a_ping_is_answered_with_a_pong() {
    let relay = Relay::start().await;
    let mut socket = relay.open("/v1/worker").await;

    socket
        .send(Message::Ping(vec![7, 7].into()))
        .await
        .expect("the ping is written");
    // The pong arrives before anything else, and the connection is unchanged:
    // a `worker_hello` after it still registers the route.
    let mut answered = false;
    for _ in 0..8 {
        match socket.next().await {
            Some(Ok(Message::Pong(payload))) => {
                assert_eq!(payload.as_ref(), &[7, 7]);
                answered = true;
                break;
            }
            Some(Ok(_)) => continue,
            other => panic!("the socket ended instead of answering: {other:?}"),
        }
    }
    assert!(answered, "the relay answered no pong");

    send(
        &mut socket,
        "worker",
        "worker_hello",
        Some("w-1"),
        json!({"instance_id": "instance-a", "projects": projects(), "selected_project_id": null}),
    )
    .await;
    assert_eq!(expect(&mut socket).await["type"], "worker_ready");
}

// WSK-FR-TPJS: the relay forwards the frames of one connection in the order it
// received them.
#[tokio::test]
async fn the_frames_of_one_connection_keep_their_order() {
    let relay = Relay::start().await;
    let mut worker_socket = worker(&relay).await;
    let handle = accepted_handle(&relay, &mut worker_socket, "reg-1").await;
    let mut client = authenticated_client(&relay, &mut worker_socket, &handle).await;

    // Each payload is the base64url of its own index, so the order the worker
    // reads them in is checkable.
    let payloads: Vec<String> = (0..16).map(|index| format!("bXNn{index}")).collect();
    for payload in &payloads {
        send(
            &mut client,
            "client",
            "application_message",
            None,
            json!({ "payload": payload }),
        )
        .await;
    }
    for payload in &payloads {
        let forwarded = expect(&mut worker_socket).await;
        assert_eq!(forwarded["type"], "application_message");
        assert_eq!(forwarded["body"]["payload"], payload.as_str());
    }

    // And the same in the other direction.
    for payload in &payloads {
        send(
            &mut worker_socket,
            "worker",
            "application_message",
            None,
            json!({ "payload": payload }),
        )
        .await;
    }
    for payload in &payloads {
        assert_eq!(
            expect(&mut client).await["body"]["payload"],
            payload.as_str()
        );
    }
}
