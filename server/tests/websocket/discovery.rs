//! Project discovery, what a restart leaves, and what a relay record holds.
//!
//! Specifications: `specifications/server/WSK-websocket.md` and
//! `specifications/server/RSN-remote-session.md`.

use super::*;

// RSN-FR-MHDV, RSN-FR-QLEC, RSN-FR-ANWK, RSN-FR-GXWB: the discovery route
// reports one worker's projects to one registered client, and reports the
// announcement the route holds now.
#[tokio::test]
async fn the_discovery_route_reports_one_worker_to_one_client() {
    let relay = Relay::start().await;
    let mut worker_socket = worker(&relay).await;
    let handle = accepted_handle(&relay, &mut worker_socket, "reg-1").await;

    let (status, body) = relay
        .get(
            "/v1/relay/projects",
            &[(HANDLE_HEADER, &handle), (INSTANCE_HEADER, "instance-a")],
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["instance_id"], "instance-a");
    assert_eq!(body["selected_project_id"], "p1");
    assert_eq!(body["projects"][0]["project_id"], "p1");
    assert_eq!(body["projects"][0]["display_name"], "The first project");

    // The answer holds the three members and nothing else: no handle, no key,
    // no user, and no ownership data.
    let object = body.as_object().expect("an object");
    let mut names: Vec<&str> = object.keys().map(String::as_str).collect();
    names.sort();
    assert_eq!(
        names,
        vec!["instance_id", "projects", "selected_project_id"]
    );
    assert!(!body.to_string().contains(&handle));

    // RSN-FR-GXWB: a later announcement is what the next read reports.
    send(
        &mut worker_socket,
        "worker",
        "projects_changed",
        None,
        json!({"projects": [{"project_id": "p3", "display_name": "The third project"}], "selected_project_id": "p3"}),
    )
    .await;
    let mut reported = Value::Null;
    for _ in 0..50 {
        let (_, body) = relay
            .get(
                "/v1/relay/projects",
                &[(HANDLE_HEADER, &handle), (INSTANCE_HEADER, "instance-a")],
            )
            .await;
        if body["selected_project_id"] == "p3" {
            reported = body;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(reported["projects"].as_array().expect("a list").len(), 1);
    assert_eq!(reported["projects"][0]["project_id"], "p3");
}

// RSN-FR-TFJU, RSN-FR-ANWK, RSN-FR-ZDPA: every refusal of the discovery route
// names its own reason, and a relay that holds no handle serves no project.
#[tokio::test]
async fn the_discovery_route_refuses_every_request_that_names_no_session() {
    let relay = Relay::start().await;
    let mut worker_socket = worker(&relay).await;
    let handle = accepted_handle(&relay, &mut worker_socket, "reg-1").await;

    // No handle at all, and a handle no route holds.
    for headers in [
        vec![(INSTANCE_HEADER, "instance-a")],
        vec![
            (HANDLE_HEADER, "no-such-handle"),
            (INSTANCE_HEADER, "instance-a"),
        ],
    ] {
        let (status, body) = relay.get("/v1/relay/projects", &headers).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "handle_not_found");
    }

    // A handle presented against another worker never reaches that worker.
    let (status, body) = relay
        .get(
            "/v1/relay/projects",
            &[(HANDLE_HEADER, &handle), (INSTANCE_HEADER, "instance-b")],
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "handle_not_found");

    // An invalidated handle names its own reason.
    send(
        &mut worker_socket,
        "worker",
        "handle_invalidated",
        None,
        json!({"handle": handle, "reason": "handle_expired"}),
    )
    .await;
    let mut refused = Value::Null;
    for _ in 0..50 {
        let (status, body) = relay
            .get(
                "/v1/relay/projects",
                &[(HANDLE_HEADER, &handle), (INSTANCE_HEADER, "instance-a")],
            )
            .await;
        if status == StatusCode::FORBIDDEN {
            refused = body;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(refused["error"]["code"], "handle_expired");

    // RSN-FR-ZDPA: a relay that started fresh holds no handle at all.
    let restarted = Relay::start().await;
    let (status, body) = restarted
        .get(
            "/v1/relay/projects",
            &[(HANDLE_HEADER, &handle), (INSTANCE_HEADER, "instance-a")],
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "handle_not_found");
}

// RSN-FR-XTPN, RSN-FR-RWIB: a relay that started fresh reports no session as
// active, and a worker reconnects by announcing again.
#[tokio::test]
async fn a_restarted_relay_reports_no_session_as_active() {
    let relay = Relay::start().await;
    let mut worker_socket = worker(&relay).await;
    let _ = accepted_handle(&relay, &mut worker_socket, "reg-1").await;

    let restarted = Relay::start().await;
    assert!(restarted.state.http.relay.workers().is_empty());
    let (status, body) = restarted.get("/v1/relay/workers", &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["items"].as_array().expect("a list").is_empty());

    let mut again = worker(&restarted).await;
    assert_eq!(restarted.state.http.relay.workers().len(), 1);
    send(
        &mut again,
        "worker",
        "projects_changed",
        None,
        json!({"projects": projects(), "selected_project_id": null}),
    )
    .await;
}

// SRB-FR-IZAB, RSN-FR-BSXO, RSN-FR-WOFT: no relay record names a handle value,
// a payload, a device proof, or the token.
#[tokio::test]
async fn no_record_names_a_handle_a_payload_or_the_token() {
    let (state, _ports, logs) = synthesis_server::testing::test_state_with_logs();
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("the listener binds");
    let address = listener.local_addr().expect("an address");
    let router = build_router(state.clone());
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    let relay = Relay { address, state };

    let mut worker_socket = worker(&relay).await;
    let handle = accepted_handle(&relay, &mut worker_socket, "reg-1").await;
    let mut client = authenticated_client(&relay, &mut worker_socket, &handle).await;
    send(
        &mut client,
        "client",
        "application_message",
        None,
        json!({"payload": "c2VjcmV0LXBheWxvYWQ"}),
    )
    .await;
    let _ = expect(&mut worker_socket).await;

    let text = Arc::clone(&logs).text();
    assert!(!text.contains(&handle), "a record named a handle: {text}");
    assert!(
        !text.contains("c2VjcmV0LXBheWxvYWQ"),
        "a record named a payload"
    );
    // RSN-FR-WOFT: the device proof and the registration payload reach no
    // record either.
    assert!(
        !text.contains("ZGV2aWNlLXByb29m"),
        "a record named the device proof"
    );
    assert!(
        !text.contains("cXItcGF5bG9hZA"),
        "a record named the registration payload"
    );
    assert!(!text.contains(TEST_TOKEN), "a record named the token");
}

// RSN-FR-MDLB, RSN-FR-PJHN, RSN-FR-YBGP: the relay holds no user, no ownership,
// and no membership, and it serves no route through which a worker publishes
// projects.
#[tokio::test]
async fn the_relay_holds_no_user_or_ownership_data_and_publishes_no_projects() {
    let relay = Relay::start().await;
    let mut worker_socket = worker(&relay).await;
    let handle = accepted_handle(&relay, &mut worker_socket, "reg-1").await;

    for path in ["/v1/relay/projects", "/v1/relay/workers"] {
        let (_, body) = relay
            .get(
                path,
                &[(HANDLE_HEADER, &handle), (INSTANCE_HEADER, "instance-a")],
            )
            .await;
        let text = body.to_string();
        for absent in [
            "user",
            "owner",
            "member",
            "permission",
            "grant",
            "role",
            "device",
        ] {
            assert!(!text.contains(absent), "{path} named {absent}: {text}");
        }
    }

    // RSN-FR-YBGP: `POST /v1/projects` is the application service's own project
    // route, and it creates a Project rather than announcing an exposed set.
    // No relay route accepts a worker announcement over HTTP.
    let announce = Request::builder()
        .method("POST")
        .uri("/v1/relay/projects")
        .header("authorization", format!("Bearer {TEST_TOKEN}"))
        .header("content-type", "application/json")
        .header(HANDLE_HEADER, &handle)
        .header(INSTANCE_HEADER, "instance-a")
        .body(Body::from(
            json!({"projects": [{"project_id": "p9", "display_name": "Nine"}]}).to_string(),
        ))
        .expect("the request is well formed");
    let response = tower::ServiceExt::oneshot(build_router(relay.state.clone()), announce)
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);

    // The exposed set is exactly what the worker announced over its own socket.
    let (_, body) = relay
        .get(
            "/v1/relay/projects",
            &[(HANDLE_HEADER, &handle), (INSTANCE_HEADER, "instance-a")],
        )
        .await;
    assert_eq!(body["projects"].as_array().expect("a list").len(), 2);
}

// RSN-FR-IRZW: every client of one worker observes that worker's one selected
// project, and no client observes the selection of another worker.
#[tokio::test]
async fn every_client_of_one_worker_observes_one_selected_project() {
    let relay = Relay::start().await;
    let mut worker_socket = worker(&relay).await;
    let first = accepted_handle(&relay, &mut worker_socket, "reg-1").await;
    let second = accepted_handle(&relay, &mut worker_socket, "reg-2").await;

    for handle in [&first, &second] {
        let (status, body) = relay
            .get(
                "/v1/relay/projects",
                &[(HANDLE_HEADER, handle), (INSTANCE_HEADER, "instance-a")],
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["selected_project_id"], "p1");
    }

    // A second worker holds its own selection, which neither client observes.
    let mut other = relay.open("/v1/worker").await;
    send(
        &mut other,
        "worker",
        "worker_hello",
        Some("w-2"),
        json!({"instance_id": "instance-b", "projects": projects(), "selected_project_id": "p2"}),
    )
    .await;
    assert_eq!(expect(&mut other).await["type"], "worker_ready");

    let (status, _) = relay
        .get(
            "/v1/relay/projects",
            &[(HANDLE_HEADER, &first), (INSTANCE_HEADER, "instance-b")],
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// RSN-FR-XBRC: the project-discovery route needs the bearer token, exactly as
// every WebSocket upgrade does. `GET /v1/health` is the one exception.
#[tokio::test]
async fn the_discovery_route_needs_the_bearer_token() {
    let relay = Relay::start().await;
    let mut worker_socket = worker(&relay).await;
    let handle = accepted_handle(&relay, &mut worker_socket, "reg-1").await;

    for token in [None, Some("another-token-0123456789")] {
        let mut request = Request::builder()
            .uri("/v1/relay/projects")
            .header(HANDLE_HEADER, &handle)
            .header(INSTANCE_HEADER, "instance-a");
        if let Some(token) = token {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        let response = tower::ServiceExt::oneshot(
            build_router(relay.state.clone()),
            request
                .body(Body::empty())
                .expect("the request is well formed"),
        )
        .await
        .expect("the router answers");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}

// RSN-FR-TFJU: each refusal of the discovery route carries its own reason —
// including a revoked handle, a reset one, and a worker that holds no route.
#[tokio::test]
async fn every_refusal_of_the_discovery_route_carries_its_own_reason() {
    // A revoked handle, and a worker that holds no route for a handle bound to
    // it.
    let relay = Relay::start().await;
    let mut worker_socket = worker(&relay).await;
    let revoked = accepted_handle(&relay, &mut worker_socket, "reg-1").await;
    let live = accepted_handle(&relay, &mut worker_socket, "reg-2").await;

    send(
        &mut worker_socket,
        "worker",
        "handle_invalidated",
        None,
        json!({"handle": revoked, "reason": "handle_revoked"}),
    )
    .await;
    let refused = poll_refusal(&relay, &revoked, StatusCode::FORBIDDEN).await;
    assert_eq!(refused["error"]["code"], "handle_revoked");

    // RSN-FR-TFJU: the worker's route is gone, so a handle still bound to it is
    // answered `worker_not_found` rather than as an absent handle.
    worker_socket.close(None).await.expect("the socket closes");
    let refused = poll_refusal(&relay, &live, StatusCode::NOT_FOUND).await;
    assert_eq!(refused["error"]["code"], "worker_not_found");

    // A reset handle names the reset.
    let reset_relay = Relay::start().await;
    let mut reset_worker = worker(&reset_relay).await;
    let handle = accepted_handle(&reset_relay, &mut reset_worker, "reg-1").await;
    let mut client = authenticated_client(&reset_relay, &mut reset_worker, &handle).await;
    send(&mut client, "client", "protocol_reset", None, json!({})).await;
    let refused = poll_refusal(&reset_relay, &handle, StatusCode::FORBIDDEN).await;
    assert_eq!(refused["error"]["code"], "handle_reset");
}

/// The body of the first answer that carries the expected status.
async fn poll_refusal(relay: &Relay, handle: &str, expected: StatusCode) -> Value {
    for _ in 0..50 {
        let (status, body) = relay
            .get(
                "/v1/relay/projects",
                &[(HANDLE_HEADER, handle), (INSTANCE_HEADER, "instance-a")],
            )
            .await;
        if status == expected {
            return body;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("the route never answered {expected}");
}
