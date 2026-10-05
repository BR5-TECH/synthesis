//! The worker protocol.
//!
//! Specification: `specifications/server/WSK-websocket.md`.
//! Requirements: WSK-FR-TDKM, WSK-FR-FEHL, WSK-FR-XRVO, WSK-FR-JBQZ,
//! WSK-FR-MZQD, WSK-FR-UPCG, WSK-FR-ONWT, WSK-FR-IKVE, WSK-FR-LSAD,
//! WSK-FR-FXTA, WSK-FR-GRTY, WSK-FR-RCUY, WSK-FR-WKAI.
//!
//! Session rules: `specifications/server/RSN-remote-session.md` RSN-FR-OZMC,
//! RSN-FR-BPTL, RSN-FR-XNUH, RSN-FR-CQEF, RSN-FR-VTKA, RSN-FR-UDCM,
//! RSN-FR-EHUT, RSN-FR-SVMD, RSN-FR-UKCA, RSN-FR-DWLS, RSN-FR-KVBO,
//! RSN-FR-MFCX.
//!
//! Every frame that names a handle is applied to a handle **this** worker owns.
//! A worker never authenticates, invalidates, or resolves a handle bound to
//! another worker, and a connection that a replacement took the route from
//! stops being authoritative at once.

use std::sync::Arc;

use axum::extract::ws::WebSocket;
use futures_util::stream::SplitStream;
use serde_json::json;

use crate::adapters::http::state::HttpState;
use crate::adapters::relay::port::{ExposedProject, WorkerRegistration};
use crate::adapters::relay::session::{InvalidReason, ReasonCode};
use crate::adapters::relay::ws::envelope::{self, Envelope, Protocol};
use crate::adapters::relay::ws::hub::{deliver, Outbound, Sink};
use crate::adapters::relay::ws::{next_frame, Outgoing};

/// Runs one worker connection.
pub async fn run(mut incoming: SplitStream<WebSocket>, sink: Sink, state: HttpState) {
    let connection_id = state.ids.next().to_string();
    let outgoing = Outgoing {
        protocol: Protocol::Worker,
        sink,
        connection_id: connection_id.clone(),
        logs: Arc::clone(&state.logs),
    };

    let Some(frame) = next_frame(&mut incoming, Protocol::Worker, &outgoing).await else {
        return;
    };

    // WSK-FR-TDKM: the first frame is `worker_hello`, and any other first frame
    // is refused before a route is recorded.
    if frame.frame_type != "worker_hello" {
        let _ = outgoing.fail(
            ReasonCode::MissingWorkerAuthentication,
            frame.request_id.as_deref(),
        );
        return;
    }

    let Some(instance_id) = hello(&frame, &outgoing, &state, &connection_id) else {
        return;
    };

    outgoing.send(
        "worker_ready",
        frame.request_id.as_deref(),
        json!({ "instance_id": instance_id, "connection_id": connection_id }),
    );

    while let Some(frame) = next_frame(&mut incoming, Protocol::Worker, &outgoing).await {
        // RSN-FR-EHUT: a connection a replacement took the route from stops
        // being authoritative at once, whether or not its peer has read the
        // close frame it was already sent.
        if state.relay.route_connection(&instance_id).as_deref() != Some(connection_id.as_str()) {
            outgoing.close(ReasonCode::WorkerReplaced);
            return;
        }
        if !dispatch(&frame, &outgoing, &state, &instance_id, &connection_id) {
            break;
        }
    }

    // RSN-FR-UKCA, WSK-FR-WKAI: the route leaves with the connection, and every
    // client attached to it is detached.
    let _ = state.relay.drop_worker(&instance_id, &connection_id);
    state.hub.drop_worker(&instance_id, &connection_id);
}

/// The exposed set and the selected project one announcement carries
/// (RSN-FR-BPTL, RSN-FR-VTKA).
type Announcement = (Vec<ExposedProject>, Option<String>);

fn announcement(frame: &Envelope) -> Result<Announcement, ReasonCode> {
    let projects = frame.projects()?;
    let selected = frame.optional_text("selected_project_id")?;
    // RSN-FR-VTKA: a selection that is not in the same announcement's exposed
    // set is refused.
    if let Some(selected) = &selected {
        if !projects
            .iter()
            .any(|project| &project.project_id == selected)
        {
            return Err(ReasonCode::InvalidFrameShape);
        }
    }
    Ok((projects, selected))
}

