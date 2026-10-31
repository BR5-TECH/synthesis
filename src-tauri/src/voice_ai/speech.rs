//! Speech: one text segment becomes audio frames (VOP-FR-MOSY).

use serde_json::json;
use std::time::Duration;
use tokio::sync::watch;

use super::http::{self, HttpFailure, SpeechCall};
use super::pcm::{frame, FRAME_PCM};
use super::realtime::{self, ServerEvent, SessionInfo, Socket};
use super::{
    descriptor, ActiveVoice, Dialect, SpeechRoute, VoiceAi, VoiceSink, ERR_PROVIDER_UNREACHABLE,
    ERR_SEGMENT_TOO_LONG, ERR_SPEECH_FAILED,
};

/// VOP-FR-MOSY: the longest segment, in characters.
pub(crate) const SEGMENT_LIMIT: usize = 4096;

const OPEN_LIMIT: Duration = Duration::from_secs(15);
const GENERATE_LIMIT: Duration = Duration::from_secs(60);
const SAY_EXACTLY: &str = "Say exactly the following. Say nothing else.";

/// VOP-FR-UPRG: the output rate of the `openai` dialect.
const OPENAI_OUTPUT_RATE: u32 = 24_000;

pub(crate) type EmitFrame<'a> = &'a mut (dyn FnMut(Vec<u8>) + Send);

/// VOP-FR-UPRG: one `response.create` out of band.
async fn request_openai(socket: &mut Socket, text: &str, voice: &str) -> Result<(), String> {
    realtime::send(
        socket,
        json!({
            "type": "session.update",
            "session": {
                "type": "realtime",
                "audio": { "output": {
                    "format": { "type": "audio/pcm", "rate": OPENAI_OUTPUT_RATE },
                    "voice": voice,
                }},
            },
        }),
    )
    .await?;
    realtime::send(
        socket,
        json!({
            "type": "response.create",
            "response": {
                "conversation": "none",
                "input": [],
                "output_modalities": ["audio"],
                "instructions": format!("{SAY_EXACTLY}\n\n{text}"),
            },
        }),
    )
    .await
}

/// VOP-FR-TOFP: this server fixes `instructions` after the first message,
/// and the caller holds a new session for each segment.
async fn request_llamacpp(socket: &mut Socket, text: &str) -> Result<(), String> {
    realtime::send(
        socket,
        json!({
            "type": "session.update",
            "session": { "instructions": SAY_EXACTLY, "output_modalities": ["audio"] },
        }),
    )
    .await?;
    realtime::send(
        socket,
        json!({
            "type": "conversation.item.create",
            "item": {
                "type": "message",
                "role": "user",
                "content": [{ "type": "input_text", "text": text }],
            },
        }),
    )
    .await?;
    realtime::send(socket, json!({ "type": "response.create" })).await
}

async fn wait_for_session(socket: &mut Socket) -> Result<SessionInfo, String> {
    loop {
        match realtime::next_event_within(socket, OPEN_LIMIT).await? {
            Some(ServerEvent::SessionCreated(info)) => return Ok(info),
            Some(ServerEvent::Error { .. }) | None => return Err(ERR_SPEECH_FAILED.to_string()),
            Some(_) => {}
        }
    }
}

/// VOP-FR-TZDV, VOP-FR-DWSB: read the audio of one response.
async fn pump_audio(
    socket: &mut Socket,
    rate: u32,
    cancel: &mut watch::Receiver<bool>,
    emit: EmitFrame<'_>,
    sink: Option<&dyn VoiceSink>,
) -> Result<(), String> {
    loop {
        tokio::select! {
            // A cancel goes first, so that a burst of audio cannot hide it.
            biased;
            changed = cancel.changed() => {
                if changed.is_err() || *cancel.borrow() {
                    let _ = realtime::send(socket, json!({ "type": "response.cancel" })).await;
                    return Ok(());
                }
            }
            event = realtime::next_event_within(socket, GENERATE_LIMIT) => {
                match event.map_err(|_| ERR_SPEECH_FAILED.to_string())? {
                    Some(ServerEvent::AudioDelta(pcm)) => emit(frame(FRAME_PCM, rate, &pcm)),
                    Some(ServerEvent::ResponseDone) | None => return Ok(()),
                    Some(ServerEvent::Error { code, .. }) => {
                        if let Some(sink) = sink {
                            sink.diagnostic("realtime_speech", code.as_deref().unwrap_or("error"));
                        }
                        return Err(ERR_SPEECH_FAILED.to_string());
                    }
                    Some(_) => {}
                }
            }
        }
    }
}

