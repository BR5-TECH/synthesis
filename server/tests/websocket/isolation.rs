//! What one worker, and one handle, may never reach.
//!
//! Specifications: `specifications/server/RSN-remote-session.md` RSN-FR-UDCM,
//! RSN-FR-FKRE, RSN-FR-EHUT, RSN-FR-YAOC, RSN-FR-NGKW.
//!
//! Every request of these endpoints carries the transport token, which
//! authenticates transport access alone (RSN-FR-TWQJ). What separates one
//! session from another is the handle check and the worker-ownership check, so
//! these tests present a valid token throughout and attack the checks beside it.

use super::*;

/// A second worker, holding the route of `instance-b`.
async fn other_worker(relay: &Relay) -> Socket {
    let mut socket = relay.open("/v1/worker").await;
    send(
        &mut socket,
        "worker",
        "worker_hello",
        Some("w-2"),
        json!({"instance_id": "instance-b", "projects": projects(), "selected_project_id": null}),
    )
    .await;
    assert_eq!(expect(&mut socket).await["type"], "worker_ready");
    socket
}

// RSN-FR-UDCM, RSN-FR-FKRE, RSN-FR-GKZP: a worker never authenticates, revokes,
// or resolves a handle bound to another worker.
#[tokio::test]
async fn a_worker_reaches_no_handle_of_another_worker() {
    let relay = Relay::start().await;
    let mut owner = worker(&relay).await;
    let mut intruder = other_worker(&relay).await;

    let handle = accepted_handle(&relay, &mut owner, "reg-1").await;
    let mut client = relay.open("/v1/client").await;
    send(
        &mut client,
        "client",
        "client_attach",
        Some("c-1"),
        json!({"handle": handle, "instance_id": "instance-a"}),
    )
    .await;
    assert_eq!(expect(&mut client).await["type"], "client_attached");
    assert_eq!(expect(&mut owner).await["type"], "client_attach");

    // The intruder tries to authenticate a client it does not own.
    send(
        &mut intruder,
        "worker",
        "client_authentication_result",
        Some("x-1"),
        json!({"handle": handle, "status": "accepted"}),
    )
    .await;
    let failure = expect(&mut intruder).await;
    assert_eq!(failure["type"], "relay_failure");
    assert_eq!(failure["body"]["reason"], "delivery_failed");
    assert_eq!(failure["request_id"], "x-1");

    // The client is still unauthenticated, so its application messages are
    // refused rather than routed to the worker that never authenticated it.
    send(
        &mut client,
        "client",
        "application_message",
        None,
        json!({"payload": "c211Z2dsZWQ"}),
    )
    .await;
    assert_eq!(
        expect(&mut client).await["body"]["reason"],
        "invalid_frame_shape"
    );

    // The intruder tries to revoke and to fail a handle it does not own.
    let mut again = relay.open("/v1/client").await;
    send(
        &mut again,
        "client",
        "client_attach",
        Some("c-2"),
        json!({"handle": handle, "instance_id": "instance-a"}),
    )
    .await;
    assert_eq!(expect(&mut again).await["type"], "client_attached");
    assert_eq!(expect(&mut owner).await["type"], "client_attach");

    for (frame_type, body) in [
        (
            "handle_invalidated",
            json!({"handle": handle, "reason": "handle_revoked"}),
        ),
        (
            "delivery_failed",
            json!({"handle": handle, "reason": "delivery_failed"}),
        ),
    ] {
        send(&mut intruder, "worker", frame_type, Some("x-2"), body).await;
        assert_eq!(
            expect(&mut intruder).await["body"]["reason"],
            "delivery_failed",
            "frame {frame_type}"
        );
    }

    // The session the intruder attacked is untouched: its owner authenticates
    // it, and it works.
    send(
        &mut owner,
        "worker",
        "client_authentication_result",
        None,
        json!({"handle": handle, "status": "accepted"}),
    )
    .await;
    assert_eq!(
        expect(&mut again).await["type"],
        "client_authentication_result"
    );
}