/// Registers the worker route from the first frame (WSK-FR-FEHL).
fn hello(
    frame: &Envelope,
    outgoing: &Outgoing,
    state: &HttpState,
    connection_id: &str,
) -> Option<String> {
    let request_id = frame.request_id.as_deref();
    if let Err(reason) = frame.only(&["instance_id", "projects", "selected_project_id"]) {
        let _ = outgoing.fail(reason, request_id);
        return None;
    }

    // WSK-FR-XRVO: an identifier that is absent, empty, or too long is refused,
    // and no worker route is registered.
    let Ok(instance_id) = frame.text("instance_id") else {
        let _ = outgoing.fail(ReasonCode::InvalidWorkerAuthentication, request_id);
        return None;
    };

    let (projects, selected) = match announcement(frame) {
        Ok(announcement) => announcement,
        Err(reason) => {
            let _ = outgoing.fail(reason, request_id);
            return None;
        }
    };

    let registration =
        match state
            .relay
            .register_worker(&instance_id, connection_id, projects.clone())
        {
            Ok(registration) => registration,
            Err(_) => {
                let _ = outgoing.fail(ReasonCode::InvalidWorkerAuthentication, request_id);
                return None;
            }
        };

    // RSN-FR-CQEF, RSN-FR-VTKA: the exposed set and the selection are applied
    // together, so no reader observes one announcement's projects beside
    // another's selection.
    if selected.is_some()
        && state
            .relay
            .announce_projects(&instance_id, connection_id, projects, selected.as_deref())
            .is_err()
    {
        let _ = outgoing.fail(ReasonCode::WorkerNotFound, request_id);
        return None;
    }

    // RSN-FR-EHUT, RSN-FR-SVMD: the replacement closes the older worker
    // connection and notifies every client the older route held.
    if let WorkerRegistration::Replaced {
        detached_handles, ..
    } = &registration
    {
        for handle in detached_handles {
            for client in state.hub.clients_of(handle) {
                // A client that is not reading learns of the replacement when
                // its own connection is closed by the write bound instead.
                let _ = deliver(
                    &client,
                    envelope::frame(
                        Protocol::Client,
                        "worker_replaced",
                        None,
                        json!({ "instance_id": instance_id }),
                    ),
                );
            }
        }
    }

    if let Some(replaced) = state
        .hub
        .put_worker(&instance_id, connection_id, outgoing.sink.clone())
    {
        let _ = replaced.try_send(Outbound::Close(ReasonCode::WorkerReplaced.as_str()));
    }

    Some(instance_id)
}

