//! The Tauri commands of VAD sessions. The log records hold no audio and no
//! transcript (VOP-FR-YEFK).

use std::sync::Arc;

use tauri::ipc::Request;
use tauri::{Manager, State};

use super::detector::Settings;
use super::silero::{SileroModel, SpeechModel};
use super::{ActivityChange, ActivityHost, Gate, ERR_DETECTOR_UNAVAILABLE};
use crate::global_settings::GlobalSettingsStore;
use crate::log_fields;
use crate::logging::{self, Domain};
use crate::voice_ai::commands::{body_samples, SESSION_HEADER};
use crate::voice_ai::{
    resolve_active_voice, SharedSink, VoiceAi, VoiceCaptureStart, VoiceSink, ERR_SESSION_LOST,
};

const ACTIVITY_DOMAINS: &[Domain] = &[Domain::Ai, Domain::Backend];

struct AppHost {
    app: tauri::AppHandle,
}

impl ActivityHost for AppHost {
    fn settings(&self) -> Settings {
        let store = self.app.state::<GlobalSettingsStore>();
        let prefs = store.load_app_preferences().unwrap_or_default();
        Settings::new(prefs.voice_vad_silence_ms, prefs.voice_vad_sensitivity)
    }

    fn start_capture(
        &self,
        capture_rate: u32,
        keyterms: Vec<String>,
    ) -> Result<VoiceCaptureStart, String> {
        let store = self.app.state::<GlobalSettingsStore>();
        let voice = self.app.state::<VoiceAi>();
        let config = resolve_active_voice(&store, &voice)?;
        let sink: SharedSink = Arc::new(self.app.clone());
        voice.start_capture(config, capture_rate, keyterms, sink)
    }

    fn append_capture(&self, capture_id: &str, samples: Vec<i16>) -> Result<(), String> {
        self.app.state::<VoiceAi>().append_capture(capture_id, samples)
    }

    fn cancel_capture(&self, capture_id: &str) {
        let _ = self.app.state::<VoiceAi>().cancel_capture(capture_id);
    }

    fn emit(&self, change: &ActivityChange) {
        self.app.activity_changed(change);
    }

    fn note_failure(&self, code: &str, detail: &str) {
        logging::log_warn(
            &self.app,
            &logging::BUFFER,
            ACTIVITY_DOMAINS,
            "voice activity failed",
            log_fields! { "code" => code, "detail" => detail },
        );
    }
}

fn log_refusal(app: &tauri::AppHandle, message: &str, code: &str) {
    logging::log_warn(
        app,
        &logging::BUFFER,
        ACTIVITY_DOMAINS,
        message,
        log_fields! { "code" => code },
    );
}

fn load_model() -> Result<Box<dyn SpeechModel>, String> {
    Ok(Box::new(SileroModel::load()?))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceActivityOpened {
    activity_id: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceActivityClosed {
    capture_id: Option<String>,
}

#[tauri::command]
pub async fn open_voice_activity_session(
    capture_rate: u32,
    keyterms: Vec<String>,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    voice: State<'_, VoiceAi>,
) -> Result<VoiceActivityOpened, String> {
    // The first read of a key can wait for the keyring. It runs here, so the
    // worker does not wait in the middle of an utterance.
    if let Err(code) = resolve_active_voice(&store, &voice) {
        log_refusal(&app, "voice activity session refused", &code);
        return Err(code);
    }
    let host = AppHost { app: app.clone() };
    let opened = voice.open_activity(capture_rate, keyterms, host, load_model).await;
    match opened {
        Ok(activity_id) => {
            logging::log_info(
                &app,
                &logging::BUFFER,
                ACTIVITY_DOMAINS,
                "voice activity session opened",
                log_fields! { "captureRate" => capture_rate },
            );
            Ok(VoiceActivityOpened { activity_id })
        }
        Err(detail) => {
            // The detail comes from the runtime and holds no audio.
            logging::log_error(
                &app,
                &logging::BUFFER,
                ACTIVITY_DOMAINS,
                "the voice detector did not load",
                log_fields! { "detail" => detail },
            );
            Err(ERR_DETECTOR_UNAVAILABLE.to_string())
        }
    }
}

#[tauri::command]
pub async fn feed_voice_activity(
    request: Request<'_>,
    voice: State<'_, VoiceAi>,
) -> Result<(), String> {
    let activity_id = request
        .headers()
        .get(SESSION_HEADER)
        .and_then(|v| v.to_str().ok())
        .ok_or(ERR_SESSION_LOST)?
        .to_string();
    voice.feed_activity(&activity_id, body_samples(request.body())).await
}

#[tauri::command]
pub fn set_voice_activity_gate(
    activity_id: String,
    seq: u64,
    blocked: bool,
    playing: bool,
    settled: Option<String>,
    keyterms: Vec<String>,
    voice: State<'_, VoiceAi>,
) -> Result<(), String> {
    voice.set_activity_gate(&activity_id, Gate { seq, blocked, playing, settled, keyterms })
}

#[tauri::command]
pub async fn close_voice_activity_session(
    activity_id: String,
    discard: bool,
    app: tauri::AppHandle,
    voice: State<'_, VoiceAi>,
) -> Result<VoiceActivityClosed, String> {
    let capture_id = voice.close_activity(&activity_id, discard).await.inspect_err(|code| {
        log_refusal(&app, "voice activity session did not close", code);
    })?;
    logging::log_info(
        &app,
        &logging::BUFFER,
        ACTIVITY_DOMAINS,
        "voice activity session closed",
        log_fields! { "discard" => discard, "openCapture" => capture_id.is_some() },
    );
    Ok(VoiceActivityClosed { capture_id })
}
