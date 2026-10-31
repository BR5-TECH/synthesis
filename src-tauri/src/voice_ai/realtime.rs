//! The Realtime protocol: URLs, connection, server events, and the transcript
//! that items build (VOP-FR-CVXJ, VOP-FR-TLGM, VOP-FR-XXAB).
//!
//! The differences between servers stay in this file, in one place for each
//! dialect, so that a change of one server is a small change here.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{Connector, MaybeTlsStream, WebSocketStream};

use super::{api_root, Dialect};

pub(crate) type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// VOP-FR-GDVD: the input rate of the `openai` dialect.
pub(crate) const OPENAI_RATE: u32 = 24_000;
/// VOP-FR-YRQN: the input rate of the `vllm` dialect.
pub(crate) const VLLM_RATE: u32 = 16_000;

fn percent_encoded(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// VOP-FR-CVXJ: the Realtime URL of a base URL.
pub(crate) fn realtime_url(provider: &str, base_url: &str, query: &[(&str, &str)]) -> String {
    let root = api_root(provider, base_url);
    let root = match root.split_once("://") {
        Some(("https", rest)) => format!("wss://{rest}"),
        Some(("http", rest)) => format!("ws://{rest}"),
        _ => root,
    };
    let query: Vec<String> =
        query.iter().map(|(k, v)| format!("{k}={}", percent_encoded(v))).collect();
    if query.is_empty() {
        format!("{root}/realtime")
    } else {
        format!("{root}/realtime?{}", query.join("&"))
    }
}

/// VOP-FR-HAXX: the URL of a transcription session. The `openai` URL
/// refuses a model together with this intent.
pub(crate) fn transcription_url(provider: &str, base_url: &str, stt_model: &str) -> String {
    if provider == "custom" && !stt_model.is_empty() {
        realtime_url(provider, base_url, &[("intent", "transcription"), ("model", stt_model)])
    } else {
        realtime_url(provider, base_url, &[("intent", "transcription")])
    }
}

/// VOP-FR-USQZ: the URL of a speech session.
pub(crate) fn speech_url(provider: &str, base_url: &str, tts_model: &str) -> String {
    realtime_url(provider, base_url, &[("model", tts_model)])
}

/// VOP-FR-XONB, VOP-FR-YPEY: open a connection within `limit`. The error is a
/// short kind and never holds the URL or the key.
pub(crate) async fn connect(
    url: &str,
    key: Option<&str>,
    limit: Duration,
) -> Result<Socket, String> {
    let mut request = url.into_client_request().map_err(|_| "invalid_url".to_string())?;
    if let Some(key) = key {
        let value = HeaderValue::from_str(&format!("Bearer {key}"))
            .map_err(|_| "invalid_key".to_string())?;
        request.headers_mut().insert("Authorization", value);
    }
    let config = crate::tls::websocket_client_config()?;
    let connector = Connector::Rustls(config);
    let open =
        tokio_tungstenite::connect_async_tls_with_config(request, None, false, Some(connector));
    match tokio::time::timeout(limit, open).await {
        Ok(Ok((socket, _))) => Ok(socket),
        Ok(Err(_)) => Err("handshake_failed".to_string()),
        Err(_) => Err("timed_out".to_string()),
    }
}

/// One server event, in terms that all dialects share.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ServerEvent {
    SessionCreated(SessionInfo),
    SessionUpdated,
    Committed { item_id: Option<String>, previous_item_id: Option<String> },
    Delta { item_id: Option<String>, text: String },
    Completed { item_id: Option<String>, text: String },
    TranscriptionFailed,
    AudioDelta(Vec<u8>),
    ResponseDone,
    Error { code: Option<String>, message: String },
    Other,
}

/// What `session.created` tells.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SessionInfo {
    pub(crate) session_type: Option<String>,
    pub(crate) input_rate: Option<u32>,
    pub(crate) output_rate: Option<u32>,
}