/// Routes one worker frame. Reports whether the connection stays open.
fn dispatch(
    frame: &Envelope,
    outgoing: &Outgoing,
    state: &HttpState,
    instance_id: &str,
    connection_id: &str,
) -> bool {
    let request_id = frame.request_id.as_deref();
    match frame.frame_type.as_str() {
        // WSK-FR-JBQZ: the announcement replaces the whole exposed set.
        "projects_changed" => {
            if let Err(reason) = frame.only(&["projects", "selected_project_id"]) {
                return outgoing.fail(reason, request_id);
            }
            let (projects, selected) = match announcement(frame) {
                Ok(announcement) => announcement,
                Err(reason) => return outgoing.fail(reason, request_id),
            };
            if state
                .relay
                .announce_projects(instance_id, connection_id, projects, selected.as_deref())
                .is_err()
            {
                return outgoing.fail(ReasonCode::WorkerNotFound, request_id);
            }
            true
        }

        // WSK-FR-MZQD: the IDE invalidates one handle of its own, for the
        // reason it names.
        "handle_invalidated" => {
            if let Err(reason) = frame.only(&["handle", "reason"]) {
                return outgoing.fail(reason, request_id);
            }
            let (Ok(handle), Ok(reason)) = (frame.text("handle"), frame.reason()) else {
                return outgoing.fail(ReasonCode::InvalidFrameShape, request_id);
            };
            let invalid = match reason {
                ReasonCode::HandleRevoked => InvalidReason::Revoked,
                ReasonCode::HandleExpired => InvalidReason::Expired,
                _ => return outgoing.fail(ReasonCode::InvalidFrameShape, request_id),
            };
            // RSN-FR-UDCM: a handle bound to another worker is not this
            // worker's to invalidate, and refusing it costs this connection
            // nothing.
            match state
                .relay
                .invalidate_handle(&handle, Some(instance_id), invalid)
            {
                // RSN-FR-KVBO: every connection that holds the handle closes
                // with the reason the IDE named.
                Ok(_) => {
                    for client in state.hub.clients_of(&handle) {
                        let _ = client.try_send(Outbound::Close(reason.as_str()));
                    }
                    true
                }
                Err(_) => outgoing.fail(ReasonCode::DeliveryFailed, request_id),
            }
        }

        // WSK-FR-FXTA, RSN-FR-RKMD: a registration payload the IDE sends
        // reaches the registration client unchanged.
        "registration_payload" => {
            if let Err(reason) = frame.only(&["handle", "payload"]) {
                return outgoing.fail(reason, request_id);
            }
            let (Ok(handle), Ok(payload)) = (frame.text("handle"), frame.payload("payload")) else {
                return outgoing.fail(ReasonCode::InvalidFrameShape, request_id);
            };
            if !owns_handle(state, &handle, instance_id) {
                return outgoing.fail(ReasonCode::DeliveryFailed, request_id);
            }
            let Some(client) = state.hub.registration_of_handle(&handle) else {
                return outgoing.fail(ReasonCode::DeliveryFailed, request_id);
            };
            if !deliver(
                &client,
                envelope::frame(
                    Protocol::Registration,
                    "registration_payload",
                    None,
                    json!({ "payload": payload }),
                ),
            ) {
                return outgoing.fail(ReasonCode::DeliveryFailed, request_id);
            }
            true
        }

        // WSK-FR-UPCG: the one terminal result of a registration of this
        // worker's own.
        "registration_result" => {
            if let Err(reason) = frame.only(&["registration_id", "status", "payload"]) {
                return outgoing.fail(reason, request_id);
            }
            let (Ok(registration_id), Ok(accepted)) =
                (frame.text("registration_id"), frame.accepted())
            else {
                return outgoing.fail(ReasonCode::InvalidFrameShape, request_id);
            };
            let payload = match frame.optional_payload("payload") {
                Ok(payload) => payload,
                Err(reason) => return outgoing.fail(reason, request_id),
            };

            // RSN-FR-GPLZ: a second result, and a result for a registration
            // this worker does not hold, are answered to the worker and cost it
            // nothing. The registration connection is the one that closes.
            let Some(handle) = state.hub.registration_handle(&registration_id) else {
                return outgoing.fail(ReasonCode::DeliveryFailed, request_id);
            };
            if state
                .relay
                .resolve_registration(&handle, instance_id, accepted)
                .is_err()
            {
                return outgoing.fail(ReasonCode::DeliveryFailed, request_id);
            }
            let Some(client) = state.hub.registration_of(&registration_id) else {
                return outgoing.fail(ReasonCode::DeliveryFailed, request_id);
            };
            let mut body = json!({ "status": if accepted { "accepted" } else { "rejected" } });
            if let Some(payload) = payload {
                body["payload"] = json!(payload);
            }
            let delivered = deliver(
                &client,
                envelope::frame(Protocol::Registration, "registration_result", None, body),
            );
            // The terminal result already told the registration client the
            // outcome, so the connection closes normally either way.
            let _ = client.try_send(Outbound::Finish);
            if !delivered {
                return outgoing.fail(ReasonCode::DeliveryFailed, request_id);
            }
            true
        }

        // WSK-FR-ONWT: the relay reads the status alone.
        "client_authentication_result" => {
            if let Err(reason) = frame.only(&["handle", "status", "payload"]) {
                return outgoing.fail(reason, request_id);
            }
            let (Ok(handle), Ok(accepted)) = (frame.text("handle"), frame.accepted()) else {
                return outgoing.fail(ReasonCode::InvalidFrameShape, request_id);
            };
            let payload = match frame.optional_payload("payload") {
                Ok(payload) => payload,
                Err(reason) => return outgoing.fail(reason, request_id),
            };

            // RSN-FR-UDCM: one worker never authenticates another worker's
            // client, and a handle it does not own is not routed at all.
            if !owns_handle(state, &handle, instance_id) {
                return outgoing.fail(ReasonCode::DeliveryFailed, request_id);
            }
            if accepted
                && state
                    .relay
                    .authenticate_client(&handle, instance_id)
                    .is_err()
            {
                return outgoing.fail(ReasonCode::DeliveryFailed, request_id);
            }

            let mut body = json!({ "status": if accepted { "accepted" } else { "rejected" } });
            if let Some(payload) = payload {
                body["payload"] = json!(payload);
            }
            let sinks = state.hub.clients_of(&handle);
            if sinks.is_empty() {
                // WSK-FR-RCUY: the relay reports no delivery it did not make.
                return outgoing.fail(ReasonCode::DeliveryFailed, request_id);
            }
            let mut delivered = false;
            for client in sinks {
                delivered |= deliver(
                    &client,
                    envelope::frame(
                        Protocol::Client,
                        "client_authentication_result",
                        None,
                        body.clone(),
                    ),
                );
                // RSN-FR-DRQK: a rejected result closes the connection and
                // leaves the handle as it was.
                if !accepted {
                    let _ = client.try_send(Outbound::Finish);
                }
            }
            if !delivered {
                return outgoing.fail(ReasonCode::DeliveryFailed, request_id);
            }
            true
        }

        // WSK-FR-IKVE: a worker message reaches every authenticated client of
        // that worker, and no connection of another route.
        "application_message" => {
            if let Err(reason) = frame.only(&["payload"]) {
                return outgoing.fail(reason, request_id);
            }
            let payload = match frame.payload("payload") {
                Ok(payload) => payload,
                Err(reason) => return outgoing.fail(reason, request_id),
            };
            let targets = match state.relay.broadcast_targets(instance_id, connection_id) {
                Ok(targets) => targets,
                Err(refusal) => return outgoing.fail(refusal, request_id),
            };
            let sinks = state.hub.clients_of_all(&targets);
            if sinks.is_empty() {
                return outgoing.fail(ReasonCode::DeliveryFailed, request_id);
            }
            // WSK-FR-RCUY: a client whose queue is full did not take the
            // frame, so the worker is told rather than left believing every
            // client of its route read it.
            let mut undelivered = 0;
            for client in sinks {
                if !deliver(
                    &client,
                    envelope::frame(
                        Protocol::Client,
                        "application_message",
                        None,
                        json!({ "payload": payload }),
                    ),
                ) {
                    undelivered += 1;
                }
            }
            if undelivered > 0 {
                return outgoing.fail(ReasonCode::DeliveryFailed, request_id);
            }
            true
        }

        // WSK-FR-LSAD, RSN-FR-MFCX: the worker's own delivery failure reaches
        // the client the frame came from.
        "delivery_failed" => {
            if let Err(reason) = frame.only(&["handle", "reason", "request_id"]) {
                return outgoing.fail(reason, request_id);
            }
            let (Ok(handle), Ok(reason)) = (frame.text("handle"), frame.reason()) else {
                return outgoing.fail(ReasonCode::InvalidFrameShape, request_id);
            };
            if !owns_handle(state, &handle, instance_id) {
                return outgoing.fail(ReasonCode::DeliveryFailed, request_id);
            }
            let answered = frame.optional_text("request_id").ok().flatten();
            let mut body = json!({ "reason": reason.as_str() });
            if let Some(answered) = answered {
                body["request_id"] = json!(answered);
            }
            for client in state.hub.clients_of(&handle) {
                let _ = deliver(
                    &client,
                    envelope::frame(Protocol::Client, "delivery_failed", None, body.clone()),
                );
            }
            true
        }

        // WSK-FR-EBTS, WSK-FR-ZWOE: a type outside the worker table is refused,
        // including a registration or client frame that names this endpoint.
        _ => outgoing.fail(ReasonCode::InvalidFrameShape, request_id),
    }
}

/// Whether a handle is bound to this worker (RSN-FR-UDCM).
///
/// Read without a record: the transport asks it on every frame that names a
/// handle, and a record for each would evict the records a reader needs.
fn owns_handle(state: &HttpState, handle: &str, instance_id: &str) -> bool {
    state
        .relay
        .peek_client(handle)
        .is_ok_and(|client| client.bound_instance_id == instance_id)
}
