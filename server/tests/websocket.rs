//! The three WebSocket endpoints, driven over a real socket.
//!
//! Specifications: `specifications/server/WSK-websocket.md` and
//! `specifications/server/RSN-remote-session.md`.
//!
//! Every test here opens a listener, connects a client, and speaks the wire
//! protocol. The envelope rules are unit-tested beside the parser; what these
//! tests establish is the behaviour of the three protocols against a live
//! routing state — the boundaries between the endpoints, the handle lifecycle,
//! the delivery failures, the route cleanup, and what a restart leaves.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures_util::{SinkExt, StreamExt};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use synthesis_server::adapters::relay::projects::{HANDLE_HEADER, INSTANCE_HEADER};
use synthesis_server::router::{build_router, ServiceState};
use synthesis_server::testing::{test_state, TEST_TOKEN};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

/// A relay serving on a port of its own.
struct Relay {
    address: SocketAddr,
    state: ServiceState,
}

impl Relay {
    async fn start() -> Relay {
        let state = test_state().0;
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("the listener binds an ephemeral port");
        let address = listener
            .local_addr()
            .expect("the listener holds an address");
        let router = build_router(state.clone());
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Relay { address, state }
    }

    /// Opens one endpoint with the transport credential (WSK-FR-HGVU).
    async fn open(&self, path: &str) -> Socket {
        self.open_with(path, Some(TEST_TOKEN))
            .await
            .expect("the upgrade is accepted")
    }

    async fn open_with(&self, path: &str, token: Option<&str>) -> Result<Socket, StatusCode> {
        let mut request = format!("ws://{}{path}", self.address)
            .into_client_request()
            .expect("the request is well formed");
        if let Some(token) = token {
            request.headers_mut().insert(
                AUTHORIZATION,
                format!("Bearer {token}").parse().expect("a header value"),
            );
        }
        match connect_async(request).await {
            Ok((socket, _)) => Ok(socket),
            Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
                Err(StatusCode::from_u16(response.status().as_u16()).expect("a status"))
            }
            Err(other) => panic!("the upgrade failed for another reason: {other}"),
        }
    }

    /// Calls an HTTP route of the same service, without opening a socket.
    async fn get(&self, path: &str, headers: &[(&str, &str)]) -> (StatusCode, Value) {
        let mut request = Request::builder()
            .uri(path)
            .header("authorization", format!("Bearer {TEST_TOKEN}"));
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let response = tower::ServiceExt::oneshot(
            build_router(self.state.clone()),
            request
                .body(Body::empty())
                .expect("the request is well formed"),
        )
        .await
        .expect("the router answers");
        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("the body is readable")
            .to_bytes();
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, body)
    }
}

/// Sends one frame of a protocol.
async fn send(
    socket: &mut Socket,
    protocol: &str,
    frame_type: &str,
    request_id: Option<&str>,
    body: Value,
) {
    let mut envelope = json!({
        "protocol": protocol,
        "version": 1,
        "type": frame_type,
        "body": body,
    });
    if let Some(request_id) = request_id {
        envelope["request_id"] = json!(request_id);
    }
    socket
        .send(Message::Text(envelope.to_string().into()))
        .await
        .expect("the frame is written");
}

/// How a connection ended: the close code and the reason the relay named
/// (WSK-FR-OGLM).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Closed {
    pub code: u16,
    pub reason: String,
}

/// The next text frame, or nothing when the socket closed first.
async fn next(socket: &mut Socket) -> Option<Value> {
    next_or_close(socket).await.ok()
}

/// The next text frame, or the close the relay sent.
async fn next_or_close(socket: &mut Socket) -> Result<Value, Option<Closed>> {
    loop {
        match socket.next().await {
            Some(Ok(Message::Text(text))) => {
                return Ok(serde_json::from_str(&text).expect("a frame is JSON"))
            }
            Some(Ok(Message::Close(frame))) => {
                return Err(frame.map(|frame| Closed {
                    code: frame.code.into(),
                    reason: frame.reason.to_string(),
                }))
            }
            None => return Err(None),
            Some(Ok(_)) => continue,
            Some(Err(_)) => return Err(None),
        }
    }
}

