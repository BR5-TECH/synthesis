//! The client protocol.
//!
//! Specification: `specifications/server/WSK-websocket.md`.
//! Requirements: WSK-FR-YHBP, WSK-FR-EQRV, WSK-FR-CJWA, WSK-FR-TMOB,
//! WSK-FR-VDNQ, WSK-FR-ZUFK, WSK-FR-ARJP, WSK-FR-GRTY, WSK-FR-RCUY,
//! WSK-FR-WKAI.
//!
//! Session rules: `specifications/server/RSN-remote-session.md` RSN-FR-NUAB,
//! RSN-FR-GKZP, RSN-FR-LOJD, RSN-FR-WBHS, RSN-FR-OKPD, RSN-FR-HZTV.

use std::sync::Arc;

use axum::extract::ws::WebSocket;
use futures_util::stream::SplitStream;
use serde_json::json;

use crate::adapters::http::state::HttpState;
use crate::adapters::relay::session::ReasonCode;
use crate::adapters::relay::ws::envelope::{self, Envelope, Protocol};
use crate::adapters::relay::ws::hub::{deliver, Outbound, Sink};
use crate::adapters::relay::ws::{next_frame, Outgoing};

/// Runs one client connection.
pub async fn run(mut incoming: SplitStream<WebSocket>, sink: Sink, state: HttpState) {
    let connection_id = state.ids.next().to_string();
    let outgoing = Outgoing {
        protocol: Protocol::Client,
        sink,
        connection_id,
        logs: Arc::clone(&state.logs),
    };
    let socket_id = state.hub.next_socket_id();

    let Some(frame) = next_frame(&mut incoming, Protocol::Client, &outgoing).await else {
        return;
    };

    // WSK-FR-EQRV: `client_attach` is the frame that starts the session. Any
    // other first frame is refused before a route is touched.
    if frame.frame_type != "client_attach" {
        let _ = outgoing.fail(ReasonCode::InvalidFrameShape, frame.request_id.as_deref());
        return;
    }
    let Some(handle) = attach(&frame, &outgoing, &state, socket_id) else {
        return;
    };

    while let Some(frame) = next_frame(&mut incoming, Protocol::Client, &outgoing).await {
        if !dispatch(&frame, &outgoing, &state, &handle, socket_id) {
            break;
        }
    }

    // WSK-FR-WKAI: the sink leaves with the socket. The handle stays as it is,
    // so the client attaches again with it (RSN-FR-ATYV).
    state.hub.drop_client(socket_id);
}

/// Attaches the connection to the worker its handle is bound to (WSK-FR-YHBP).
fn attach(
    frame: &Envelope,
    outgoing: &Outgoing,
    state: &HttpState,
    socket_id: u64,
) -> Option<String> {
    let request_id = frame.request_id.as_deref();
    if let Err(reason) = frame.only(&["handle", "instance_id"]) {
        let _ = outgoing.fail(reason, request_id);
        return None;
    }
    let (Ok(handle), Ok(instance_id)) = (frame.text("handle"), frame.text("instance_id")) else {
        let _ = outgoing.fail(ReasonCode::InvalidFrameShape, request_id);
        return None;
    };

    let client = match state.relay.attach_client(&handle, &instance_id) {
        Ok(client) => client,
        Err(refusal) => {
            let _ = outgoing.fail(refusal, request_id);
            return None;
        }
    };

    state
        .hub
        .put_client(socket_id, &handle, outgoing.sink.clone());
    outgoing.send(
        "client_attached",
        request_id,
        json!({ "instance_id": instance_id, "authenticated": client.authenticated }),
    );

    // WSK-FR-GRTY: the worker learns that a client of its own attached, so it
    // can start the authentication exchange rather than wait to be spoken to.
    // A worker that did not take the frame is reported rather than assumed:
    // the client would otherwise wait for an exchange nobody knows to start.
    let announced = state.hub.worker(&instance_id).is_some_and(|worker| {
        deliver(
            &worker,
            envelope::frame(
                Protocol::Worker,
                "client_attach",
                None,
                json!({ "handle": handle }),
            ),
        )
    });
    if !announced {
        let _ = outgoing.fail(ReasonCode::DeliveryFailed, request_id);
    }
    Some(handle)
}

