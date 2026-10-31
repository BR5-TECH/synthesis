//! Voice AI providers (`specifications/core/VOP-voice-ai-providers.md`).
//!
//! The speech endpoints that the application calls, apart from the
//! conversational endpoints of `ai_api`. This module holds one record per
//! provider, finds the routes each provider offers, and does the transcription
//! and speech work of voice mode. Realtime is the first-class transport: a
//! provider that offers a Realtime route uses it by default.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::Emitter;

use crate::ai_shared::{mask_hint, SecretStore};
use crate::global_settings::GlobalSettingsStore;

pub(crate) mod activity;
pub(crate) mod capture;
pub(crate) mod commands;
pub(crate) mod detect;
pub(crate) mod http;
pub(crate) mod models;
pub(crate) mod pcm;
pub(crate) mod realtime;
pub(crate) mod registry;
pub(crate) mod speech;
pub(crate) mod verify;

#[cfg(target_os = "linux")]
pub mod linux_media;

pub use activity::commands::*;
pub use commands::*;

#[cfg(test)]
mod tests;

/// VOP-FR-ZSXI: the namespace of provider keys in the application secret vault.
pub const VAULT_NAMESPACE: &[&str] = &["voice_ai", "providers"];

/// VOP-FR-NWHQ: a record or the active provider changed.
pub const VOICE_AI_CHANGED: &str = "voice-ai-changed";
/// VOP-FR-JAOB: the live transcript of a capture session changed.
pub const VOICE_TRANSCRIPT_CHANGED: &str = "voice-transcript-changed";
/// VOP-FR-DICU: a VAD session reports each step of an utterance.
pub const VOICE_ACTIVITY_CHANGED: &str = "voice-activity-changed";

pub const ERR_BASE_URL_EMPTY: &str = "base_url_empty";
pub const ERR_BASE_URL_INVALID: &str = "base_url_invalid";
pub const ERR_KEY_MISSING: &str = "key_missing";
pub const ERR_UNREACHABLE: &str = "unreachable";
pub const ERR_REJECTED: &str = "rejected";
pub const ERR_NO_TRANSCRIPTION_ROUTE: &str = "no_transcription_route";
pub const ERR_NO_SPEECH_ROUTE: &str = "no_speech_route";
pub const ERR_KEYCHAIN_UNAVAILABLE: &str = "keychain_unavailable";
pub const ERR_UNKNOWN_PROVIDER: &str = "unknown_provider";
pub const ERR_NOT_VERIFIED: &str = "not_verified";
pub const ERR_ROUTE_NOT_DETECTED: &str = "route_not_detected";
pub const ERR_NO_ACTIVE_PROVIDER: &str = "no_active_provider";
pub const ERR_SESSION_ACTIVE: &str = "voice_session_active";
pub const ERR_SESSION_LOST: &str = "voice_session_lost";
pub const ERR_PROVIDER_UNREACHABLE: &str = "provider_unreachable";
pub const ERR_PROVIDER_REJECTED: &str = "provider_rejected";
pub const ERR_TRANSCRIPTION_FAILED: &str = "transcription_failed";
pub const ERR_SPEECH_FAILED: &str = "speech_failed";
pub const ERR_SEGMENT_TOO_LONG: &str = "voice_segment_too_long";

/// VOP-FR-IAEN: the OpenAI TTS model. `/v1/audio/speech` has no model
/// after 2027-01-06, so this name is reached through Realtime only.
pub const OPENAI_TTS_MODEL: &str = "gpt-realtime-2.1-mini";

const OPENAI_VOICES: &[&str] = &[
    "alloy", "ash", "ballad", "coral", "echo", "fable", "nova", "onyx", "sage", "shimmer", "verse",
    "marin", "cedar",
];

/// VOP-FR-CVLB: what ships with the application about one provider.
pub struct VoiceDescriptor {
    pub id: &'static str,
    pub base_url: &'static str,
    pub key_required: bool,
    /// The STT model of the first-class route.
    pub stt_model: &'static str,
    /// The model of `/audio/transcriptions`, where the first-class model is
    /// not served there.
    pub http_stt_model: &'static str,
    pub tts_model: &'static str,
    pub default_voice: &'static str,
    pub voices: &'static [&'static str],
    /// VOP-FR-IAEN: whether the descriptor has an HTTP speech route.
    pub http_speech: bool,
    /// What goes between a base URL and an API path.
    pub api_prefix: &'static str,
}

