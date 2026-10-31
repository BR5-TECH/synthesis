//! The HTTP routes: file transcription and speech (VOP-FR-EYHP, VOP-FR-QTZJ,
//! VOP-FR-TSVV).

use std::time::Duration;

use serde_json::{json, Value};

use super::pcm::{fix_riff_sizes, frame, wav_of, FRAME_FILE, FRAME_PCM};
use super::{
    api_root, ERR_PROVIDER_REJECTED, ERR_PROVIDER_UNREACHABLE, ERR_SPEECH_FAILED,
    ERR_TRANSCRIPTION_FAILED,
};

/// VOP-FR-EYHP: the rate of the HTTP route. Ten minutes stay under the 25 MB
/// limit of the providers.
pub(crate) const HTTP_RATE: u32 = 16_000;

const TRANSCRIBE_LIMIT: Duration = Duration::from_secs(120);
const SPEECH_LIMIT: Duration = Duration::from_secs(60);

/// How one HTTP call ended, before a caller names it. The status stays for the
/// log record, because it says more than the code that the author sees.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HttpFailure {
    Unreachable,
    Rejected(u16),
    Absent(u16),
    /// A 400. Only this status means that another request shape can work
    /// (VOP-FR-QTZJ, VOP-FR-RABZ).
    BadRequest,
    Refused(u16),
    /// A 200 with a body that is not what the route gives.
    Malformed,
}

impl HttpFailure {
    fn of_status(status: u16) -> Self {
        match status {
            400 => Self::BadRequest,
            401 | 403 => Self::Rejected(status),
            404 | 405 | 501 => Self::Absent(status),
            _ => Self::Refused(status),
        }
    }

    pub(crate) fn transcription_code(self) -> &'static str {
        match self {
            Self::Unreachable => ERR_PROVIDER_UNREACHABLE,
            Self::Rejected(_) => ERR_PROVIDER_REJECTED,
            _ => ERR_TRANSCRIPTION_FAILED,
        }
    }

    pub(crate) fn speech_code(self) -> &'static str {
        match self {
            Self::Unreachable => ERR_PROVIDER_UNREACHABLE,
            Self::Rejected(_) => ERR_PROVIDER_REJECTED,
            _ => ERR_SPEECH_FAILED,
        }
    }

    /// The kind and the status, for a log record. It holds no URL and no body.
    pub(crate) fn detail(self) -> String {
        match self {
            Self::Unreachable => "unreachable".to_string(),
            Self::Rejected(status) => format!("rejected status={status}"),
            Self::Absent(status) => format!("absent status={status}"),
            Self::BadRequest => "bad_request status=400".to_string(),
            Self::Refused(status) => format!("refused status={status}"),
            Self::Malformed => "malformed_body".to_string(),
        }
    }
}

/// AAP-FR-WMCX: the shared client.
pub(crate) fn client() -> Result<(reqwest013::Client, crate::tls::TlsRecord), HttpFailure> {
    crate::tls::rig_http_client().map_err(|_| HttpFailure::Unreachable)
}

fn authorized(
    request: reqwest013::RequestBuilder,
    key: Option<&str>,
) -> reqwest013::RequestBuilder {
    match key {
        Some(key) => request.bearer_auth(key),
        None => request,
    }
}

/// The hints of one provider's HTTP route (VOP-FR-INCC).
pub(crate) fn hint_fields(provider: &str, keyterms: &[String]) -> Vec<(&'static str, String)> {
    match provider {
        "openai" if !keyterms.is_empty() => {
            vec![("prompt", keyterms.join(", "))]
        }
        "openrouter" => keyterms.iter().map(|k| ("keyterms[]", k.clone())).collect(),
        _ => Vec::new(),
    }
}

