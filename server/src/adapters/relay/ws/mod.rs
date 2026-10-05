//! The WebSocket transport of the relay.
//!
//! Specification: `specifications/server/WSK-websocket.md`.
//! Requirements: WSK-FR-PLQD, WSK-FR-KTRB, WSK-FR-BJTN, WSK-FR-HGVU,
//! WSK-FR-NAXC, WSK-FR-ZWOE, WSK-FR-OGLM, WSK-FR-SEBH, WSK-FR-WVLC,
//! WSK-FR-TPJS, WSK-FR-YMOR.
//!
//! Three endpoints, one envelope, and three frame tables. The transport holds
//! the sinks of the open sockets (`hub`) and reads every routing decision from
//! the relay port of `SRB-server-relay-boundary.md` SRB-FR-DYAE.

pub mod client;
pub mod envelope;
pub mod hub;
pub mod registration;
pub mod worker;

#[cfg(test)]
mod tests;

use std::time::Duration;

use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use futures_util::stream::{SplitSink, SplitStream, StreamExt};
use futures_util::SinkExt;
use serde_json::Value;
use tokio::sync::mpsc::{channel, Receiver};

use std::sync::Arc;

use crate::adapters::http::state::HttpState;
use crate::adapters::relay::session::ReasonCode;
use crate::adapters::relay::ws::envelope::{Envelope, Protocol, MAX_FRAME_BYTES};
use crate::adapters::relay::ws::hub::{Outbound, Sink};
use crate::application::logging::LogSink;
use crate::application::ports::Principal;

/// The path of the worker endpoint.
pub const WORKER_PATH: &str = "/v1/worker";
/// The path of the client endpoint.
pub const CLIENT_PATH: &str = "/v1/client";
/// The path of the registration endpoint.
pub const REGISTRATION_PATH: &str = "/v1/registration";

/// The close code every protocol failure carries (WSK-FR-OGLM).
const POLICY_VIOLATION: u16 = 1008;

/// How long a connection may be idle before the relay pings it (WSK-FR-YMOR).
const PING_PERIOD: Duration = Duration::from_secs(30);

/// How far over the frame bound the transport still reads (WSK-FR-WVLC).
///
/// A frame just over the bound reaches the relay, so it is refused with
/// `invalid_frame_shape` and the sender learns why. Anything beyond the
/// headroom is dropped by the transport itself rather than held in memory.
const FRAME_HEADROOM: usize = 4096;

/// How many frames one connection may hold before the relay refuses to hold
/// more.
///
/// WSK-FR-GZDU: the relay holds no queue for a connection that is closed, and
/// a connection that is open but not reading is not allowed to grow one either.
/// A frame that does not fit is not delivered, and the sender is told
/// (WSK-FR-RCUY).
const OUTBOUND_CAPACITY: usize = 256;

/// How long one frame may take to reach the socket before the relay gives up on
/// the connection.
///
/// A peer that has stopped reading parks the writer for good otherwise, holding
/// a task, a socket, and its queue. The connection is closed instead, and its
/// routes leave with it (WSK-FR-WKAI).
const WRITE_TIMEOUT: Duration = Duration::from_secs(30);

/// The three upgrades of `WSK-websocket.md` (WSK-FR-KTRB).
///
/// WSK-FR-NAXC: a `GET` that is not an upgrade reaches the handler and is
/// answered `400 Bad Request` by the `WebSocketUpgrade` extractor, and no other
/// method is served on these paths.
pub fn routes() -> Router<HttpState> {
    Router::new()
        .route("/v1/worker", get(worker_upgrade))
        .route("/v1/client", get(client_upgrade))
        .route("/v1/registration", get(registration_upgrade))
}

/// The upgrade of the worker endpoint.
///
/// WSK-FR-HGVU: `Principal` resolves the bearer token before the upgrade is
/// accepted, so an upgrade with no token, with another scheme, or with the
/// wrong token is answered `401 Unauthorized` and no frame is ever read.
async fn worker_upgrade(
    State(state): State<HttpState>,
    _principal: Principal,
    upgrade: WebSocketUpgrade,
) -> Response {
    upgrade
        .max_message_size(MAX_FRAME_BYTES + FRAME_HEADROOM)
        .on_upgrade(move |socket| drive(socket, Protocol::Worker, state))
}

/// The upgrade of the client endpoint (WSK-FR-HGVU).
async fn client_upgrade(
    State(state): State<HttpState>,
    _principal: Principal,
    upgrade: WebSocketUpgrade,
) -> Response {
    upgrade
        .max_message_size(MAX_FRAME_BYTES + FRAME_HEADROOM)
        .on_upgrade(move |socket| drive(socket, Protocol::Client, state))
}

/// The upgrade of the registration endpoint (WSK-FR-HGVU, WSK-FR-BJTN).
async fn registration_upgrade(
    State(state): State<HttpState>,
    _principal: Principal,
    upgrade: WebSocketUpgrade,
) -> Response {
    upgrade
        .max_message_size(MAX_FRAME_BYTES + FRAME_HEADROOM)
        .on_upgrade(move |socket| drive(socket, Protocol::Registration, state))
}

/// Runs one connection of one protocol.
///
/// The socket is split so that a frame the relay routes to this connection is
/// written while the connection is reading. The reader holds the protocol, and
/// the writer holds the socket alone.
async fn drive(socket: WebSocket, protocol: Protocol, state: HttpState) {
    let (out, incoming) = socket.split();
    let (sink, outbound) = channel(OUTBOUND_CAPACITY);
    let writer = tokio::spawn(write_loop(out, outbound));

    match protocol {
        Protocol::Worker => worker::run(incoming, sink, state).await,
        Protocol::Client => client::run(incoming, sink, state).await,
        Protocol::Registration => registration::run(incoming, sink, state).await,
    }

    // Dropping the sink ends the writer, which closes the socket.
    let _ = writer.await;
}

