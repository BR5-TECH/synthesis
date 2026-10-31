//! A local server that answers HTTP and WebSocket requests on one port.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{accept_hdr_async, WebSocketStream};

use crate::ai_shared::{SecretStore, SecretUnavailable};
use crate::global_settings::GlobalSettingsStore;
use crate::voice_ai::{VoiceAi, VoiceSink};

/// The secrets of a test. The handle stays with the test after the store moved
/// into `VoiceAi`.
#[derive(Clone, Default)]
pub(super) struct MemorySecrets(pub(super) Arc<Mutex<HashMap<String, String>>>);

impl SecretStore for MemorySecrets {
    fn set(&self, id: &str, secret: &str) -> Result<(), SecretUnavailable> {
        self.0.lock().unwrap().insert(id.to_string(), secret.to_string());
        Ok(())
    }

    fn get(&self, id: &str) -> Result<Option<String>, SecretUnavailable> {
        Ok(self.0.lock().unwrap().get(id).cloned())
    }

    fn delete(&self, id: &str) -> Result<(), SecretUnavailable> {
        self.0.lock().unwrap().remove(id);
        Ok(())
    }
}

/// A store in a temporary directory, with `VoiceAi` beside it.
pub(super) struct Bench {
    pub(super) store: GlobalSettingsStore,
    pub(super) voice: VoiceAi,
    pub(super) secrets: MemorySecrets,
    _dir: TempDir,
}

impl Bench {
    pub(super) fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let store = GlobalSettingsStore::with_path(dir.path().join("synthesis.toml"));
        let secrets = MemorySecrets::default();
        let voice = VoiceAi::new(Box::new(secrets.clone()));
        Self { store, voice, secrets, _dir: dir }
    }
}

/// What a test reads of the events of this module.
#[derive(Default)]
pub(super) struct RecordingSink {
    pub(super) transcripts: Mutex<Vec<String>>,
    pub(super) diagnostics: Mutex<Vec<String>>,
}

impl VoiceSink for RecordingSink {
    fn integrations_changed(&self, _: &crate::voice_ai::VoiceAiIntegrations) {}

    fn transcript_changed(&self, _: &str, text: &str) {
        self.transcripts.lock().unwrap().push(text.to_string());
    }

    fn activity_changed(&self, _: &crate::voice_ai::activity::ActivityChange) {}

    fn diagnostic(&self, stage: &str, detail: &str) {
        self.diagnostics.lock().unwrap().push(format!("{stage}: {detail}"));
    }
}

/// One HTTP request that the server saw.
#[derive(Clone, Debug)]
pub(super) struct SeenHttp {
    pub(super) path: String,
    pub(super) authorization: Option<String>,
    pub(super) body: Vec<u8>,
}

/// One WebSocket connection that the server saw.
#[derive(Clone, Debug)]
pub(super) struct SeenWs {
    pub(super) uri: String,
    pub(super) authorization: Option<String>,
}

pub(super) struct HttpReply {
    pub(super) status: u16,
    pub(super) content_type: String,
    pub(super) body: Vec<u8>,
}

impl HttpReply {
    pub(super) fn json(body: &str) -> Self {
        Self { status: 200, content_type: "application/json".into(), body: body.into() }
    }

    pub(super) fn status(status: u16) -> Self {
        Self { status, content_type: "application/json".into(), body: b"{}".to_vec() }
    }

    pub(super) fn audio(content_type: &str, body: Vec<u8>) -> Self {
        Self { status: 200, content_type: content_type.into(), body }
    }
}

pub(super) type Socket = WebSocketStream<TcpStream>;
pub(super) type Boxed = Pin<Box<dyn Future<Output = ()> + Send>>;
type HttpHandler = Arc<dyn Fn(&str, &[u8]) -> HttpReply + Send + Sync>;
type WsHandler = Arc<dyn Fn(String, Socket) -> Boxed + Send + Sync>;

/// The server, on a free local port.
pub(super) struct Fake {
    pub(super) url: String,
    pub(super) http: Arc<Mutex<Vec<SeenHttp>>>,
    pub(super) ws: Arc<Mutex<Vec<SeenWs>>>,
}

pub(super) async fn send(socket: &mut Socket, value: Value) {
    let _ = socket.send(Message::text(value.to_string())).await;
}

/// The next JSON text message, or `None` when the client closed.
pub(super) async fn recv(socket: &mut Socket) -> Option<Value> {
    while let Some(Ok(message)) = socket.next().await {
        match message {
            Message::Text(text) => return serde_json::from_str(text.as_str()).ok(),
            Message::Close(_) => return None,
            _ => {}
        }
    }
    None
}

/// The next client event of a type, and the events before it are dropped.
pub(super) async fn recv_type(socket: &mut Socket, kind: &str) -> Option<Value> {
    while let Some(event) = recv(socket).await {
        if event["type"] == kind {
            return Some(event);
        }
    }
    None
}

