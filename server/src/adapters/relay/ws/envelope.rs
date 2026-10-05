//! The frame envelope and its validation.
//!
//! Specification: `specifications/server/WSK-websocket.md`.
//! Requirements: WSK-FR-DMJT, WSK-FR-VQPH, WSK-FR-CLYA, WSK-FR-RUKN,
//! WSK-FR-EBTS, WSK-FR-WJIC, WSK-FR-QOZF, WSK-FR-AGVX, WSK-FR-SYNB,
//! WSK-FR-ZWOE, WSK-FR-WVLC.
//!
//! Validation is a pure function of the frame bytes and the protocol of the
//! endpoint, so it is driven by a test without a socket.

use serde_json::{Map, Value};

use crate::adapters::relay::port::ExposedProject;
use crate::adapters::relay::session::ReasonCode;

/// The largest frame the relay reads (WSK-FR-WVLC).
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

/// The one protocol version of V1 (WSK-FR-RUKN).
pub const PROTOCOL_VERSION: i64 = 1;

/// The most descriptors one announcement carries.
pub const MAX_PROJECTS: usize = 500;

/// The longest opaque identifier and display name.
pub const MAX_FIELD_CHARS: usize = 200;

/// Which endpoint a frame belongs to (WSK-FR-CLYA).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Worker,
    Client,
    Registration,
}

impl Protocol {
    /// The identifier the envelope carries.
    pub fn as_str(self) -> &'static str {
        match self {
            Protocol::Worker => "worker",
            Protocol::Client => "client",
            Protocol::Registration => "registration",
        }
    }
}

/// The members the envelope defines. A frame that carries another member is
/// refused (WSK-FR-VQPH).
const ENVELOPE_MEMBERS: [&str; 5] = ["protocol", "version", "type", "request_id", "body"];

/// One validated frame.
#[derive(Debug, Clone)]
pub struct Envelope {
    pub protocol: Protocol,
    pub frame_type: String,
    pub request_id: Option<String>,
    pub body: Map<String, Value>,
}

impl Envelope {
    /// A required string member of the body (WSK-FR-QOZF).
    pub fn text(&self, name: &str) -> Result<String, ReasonCode> {
        let value = self
            .body
            .get(name)
            .and_then(Value::as_str)
            .ok_or(ReasonCode::InvalidFrameShape)?;
        if value.is_empty() || value.chars().count() > MAX_FIELD_CHARS {
            return Err(ReasonCode::InvalidFrameShape);
        }
        Ok(value.to_string())
    }

    /// An optional string member of the body, which may be null.
    pub fn optional_text(&self, name: &str) -> Result<Option<String>, ReasonCode> {
        match self.body.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(_) => self.text(name).map(Some),
        }
    }

    /// A required opaque payload, which is an unpadded base64url string
    /// (WSK-FR-AGVX).
    ///
    /// The relay checks the alphabet alone. It does not decode the payload, and
    /// it never reads what the payload holds.
    pub fn payload(&self, name: &str) -> Result<String, ReasonCode> {
        let value = self
            .body
            .get(name)
            .and_then(Value::as_str)
            .ok_or(ReasonCode::InvalidFrameShape)?;
        if !is_unpadded_base64url(value) {
            return Err(ReasonCode::InvalidFrameShape);
        }
        Ok(value.to_string())
    }

    /// An optional opaque payload.
    pub fn optional_payload(&self, name: &str) -> Result<Option<String>, ReasonCode> {
        match self.body.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(_) => self.payload(name).map(Some),
        }
    }

    /// A required status of `accepted` or `rejected` (WSK-FR-UPCG,
    /// WSK-FR-ONWT).
    pub fn accepted(&self) -> Result<bool, ReasonCode> {
        match self.body.get("status").and_then(Value::as_str) {
            Some("accepted") => Ok(true),
            Some("rejected") => Ok(false),
            _ => Err(ReasonCode::InvalidFrameShape),
        }
    }

    /// A required reason from the vocabulary of WSK-FR-BWZT.
    pub fn reason(&self) -> Result<ReasonCode, ReasonCode> {
        self.body
            .get("reason")
            .and_then(Value::as_str)
            .and_then(ReasonCode::parse)
            .ok_or(ReasonCode::InvalidFrameShape)
    }

    /// Refuses a body member the frame type does not define (WSK-FR-QOZF).
    pub fn only(&self, members: &[&str]) -> Result<(), ReasonCode> {
        if self.body.keys().any(|key| !members.contains(&key.as_str())) {
            return Err(ReasonCode::InvalidFrameShape);
        }
        Ok(())
    }

    /// The exposed-project list one announcement carries
    /// (RSN-FR-BPTL, RSN-FR-XNUH).
    pub fn projects(&self) -> Result<Vec<ExposedProject>, ReasonCode> {
        let items = self
            .body
            .get("projects")
            .and_then(Value::as_array)
            .ok_or(ReasonCode::InvalidFrameShape)?;
        if items.len() > MAX_PROJECTS {
            return Err(ReasonCode::InvalidFrameShape);
        }

        let mut projects: Vec<ExposedProject> = Vec::with_capacity(items.len());
        for item in items {
            let object = item.as_object().ok_or(ReasonCode::InvalidFrameShape)?;
            if object
                .keys()
                .any(|key| key != "project_id" && key != "display_name")
            {
                return Err(ReasonCode::InvalidFrameShape);
            }
            let project_id = bounded_text(object.get("project_id"))?;
            let display_name = bounded_text(object.get("display_name"))?;
            // RSN-FR-XNUH: two descriptors with one identifier are refused, and
            // the exposed set does not change.
            if projects
                .iter()
                .any(|project| project.project_id == project_id)
            {
                return Err(ReasonCode::InvalidFrameShape);
            }
            projects.push(ExposedProject {
                project_id,
                display_name,
            });
        }
        Ok(projects)
    }
}