// RSN-FR-UDCM, RSN-FR-GPLZ: a worker never resolves another worker's
// registration, and the registration client learns nothing from the attempt.
#[tokio::test]
async fn a_worker_resolves_no_registration_of_another_worker() {
    let relay = Relay::start().await;
    let mut owner = worker(&relay).await;
    let mut intruder = other_worker(&relay).await;

    let mut registration = relay.open("/v1/registration").await;
    send(
        &mut registration,
        "registration",
        "registration_start",
        Some("r-1"),
        json!({"registration_id": "reg-1", "instance_id": "instance-a", "payload": "cXI"}),
    )
    .await;
    let bound = expect(&mut registration).await;
    let handle = bound["body"]["handle"]
        .as_str()
        .expect("a handle")
        .to_string();
    assert_eq!(expect(&mut owner).await["type"], "registration_start");

    send(
        &mut intruder,
        "worker",
        "registration_result",
        Some("x-1"),
        json!({"registration_id": "reg-1", "status": "accepted"}),
    )
    .await;
    assert_eq!(
        expect(&mut intruder).await["body"]["reason"],
        "delivery_failed"
    );

    // The handle is still pending, so it attaches to nothing.
    let mut client = relay.open("/v1/client").await;
    send(
        &mut client,
        "client",
        "client_attach",
        Some("c-1"),
        json!({"handle": handle, "instance_id": "instance-a"}),
    )
    .await;
    assert_eq!(
        expect(&mut client).await["body"]["reason"],
        "handle_not_found"
    );

    // Its own worker resolves it, and only then is it a session.
    send(
        &mut owner,
        "worker",
        "registration_result",
        None,
        json!({"registration_id": "reg-1", "status": "accepted"}),
    )
    .await;
    assert_eq!(
        expect(&mut registration).await["body"]["status"],
        "accepted"
    );
}