fn text_of(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn string_of(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn rate_at(session: &Value, direction: &str) -> Option<u32> {
    session
        .pointer(&format!("/audio/{direction}/format/rate"))
        .and_then(Value::as_u64)
        .and_then(|r| u32::try_from(r).ok())
}

/// Read one event. A message that is not JSON, or of a type that no dialect
/// uses here, is `Other`.
pub(crate) fn parse_event(text: &str) -> ServerEvent {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return ServerEvent::Other;
    };
    match value.get("type").and_then(Value::as_str).unwrap_or_default() {
        "session.created" => {
            let session = value.get("session").cloned().unwrap_or(Value::Null);
            ServerEvent::SessionCreated(SessionInfo {
                session_type: string_of(&session, "type"),
                input_rate: rate_at(&session, "input"),
                output_rate: rate_at(&session, "output"),
            })
        }
        "session.updated" => ServerEvent::SessionUpdated,
        "input_audio_buffer.committed" => ServerEvent::Committed {
            item_id: string_of(&value, "item_id"),
            previous_item_id: string_of(&value, "previous_item_id"),
        },
        "conversation.item.input_audio_transcription.delta" | "transcription.delta" => {
            ServerEvent::Delta {
                item_id: string_of(&value, "item_id"),
                text: text_of(&value, "delta"),
            }
        }
        "conversation.item.input_audio_transcription.completed" => ServerEvent::Completed {
            item_id: string_of(&value, "item_id"),
            text: text_of(&value, "transcript"),
        },
        "transcription.done" => {
            ServerEvent::Completed { item_id: None, text: text_of(&value, "text") }
        }
        "conversation.item.input_audio_transcription.failed" => ServerEvent::TranscriptionFailed,
        "response.output_audio.delta" => {
            match value.get("delta").and_then(Value::as_str).and_then(|b| STANDARD.decode(b).ok()) {
                Some(pcm) => ServerEvent::AudioDelta(pcm),
                None => ServerEvent::Other,
            }
        }
        "response.done" => ServerEvent::ResponseDone,
        "error" => {
            let error = value.get("error").cloned().unwrap_or(Value::Null);
            ServerEvent::Error {
                code: string_of(&error, "code").or_else(|| string_of(&error, "type")),
                message: error
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| text_of(&error, "message")),
            }
        }
        _ => ServerEvent::Other,
    }
}

/// The next event, or `None` when the socket ended with a clean close.
pub(crate) async fn next_event(socket: &mut Socket) -> Result<Option<ServerEvent>, String> {
    loop {
        match socket.next().await {
            None | Some(Ok(Message::Close(_))) => return Ok(None),
            Some(Ok(Message::Text(text))) => return Ok(Some(parse_event(text.as_str()))),
            Some(Ok(_)) => {}
            Some(Err(_)) => return Err("socket_failed".to_string()),
        }
    }
}

/// `next_event` within `limit`.
pub(crate) async fn next_event_within(
    socket: &mut Socket,
    limit: Duration,
) -> Result<Option<ServerEvent>, String> {
    tokio::time::timeout(limit, next_event(socket)).await.map_err(|_| "timed_out".to_string())?
}

pub(crate) async fn send(socket: &mut Socket, event: Value) -> Result<(), String> {
    socket.send(Message::text(event.to_string())).await.map_err(|_| "socket_failed".to_string())
}

/// VOP-FR-GYKP: one chunk of input audio.
pub(crate) async fn send_audio(socket: &mut Socket, samples: &[i16]) -> Result<(), String> {
    if samples.is_empty() {
        return Ok(());
    }
    let audio = STANDARD.encode(super::pcm::bytes_of(samples));
    send(socket, json!({ "type": "input_audio_buffer.append", "audio": audio })).await
}

/// VOP-FR-TLGM: which dialect a session uses.
pub(crate) fn dialect_of(provider: &str, session: &SessionInfo) -> Dialect {
    if provider == "openai" {
        Dialect::Openai
    } else if session.input_rate.is_some() {
        Dialect::Llamacpp
    } else {
        Dialect::Vllm
    }
}

/// VOP-FR-FHDE: the input rate of a session.
pub(crate) fn input_rate_of(dialect: Dialect, session: &SessionInfo) -> u32 {
    match dialect {
        Dialect::Openai => OPENAI_RATE,
        Dialect::Llamacpp => session.input_rate.unwrap_or(OPENAI_RATE),
        Dialect::Vllm => VLLM_RATE,
    }
}

/// VOP-FR-BTDW: a session that did not start as a transcription session.
pub(crate) fn is_turn_based(session: &SessionInfo) -> bool {
    session.session_type.as_deref().is_some_and(|t| t != "transcription")
}

