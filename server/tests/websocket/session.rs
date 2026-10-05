//! One remote session: registration, attachment, authentication, and the ways it ends.
//!
//! Specifications: `specifications/server/WSK-websocket.md` and
//! `specifications/server/RSN-remote-session.md`.

use super::*;

// RSN-FR-LQAF, RSN-FR-EIVS, RSN-FR-RKMD, RSN-FR-HSNA, RSN-FR-GKZP,
// RSN-FR-WBHS: one registration produces one handle, the payloads reach the
// worker unchanged, and an authenticated session carries application messages
// both ways.
#[tokio::test]
async fn a_registration_becomes_an_authenticated_session() {
    let relay = Relay::start().await;
    let mut worker_socket = worker(&relay).await;
    let handle = accepted_handle(&relay, &mut worker_socket, "reg-1").await;
    let mut client = authenticated_client(&relay, &mut worker_socket, &handle).await;

    send(
        &mut client,
        "client",
        "application_message",
        None,
        json!({"payload": "Y2xpZW50LXJlcXVlc3Q"}),
    )
    .await;
    let forwarded = expect(&mut worker_socket).await;
    assert_eq!(forwarded["type"], "application_message");
    assert_eq!(forwarded["protocol"], "worker");
    assert_eq!(forwarded["body"]["payload"], "Y2xpZW50LXJlcXVlc3Q");
    assert_eq!(forwarded["body"]["handle"], handle);

    send(
        &mut worker_socket,
        "worker",
        "application_message",
        None,
        json!({"payload": "d29ya2VyLWFuc3dlcg"}),
    )
    .await;
    let broadcast = expect(&mut client).await;
    assert_eq!(broadcast["type"], "application_message");
    assert_eq!(broadcast["protocol"], "client");
    assert_eq!(broadcast["body"]["payload"], "d29ya2VyLWFuc3dlcg");
}

// RSN-FR-BUXE: a registration whose target holds no worker route is refused
// with a typed failure, and the connection closes.
#[tokio::test]
async fn a_registration_without_a_worker_is_refused() {
    let relay = Relay::start().await;
    let mut registration = relay.open("/v1/registration").await;
    send(
        &mut registration,
        "registration",
        "registration_start",
        Some("r-1"),
        json!({"registration_id": "reg-1", "instance_id": "instance-none", "payload": "cXI"}),
    )
    .await;
    let failure = expect(&mut registration).await;
    assert_eq!(failure["type"], "registration_failed");
    assert_eq!(failure["body"]["reason"], "worker_not_found");
    assert_eq!(failure["request_id"], "r-1");
    assert!(next(&mut registration).await.is_none());
}

// RSN-FR-HSNA, RSN-FR-LOJD: a rejected registration leaves a handle that never
// attaches, and an unauthenticated connection routes no application message.
#[tokio::test]
async fn a_rejected_registration_never_becomes_a_session() {
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
    let _routed = expect(&mut worker_socket).await;
    send(
        &mut worker_socket,
        "worker",
        "registration_result",
        None,
        json!({"registration_id": "reg-1", "status": "rejected"}),
    )
    .await;
    assert_eq!(
        expect(&mut registration).await["body"]["status"],
        "rejected"
    );

    let mut client = relay.open("/v1/client").await;
    send(
        &mut client,
        "client",
        "client_attach",
        Some("c-1"),
        json!({"handle": handle, "instance_id": "instance-a"}),
    )
    .await;
    let failure = expect(&mut client).await;
    assert_eq!(failure["body"]["reason"], "handle_not_found");

    // RSN-FR-LOJD: a connection that is attached but not authenticated routes
    // no application message.
    let mut worker_two = worker_socket;
    let accepted = accepted_handle(&relay, &mut worker_two, "reg-2").await;
    let mut attached = relay.open("/v1/client").await;
    send(
        &mut attached,
        "client",
        "client_attach",
        Some("c-2"),
        json!({"handle": accepted, "instance_id": "instance-a"}),
    )
    .await;
    let _ = expect(&mut attached).await;
    send(
        &mut attached,
        "client",
        "application_message",
        None,
        json!({"payload": "dG9vLWVhcmx5"}),
    )
    .await;
    let failure = expect(&mut attached).await;
    assert_eq!(failure["body"]["reason"], "invalid_frame_shape");
}

