//! The registration protocol.
//!
//! Specification: `specifications/server/WSK-websocket.md`.
//! Requirements: WSK-FR-HNSC, WSK-FR-QBLW, WSK-FR-FXTA, WSK-FR-DPGI,
//! WSK-FR-KOEV, WSK-FR-ZWOE.
//!
//! Session rules: `specifications/server/RSN-remote-session.md` RSN-FR-LQAF,
//! RSN-FR-NGKW, RSN-FR-BUXE, RSN-FR-YAOC, RSN-FR-EIVS, RSN-FR-RKMD.

use std::sync::Arc;

use axum::extract::ws::WebSocket;
use futures_util::stream::SplitStream;
use serde_json::json;

use crate::adapters::http::state::HttpState;
use crate::adapters::relay::session::ReasonCode;
use crate::adapters::relay::ws::envelope::{self, Envelope, Protocol};
use crate::adapters::relay::ws::hub::{deliver, Sink};
use crate::adapters::relay::ws::{next_frame, Outgoing};

/// Runs one registration connection.
pub async fn run(mut incoming: SplitStream<WebSocket>, sink: Sink, state: HttpState) {
    let connection_id = state.ids.next().to_string();
    let outgoing = Outgoing {
        protocol: Protocol::Registration,
        sink,
        connection_id,
        logs: Arc::clone(&state.logs),
    };
    let socket_id = state.hub.next_socket_id();

    let Some(frame) = next_frame(&mut incoming, Protocol::Registration, &outgoing).await else {
        return;
    };

    // WSK-FR-HNSC: the first frame is `registration_start`.
    if frame.frame_type != "registration_start" {
        let _ = outgoing.fail(ReasonCode::InvalidFrameShape, frame.request_id.as_deref());
        return;
    }

    let Some(handle) = start(&frame, &outgoing, &state, socket_id) else {
        state.hub.drop_registration(socket_id);
        return;
    };

    while let Some(frame) = next_frame(&mut incoming, Protocol::Registration, &outgoing).await {
        if !dispatch(&frame, &outgoing, &state, &handle) {
            break;
        }
    }

    // RSN-FR-YAOC: the registration state leaves with the connection, and a
    // handle the IDE never resolved leaves with it too.
    state.hub.drop_registration(socket_id);
    state.relay.drop_pending_registration(&handle);
}

/// Routes one registration to its worker and assigns the handle
/// (WSK-FR-QBLW, RSN-FR-EIVS).
fn start(
    frame: &Envelope,
    outgoing: &Outgoing,
    state: &HttpState,
    socket_id: u64,
) -> Option<String> {
    let request_id = frame.request_id.as_deref();
    if let Err(reason) = frame.only(&["registration_id", "instance_id", "payload"]) {
        let _ = outgoing.fail(reason, request_id);
        return None;
    }
    let (Ok(registration_id), Ok(instance_id), Ok(payload)) = (
        frame.text("registration_id"),
        frame.text("instance_id"),
        frame.payload("payload"),
    ) else {
        let _ = outgoing.fail(ReasonCode::InvalidFrameShape, request_id);
        return None;
    };

    // RSN-FR-NGKW: the registration identifier is routing state, so two live
    // exchanges never share one. The second is refused before a handle is
    // assigned, rather than becoming reachable by the first one's answers.
    if !state
        .hub
        .put_registration(socket_id, &registration_id, outgoing.sink.clone())
    {
        let _ = outgoing.fail(ReasonCode::InvalidFrameShape, request_id);
        return None;
    }

    // RSN-FR-BUXE: an absent worker route is a typed failure, and the
    // registration connection closes.
    let client = match state.relay.register_client(&registration_id, &instance_id) {
        Ok(client) => client,
        Err(refusal) => {
            let _ = outgoing.fail(refusal, request_id);
            return None;
        }
    };

    let Some(worker) = state.hub.worker(&instance_id) else {
        let _ = outgoing.fail(ReasonCode::WorkerNotFound, request_id);
        state.relay.drop_pending_registration(&client.handle);
        return None;
    };

    state.hub.bind_registration(socket_id, &client.handle);

    // RSN-FR-EIVS: the handle reaches the mobile client alone, and the worker
    // learns which handle the exchange belongs to (WSK-FR-GRTY).
    // The worker is told first: a registration the IDE never hears about is a
    // handle the mobile client would wait on for nothing (WSK-FR-RCUY).
    if !deliver(
        &worker,
        envelope::frame(
            Protocol::Worker,
            "registration_start",
            None,
            json!({
                "registration_id": registration_id,
                "handle": client.handle,
                "payload": payload,
            }),
        ),
    ) {
        let _ = outgoing.fail(ReasonCode::DeliveryFailed, request_id);
        state.relay.drop_pending_registration(&client.handle);
        return None;
    }
    outgoing.send(
        "registration_bound",
        request_id,
        json!({ "handle": client.handle }),
    );

    Some(client.handle)
}

/// Routes one registration frame. Reports whether the connection stays open.
fn dispatch(frame: &Envelope, outgoing: &Outgoing, state: &HttpState, handle: &str) -> bool {
    let request_id = frame.request_id.as_deref();
    match frame.frame_type.as_str() {
        // WSK-FR-FXTA, RSN-FR-RKMD: the payload reaches the worker unchanged
        // and in the order it arrived.
        "registration_payload" => {
            if let Err(reason) = frame.only(&["payload"]) {
                return outgoing.fail(reason, request_id);
            }
            let payload = match frame.payload("payload") {
                Ok(payload) => payload,
                Err(reason) => return outgoing.fail(reason, request_id),
            };
            let client = match state.relay.peek_client(handle) {
                Ok(client) => client,
                Err(refusal) => return outgoing.fail(refusal, request_id),
            };
            let Some(worker) = state.hub.worker(&client.bound_instance_id) else {
                return outgoing.fail(ReasonCode::WorkerNotFound, request_id);
            };
            // WSK-FR-WJIC: the worker's `registration_payload` is a
            // notification, so no request identifier travels with it.
            if !deliver(
                &worker,
                envelope::frame(
                    Protocol::Worker,
                    "registration_payload",
                    None,
                    json!({ "handle": handle, "payload": payload }),
                ),
            ) {
                return outgoing.fail(ReasonCode::DeliveryFailed, request_id);
            }
            true
        }

        // WSK-FR-ZWOE: a worker or client frame is never read here.
        _ => outgoing.fail(ReasonCode::InvalidFrameShape, request_id),
    }
}