/// Writes the frames of one connection, and pings it while it is idle.
async fn write_loop(mut out: SplitSink<WebSocket, Message>, mut outbound: Receiver<Outbound>) {
    let mut ping = tokio::time::interval(PING_PERIOD);
    // The first tick completes at once; the connection is not idle yet.
    ping.tick().await;

    loop {
        tokio::select! {
            message = outbound.recv() => match message {
                None => break,
                Some(Outbound::Text(text)) => {
                    match tokio::time::timeout(WRITE_TIMEOUT, out.send(Message::Text(text.into())))
                        .await
                    {
                        Ok(Ok(())) => {}
                        // The socket failed, or the peer stopped reading for
                        // longer than one frame is worth waiting on.
                        Ok(Err(_)) | Err(_) => break,
                    }
                }
                Some(Outbound::Finish) => {
                    // A normal closure: the exchange the connection carried is
                    // over, and no reason of WSK-FR-BWZT applies.
                    let _ = tokio::time::timeout(WRITE_TIMEOUT, out.send(Message::Close(None)))
                        .await;
                    break;
                }
                Some(Outbound::Close(reason)) => {
                    // WSK-FR-OGLM: the close carries the policy-violation code
                    // and the reason of WSK-FR-BWZT.
                    let _ = tokio::time::timeout(
                        WRITE_TIMEOUT,
                        out.send(Message::Close(Some(CloseFrame {
                            code: POLICY_VIOLATION,
                            reason: reason.into(),
                        }))),
                    )
                    .await;
                    break;
                }
            },
            // WSK-FR-YMOR: an idle connection is pinged, so a socket that no
            // longer carries traffic is still found to be closed.
            _ = ping.tick() => {
                match tokio::time::timeout(WRITE_TIMEOUT, out.send(Message::Ping(Vec::new().into()))).await {
                    Ok(Ok(())) => {}
                    Ok(Err(_)) | Err(_) => break,
                }
            }
        }
    }
    let _ = out.close().await;
}

/// What one connection sends.
///
/// WSK-FR-SEBH: a failure names the reason and the request identifier alone. No
/// payload, key, proof, token, or handle value reaches an answer the relay
/// writes itself.
#[derive(Clone)]
pub struct Outgoing {
    pub protocol: Protocol,
    pub sink: Sink,
    /// The identifier a record names. It is not a handle and unlocks nothing.
    pub connection_id: String,
    pub logs: Arc<dyn LogSink>,
}

impl Outgoing {
    /// Sends one frame of this connection's protocol.
    pub fn send(&self, frame_type: &str, request_id: Option<&str>, body: Value) {
        let _ = self.sink.try_send(Outbound::Text(envelope::frame(
            self.protocol,
            frame_type,
            request_id,
            body,
        )));
    }

    /// Sends the failure frame of this protocol, and closes the connection when
    /// the reason is unrecoverable (WSK-FR-NQXD, WSK-FR-OGLM).
    ///
    /// Reports whether the connection stays open, so a caller never keeps
    /// applying frames to a connection it has just closed.
    #[must_use]
    pub fn fail(&self, reason: ReasonCode, request_id: Option<&str>) -> bool {
        let _ = self.sink.try_send(Outbound::Text(envelope::relay_failure(
            self.protocol,
            reason,
            request_id,
        )));
        self.record("refused", reason);
        if reason.closes_the_connection() {
            let _ = self.sink.try_send(Outbound::Close(reason.as_str()));
            return false;
        }
        true
    }

    /// Closes the connection with one reason and no failure frame.
    pub fn close(&self, reason: ReasonCode) {
        self.record("closed", reason);
        let _ = self.sink.try_send(Outbound::Close(reason.as_str()));
    }

    /// Writes the record of a refusal or a close (SRB-FR-IZAB, RSN-FR-BSXO).
    ///
    /// The record names the connection, the protocol, and the reason. It names
    /// no payload, no key, no proof, no token, and no handle value.
    pub fn record(&self, outcome: &str, reason: ReasonCode) {
        self.logs.write(serde_json::json!({
            "event": "relay_transport",
            "operation": self.protocol.as_str(),
            "connection_id": self.connection_id,
            "outcome": outcome,
            "reason": reason.as_str(),
        }));
    }
}

/// Reads the next frame of one connection, or reports why the loop ends.
///
/// WSK-FR-DMJT: a binary message and a message that is not one JSON object are
/// each refused with `invalid_frame_shape`.
pub async fn next_frame(
    incoming: &mut SplitStream<WebSocket>,
    protocol: Protocol,
    outgoing: &Outgoing,
) -> Option<Envelope> {
    while let Some(message) = incoming.next().await {
        let message = match message {
            Ok(message) => message,
            // The socket failed. Nothing is answered on a socket that is gone.
            Err(_) => return None,
        };
        match message {
            Message::Text(text) => {
                return match envelope::parse(text.as_str(), protocol) {
                    Ok(frame) => Some(frame),
                    Err(reason) => {
                        let _ = outgoing.fail(reason, None);
                        None
                    }
                }
            }
            Message::Binary(_) => {
                let _ = outgoing.fail(ReasonCode::InvalidFrameShape, None);
                return None;
            }
            Message::Close(_) => return None,
            // A ping is answered by the transport below, and a pong needs no
            // answer at all.
            Message::Ping(_) | Message::Pong(_) => continue,
        }
    }
    None
}
