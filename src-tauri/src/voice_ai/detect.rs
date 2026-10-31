//! Detection: which routes a server offers (VOP-FR-GYMG).
//!
//! Each probe sends a real, short request. A route that does not answer is a
//! fact about the server, not a failure. Only `verify.rs` decides what fails.

use std::time::Duration;

use tokio::sync::watch;

use super::http::{self, HttpFailure, SpeechCall};
use super::realtime::{self, ServerEvent};
use super::speech::{speak_realtime, RealtimeSpeech};
use super::{descriptor, RealtimeTranscription, VoiceCapabilities};

/// VOP-FR-YRQL.
const NAMED_LIMIT: Duration = Duration::from_secs(15);
const CUSTOM_OPEN_LIMIT: Duration = Duration::from_secs(60);

/// VOP-FR-OMTM: the length of the silent WAV.
const SILENCE_SAMPLES: usize = 8_000;

pub(crate) struct Probe<'a> {
    pub(crate) provider: &'a str,
    pub(crate) base_url: &'a str,
    pub(crate) key: Option<&'a str>,
    pub(crate) stt_model: &'a str,
    pub(crate) tts_model: &'a str,
}

pub(crate) struct Detection {
    pub(crate) capabilities: VoiceCapabilities,
    pub(crate) note: Option<String>,
    /// Why an HTTP probe found nothing, for the error of `verify.rs`.
    pub(crate) http_failure: Option<HttpFailure>,
    /// VOP-FR-DBDI: a refused certificate, as its wire text.
    pub(crate) tls_failure: Option<String>,
}

fn open_limit(provider: &str) -> Duration {
    if provider == "custom" {
        CUSTOM_OPEN_LIMIT
    } else {
        NAMED_LIMIT
    }
}

/// VOP-FR-IQAY, VOP-FR-AVXY, VOP-FR-FHDE, VOP-FR-WUEI.
async fn probe_realtime_transcription(
    probe: &Probe<'_>,
    capabilities: &mut VoiceCapabilities,
) -> Option<String> {
    let url = realtime::transcription_url(probe.provider, probe.base_url, probe.stt_model);
    let limit = open_limit(probe.provider);
    let Ok(mut socket) = realtime::connect(&url, probe.key, limit).await else {
        return None;
    };
    let session = loop {
        match realtime::next_event_within(&mut socket, limit).await {
            Ok(Some(ServerEvent::SessionCreated(info))) => break info,
            Ok(Some(ServerEvent::Error { message, .. })) => return Some(message),
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => return None,
        }
    };
    let dialect = realtime::dialect_of(probe.provider, &session);
    let rate = realtime::input_rate_of(dialect, &session);
    let configured =
        realtime::configure_transcription(&mut socket, dialect, &session, probe.stt_model, &[])
            .await;
    if configured.is_err() {
        return None;
    }
    // A fifth of a second of silence.
    let silence = vec![0i16; (rate / 5) as usize];
    if realtime::send_audio(&mut socket, &silence).await.is_err()
        || realtime::end_input(&mut socket, dialect).await.is_err()
    {
        return None;
    }
    let confirmed = loop {
        match realtime::next_event_within(&mut socket, NAMED_LIMIT).await {
            Ok(Some(
                ServerEvent::Committed { .. }
                | ServerEvent::Completed { .. }
                | ServerEvent::Delta { .. },
            )) => break true,
            Ok(Some(ServerEvent::Error { message, .. })) => return Some(message),
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => break false,
        }
    };
    let _ = socket.close(None).await;
    if confirmed {
        capabilities.dialect = Some(dialect);
        capabilities.realtime_input_rate = Some(rate);
        capabilities.realtime_transcription = Some(if realtime::is_turn_based(&session) {
            RealtimeTranscription::Turn
        } else {
            RealtimeTranscription::Live
        });
    }
    None
}

/// VOP-FR-USQZ: one word, and then a cancel.
async fn probe_realtime_speech(probe: &Probe<'_>) -> bool {
    let (stop, mut cancel) = watch::channel(false);
    let mut heard = false;
    let mut emit = |_frame: Vec<u8>| {
        heard = true;
        let _ = stop.send(true);
    };
    let voice = descriptor(probe.provider).map_or("", |d| d.default_voice);
    let call = RealtimeSpeech {
        provider: probe.provider,
        base_url: probe.base_url,
        key: probe.key,
        tts_model: probe.tts_model,
        text: "OK.",
        voice,
        sink: None,
    };
    let spoken = speak_realtime(call, &mut cancel, &mut emit).await;
    spoken.is_ok() && heard
}

/// The model of `/audio/transcriptions`. `gpt-live-transcribe` is not served
/// there, so the descriptor gives another one.
pub(crate) fn http_stt_model(provider: &str, stt_model: &str) -> String {
    match descriptor(provider) {
        Some(d) if stt_model == d.stt_model && !d.http_stt_model.is_empty() => {
            d.http_stt_model.to_string()
        }
        _ => stt_model.to_string(),
    }
}

/// VOP-FR-OMTM.
async fn probe_http(probe: &Probe<'_>, detection: &mut Detection) {
    let (client, tls_record) = match http::client() {
        Ok(pair) => pair,
        Err(failure) => {
            detection.http_failure = Some(failure);
            return;
        }
    };
    let silence = vec![0i16; SILENCE_SAMPLES];
    let model = http_stt_model(probe.provider, probe.stt_model);
    let transcribed =
        http::transcribe(&client, probe.provider, probe.base_url, probe.key, &model, &silence, &[])
            .await;
    match transcribed {
        Ok(_) => detection.capabilities.http_transcription = true,
        Err(failure) => {
            detection.http_failure = Some(failure);
            detection.tls_failure = tls_record.take().map(|f| f.wire());
        }
    }
    let http_speech = descriptor(probe.provider).is_some_and(|d| d.http_speech);
    if probe.tts_model.is_empty() || !http_speech {
        return;
    }
    let call = SpeechCall {
        client: &client,
        provider: probe.provider,
        base_url: probe.base_url,
        key: probe.key,
        model: probe.tts_model,
        text: "OK.",
        voice: None,
    };
    let compatible = if probe.provider == "openrouter" {
        http::speak_openrouter(&call).await
    } else {
        http::speak_openai_compatible(&call).await
    };
    detection.capabilities.openai_speech = compatible.is_ok();
    if probe.provider == "custom" {
        detection.capabilities.llama_tts = http::speak_llama_tts(&call).await.is_ok();
    }
}

/// VOP-FR-GYMG: all routes of one server.
pub(crate) async fn detect(probe: &Probe<'_>) -> Detection {
    let mut detection = Detection {
        capabilities: VoiceCapabilities::default(),
        note: None,
        http_failure: None,
        tls_failure: None,
    };
    detection.note = probe_realtime_transcription(probe, &mut detection.capabilities).await;
    if !probe.tts_model.is_empty() {
        detection.capabilities.realtime_speech = probe_realtime_speech(probe).await;
    }
    probe_http(probe, &mut detection).await;
    detection
}
