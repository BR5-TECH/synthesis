//! CVL-FR-TQRD: the two tolerances this application adds to a reply of the
//! Custom gateway, before the framework decodes it.
//!
//! `rig` decodes a Responses reply into structs that are stricter than some
//! gateways. Its `text` object requires `format`, and its `output_text` part
//! requires `text` as a string. A gateway can return `"text": {}`, or an
//! `output_text` part with `"text": null` beside a valid tool call. Then the
//! framework refuses the whole reply and the answer of the model is lost. The
//! HTTP client that `rig` is given is wrapped here, so the body is repaired
//! before `rig` reads it. Every other body passes byte-for-byte.

use std::future::Future;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use rig::http_client::{
    HttpClientExt, LazyBody, MultipartForm, Request, Response, Result, StreamingResponse,
};
use rig::wasm_compat::WasmCompatSend;
use serde_json::Value;

/// What the repair changed in one reply. The default changed nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReplyRepairs {
    /// The top-level `text` object got the plain text format.
    pub text_format: bool,
    /// How many `output_text` parts without text were removed.
    pub null_text_parts: usize,
}

impl ReplyRepairs {
    /// Whether the repair changed anything.
    pub fn any(&self) -> bool {
        self.text_format || self.null_text_parts > 0
    }
}

/// The format a `text` object without one is read as.
fn plain_text_format() -> Value {
    serde_json::json!({ "type": "text" })
}

/// Sets the plain text format where the top-level `text` object has no
/// `format` or a null one. Returns whether it changed the reply.
fn repair_text_format(reply: &mut Value) -> bool {
    let Some(text) = reply.get_mut("text").and_then(Value::as_object_mut) else {
        return false;
    };
    if !text.get("format").is_none_or(Value::is_null) {
        return false;
    }
    text.insert("format".to_string(), plain_text_format());
    true
}

/// Removes each `output_text` part whose `text` is null or absent from each
/// `message` item of the top-level `output` array. Returns how many it removed.
///
/// Every other part and every other item stays. A `message` item can be left
/// with no parts: the framework reads it as no text, and a tool call beside it
/// still arrives.
fn drop_null_text_parts(reply: &mut Value) -> usize {
    let Some(output) = reply.get_mut("output").and_then(Value::as_array_mut) else {
        return 0;
    };
    let mut removed = 0;
    for item in output {
        if item.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        let Some(content) = item.get_mut("content").and_then(Value::as_array_mut) else {
            continue;
        };
        let before = content.len();
        content.retain(|part| {
            part.get("type").and_then(Value::as_str) != Some("output_text")
                || !part.get("text").is_none_or(Value::is_null)
        });
        removed += before - content.len();
    }
    removed
}

/// CVL-FR-TQRD: the repaired body and what was repaired, or `None` where the
/// body needs no repair.
///
/// `None` covers every body this requirement does not name: one that needs
/// neither tolerance, one that is not a JSON object, and one that is not valid
/// JSON. The caller then passes the original bytes, so the framework reports
/// exactly what it would have reported without this function.
pub(super) fn repair_reply(body: &[u8]) -> Option<(Bytes, ReplyRepairs)> {
    let mut value: Value = serde_json::from_slice(body).ok()?;
    if !value.is_object() {
        return None;
    }
    let repairs = ReplyRepairs {
        text_format: repair_text_format(&mut value),
        null_text_parts: drop_null_text_parts(&mut value),
    };
    if !repairs.any() {
        return None;
    }
    let bytes = serde_json::to_vec(&value).ok()?;
    Some((Bytes::from(bytes), repairs))
}

/// CVL-FR-TQRD: the HTTP client of the OpenAI-compatible adapter, with the
/// repair applied to a Responses reply when `enabled` is set.
///
/// Only the Custom gateway sets `enabled`. The `openai` provider travels the
/// same adapter with it unset, so its replies pass unchanged.
#[derive(Clone, Debug, Default)]
pub(super) struct ResponsesRepair<C> {
    inner: C,
    enabled: bool,
    repairs: Arc<Mutex<ReplyRepairs>>,
}

impl<C> ResponsesRepair<C> {
    pub(super) fn new(inner: C, enabled: bool) -> Self {
        Self {
            inner,
            enabled,
            repairs: Arc::new(Mutex::new(ReplyRepairs::default())),
        }
    }

    /// What the repair changed in the reply this client received. Read after
    /// the call, so the loop can record it (the client has no access to the
    /// log).
    pub(super) fn repairs(&self) -> ReplyRepairs {
        *self.repairs.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Whether a request goes to the Responses route.
pub(super) fn is_responses_route<T>(req: &Request<T>) -> bool {
    req.uri().path().trim_end_matches('/').ends_with("/responses")
}

impl<C: HttpClientExt> HttpClientExt for ResponsesRepair<C> {
    fn send<T, U>(
        &self,
        req: Request<T>,
    ) -> impl Future<Output = Result<Response<LazyBody<U>>>> + WasmCompatSend + 'static
    where
        T: Into<Bytes>,
        T: WasmCompatSend,
        U: From<Bytes>,
        U: WasmCompatSend + 'static,
    {
        let repair = self.enabled && is_responses_route(&req);
        let record = self.repairs.clone();
        let sent = self.inner.send::<T, Bytes>(req);
        async move {
            let response = sent.await?;
            let (parts, body) = response.into_parts();
            let body: LazyBody<U> = Box::pin(async move {
                let bytes = body.await?;
                if !repair {
                    return Ok(U::from(bytes));
                }
                // The record always names the last Responses reply, so a reply
                // that needs no repair clears what an earlier one recorded.
                let (body, repairs) = match repair_reply(&bytes) {
                    Some((fixed, repairs)) => (fixed, repairs),
                    None => (bytes, ReplyRepairs::default()),
                };
                *record.lock().unwrap_or_else(|e| e.into_inner()) = repairs;
                Ok(U::from(body))
            });
            Ok(Response::from_parts(parts, body))
        }
    }

    fn send_multipart<U>(
        &self,
        req: Request<MultipartForm>,
    ) -> impl Future<Output = Result<Response<LazyBody<U>>>> + WasmCompatSend + 'static
    where
        U: From<Bytes>,
        U: WasmCompatSend + 'static,
    {
        self.inner.send_multipart(req)
    }

    fn send_streaming<T>(
        &self,
        req: Request<T>,
    ) -> impl Future<Output = Result<StreamingResponse>> + WasmCompatSend
    where
        T: Into<Bytes> + WasmCompatSend,
    {
        self.inner.send_streaming(req)
    }
}