// RSN-FR-EHUT: a worker connection a replacement took the route from stops
// being authoritative at once, even while its peer keeps writing.
#[tokio::test]
async fn a_replaced_worker_connection_changes_nothing() {
    let relay = Relay::start().await;
    let mut first = worker(&relay).await;
    let handle = accepted_handle(&relay, &mut first, "reg-1").await;

    let mut second = relay.open("/v1/worker").await;
    send(
        &mut second,
        "worker",
        "worker_hello",
        Some("w-2"),
        json!({"instance_id": "instance-a", "projects": projects(), "selected_project_id": "p2"}),
    )
    .await;
    assert_eq!(expect(&mut second).await["type"], "worker_ready");

    // The replaced connection keeps writing rather than reading its close.
    send(
        &mut first,
        "worker",
        "projects_changed",
        None,
        json!({
            "projects": [{"project_id": "p9", "display_name": "Nine"}],
            "selected_project_id": "p9"
        }),
    )
    .await;
    send(
        &mut first,
        "worker",
        "handle_invalidated",
        None,
        json!({"handle": handle, "reason": "handle_revoked"}),
    )
    .await;

    // The route is the replacement's, and the handle is still usable.
    let mut reported = Value::Null;
    for _ in 0..50 {
        let (status, body) = relay
            .get(
                "/v1/relay/projects",
                &[(HANDLE_HEADER, &handle), (INSTANCE_HEADER, "instance-a")],
            )
            .await;
        if status == StatusCode::OK && body["selected_project_id"] == "p2" {
            reported = body;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let exposed: Vec<&str> = reported["projects"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|project| project["project_id"].as_str().expect("an identifier"))
        .collect();
    assert_eq!(exposed, vec!["p1", "p2"]);

    let mut client = relay.open("/v1/client").await;
    send(
        &mut client,
        "client",
        "client_attach",
        Some("c-1"),
        json!({"handle": handle, "instance_id": "instance-a"}),
    )
    .await;
    assert_eq!(expect(&mut client).await["type"], "client_attached");
}

// RSN-FR-NGKW: a registration identifier is routing state, so two live
// exchanges never share one and no answer is delivered to the wrong socket.
#[tokio::test]
async fn a_second_live_registration_may_not_take_an_identifier_in_use() {
    let relay = Relay::start().await;
    let mut worker_socket = worker(&relay).await;

    let mut first = relay.open("/v1/registration").await;
    send(
        &mut first,
        "registration",
        "registration_start",
        Some("r-1"),
        json!({"registration_id": "reg-1", "instance_id": "instance-a", "payload": "cXI"}),
    )
    .await;
    let handle = expect(&mut first).await["body"]["handle"]
        .as_str()
        .expect("a handle")
        .to_string();
    assert_eq!(
        expect(&mut worker_socket).await["type"],
        "registration_start"
    );

    let mut replay = relay.open("/v1/registration").await;
    send(
        &mut replay,
        "registration",
        "registration_start",
        Some("r-2"),
        json!({"registration_id": "reg-1", "instance_id": "instance-a", "payload": "cXI"}),
    )
    .await;
    let failure = expect(&mut replay).await;
    assert_eq!(failure["type"], "registration_failed");
    assert_eq!(failure["body"]["reason"], "invalid_frame_shape");
    assert_eq!(
        expect_close(&mut replay).await.reason,
        "invalid_frame_shape"
    );

    // The terminal result reaches the exchange that opened the identifier.
    send(
        &mut worker_socket,
        "worker",
        "registration_result",
        None,
        json!({"registration_id": "reg-1", "status": "accepted", "payload": "a2V5"}),
    )
    .await;
    let result = expect(&mut first).await;
    assert_eq!(result["body"]["status"], "accepted");
    assert_eq!(result["body"]["payload"], "a2V5");

    let mut client = relay.open("/v1/client").await;
    send(
        &mut client,
        "client",
        "client_attach",
        Some("c-1"),
        json!({"handle": handle, "instance_id": "instance-a"}),
    )
    .await;
    assert_eq!(expect(&mut client).await["type"], "client_attached");
}

// RSN-FR-YAOC: a registration connection that closes before the IDE resolves it
// leaves no handle and no record behind.
#[tokio::test]
async fn an_abandoned_registration_leaves_no_handle() {
    let relay = Relay::start().await;
    let mut worker_socket = worker(&relay).await;

    let mut registration = relay.open("/v1/registration").await;
    send(
        &mut registration,
        "registration",
        "registration_start",
        Some("r-1"),
        json!({"registration_id": "reg-1", "instance_id": "instance-a", "payload": "cXI"}),
    )
    .await;
    let handle = expect(&mut registration).await["body"]["handle"]
        .as_str()
        .expect("a handle")
        .to_string();
    assert_eq!(
        expect(&mut worker_socket).await["type"],
        "registration_start"
    );

    registration.close(None).await.expect("the socket closes");
    for _ in 0..50 {
        if relay.state.http.relay.peek_client(&handle).is_err() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        relay.state.http.relay.peek_client(&handle).is_err(),
        "an abandoned registration left a handle behind"
    );

    // SRB-FR-XSAG: the handle is still never assigned again.
    for index in 0..8 {
        let fresh = accepted_handle(&relay, &mut worker_socket, &format!("reg-{index}")).await;
        assert_ne!(fresh, handle);
    }
}

// RSN-FR-CLGY: a shutdown closes every open connection with `relay_restarted`,
// so a client distinguishes a stopping relay from a protocol failure of its own.
#[tokio::test]
async fn a_shutdown_closes_every_open_connection_with_its_own_reason() {
    let relay = Relay::start().await;
    let mut worker_socket = worker(&relay).await;
    let handle = accepted_handle(&relay, &mut worker_socket, "reg-1").await;
    let mut client = authenticated_client(&relay, &mut worker_socket, &handle).await;
    let mut registration = relay.open("/v1/registration").await;
    send(
        &mut registration,
        "registration",
        "registration_start",
        Some("r-2"),
        json!({"registration_id": "reg-2", "instance_id": "instance-a", "payload": "cXI"}),
    )
    .await;
    let _ = expect(&mut registration).await;
    let _ = expect(&mut worker_socket).await;

    let closed = relay.state.http.hub.close_all("relay_restarted");
    assert_eq!(closed, 3, "the worker, the client, and the registration");

    for socket in [&mut worker_socket, &mut client, &mut registration] {
        let closed = expect_close(socket).await;
        assert_eq!(closed.reason, "relay_restarted");
    }
}