/// One request for Realtime speech.
pub(crate) struct RealtimeSpeech<'a> {
    pub(crate) provider: &'a str,
    pub(crate) base_url: &'a str,
    pub(crate) key: Option<&'a str>,
    pub(crate) tts_model: &'a str,
    pub(crate) text: &'a str,
    pub(crate) voice: &'a str,
    pub(crate) sink: Option<&'a dyn VoiceSink>,
}

/// One segment on the Realtime route.
pub(crate) async fn speak_realtime(
    call: RealtimeSpeech<'_>,
    cancel: &mut watch::Receiver<bool>,
    emit: EmitFrame<'_>,
) -> Result<(), String> {
    let RealtimeSpeech { provider, base_url, key, tts_model, text, voice, sink } = call;
    let url = realtime::speech_url(provider, base_url, tts_model);
    let mut socket = realtime::connect(&url, key, OPEN_LIMIT).await.map_err(|kind| {
        if let Some(sink) = sink {
            sink.diagnostic("realtime_speech_open", &kind);
        }
        ERR_PROVIDER_UNREACHABLE.to_string()
    })?;
    let session = wait_for_session(&mut socket).await.map_err(|_| ERR_SPEECH_FAILED.to_string())?;
    // VOP-FR-DWSB: a cancel can come while the socket opens.
    if *cancel.borrow() {
        let _ = socket.close(None).await;
        return Ok(());
    }
    let dialect = if provider == "openai" { Dialect::Openai } else { Dialect::Llamacpp };
    let rate = match dialect {
        Dialect::Openai => session.output_rate.unwrap_or(OPENAI_OUTPUT_RATE),
        // VOP-FR-USQZ: a server with no output rate cannot speak.
        _ => session.output_rate.ok_or(ERR_SPEECH_FAILED)?,
    };
    let request = match dialect {
        Dialect::Openai => request_openai(&mut socket, text, voice).await,
        _ => request_llamacpp(&mut socket, text).await,
    };
    request.map_err(|_| ERR_SPEECH_FAILED.to_string())?;
    let result = pump_audio(&mut socket, rate, cancel, emit, sink).await;
    let _ = socket.close(None).await;
    result
}

/// VOP-FR-RABZ: the voices that the active provider is known to offer, or
/// `None` where only the server can say.
async fn known_voices(voice: &VoiceAi, active: &ActiveVoice) -> Option<Vec<String>> {
    let d = descriptor(&active.record.provider)?;
    if !d.voices.is_empty() {
        return Some(d.voices.iter().map(|v| (*v).to_string()).collect());
    }
    if d.id != "openrouter" {
        return None;
    }
    let model = active.record.tts_model.clone();
    if let Ok(cache) = voice.voice_cache.lock() {
        if let Some((_, voices)) = cache.as_ref().filter(|(m, _)| *m == model) {
            return Some(voices.clone());
        }
    }
    let voices = super::models::openrouter_voices(&active.record, active.key.as_deref()).await?;
    if let Ok(mut cache) = voice.voice_cache.lock() {
        *cache = Some((model, voices.clone()));
    }
    Some(voices)
}

/// VOP-FR-RABZ, VOM-FR-UALP: the voice of a segment.
fn voice_for(requested: Option<&str>, default_voice: &str, known: Option<&[String]>) -> String {
    match requested.filter(|v| !v.is_empty()) {
        Some(voice) => match known {
            Some(known) if !known.iter().any(|k| k == voice) => default_voice.to_string(),
            _ => voice.to_string(),
        },
        None => default_voice.to_string(),
    }
}

