//! Model and voice lists (VOP-FR-MUFH, VOP-FR-XJRF).

use serde_json::Value;

use super::http::{self, get_json};
use super::*;

fn descriptor_models(d: &VoiceDescriptor) -> VoiceModels {
    let mut stt_models = Vec::new();
    for model in [d.stt_model, d.http_stt_model] {
        if !model.is_empty() && !stt_models.iter().any(|m| m == model) {
            stt_models.push(model.to_string());
        }
    }
    let tts_models =
        if d.tts_model.is_empty() { Vec::new() } else { vec![d.tts_model.to_string()] };
    VoiceModels { stt_models, tts_models }
}

fn ids_of(body: &Value) -> Vec<String> {
    body.get("data")
        .and_then(Value::as_array)
        .map(|models| {
            models
                .iter()
                .filter_map(|m| m.get("id").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// VOP-FR-MUFH: a failed list gives the descriptor list and no error.
pub async fn list_models(
    provider: &str,
    base_url: &str,
    key: Option<&str>,
) -> Result<VoiceModels, String> {
    let d = descriptor(provider).ok_or(ERR_UNKNOWN_PROVIDER)?;
    let fallback = descriptor_models(d);
    let Ok((client, _)) = http::client() else {
        return Ok(fallback);
    };
    let root = api_root(provider, base_url);
    match provider {
        "openrouter" => {
            let list = |modality: &'static str| {
                get_json(&client, format!("{root}/models?output_modalities={modality}"), None)
            };
            let (stt, tts) = tokio::join!(list("transcription"), list("speech"));
            Ok(VoiceModels {
                stt_models: stt.map(|b| ids_of(&b)).unwrap_or(fallback.stt_models),
                tts_models: tts.map(|b| ids_of(&b)).unwrap_or(fallback.tts_models),
            })
        }
        "custom" => {
            let models = get_json(&client, format!("{root}/models"), key)
                .await
                .map(|b| ids_of(&b))
                .unwrap_or_default();
            Ok(VoiceModels { stt_models: models.clone(), tts_models: models })
        }
        _ => Ok(fallback),
    }
}

/// VOP-FR-XJRF: `supported_voices` of the TTS model.
pub(crate) async fn openrouter_voices(
    record: &VoiceAiRecord,
    key: Option<&str>,
) -> Option<Vec<String>> {
    let (client, _) = http::client().ok()?;
    let root = api_root(&record.provider, &record.base_url);
    let body =
        get_json(&client, format!("{root}/models?output_modalities=speech"), key).await.ok()?;
    let model = body
        .get("data")?
        .as_array()?
        .iter()
        .find(|m| m.get("id").and_then(Value::as_str) == Some(record.tts_model.as_str()))?;
    let voices: Vec<String> = model
        .get("supported_voices")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    if voices.is_empty() {
        None
    } else {
        Some(voices)
    }
}

/// VOP-FR-XJRF: `None` means that voice names are free text.
pub async fn list_voices(
    store: &GlobalSettingsStore,
    voice: &VoiceAi,
    provider: Option<&str>,
) -> Result<Option<Vec<String>>, String> {
    let active = match provider {
        Some(provider) => {
            let d = descriptor(provider).ok_or(ERR_UNKNOWN_PROVIDER)?;
            let (records, _) = store.load_voice_ai_registry()?;
            let record = records
                .into_iter()
                .find(|r| r.provider == provider)
                .unwrap_or_else(|| VoiceAiRecord::seeded(d));
            ActiveVoice { record, key: None }
        }
        None => match resolve_active_voice(store, voice) {
            Ok(active) => active,
            Err(e) if e == ERR_NO_ACTIVE_PROVIDER => return Ok(None),
            Err(e) => return Err(e),
        },
    };
    let d = descriptor(&active.record.provider).ok_or(ERR_UNKNOWN_PROVIDER)?;
    if !d.voices.is_empty() {
        return Ok(Some(d.voices.iter().map(|v| (*v).to_string()).collect()));
    }
    if d.id == "openrouter" {
        return Ok(openrouter_voices(&active.record, active.key.as_deref()).await);
    }
    Ok(None)
}

/// VOP-FR-TOCX: the URL and the key of a model list. A stored key goes only to
/// the saved base URL, and a URL that is not valid gets no request.
pub(crate) fn listing_target(
    store: &GlobalSettingsStore,
    voice: &VoiceAi,
    provider: &str,
    base_url: &str,
    supplied: Option<&str>,
) -> Option<(String, Option<String>)> {
    let normalized = crate::ai_shared::normalize_base_url(base_url).ok()?;
    if provider == "custom" && crate::ai_api::custom_gateway::has_v1_suffix(&normalized) {
        return None;
    }
    if let Some(key) = supplied.map(str::trim).filter(|k| !k.is_empty()) {
        return Some((normalized, Some(key.to_string())));
    }
    let saved = store
        .load_voice_ai_registry()
        .ok()?
        .0
        .iter()
        .any(|r| r.provider == provider && r.base_url == normalized);
    let key = saved
        .then(|| voice.secrets.get(provider).ok().flatten())
        .flatten()
        .filter(|k| !k.is_empty());
    Some((normalized, key))
}

/// The descriptor list, for a URL that gets no request.
pub(crate) fn fallback_models(provider: &str) -> Result<VoiceModels, String> {
    Ok(descriptor_models(descriptor(provider).ok_or(ERR_UNKNOWN_PROVIDER)?))
}