// RSN-FR-DWLS, RSN-FR-KVBO, RSN-FR-ADXL: the IDE invalidates a handle, every
// connection holding it closes, and a later attachment is refused with the
// reason the IDE named.
#[tokio::test]
async fn the_ide_revokes_a_handle_and_the_session_ends() {
    let relay = Relay::start().await;
    let mut worker_socket = worker(&relay).await;
    let handle = accepted_handle(&relay, &mut worker_socket, "reg-1").await;
    let mut client = authenticated_client(&relay, &mut worker_socket, &handle).await;

    send(
        &mut worker_socket,
        "worker",
        "handle_invalidated",
        None,
        json!({"handle": handle, "reason": "handle_revoked"}),
    )
    .await;
    assert!(next(&mut client).await.is_none(), "the connection closes");

    let mut again = relay.open("/v1/client").await;
    send(
        &mut again,
        "client",
        "client_attach",
        Some("c-2"),
        json!({"handle": handle, "instance_id": "instance-a"}),
    )
    .await;
    let failure = expect(&mut again).await;
    assert_eq!(failure["body"]["reason"], "handle_revoked");
}

// RSN-FR-OKPD, RSN-FR-SGQX, WSK-FR-ARJP: a reset invalidates the handle for
// good, and a new registration is the only way back.
#[tokio::test]
async fn a_protocol_reset_invalidates_the_handle_for_good() {
    let relay = Relay::start().await;
    let mut worker_socket = worker(&relay).await;
    let handle = accepted_handle(&relay, &mut worker_socket, "reg-1").await;
    let mut client = authenticated_client(&relay, &mut worker_socket, &handle).await;

    send(&mut client, "client", "protocol_reset", None, json!({})).await;
    let notice = expect(&mut client).await;
    assert_eq!(notice["type"], "protocol_reset");
    assert_eq!(notice["body"]["reason"], "protocol_reset");
    assert!(next(&mut client).await.is_none());

    let mut again = relay.open("/v1/client").await;
    send(
        &mut again,
        "client",
        "client_attach",
        Some("c-2"),
        json!({"handle": handle, "instance_id": "instance-a"}),
    )
    .await;
    assert_eq!(expect(&mut again).await["body"]["reason"], "handle_reset");

    let fresh = accepted_handle(&relay, &mut worker_socket, "reg-2").await;
    assert_ne!(fresh, handle);
    let _ = authenticated_client(&relay, &mut worker_socket, &fresh).await;
}

// RSN-FR-EHUT, RSN-FR-SVMD, RSN-FR-BQZL, WSK-FR-ZUFK: a replacement closes the
// older worker connection, notifies every client of that route, and buffers
// nothing.
#[tokio::test]
async fn a_replacement_closes_the_old_worker_and_notifies_its_clients() {
    let relay = Relay::start().await;
    let mut first = worker(&relay).await;
    let handle = accepted_handle(&relay, &mut first, "reg-1").await;
    let mut client = authenticated_client(&relay, &mut first, &handle).await;

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

    let notice = expect(&mut client).await;
    assert_eq!(notice["type"], "worker_replaced");
    assert_eq!(notice["body"]["instance_id"], "instance-a");
    assert!(next(&mut first).await.is_none(), "the older worker closes");

    // RSN-FR-FOMR: the handle survives the replacement, so the client attaches
    // again with it.
    let mut again = relay.open("/v1/client").await;
    send(
        &mut again,
        "client",
        "client_attach",
        Some("c-2"),
        json!({"handle": handle, "instance_id": "instance-a"}),
    )
    .await;
    let attached = expect(&mut again).await;
    assert_eq!(attached["type"], "client_attached");
    assert_eq!(attached["body"]["authenticated"], false);
}

