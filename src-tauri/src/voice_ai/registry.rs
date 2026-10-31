//! The provider records (VOP-FR-SURK, VOP-FR-KRMY, VOP-FR-NWHQ).

use super::*;

/// VOP-FR-SURK: a record for each provider, stored or seeded, in the order of
/// `PROVIDERS`.
pub(crate) fn integrations_of(
    records: &[VoiceAiRecord],
    active: Option<&str>,
) -> VoiceAiIntegrations {
    VoiceAiIntegrations {
        active: active.map(str::to_string),
        records: PROVIDERS
            .iter()
            .map(|d| {
                records
                    .iter()
                    .find(|r| r.provider == d.id)
                    .cloned()
                    .unwrap_or_else(|| VoiceAiRecord::seeded(d))
            })
            .collect(),
    }
}

pub fn list_integrations(store: &GlobalSettingsStore) -> Result<VoiceAiIntegrations, String> {
    let (records, active) = store.load_voice_ai_registry()?;
    Ok(integrations_of(&records, active.as_deref()))
}

pub(crate) fn transcription_route_detected(
    capabilities: &VoiceCapabilities,
    route: TranscriptionRoute,
) -> bool {
    match route {
        TranscriptionRoute::Realtime => capabilities.realtime_transcription.is_some(),
        TranscriptionRoute::Http => capabilities.http_transcription,
    }
}

pub(crate) fn speech_route_detected(capabilities: &VoiceCapabilities, route: SpeechRoute) -> bool {
    match route {
        SpeechRoute::Realtime => capabilities.realtime_speech,
        SpeechRoute::OpenaiSpeech => capabilities.openai_speech,
        SpeechRoute::LlamaTts => capabilities.llama_tts,
    }
}

/// Change one stored record, under the write lock.
pub(crate) fn update_records(
    store: &GlobalSettingsStore,
    voice: &VoiceAi,
    provider: &str,
    change: impl FnOnce(&mut VoiceAiRecord) -> Result<(), String>,
) -> Result<VoiceAiRecord, String> {
    let d = descriptor(provider).ok_or(ERR_UNKNOWN_PROVIDER)?;
    let _guard = voice.write_lock.lock().map_err(|e| format!("voice registry poisoned: {e}"))?;
    let (mut records, active) = store.load_voice_ai_registry()?;
    let at = match records.iter().position(|r| r.provider == provider) {
        Some(at) => at,
        None => {
            records.push(VoiceAiRecord::seeded(d));
            records.len() - 1
        }
    };
    change(&mut records[at])?;
    let record = records[at].clone();
    store.save_voice_ai_registry(records, active)?;
    Ok(record)
}

pub fn set_routes(
    store: &GlobalSettingsStore,
    voice: &VoiceAi,
    provider: &str,
    transcription: TranscriptionRoute,
    speech: Option<SpeechRoute>,
) -> Result<VoiceAiRecord, String> {
    update_records(store, voice, provider, |record| {
        let capabilities = record.capabilities.as_ref().ok_or(ERR_ROUTE_NOT_DETECTED)?;
        let detected = transcription_route_detected(capabilities, transcription)
            && speech.is_none_or(|s| speech_route_detected(capabilities, s));
        if !detected {
            return Err(ERR_ROUTE_NOT_DETECTED.to_string());
        }
        record.transcription_route = transcription;
        record.speech_route = speech;
        record.routes_chosen = true;
        Ok(())
    })
}

pub fn set_default_voice(
    store: &GlobalSettingsStore,
    voice: &VoiceAi,
    provider: &str,
    default_voice: &str,
) -> Result<VoiceAiRecord, String> {
    update_records(store, voice, provider, |record| {
        record.default_voice = default_voice.trim().to_string();
        Ok(())
    })
}

/// VOP-FR-KRMY: only a verified provider can become active.
pub fn set_active(
    store: &GlobalSettingsStore,
    voice: &VoiceAi,
    provider: &str,
) -> Result<VoiceAiIntegrations, String> {
    descriptor(provider).ok_or(ERR_UNKNOWN_PROVIDER)?;
    let _guard = voice.write_lock.lock().map_err(|e| format!("voice registry poisoned: {e}"))?;
    let (records, _) = store.load_voice_ai_registry()?;
    if !records.iter().any(|r| r.provider == provider && r.verified_at.is_some()) {
        return Err(ERR_NOT_VERIFIED.into());
    }
    store.save_voice_ai_registry(records.clone(), Some(provider.to_string()))?;
    Ok(integrations_of(&records, Some(provider)))
}

/// Remove a provider's record and key. A cleared provider is no longer active.
pub fn clear_integration(
    store: &GlobalSettingsStore,
    voice: &VoiceAi,
    provider: &str,
) -> Result<VoiceAiIntegrations, String> {
    descriptor(provider).ok_or(ERR_UNKNOWN_PROVIDER)?;
    let _guard = voice.write_lock.lock().map_err(|e| format!("voice registry poisoned: {e}"))?;
    voice.secrets.delete(provider).map_err(|_| ERR_KEYCHAIN_UNAVAILABLE.to_string())?;
    let (mut records, active) = store.load_voice_ai_registry()?;
    records.retain(|r| r.provider != provider);
    let active = active.filter(|a| a != provider);
    store.save_voice_ai_registry(records.clone(), active.clone())?;
    Ok(integrations_of(&records, active.as_deref()))
}