fn bounded_text(value: Option<&Value>) -> Result<String, ReasonCode> {
    let text = value
        .and_then(Value::as_str)
        .ok_or(ReasonCode::InvalidFrameShape)?;
    if text.is_empty() || text.chars().count() > MAX_FIELD_CHARS {
        return Err(ReasonCode::InvalidFrameShape);
    }
    Ok(text.to_string())
}

/// Whether a value is an unpadded base64url string (WSK-FR-AGVX).
fn is_unpadded_base64url(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_FRAME_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// Reads one frame of the endpoint's protocol.
///
/// The relay reads the envelope members alone. It never parses, decrypts, or
/// rewrites the opaque members the frame carries (WSK-FR-SYNB).
pub fn parse(text: &str, endpoint: Protocol) -> Result<Envelope, ReasonCode> {
    // WSK-FR-WVLC: a frame over the bound is refused before it is parsed.
    if text.len() > MAX_FRAME_BYTES {
        return Err(ReasonCode::InvalidFrameShape);
    }

    // WSK-FR-DMJT: exactly one JSON object. `from_str` refuses a trailing
    // value, so a frame that carries two documents is refused here.
    let value: Value = serde_json::from_str(text).map_err(|_| ReasonCode::InvalidFrameShape)?;
    let object = value.as_object().ok_or(ReasonCode::InvalidFrameShape)?;

    // WSK-FR-VQPH: a member the envelope does not define is refused.
    if object
        .keys()
        .any(|key| !ENVELOPE_MEMBERS.contains(&key.as_str()))
    {
        return Err(ReasonCode::InvalidFrameShape);
    }

    // WSK-FR-CLYA, WSK-FR-ZWOE: the protocol must equal the endpoint's own, so
    // a frame of one protocol is never read as a frame of another.
    let protocol = object
        .get("protocol")
        .and_then(Value::as_str)
        .ok_or(ReasonCode::InvalidFrameShape)?;
    if protocol != endpoint.as_str() {
        return Err(ReasonCode::ProtocolMismatch);
    }

    // WSK-FR-RUKN: the version is the integer 1, and nothing else.
    let version = object.get("version").ok_or(ReasonCode::InvalidFrameShape)?;
    match version.as_i64() {
        Some(PROTOCOL_VERSION) => {}
        Some(_) => return Err(ReasonCode::UnsupportedVersion),
        None => {
            return Err(if version.is_number() {
                ReasonCode::UnsupportedVersion
            } else {
                ReasonCode::InvalidFrameShape
            })
        }
    }

    // WSK-FR-EBTS: the type is a stable identifier. Whether the endpoint's
    // table holds it is decided by the protocol handler.
    let frame_type = object
        .get("type")
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty() && text.chars().count() <= MAX_FIELD_CHARS)
        .ok_or(ReasonCode::InvalidFrameShape)?
        .to_string();

    // WSK-FR-WJIC: a request identifier is an opaque string when it is present.
    let request_id = match object.get("request_id") {
        None | Some(Value::Null) => None,
        Some(value) => Some(
            value
                .as_str()
                .filter(|text| !text.is_empty() && text.chars().count() <= MAX_FIELD_CHARS)
                .ok_or(ReasonCode::InvalidFrameShape)?
                .to_string(),
        ),
    };

    // WSK-FR-QOZF: the body is a JSON object.
    let body = object
        .get("body")
        .and_then(Value::as_object)
        .cloned()
        .ok_or(ReasonCode::InvalidFrameShape)?;

    Ok(Envelope {
        protocol: endpoint,
        frame_type,
        request_id,
        body,
    })
}

/// Builds one outbound frame (WSK-FR-SYNB).
///
/// The relay stamps the destination endpoint's protocol on the envelope it
/// delivers, and carries the body it routes unchanged.
pub fn frame(
    protocol: Protocol,
    frame_type: &str,
    request_id: Option<&str>,
    body: Value,
) -> String {
    let mut envelope = Map::new();
    envelope.insert("protocol".to_string(), Value::from(protocol.as_str()));
    envelope.insert("version".to_string(), Value::from(PROTOCOL_VERSION));
    envelope.insert("type".to_string(), Value::from(frame_type));
    if let Some(request_id) = request_id {
        envelope.insert("request_id".to_string(), Value::from(request_id));
    }
    envelope.insert("body".to_string(), body);
    Value::Object(envelope).to_string()
}

/// The failure frame of the worker and the client endpoints (WSK-FR-NQXD).
pub fn relay_failure(protocol: Protocol, reason: ReasonCode, request_id: Option<&str>) -> String {
    let frame_type = if protocol == Protocol::Registration {
        "registration_failed"
    } else {
        "relay_failure"
    };
    frame(
        protocol,
        frame_type,
        request_id,
        serde_json::json!({ "reason": reason.as_str() }),
    )
}