pub const PROVIDERS: &[VoiceDescriptor] = &[
    VoiceDescriptor {
        id: "openai",
        base_url: "https://api.openai.com/v1",
        key_required: true,
        stt_model: "gpt-live-transcribe",
        http_stt_model: "gpt-transcribe",
        tts_model: OPENAI_TTS_MODEL,
        default_voice: "marin",
        voices: OPENAI_VOICES,
        http_speech: false,
        api_prefix: "",
    },
    VoiceDescriptor {
        id: "openrouter",
        base_url: "https://openrouter.ai/api/v1",
        key_required: true,
        stt_model: "openai/gpt-transcribe",
        http_stt_model: "openai/gpt-transcribe",
        tts_model: "elevenlabs/eleven-v4-turbo",
        default_voice: "george",
        voices: &[],
        http_speech: true,
        api_prefix: "",
    },
    VoiceDescriptor {
        id: "custom",
        base_url: "",
        key_required: false,
        stt_model: "",
        http_stt_model: "",
        tts_model: "",
        default_voice: "",
        voices: &[],
        http_speech: true,
        api_prefix: "/v1",
    },
];

pub fn descriptor(provider: &str) -> Option<&'static VoiceDescriptor> {
    PROVIDERS.iter().find(|d| d.id == provider)
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionRoute {
    Realtime,
    #[default]
    Http,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpeechRoute {
    Realtime,
    OpenaiSpeech,
    LlamaTts,
}

/// VOP-FR-TLGM: the Realtime protocol that one server speaks.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Dialect {
    Openai,
    Llamacpp,
    Vllm,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RealtimeTranscription {
    Live,
    Turn,
}

/// VOP-FR-SURK: what detection found.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct VoiceCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dialect: Option<Dialect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub realtime_transcription: Option<RealtimeTranscription>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub realtime_input_rate: Option<u32>,
    pub http_transcription: bool,
    pub realtime_speech: bool,
    pub openai_speech: bool,
    pub llama_tts: bool,
}

/// VOP-FR-SURK: one provider. It holds no key, so it is safe to log and to
/// store in `synthesis.toml`.
///
/// `capabilities` is the only table, and it stays last: TOML needs scalars
/// before tables, and serde writes fields in declaration order.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct VoiceAiRecord {
    pub provider: String,
    pub base_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub masked_hint: Option<String>,
    pub stt_model: String,
    pub tts_model: String,
    pub transcription_route: TranscriptionRoute,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speech_route: Option<SpeechRoute>,
    pub default_voice: String,
    /// VOP-FR-AVXY: the server's message for a refused model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability_note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified_at: Option<String>,
    /// VOP-FR-IKUE: the author chose the routes, so a later
    /// verification keeps them while the capabilities still hold them.
    pub routes_chosen: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<VoiceCapabilities>,
}