/// VOP-FR-GDVD, VOP-FR-CSWH, VOP-FR-YRQN, VOP-FR-BTDW, VOP-FR-INCC:
/// configure a transcription session.
pub(crate) async fn configure_transcription(
    socket: &mut Socket,
    dialect: Dialect,
    session: &SessionInfo,
    stt_model: &str,
    keywords: &[String],
) -> Result<(), String> {
    match dialect {
        Dialect::Openai => {
            let mut transcription = json!({ "model": stt_model });
            if !keywords.is_empty() {
                transcription["keywords"] = json!(keywords);
            }
            send(
                socket,
                json!({
                    "type": "session.update",
                    "session": {
                        "type": "transcription",
                        "audio": { "input": {
                            "format": { "type": "audio/pcm", "rate": OPENAI_RATE },
                            "transcription": transcription,
                            "turn_detection": null,
                        }},
                    },
                }),
            )
            .await
        }
        Dialect::Llamacpp => {
            if is_turn_based(session) {
                send(
                    socket,
                    json!({ "type": "session.update", "session": { "type": "transcription" } }),
                )
                .await
            } else {
                Ok(())
            }
        }
        Dialect::Vllm => {
            send(socket, json!({ "type": "session.update", "model": stt_model })).await?;
            send(socket, json!({ "type": "input_audio_buffer.commit" })).await
        }
    }
}

/// VOP-FR-HOGR: the end of the input of a session.
pub(crate) async fn end_input(socket: &mut Socket, dialect: Dialect) -> Result<(), String> {
    let event = match dialect {
        Dialect::Openai => json!({ "type": "input_audio_buffer.commit" }),
        Dialect::Llamacpp | Dialect::Vllm => {
            json!({ "type": "input_audio_buffer.commit", "final": true })
        }
    };
    send(socket, event).await
}

/// VOP-FR-XXAB: the transcript of a session, joined from its items.
#[derive(Default)]
pub(crate) struct Transcript {
    /// Items in the order that they were first seen.
    seen: Vec<String>,
    /// What each item follows, from `previous_item_id`.
    follows: HashMap<String, String>,
    deltas: HashMap<String, String>,
    done: HashMap<String, String>,
}

/// A dialect with no item id has one implicit item.
const IMPLICIT_ITEM: &str = "";

impl Transcript {
    fn known(&mut self, item_id: &str) {
        if !self.seen.iter().any(|i| i == item_id) {
            self.seen.push(item_id.to_string());
        }
    }

    pub(crate) fn committed(&mut self, item_id: Option<&str>, previous: Option<&str>) {
        let Some(item_id) = item_id else { return };
        self.known(item_id);
        if let Some(previous) = previous.filter(|p| *p != item_id) {
            self.known(previous);
            self.follows.insert(item_id.to_string(), previous.to_string());
        }
    }

    pub(crate) fn delta(&mut self, item_id: Option<&str>, text: &str) {
        let id = item_id.unwrap_or(IMPLICIT_ITEM).to_string();
        self.known(&id);
        self.deltas.entry(id).or_default().push_str(text);
    }

    pub(crate) fn completed(&mut self, item_id: Option<&str>, text: &str) {
        let id = item_id.unwrap_or(IMPLICIT_ITEM).to_string();
        self.known(&id);
        self.done.insert(id, text.to_string());
    }

    /// Each item that a server named has its final text.
    pub(crate) fn all_completed(&self) -> bool {
        !self.seen.is_empty() && self.seen.iter().all(|i| self.done.contains_key(i))
    }

    pub(crate) fn is_completed(&self, item_id: &str) -> bool {
        self.done.contains_key(item_id)
    }

    /// The items in commit order: an item stands after the item that it
    /// follows, in whatever order the events came.
    fn ordered(&self) -> Vec<&String> {
        let mut out: Vec<&String> = Vec::with_capacity(self.seen.len());
        let mut placed: HashSet<&str> = HashSet::new();
        for id in &self.seen {
            // Walk back to the first item of the chain, then place forward. A
            // chain can loop only on bad server data, and `chain` ends that.
            let mut chain = vec![id];
            let mut at = id;
            while let Some(previous) = self.follows.get(at) {
                if placed.contains(previous.as_str()) || chain.contains(&previous) {
                    break;
                }
                chain.push(previous);
                at = previous;
            }
            for item in chain.into_iter().rev() {
                if placed.insert(item) {
                    out.push(item);
                }
            }
        }
        out
    }

    pub(crate) fn text(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        for id in self.ordered() {
            let part = self.done.get(id).or_else(|| self.deltas.get(id));
            if let Some(part) = part.map(|p| p.trim()).filter(|p| !p.is_empty()) {
                parts.push(part);
            }
        }
        parts.join("\n")
    }
}