// RSN-FR-HZTV, WSK-FR-RCUY: the relay reports delivery success for no frame it
// did not deliver.
#[tokio::test]
async fn an_undeliverable_frame_is_answered_with_a_failure() {
    let relay = Relay::start().await;
    let mut worker_socket = worker(&relay).await;

    // No client is attached, so a broadcast reaches nobody.
    send(
        &mut worker_socket,
        "worker",
        "application_message",
        None,
        json!({"payload": "bm9ib2R5"}),
    )
    .await;
    let failure = expect(&mut worker_socket).await;
    assert_eq!(failure["type"], "relay_failure");
    assert_eq!(failure["body"]["reason"], "delivery_failed");

    // A delivery failure is recoverable: the worker keeps its connection.
    send(
        &mut worker_socket,
        "worker",
        "projects_changed",
        None,
        json!({"projects": projects(), "selected_project_id": "p2"}),
    )
    .await;
    let mut reported = Value::Null;
    for _ in 0..50 {
        let (status, body) = relay.get("/v1/relay/workers/instance-a", &[]).await;
        assert_eq!(status, StatusCode::OK);
        if body["selected_project_id"] == "p2" {
            reported = body;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(reported["selected_project_id"], "p2");
}

// RSN-FR-MFCX, WSK-FR-LSAD: the worker's own delivery failure reaches the
// client the frame came from.
#[tokio::test]
async fn a_worker_delivery_failure_reaches_its_client() {
    let relay = Relay::start().await;
    let mut worker_socket = worker(&relay).await;
    let handle = accepted_handle(&relay, &mut worker_socket, "reg-1").await;
    let mut client = authenticated_client(&relay, &mut worker_socket, &handle).await;

    send(
        &mut worker_socket,
        "worker",
        "delivery_failed",
        None,
        json!({"handle": handle, "reason": "delivery_failed", "request_id": "a-7"}),
    )
    .await;
    let failure = expect(&mut client).await;
    assert_eq!(failure["type"], "delivery_failed");
    assert_eq!(failure["body"]["reason"], "delivery_failed");
    assert_eq!(failure["body"]["request_id"], "a-7");
}

// RSN-FR-UKCA, WSK-FR-WKAI: a worker socket that closes drops its route, and a
// later attachment is refused.
#[tokio::test]
async fn a_closed_worker_socket_drops_its_route() {
    let relay = Relay::start().await;
    let mut worker_socket = worker(&relay).await;
    let handle = accepted_handle(&relay, &mut worker_socket, "reg-1").await;

    worker_socket.close(None).await.expect("the socket closes");
    // The relay drops the route when it sees the close.
    for _ in 0..50 {
        if relay.state.http.relay.workers().is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(relay.state.http.relay.workers().is_empty());

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
        "worker_not_found"
    );
}

// RSN-FR-JAVE: several client connections may hold one handle, and a broadcast
// reaches every one of them.
#[tokio::test]
async fn a_broadcast_reaches_every_socket_of_one_handle() {
    let relay = Relay::start().await;
    let mut worker_socket = worker(&relay).await;
    let handle = accepted_handle(&relay, &mut worker_socket, "reg-1").await;
    let mut first = authenticated_client(&relay, &mut worker_socket, &handle).await;

    let mut second = relay.open("/v1/client").await;
    send(
        &mut second,
        "client",
        "client_attach",
        Some("c-2"),
        json!({"handle": handle, "instance_id": "instance-a"}),
    )
    .await;
    let attached = expect(&mut second).await;
    // RSN-FR-JAVE: the attachment and the authentication belong to the handle,
    // so the second socket observes the state the first one earned rather than
    // clearing it.
    assert_eq!(attached["body"]["authenticated"], true);
    let announced = expect(&mut worker_socket).await;
    assert_eq!(announced["type"], "client_attach");

    send(
        &mut worker_socket,
        "worker",
        "application_message",
        None,
        json!({"payload": "Ym90aA"}),
    )
    .await;
    assert_eq!(expect(&mut first).await["body"]["payload"], "Ym90aA");
    assert_eq!(expect(&mut second).await["body"]["payload"], "Ym90aA");
}

// RSN-FR-PXVC: the relay reads the status of an authentication result alone. The
// key-agreement payload beside it is carried through unchanged and acted on by
// nothing in the relay.
#[tokio::test]
async fn the_relay_reads_only_the_status_of_an_authentication_result() {
    let relay = Relay::start().await;
    let mut worker_socket = worker(&relay).await;
    let handle = accepted_handle(&relay, &mut worker_socket, "reg-1").await;

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
    assert_eq!(expect(&mut worker_socket).await["type"], "client_attach");
    send(
        &mut client,
        "client",
        "client_authenticate",
        None,
        json!({"payload": "ZGV2aWNlLXByb29m"}),
    )
    .await;
    let forwarded = expect(&mut worker_socket).await;
    // The proof reaches the worker exactly as the client wrote it.
    assert_eq!(forwarded["body"]["payload"], "ZGV2aWNlLXByb29m");

    send(
        &mut worker_socket,
        "worker",
        "client_authentication_result",
        None,
        json!({"handle": handle, "status": "accepted", "payload": "a2V5LWFncmVlbWVudA"}),
    )
    .await;
    let result = expect(&mut client).await;
    assert_eq!(result["body"]["status"], "accepted");
    assert_eq!(result["body"]["payload"], "a2V5LWFncmVlbWVudA");
    assert_eq!(
        result["body"].as_object().expect("a body").len(),
        2,
        "the relay added nothing to the result it carried"
    );
}

// RSN-FR-DRQK, WSK-FR-TMOB: a rejected authentication result closes the
// connection, leaves the handle as it was, and the client attaches again.
#[tokio::test]
async fn a_rejected_authentication_leaves_the_handle_and_closes_the_connection() {
    let relay = Relay::start().await;
    let mut worker_socket = worker(&relay).await;
    let handle = accepted_handle(&relay, &mut worker_socket, "reg-1").await;

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
    assert_eq!(expect(&mut worker_socket).await["type"], "client_attach");

    send(
        &mut worker_socket,
        "worker",
        "client_authentication_result",
        None,
        json!({"handle": handle, "status": "rejected", "payload": "d2h5"}),
    )
    .await;
    let result = expect(&mut client).await;
    assert_eq!(result["type"], "client_authentication_result");
    assert_eq!(result["body"]["status"], "rejected");
    assert_eq!(result["body"]["payload"], "d2h5");
    assert!(next(&mut client).await.is_none(), "the connection closes");

    // The handle is exactly as it was, so the client attaches again with it.
    let mut again = relay.open("/v1/client").await;
    send(
        &mut again,
        "client",
        "client_attach",
        Some("c-2"),
        json!({"handle": handle, "instance_id": "instance-a"}),
    )
    .await;
    let attached = expect(&mut again).await;
    assert_eq!(attached["type"], "client_attached");
    assert_eq!(attached["body"]["authenticated"], false);
}

// WSK-FR-FXTA, RSN-FR-RKMD: a registration payload is forwarded unchanged in
// both directions, and in the order it arrived.
#[tokio::test]
async fn a_registration_payload_is_forwarded_unchanged_in_both_directions() {
    let relay = Relay::start().await;
    let mut worker_socket = worker(&relay).await;

    let mut registration = relay.open("/v1/registration").await;
    send(
        &mut registration,
        "registration",
        "registration_start",
        Some("r-1"),
        json!({"registration_id": "reg-1", "instance_id": "instance-a", "payload": "cXItcGF5bG9hZA"}),
    )
    .await;
    let handle = expect(&mut registration).await["body"]["handle"]
        .as_str()
        .expect("a handle")
        .to_string();
    let routed = expect(&mut worker_socket).await;
    assert_eq!(routed["body"]["payload"], "cXItcGF5bG9hZA");

    // The client's half of the exchange, in order.
    for payload in ["cHJvb2Yt", "cHJvb2Yy", "cHJvb2Yz"] {
        send(
            &mut registration,
            "registration",
            "registration_payload",
            None,
            json!({"payload": payload}),
        )
        .await;
    }
    for payload in ["cHJvb2Yt", "cHJvb2Yy", "cHJvb2Yz"] {
        let forwarded = expect(&mut worker_socket).await;
        assert_eq!(forwarded["type"], "registration_payload");
        assert_eq!(forwarded["protocol"], "worker");
        assert_eq!(forwarded["body"]["handle"], handle);
        assert_eq!(forwarded["body"]["payload"], payload);
    }

    // The IDE's half, which reaches the registration client unchanged.
    send(
        &mut worker_socket,
        "worker",
        "registration_payload",
        None,
        json!({"handle": handle, "payload": "aWRlLWtleQ"}),
    )
    .await;
    let answered = expect(&mut registration).await;
    assert_eq!(answered["type"], "registration_payload");
    assert_eq!(answered["protocol"], "registration");
    assert_eq!(answered["body"]["payload"], "aWRlLWtleQ");
    assert_eq!(
        answered["body"].as_object().expect("a body").len(),
        1,
        "the relay added nothing to the payload it carried"
    );
}

// RSN-FR-GPLZ: the IDE sends exactly one terminal result for one registration.
// A second is refused, and it costs the worker nothing.
#[tokio::test]
async fn a_second_registration_result_is_refused_and_closes_no_worker() {
    let relay = Relay::start().await;
    let mut worker_socket = worker(&relay).await;
    let handle = accepted_handle(&relay, &mut worker_socket, "reg-1").await;

    send(
        &mut worker_socket,
        "worker",
        "registration_result",
        Some("x-1"),
        json!({"registration_id": "reg-1", "status": "rejected"}),
    )
    .await;
    let failure = expect(&mut worker_socket).await;
    assert_eq!(failure["type"], "relay_failure");
    assert_eq!(failure["body"]["reason"], "delivery_failed");
    assert_eq!(failure["request_id"], "x-1");

    // The worker connection stands, and the handle the first result accepted is
    // still a session.
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
    assert_eq!(expect(&mut worker_socket).await["type"], "client_attach");
}