impl VoiceAiRecord {
    /// A provider that nobody configured: the values of its descriptor.
    pub fn seeded(d: &VoiceDescriptor) -> Self {
        Self {
            provider: d.id.to_string(),
            base_url: d.base_url.to_string(),
            stt_model: d.stt_model.to_string(),
            tts_model: d.tts_model.to_string(),
            default_voice: d.default_voice.to_string(),
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VoiceAiIntegrations {
    pub active: Option<String>,
    pub records: Vec<VoiceAiRecord>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VoiceCaptureStart {
    pub session_id: String,
    pub route: TranscriptionRoute,
    pub live: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VoiceModels {
    pub stt_models: Vec<String>,
    pub tts_models: Vec<String>,
}

/// What this module tells the frontend. A trait, so a test reads the events
/// without a webview.
pub trait VoiceSink: Send + Sync {
    fn integrations_changed(&self, integrations: &VoiceAiIntegrations);
    fn transcript_changed(&self, session_id: &str, text: &str);
    fn activity_changed(&self, change: &activity::ActivityChange);
    /// A failure that the typed code does not explain. `detail` is a kind, a
    /// status, or a server code, and never a URL, a key, or text.
    fn diagnostic(&self, stage: &str, detail: &str);
}

impl VoiceSink for tauri::AppHandle {
    fn integrations_changed(&self, integrations: &VoiceAiIntegrations) {
        let _ = self.emit(VOICE_AI_CHANGED, integrations);
    }

    fn transcript_changed(&self, session_id: &str, text: &str) {
        let _ = self.emit(
            VOICE_TRANSCRIPT_CHANGED,
            serde_json::json!({ "sessionId": session_id, "text": text }),
        );
    }

    fn activity_changed(&self, change: &activity::ActivityChange) {
        let _ = self.emit(VOICE_ACTIVITY_CHANGED, change.payload());
    }

    fn diagnostic(&self, stage: &str, detail: &str) {
        crate::logging::log_warn(
            self,
            &crate::logging::BUFFER,
            &[crate::logging::Domain::Ai, crate::logging::Domain::Remote],
            "voice provider call failed",
            crate::log_fields! { "stage" => stage, "detail" => detail },
        );
    }
}

/// The copy of a provider's configuration that one call runs on
/// (VOP-FR-KBPZ). It holds a key, so it does not derive `Debug`.
#[derive(Clone)]
pub struct ActiveVoice {
    pub record: VoiceAiRecord,
    pub key: Option<String>,
}

/// The managed state of this module.
pub struct VoiceAi {
    pub(crate) secrets: Box<dyn SecretStore>,
    pub(crate) write_lock: Mutex<()>,
    pub(crate) capture: Mutex<Option<capture::CaptureSession>>,
    pub(crate) activity: Mutex<Option<activity::ActivityHandle>>,
    pub(crate) speeches: Mutex<HashMap<String, tokio::sync::watch::Sender<bool>>>,
    /// VOP-FR-DWSB: the last cancelled speech. A cancel can arrive between two
    /// segments of one reply, when no segment is registered. One slot is
    /// enough, because one speech runs at a time.
    pub(crate) cancelled_speech: Mutex<Option<String>>,
    /// The voices of one OpenRouter model. This avoids a catalogue call for
    /// each spoken segment.
    pub(crate) voice_cache: Mutex<Option<(String, Vec<String>)>>,
    pub(crate) next_session: AtomicU64,
}

impl Default for VoiceAi {
    fn default() -> Self {
        Self::new(Box::new(crate::secret_vault::VaultSecrets::new(
            crate::secret_vault::global(),
            VAULT_NAMESPACE,
        )))
    }
}

impl VoiceAi {
    pub fn new(secrets: Box<dyn SecretStore>) -> Self {
        Self {
            secrets,
            write_lock: Mutex::new(()),
            capture: Mutex::new(None),
            activity: Mutex::new(None),
            speeches: Mutex::new(HashMap::new()),
            cancelled_speech: Mutex::new(None),
            voice_cache: Mutex::new(None),
            next_session: AtomicU64::new(1),
        }
    }

    pub(crate) fn new_session_id(&self) -> String {
        format!("voice-{}", self.next_session.fetch_add(1, Ordering::Relaxed))
    }
}

/// VOP-FR-EBQI: the one read path for a key of this module. It is not a Tauri
/// command, and `lib_tests.rs` keeps it that way.
pub(crate) fn resolve_active_voice(
    store: &GlobalSettingsStore,
    voice: &VoiceAi,
) -> Result<ActiveVoice, String> {
    let (records, active) = store.load_voice_ai_registry()?;
    let active = active.ok_or(ERR_NO_ACTIVE_PROVIDER)?;
    let record = records
        .into_iter()
        .find(|r| r.provider == active && r.verified_at.is_some())
        .ok_or(ERR_NO_ACTIVE_PROVIDER)?;
    let key = voice
        .secrets
        .get(&record.provider)
        .map_err(|_| ERR_KEYCHAIN_UNAVAILABLE.to_string())?
        .filter(|k| !k.is_empty());
    Ok(ActiveVoice { record, key })
}

/// The root that API paths join on. A `custom` base URL holds no `/v1`
/// (VOP-FR-TLVI), and a named provider's base URL holds it.
pub(crate) fn api_root(provider: &str, base_url: &str) -> String {
    let prefix = descriptor(provider).map_or("", |d| d.api_prefix);
    format!("{}{prefix}", base_url.trim_end_matches('/'))
}

/// The value of the masked hint for a key.
pub(crate) fn hint_of(key: Option<&str>) -> Option<String> {
    key.map(mask_hint)
}

pub(crate) type SharedSink = Arc<dyn VoiceSink>;