/// Routes one client frame. Reports whether the connection stays open.
fn dispatch(
    frame: &Envelope,
    outgoing: &Outgoing,
    state: &HttpState,
    handle: &str,
    socket_id: u64,
) -> bool {
    let request_id = frame.request_id.as_deref();

    // RSN-FR-KVBO, RSN-FR-ADXL: a handle that was invalidated refuses every
    // later frame with the reason it was invalidated for. Read without a
    // record: this happens on every inbound frame.
    let client = match state.relay.peek_client(handle) {
        Ok(client) => client,
        Err(refusal) => return outgoing.fail(refusal, request_id),
    };
    if let Some(refusal) = client.state.refusal() {
        return outgoing.fail(refusal, request_id);
    }

    match frame.frame_type.as_str() {
        // WSK-FR-EQRV: `client_attach` stays in the pre-authentication set, so
        // a client that re-attaches on a live socket is not thrown off it.
        "client_attach" => {
            if attach(frame, outgoing, state, socket_id).is_none() {
                return false;
            }
            true
        }

        // WSK-FR-CJWA: the proof and the key agreement reach the worker unread.
        "client_authenticate" => {
            if let Err(reason) = frame.only(&["payload"]) {
                return outgoing.fail(reason, request_id);
            }
            let payload = match frame.payload("payload") {
                Ok(payload) => payload,
                Err(reason) => return outgoing.fail(reason, request_id),
            };
            forward(
                state,
                outgoing,
                client.instance_id.as_deref(),
                "client_authenticate",
                json!({ "handle": handle, "payload": payload }),
                request_id,
            )
        }

        // WSK-FR-VDNQ, RSN-FR-LOJD: an application message is routed only for a
        // connection the worker has authenticated.
        "application_message" => {
            if !client.authenticated {
                return outgoing.fail(ReasonCode::InvalidFrameShape, request_id);
            }
            if let Err(reason) = frame.only(&["payload"]) {
                return outgoing.fail(reason, request_id);
            }
            let payload = match frame.payload("payload") {
                Ok(payload) => payload,
                Err(reason) => return outgoing.fail(reason, request_id),
            };
            forward(
                state,
                outgoing,
                client.instance_id.as_deref(),
                "application_message",
                json!({ "handle": handle, "payload": payload }),
                request_id,
            )
        }

        // WSK-FR-ARJP, RSN-FR-OKPD: a reset invalidates the handle for good and
        // closes every connection that holds it.
        "protocol_reset" => {
            if let Err(reason) = frame.only(&["reason"]) {
                return outgoing.fail(reason, request_id);
            }
            let _ = state.relay.reset_client(handle);
            outgoing.record("reset", ReasonCode::ProtocolReset);
            for socket in state.hub.clients_of(handle) {
                let _ = deliver(
                    &socket,
                    envelope::frame(
                        Protocol::Client,
                        "protocol_reset",
                        None,
                        json!({ "reason": ReasonCode::ProtocolReset.as_str() }),
                    ),
                );
                let _ = socket.try_send(Outbound::Close(ReasonCode::ProtocolReset.as_str()));
            }
            false
        }

        // WSK-FR-EQRV, WSK-FR-ZWOE: a type outside the client table is refused.
        _ => outgoing.fail(ReasonCode::InvalidFrameShape, request_id),
    }
}

/// Sends one frame to the worker of the attached route.
///
/// RSN-FR-HZTV, WSK-FR-RCUY: a frame the relay cannot deliver is answered to
/// its sender rather than reported as delivered.
fn forward(
    state: &HttpState,
    outgoing: &Outgoing,
    instance_id: Option<&str>,
    frame_type: &str,
    body: serde_json::Value,
    request_id: Option<&str>,
) -> bool {
    let Some(instance_id) = instance_id else {
        return outgoing.fail(ReasonCode::WorkerNotFound, request_id);
    };
    let Some(worker) = state.hub.worker(instance_id) else {
        return outgoing.fail(ReasonCode::WorkerNotFound, request_id);
    };
    // WSK-FR-WJIC: the frames of the worker table above are notifications, so
    // the request identifier of the client's own frame does not travel with
    // them.
    if !deliver(
        &worker,
        envelope::frame(Protocol::Worker, frame_type, None, body),
    ) {
        return outgoing.fail(ReasonCode::DeliveryFailed, request_id);
    }
    true
}