/// The close the relay sent, which the test requires to be there.
async fn expect_close(socket: &mut Socket) -> Closed {
    match next_or_close(socket).await {
        Err(Some(closed)) => closed,
        Err(None) => panic!("the socket ended without a close frame"),
        Ok(frame) => panic!("the socket carried another frame: {frame}"),
    }
}

/// The next frame, which the test requires to be there.
async fn expect(socket: &mut Socket) -> Value {
    next(socket).await.expect("a frame arrives")
}

fn projects() -> Value {
    json!([
        {"project_id": "p1", "display_name": "The first project"},
        {"project_id": "p2", "display_name": "The second project"}
    ])
}

/// A worker connection that holds the route of `instance-a`.
async fn worker(relay: &Relay) -> Socket {
    let mut socket = relay.open("/v1/worker").await;
    send(
        &mut socket,
        "worker",
        "worker_hello",
        Some("w-1"),
        json!({"instance_id": "instance-a", "projects": projects(), "selected_project_id": "p1"}),
    )
    .await;
    let ready = expect(&mut socket).await;
    assert_eq!(ready["type"], "worker_ready");
    assert_eq!(ready["request_id"], "w-1");
    socket
}

/// A handle the IDE accepted, and the registration socket it was assigned on.
async fn accepted_handle(relay: &Relay, worker: &mut Socket, registration_id: &str) -> String {
    let mut registration = relay.open("/v1/registration").await;
    send(
        &mut registration,
        "registration",
        "registration_start",
        Some("r-1"),
        json!({"registration_id": registration_id, "instance_id": "instance-a", "payload": "cXItcGF5bG9hZA"}),
    )
    .await;

    let bound = expect(&mut registration).await;
    assert_eq!(bound["type"], "registration_bound");
    let handle = bound["body"]["handle"]
        .as_str()
        .expect("the handle is a string")
        .to_string();

    let routed = expect(worker).await;
    assert_eq!(routed["type"], "registration_start");
    assert_eq!(routed["body"]["handle"], handle);

    send(
        worker,
        "worker",
        "registration_result",
        None,
        json!({"registration_id": registration_id, "status": "accepted"}),
    )
    .await;
    let result = expect(&mut registration).await;
    assert_eq!(result["type"], "registration_result");
    assert_eq!(result["body"]["status"], "accepted");
    handle
}

/// A client connection that is attached and authenticated.
async fn authenticated_client(relay: &Relay, worker: &mut Socket, handle: &str) -> Socket {
    let mut client = relay.open("/v1/client").await;
    send(
        &mut client,
        "client",
        "client_attach",
        Some("c-1"),
        json!({"handle": handle, "instance_id": "instance-a"}),
    )
    .await;
    let attached = expect(&mut client).await;
    assert_eq!(attached["type"], "client_attached");
    assert_eq!(attached["body"]["authenticated"], false);

    // WSK-FR-GRTY: the worker learns that a client of its own attached.
    let announced = expect(worker).await;
    assert_eq!(announced["type"], "client_attach");
    assert_eq!(announced["body"]["handle"], handle);

    send(
        &mut client,
        "client",
        "client_authenticate",
        None,
        json!({"payload": "ZGV2aWNlLXByb29m"}),
    )
    .await;
    let forwarded = expect(worker).await;
    assert_eq!(forwarded["type"], "client_authenticate");
    assert_eq!(forwarded["protocol"], "worker");

    send(
        worker,
        "worker",
        "client_authentication_result",
        None,
        json!({"handle": handle, "status": "accepted"}),
    )
    .await;
    let result = expect(&mut client).await;
    assert_eq!(result["type"], "client_authentication_result");
    assert_eq!(result["body"]["status"], "accepted");
    client
}

// The tests themselves, grouped by what they establish. The harness above is
// shared by all three (CLAUDE.md: a source or test file holds under a thousand
// lines).
#[path = "websocket/discovery.rs"]
mod discovery;
#[path = "websocket/isolation.rs"]
mod isolation;
#[path = "websocket/protocol.rs"]
mod protocol;
#[path = "websocket/session.rs"]
mod session;
