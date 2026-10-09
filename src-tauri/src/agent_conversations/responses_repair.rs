//! CVL-FR-TQRD: the one tolerance this application adds to a reply of the
//! Custom gateway, before the framework decodes it.
//!
//! `rig` decodes a Responses reply into a struct whose `text` object requires
//! `format`. A gateway can return `"text": {}`, and then the framework refuses
//! the whole reply and the answer of the model is lost. The HTTP client that
//! `rig` is given is wrapped here, so the body is repaired before `rig` reads
//! it. Every other body passes byte-for-byte.

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use bytes::Bytes;
use rig::http_client::{
    HttpClientExt, LazyBody, MultipartForm, Request, Response, Result, StreamingResponse,
};
use rig::wasm_compat::WasmCompatSend;

/// The format a `text` object without one is read as.
fn plain_text_format() -> serde_json::Value {
    serde_json::json!({ "type": "text" })
}

/// CVL-FR-TQRD: the body with `text.format` set to plain text where the
/// top-level `text` object has no `format` or a null one, or `None` where the
/// body needs no repair.
///
/// `None` covers every body this requirement does not name: one that already
/// has a format, one without a `text` object, and one that is not valid JSON.
/// The caller then passes the original bytes, so the framework reports exactly
/// what it would have reported without this function.
pub(super) fn repair_text_format(body: &[u8]) -> Option<Bytes> {
    let mut value: serde_json::Value = serde_json::from_slice(body).ok()?;
    let text = value.as_object_mut()?.get_mut("text")?.as_object_mut()?;
    if !text.get("format").is_none_or(serde_json::Value::is_null) {
        return None;
    }
    text.insert("format".to_string(), plain_text_format());
    serde_json::to_vec(&value).ok().map(Bytes::from)
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
    repaired: Arc<AtomicBool>,
}

impl<C> ResponsesRepair<C> {
    pub(super) fn new(inner: C, enabled: bool) -> Self {
        Self {
            inner,
            enabled,
            repaired: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Whether a reply this client received was repaired. Read after the call,
    /// so the loop can record it (the client has no access to the log).
    pub(super) fn repaired(&self) -> bool {
        self.repaired.load(Ordering::SeqCst)
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
        let repaired = self.repaired.clone();
        let sent = self.inner.send::<T, Bytes>(req);
        async move {
            let response = sent.await?;
            let (parts, body) = response.into_parts();
            let body: LazyBody<U> = Box::pin(async move {
                let bytes = body.await?;
                if !repair {
                    return Ok(U::from(bytes));
                }
                match repair_text_format(&bytes) {
                    Some(fixed) => {
                        repaired.store(true, Ordering::SeqCst);
                        Ok(U::from(fixed))
                    }
                    None => Ok(U::from(bytes)),
                }
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