async fn request_head(stream: &TcpStream) -> String {
    let mut buffer = vec![0u8; 8192];
    loop {
        let read = stream.peek(&mut buffer).await.unwrap_or(0);
        let head = String::from_utf8_lossy(&buffer[..read]).to_string();
        if head.contains("\r\n\r\n") || read == 0 {
            return head;
        }
        let _ = stream.readable().await;
        tokio::task::yield_now().await;
    }
}

fn header_of(head: &str, name: &str) -> Option<String> {
    head.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.eq_ignore_ascii_case(name).then(|| value.trim().to_string())
    })
}

async fn serve_http(
    mut stream: TcpStream,
    head: String,
    handler: HttpHandler,
    seen: Arc<Mutex<Vec<SeenHttp>>>,
) {
    let head_len = head.find("\r\n\r\n").map_or(head.len(), |i| i + 4);
    let total: usize = header_of(&head, "content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut buffer = vec![0u8; head_len + total];
    if stream.read_exact(&mut buffer).await.is_err() {
        return;
    }
    let path = head.split_whitespace().nth(1).unwrap_or_default().to_string();
    let body = buffer[head_len..].to_vec();
    seen.lock().unwrap().push(SeenHttp {
        path: path.clone(),
        authorization: header_of(&head, "authorization"),
        body: body.clone(),
    });
    let reply = handler(&path, &body);
    let answer = format!(
        "HTTP/1.1 {} X\r\ncontent-type: {}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        reply.status,
        reply.content_type,
        reply.body.len()
    );
    let _ = stream.write_all(answer.as_bytes()).await;
    let _ = stream.write_all(&reply.body).await;
    let _ = stream.shutdown().await;
}

/// Start a server. `ws` is `None` for a server with no Realtime route, and the
/// handshake then gets a 404, as Ollama gives one.
// The error type of the handshake callback belongs to `tokio-tungstenite`.
#[allow(clippy::result_large_err)]
pub(super) async fn fake(
    http: impl Fn(&str, &[u8]) -> HttpReply + Send + Sync + 'static,
    ws: Option<WsHandler>,
) -> Fake {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let http: HttpHandler = Arc::new(http);
    let seen_http = Arc::new(Mutex::new(Vec::new()));
    let seen_ws = Arc::new(Mutex::new(Vec::new()));
    let (log_http, log_ws) = (seen_http.clone(), seen_ws.clone());
    tauri::async_runtime::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let (http, ws, log_http, log_ws) =
                (http.clone(), ws.clone(), log_http.clone(), log_ws.clone());
            tauri::async_runtime::spawn(async move {
                let head = request_head(&stream).await;
                let upgrade = header_of(&head, "upgrade")
                    .is_some_and(|v| v.eq_ignore_ascii_case("websocket"));
                match (upgrade, ws) {
                    (true, Some(ws)) => {
                        let uri = Arc::new(Mutex::new(String::new()));
                        let auth = Arc::new(Mutex::new(None));
                        let (uri_in, auth_in) = (uri.clone(), auth.clone());
                        let accepted =
                            accept_hdr_async(stream, move |req: &Request, res: Response| {
                                *uri_in.lock().unwrap() = req.uri().to_string();
                                *auth_in.lock().unwrap() = req
                                    .headers()
                                    .get("authorization")
                                    .and_then(|v| v.to_str().ok())
                                    .map(str::to_string);
                                Ok(res)
                            })
                            .await;
                        if let Ok(socket) = accepted {
                            let uri = uri.lock().unwrap().clone();
                            log_ws.lock().unwrap().push(SeenWs {
                                uri: uri.clone(),
                                authorization: auth.lock().unwrap().clone(),
                            });
                            ws(uri, socket).await;
                        }
                    }
                    (true, None) => {
                        let mut stream = stream;
                        let _ = stream
                            .write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\n\r\n")
                            .await;
                    }
                    _ => serve_http(stream, head, http, log_http).await,
                }
            });
        }
    });
    Fake { url, http: seen_http, ws: seen_ws }
}

pub(super) fn ws_handler<F, Fut>(handler: F) -> Option<WsHandler>
where
    F: Fn(String, Socket) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    Some(Arc::new(move |uri, socket| Box::pin(handler(uri, socket))))
}

/// The `session.created` of llama.cpp.
pub(super) fn llama_created(session_type: &str, input: u32, output: Option<u32>) -> Value {
    let mut audio =
        serde_json::json!({ "input": { "format": { "type": "audio/pcm", "rate": input } } });
    if let Some(rate) = output {
        audio["output"] = serde_json::json!({ "format": { "type": "audio/pcm", "rate": rate } });
    }
    serde_json::json!({ "type": "session.created", "session": { "type": session_type, "audio": audio } })
}

/// The answer of `/audio/transcriptions`.
pub(super) fn transcribed(text: &str) -> HttpReply {
    HttpReply::json(&serde_json::json!({ "text": text }).to_string())
}