/// VOP-FR-EYHP: one recording as multipart `file`.
pub(crate) async fn transcribe(
    client: &reqwest013::Client,
    provider: &str,
    base_url: &str,
    key: Option<&str>,
    model: &str,
    samples: &[i16],
    hints: &[(&'static str, String)],
) -> Result<String, HttpFailure> {
    let file = reqwest013::multipart::Part::bytes(wav_of(samples, HTTP_RATE))
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .map_err(|_| HttpFailure::Malformed)?;
    let mut form =
        reqwest013::multipart::Form::new().part("file", file).text("response_format", "json");
    if !model.is_empty() {
        form = form.text("model", model.to_string());
    }
    for (name, value) in hints {
        form = form.text(*name, value.clone());
    }
    let url = format!("{}/audio/transcriptions", api_root(provider, base_url));
    let response = authorized(client.post(url), key)
        .multipart(form)
        .timeout(TRANSCRIBE_LIMIT)
        .send()
        .await
        .map_err(|_| HttpFailure::Unreachable)?;
    let status = response.status();
    if !status.is_success() {
        return Err(HttpFailure::of_status(status.as_u16()));
    }
    let body: Value = response.json().await.map_err(|_| HttpFailure::Malformed)?;
    match body.get("text").and_then(Value::as_str) {
        Some(text) => Ok(text.trim().to_string()),
        None => Err(HttpFailure::Malformed),
    }
}

/// One request for speech.
pub(crate) struct SpeechCall<'a> {
    pub(crate) client: &'a reqwest013::Client,
    pub(crate) provider: &'a str,
    pub(crate) base_url: &'a str,
    pub(crate) key: Option<&'a str>,
    pub(crate) model: &'a str,
    pub(crate) text: &'a str,
    pub(crate) voice: Option<&'a str>,
}

async fn post_speech(
    call: &SpeechCall<'_>,
    url: String,
    body: Value,
) -> Result<(String, Vec<u8>), HttpFailure> {
    let response = authorized(call.client.post(url), call.key)
        .json(&body)
        .timeout(SPEECH_LIMIT)
        .send()
        .await
        .map_err(|_| HttpFailure::Unreachable)?;
    let status = response.status();
    if !status.is_success() {
        return Err(HttpFailure::of_status(status.as_u16()));
    }
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let bytes = response.bytes().await.map_err(|_| HttpFailure::Unreachable)?;
    Ok((content_type, bytes.to_vec()))
}

fn speech_body(call: &SpeechCall<'_>, format: &str) -> Value {
    let mut body = json!({ "input": call.text, "response_format": format });
    if !call.model.is_empty() {
        body["model"] = json!(call.model);
    }
    if let Some(voice) = call.voice.filter(|v| !v.is_empty()) {
        body["voice"] = json!(voice);
    }
    body
}

/// The rate of a `Content-Type` such as `audio/pcm;rate=24000`.
pub(crate) fn rate_of_content_type(content_type: &str) -> Option<u32> {
    content_type.split(';').find_map(|part| part.trim().strip_prefix("rate=")?.parse().ok())
}

/// VOP-FR-QTZJ: `mp3`, and on a 400 one more request with `pcm`.
pub(crate) async fn speak_openrouter(call: &SpeechCall<'_>) -> Result<Vec<u8>, HttpFailure> {
    let url = format!("{}/audio/speech", api_root(call.provider, call.base_url));
    match post_speech(call, url.clone(), speech_body(call, "mp3")).await {
        Ok((_, bytes)) => Ok(frame(FRAME_FILE, 0, &bytes)),
        Err(HttpFailure::BadRequest) => {
            let (content_type, bytes) = post_speech(call, url, speech_body(call, "pcm")).await?;
            let rate = rate_of_content_type(&content_type).unwrap_or(24_000);
            Ok(frame(FRAME_PCM, rate, &bytes))
        }
        Err(e) => Err(e),
    }
}

/// VOP-FR-TSVV: `/v1/audio/speech` with `wav`.
pub(crate) async fn speak_openai_compatible(call: &SpeechCall<'_>) -> Result<Vec<u8>, HttpFailure> {
    let url = format!("{}/audio/speech", api_root(call.provider, call.base_url));
    let (_, bytes) = post_speech(call, url, speech_body(call, "wav")).await?;
    Ok(frame(FRAME_FILE, 0, &fix_riff_sizes(bytes)))
}

/// VOP-FR-TSVV: the `/tts` route of llama.cpp.
pub(crate) async fn speak_llama_tts(call: &SpeechCall<'_>) -> Result<Vec<u8>, HttpFailure> {
    let url = format!("{}/tts", call.base_url.trim_end_matches('/'));
    let (_, bytes) = post_speech(call, url, speech_body(call, "wav")).await?;
    Ok(frame(FRAME_FILE, 0, &fix_riff_sizes(bytes)))
}

/// VOP-FR-OMTM: `GET /key` of OpenRouter.
pub(crate) async fn check_openrouter_key(
    client: &reqwest013::Client,
    base_url: &str,
    key: Option<&str>,
) -> Result<(), HttpFailure> {
    let url = format!("{}/key", base_url.trim_end_matches('/'));
    let response = authorized(client.get(url), key)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|_| HttpFailure::Unreachable)?;
    match response.status().as_u16() {
        status @ (401 | 403) => Err(HttpFailure::Rejected(status)),
        _ => Ok(()),
    }
}

/// A `GET` that gives JSON, for the model lists.
pub(crate) async fn get_json(
    client: &reqwest013::Client,
    url: String,
    key: Option<&str>,
) -> Result<Value, HttpFailure> {
    let response = authorized(client.get(url), key)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|_| HttpFailure::Unreachable)?;
    let status = response.status();
    if !status.is_success() {
        return Err(HttpFailure::of_status(status.as_u16()));
    }
    response.json().await.map_err(|_| HttpFailure::Malformed)
}
