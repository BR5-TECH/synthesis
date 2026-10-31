//! Verification (VOP-FR-GVTD, VOP-FR-PGPO).

use super::detect::{self, Probe};
use super::http::{self, HttpFailure};
use super::*;
use crate::ai_shared::{normalize_base_url, BaseUrlError};
use crate::global_settings::now_iso8601;

/// The values that one verification checks.
pub struct VerifyRequest<'a> {
    pub provider: &'a str,
    pub base_url: &'a str,
    pub api_key: Option<&'a str>,
    pub stt_model: &'a str,
    pub tts_model: &'a str,
}

fn failure_code(tls: Option<String>, failure: Option<HttpFailure>, otherwise: &str) -> String {
    if let Some(tls) = tls {
        return tls;
    }
    match failure {
        Some(HttpFailure::Unreachable) => ERR_UNREACHABLE.to_string(),
        Some(HttpFailure::Rejected(_)) => ERR_REJECTED.to_string(),
        _ => otherwise.to_string(),
    }
}

/// VOP-FR-IKUE: the routes of a verified record. An earlier choice of the
/// author stands while the capabilities still hold it.
fn select_routes(
    capabilities: &VoiceCapabilities,
    previous: &VoiceAiRecord,
    tts_model: &str,
) -> (TranscriptionRoute, Option<SpeechRoute>) {
    let transcription = if capabilities.realtime_transcription.is_some() {
        TranscriptionRoute::Realtime
    } else {
        TranscriptionRoute::Http
    };
    let speech = if tts_model.is_empty() {
        None
    } else if capabilities.realtime_speech {
        Some(SpeechRoute::Realtime)
    } else if capabilities.openai_speech {
        Some(SpeechRoute::OpenaiSpeech)
    } else if capabilities.llama_tts {
        Some(SpeechRoute::LlamaTts)
    } else {
        None
    };
    if !previous.routes_chosen {
        return (transcription, speech);
    }
    let kept_transcription = if super::registry::transcription_route_detected(
        capabilities,
        previous.transcription_route,
    ) {
        previous.transcription_route
    } else {
        transcription
    };
    let kept_speech = match previous.speech_route {
        Some(route) if super::registry::speech_route_detected(capabilities, route) => Some(route),
        _ => speech,
    };
    (kept_transcription, if tts_model.is_empty() { None } else { kept_speech })
}

/// VOP-FR-GVTD: reach the provider, learn its routes, and only then write.
pub async fn verify_integration(
    store: &GlobalSettingsStore,
    voice: &VoiceAi,
    request: VerifyRequest<'_>,
) -> Result<VoiceAiRecord, String> {
    let provider = request.provider;
    let d = descriptor(provider).ok_or(ERR_UNKNOWN_PROVIDER)?;
    let normalized = normalize_base_url(request.base_url).map_err(|e| match e {
        BaseUrlError::Empty => ERR_BASE_URL_EMPTY.to_string(),
        BaseUrlError::Invalid => ERR_BASE_URL_INVALID.to_string(),
    })?;
    // VOP-FR-TLVI: a `custom` route holds `/v1` by itself.
    if provider == "custom" && crate::ai_api::custom_gateway::has_v1_suffix(&normalized) {
        return Err(ERR_BASE_URL_INVALID.into());
    }
    // VOP-FR-HTZP: an empty field means the stored key.
    let supplied = request.api_key.map(str::trim).filter(|k| !k.is_empty());
    let key = match supplied {
        Some(k) => Some(k.to_string()),
        None => voice
            .secrets
            .get(provider)
            .map_err(|_| ERR_KEYCHAIN_UNAVAILABLE.to_string())?
            .filter(|k| !k.is_empty()),
    };
    if d.key_required && key.is_none() {
        return Err(ERR_KEY_MISSING.into());
    }
    let key = key.as_deref();
    let stt_model = request.stt_model.trim();
    let tts_model = request.tts_model.trim();

    if provider == "openrouter" {
        let (client, _) = http::client().map_err(|_| ERR_UNREACHABLE.to_string())?;
        http::check_openrouter_key(&client, &normalized, key)
            .await
            .map_err(|f| failure_code(None, Some(f), ERR_UNREACHABLE))?;
    }
    let detection =
        detect::detect(&Probe { provider, base_url: &normalized, key, stt_model, tts_model }).await;
    let capabilities = detection.capabilities;
    if capabilities.realtime_transcription.is_none() && !capabilities.http_transcription {
        return Err(failure_code(
            detection.tls_failure,
            detection.http_failure,
            ERR_NO_TRANSCRIPTION_ROUTE,
        ));
    }
    let speaks =
        capabilities.realtime_speech || capabilities.openai_speech || capabilities.llama_tts;
    if !tts_model.is_empty() && !speaks {
        return Err(failure_code(
            detection.tls_failure,
            detection.http_failure,
            ERR_NO_SPEECH_ROUTE,
        ));
    }

    let _guard = voice.write_lock.lock().map_err(|e| format!("voice registry poisoned: {e}"))?;
    // VOP-FR-PGPO: the key first, so that a keychain that refuses leaves the
    // registry as it was.
    if let Some(supplied) = supplied {
        voice.secrets.set(provider, supplied).map_err(|_| ERR_KEYCHAIN_UNAVAILABLE.to_string())?;
    }
    let (mut records, active) = store.load_voice_ai_registry()?;
    let at = match records.iter().position(|r| r.provider == provider) {
        Some(at) => at,
        None => {
            records.push(VoiceAiRecord::seeded(d));
            records.len() - 1
        }
    };
    let (transcription_route, speech_route) = select_routes(&capabilities, &records[at], tts_model);
    let record = &mut records[at];
    record.base_url = normalized;
    record.masked_hint = hint_of(key);
    record.stt_model = stt_model.to_string();
    record.tts_model = tts_model.to_string();
    record.transcription_route = transcription_route;
    record.speech_route = speech_route;
    record.capability_note = detection.note;
    record.capabilities = Some(capabilities);
    record.verified_at = Some(now_iso8601());
    let verified = record.clone();
    store.save_voice_ai_registry(records, active)?;
    Ok(verified)
}