/// VOP-FR-MOSY: one segment on the selected route.
pub(crate) async fn speak_segment(
    voice: &VoiceAi,
    active: &ActiveVoice,
    text: &str,
    requested_voice: Option<&str>,
    cancel: &mut watch::Receiver<bool>,
    emit: EmitFrame<'_>,
    sink: &dyn VoiceSink,
) -> Result<(), String> {
    if text.chars().count() > SEGMENT_LIMIT {
        return Err(ERR_SEGMENT_TOO_LONG.to_string());
    }
    if *cancel.borrow() {
        return Ok(());
    }
    let record = &active.record;
    let route = record.speech_route.ok_or(ERR_SPEECH_FAILED)?;
    // A catalogue call has a cost, and with no requested voice it has no use.
    let requested = requested_voice.filter(|v| !v.is_empty() && *v != record.default_voice);
    let known = match requested {
        Some(_) => known_voices(voice, active).await,
        None => None,
    };
    let chosen = voice_for(requested, &record.default_voice, known.as_deref());
    if route == SpeechRoute::Realtime {
        let call = RealtimeSpeech {
            provider: &record.provider,
            base_url: &record.base_url,
            key: active.key.as_deref(),
            tts_model: &record.tts_model,
            text,
            voice: &chosen,
            sink: Some(sink),
        };
        return speak_realtime(call, cancel, emit).await;
    }
    let (client, _) = http::client().map_err(|f| f.speech_code().to_string())?;
    let attempt = |voice_name: Option<String>| {
        let client = client.clone();
        let provider = record.provider.clone();
        let base_url = record.base_url.clone();
        let key = active.key.clone();
        let model = record.tts_model.clone();
        let text = text.to_string();
        async move {
            let call = SpeechCall {
                client: &client,
                provider: &provider,
                base_url: &base_url,
                key: key.as_deref(),
                model: &model,
                text: &text,
                voice: voice_name.as_deref(),
            };
            match (route, provider.as_str()) {
                (SpeechRoute::LlamaTts, _) => http::speak_llama_tts(&call).await,
                (_, "openrouter") => http::speak_openrouter(&call).await,
                _ => http::speak_openai_compatible(&call).await,
            }
        }
    };
    let call = async {
        match attempt(Some(chosen.clone())).await {
            // VOP-FR-RABZ: on `custom`, a 400 for a voice gets one more
            // request with the default voice.
            Err(HttpFailure::BadRequest)
                if record.provider == "custom"
                    && !chosen.is_empty()
                    && chosen != record.default_voice =>
            {
                attempt(Some(record.default_voice.clone())).await
            }
            other => other,
        }
    };
    tokio::select! {
        result = call => {
            let bytes = result.map_err(|f| {
                sink.diagnostic("http_speech", &f.detail());
                f.speech_code().to_string()
            })?;
            if !*cancel.borrow() {
                emit(bytes);
            }
            Ok(())
        }
        // VOP-FR-DWSB: dropping the future aborts the request.
        _ = cancel.changed() => Ok(()),
    }
}

/// `cancel voice speech` of one id, and the registration of a new one.
impl VoiceAi {
    pub(crate) fn register_speech(&self, speech_id: &str) -> watch::Receiver<bool> {
        let cancelled =
            self.cancelled_speech.lock().is_ok_and(|slot| slot.as_deref() == Some(speech_id));
        let (sender, receiver) = watch::channel(cancelled);
        if let Ok(mut speeches) = self.speeches.lock() {
            speeches.insert(speech_id.to_string(), sender);
        }
        receiver
    }

    pub(crate) fn release_speech(&self, speech_id: &str) {
        if let Ok(mut speeches) = self.speeches.lock() {
            speeches.remove(speech_id);
        }
    }

    pub(crate) fn cancel_speech(&self, speech_id: &str) {
        if let Ok(mut slot) = self.cancelled_speech.lock() {
            *slot = Some(speech_id.to_string());
        }
        if let Ok(speeches) = self.speeches.lock() {
            if let Some(sender) = speeches.get(speech_id) {
                let _ = sender.send(true);
            }
        }
    }
}
