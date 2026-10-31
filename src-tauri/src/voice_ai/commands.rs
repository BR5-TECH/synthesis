//! The Tauri commands of Voice AI. The log records are here, and they hold no
//! key, no audio, and no transcript.

use std::time::Instant;

use tauri::ipc::{Channel, InvokeBody, InvokeResponseBody, Request};
use tauri::State;

use super::capture::WATCHDOG_TICK;
use super::models::{fallback_models, list_models, list_voices, listing_target};
use super::pcm::samples_of;
use super::registry::{
    clear_integration, list_integrations, set_active, set_default_voice, set_routes,
};
use super::speech::speak_segment;
use super::verify::{verify_integration, VerifyRequest};
use super::*;
use crate::ai_shared::loggable_base_url;
use crate::log_fields;
use crate::logging::{self, Domain};

const SPEECH_DOMAINS: &[Domain] = &[Domain::Ai, Domain::Remote];

/// The header of "append voice capture" and "feed voice activity".
pub(crate) const SESSION_HEADER: &str = "x-voice-session";

fn announce(app: &tauri::AppHandle, store: &GlobalSettingsStore) {
    if let Ok(integrations) = list_integrations(store) {
        app.integrations_changed(&integrations);
    }
}

/// One record per configuration change, on success and on failure.
fn log_change<T>(
    app: &tauri::AppHandle,
    message: &str,
    provider: &str,
    result: &Result<T, String>,
) {
    match result {
        Ok(_) => logging::log_info(
            app,
            &logging::BUFFER,
            SPEECH_DOMAINS,
            message,
            log_fields! { "provider" => provider },
        ),
        Err(code) => log_failure(app, message, code),
    }
}

fn log_failure(app: &tauri::AppHandle, message: &str, code: &str) {
    logging::log_error(
        app,
        &logging::BUFFER,
        SPEECH_DOMAINS,
        message,
        log_fields! { "code" => code },
    );
}

#[tauri::command]
pub fn list_voice_ai_integrations(
    store: State<'_, GlobalSettingsStore>,
) -> Result<VoiceAiIntegrations, String> {
    list_integrations(&store)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)] // The operation of VOP takes these five values.
pub async fn verify_voice_ai_integration(
    provider: String,
    base_url: String,
    api_key: Option<String>,
    stt_model: String,
    tts_model: String,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    voice: State<'_, VoiceAi>,
) -> Result<VoiceAiRecord, String> {
    logging::log_info(
        &app,
        &logging::BUFFER,
        SPEECH_DOMAINS,
        "verifying voice ai integration",
        log_fields! {
            "provider" => &provider,
            "baseUrl" => loggable_base_url(&base_url),
            "keySupplied" => api_key.as_deref().is_some_and(|k| !k.trim().is_empty()),
        },
    );
    let started = Instant::now();
    let result = verify_integration(
        &store,
        &voice,
        VerifyRequest {
            provider: &provider,
            base_url: &base_url,
            api_key: api_key.as_deref(),
            stt_model: &stt_model,
            tts_model: &tts_model,
        },
    )
    .await;
    match &result {
        Ok(record) => {
            logging::log_info(
                &app,
                &logging::BUFFER,
                SPEECH_DOMAINS,
                "voice ai integration verified",
                log_fields! {
                    "provider" => &provider,
                    "transcriptionRoute" => format!("{:?}", record.transcription_route),
                    "speechRoute" => format!("{:?}", record.speech_route),
                    "durationMs" => started.elapsed().as_millis() as u64,
                },
            );
            announce(&app, &store);
        }
        Err(code) => log_failure(&app, "voice ai verification failed", code),
    }
    result
}

#[tauri::command]
pub fn set_voice_ai_routes(
    provider: String,
    transcription_route: TranscriptionRoute,
    speech_route: Option<SpeechRoute>,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    voice: State<'_, VoiceAi>,
) -> Result<VoiceAiRecord, String> {
    let result = set_routes(&store, &voice, &provider, transcription_route, speech_route);
    log_change(&app, "voice routes selected", &provider, &result);
    let record = result?;
    announce(&app, &store);
    Ok(record)
}

#[tauri::command]
pub fn set_voice_ai_default_voice(
    provider: String,
    voice_name: String,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    voice: State<'_, VoiceAi>,
) -> Result<VoiceAiRecord, String> {
    let result = set_default_voice(&store, &voice, &provider, &voice_name);
    log_change(&app, "voice default selected", &provider, &result);
    let record = result?;
    announce(&app, &store);
    Ok(record)
}

#[tauri::command]
pub fn set_active_voice_ai_integration(
    provider: String,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    voice: State<'_, VoiceAi>,
) -> Result<VoiceAiIntegrations, String> {
    let result = set_active(&store, &voice, &provider);
    log_change(&app, "voice provider activated", &provider, &result);
    let integrations = result?;
    app.integrations_changed(&integrations);
    Ok(integrations)
}

#[tauri::command]
pub fn clear_voice_ai_integration(
    provider: String,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    voice: State<'_, VoiceAi>,
) -> Result<VoiceAiIntegrations, String> {
    let result = clear_integration(&store, &voice, &provider);
    log_change(&app, "voice provider cleared", &provider, &result);
    let integrations = result?;
    app.integrations_changed(&integrations);
    Ok(integrations)
}

#[tauri::command]
pub async fn list_voice_ai_models(
    provider: String,
    base_url: String,
    api_key: Option<String>,
    store: State<'_, GlobalSettingsStore>,
    voice: State<'_, VoiceAi>,
) -> Result<VoiceModels, String> {
    match listing_target(&store, &voice, &provider, &base_url, api_key.as_deref()) {
        Some((url, key)) => list_models(&provider, &url, key.as_deref()).await,
        None => fallback_models(&provider),
    }
}

#[tauri::command]
pub async fn list_voice_ai_voices(
    provider: Option<String>,
    store: State<'_, GlobalSettingsStore>,
    voice: State<'_, VoiceAi>,
) -> Result<Option<Vec<String>>, String> {
    list_voices(&store, &voice, provider.as_deref()).await
}

#[tauri::command]
pub async fn start_voice_capture(
    capture_rate: u32,
    keyterms: Vec<String>,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    voice: State<'_, VoiceAi>,
) -> Result<VoiceCaptureStart, String> {
    let config = resolve_active_voice(&store, &voice)?;
    let provider = config.record.provider.clone();
    let sink: SharedSink = Arc::new(app.clone());
    let started = voice.start_capture(config, capture_rate, keyterms, sink);
    match &started {
        Ok(start) => logging::log_info(
            &app,
            &logging::BUFFER,
            SPEECH_DOMAINS,
            "voice capture started",
            log_fields! {
                "provider" => &provider,
                "route" => format!("{:?}", start.route),
                "live" => start.live,
                "captureRate" => capture_rate,
            },
        ),
        Err(code) => log_failure(&app, "voice capture refused", code),
    }
    started
}

/// VOP-FR-DXPK: the raw body, or the JSON array that a webview falls back to.
pub(crate) fn body_samples(body: &InvokeBody) -> Vec<i16> {
    match body {
        InvokeBody::Raw(bytes) => samples_of(bytes),
        InvokeBody::Json(value) => {
            let bytes: Vec<u8> = value
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_u64()).map(|v| v as u8).collect())
                .unwrap_or_default();
            samples_of(&bytes)
        }
    }
}

#[tauri::command]
pub async fn append_voice_capture(
    request: Request<'_>,
    voice: State<'_, VoiceAi>,
) -> Result<(), String> {
    let session_id = request
        .headers()
        .get(SESSION_HEADER)
        .and_then(|v| v.to_str().ok())
        .ok_or(ERR_SESSION_LOST)?
        .to_string();
    voice.append_capture(&session_id, body_samples(request.body()))
}

#[tauri::command]
pub async fn finish_voice_capture(
    session_id: String,
    app: tauri::AppHandle,
    voice: State<'_, VoiceAi>,
) -> Result<String, String> {
    let result = voice.finish_capture(&session_id).await;
    match &result {
        Ok(text) => logging::log_info(
            &app,
            &logging::BUFFER,
            SPEECH_DOMAINS,
            "voice capture finished",
            log_fields! { "chars" => text.chars().count() as u64 },
        ),
        Err(code) => log_failure(&app, "voice capture failed", code),
    }
    result
}

#[tauri::command]
pub async fn retry_voice_capture(
    session_id: String,
    app: tauri::AppHandle,
    voice: State<'_, VoiceAi>,
) -> Result<String, String> {
    logging::log_info(
        &app,
        &logging::BUFFER,
        SPEECH_DOMAINS,
        "voice capture retry started",
        log_fields! {},
    );
    let sink: SharedSink = Arc::new(app.clone());
    let result = voice.retry_capture(&session_id, sink).await;
    if let Err(code) = &result {
        log_failure(&app, "voice capture retry failed", code);
    }
    result
}

#[tauri::command]
pub fn cancel_voice_capture(session_id: String, voice: State<'_, VoiceAi>) -> Result<(), String> {
    voice.cancel_capture(&session_id)
}

#[tauri::command]
pub async fn speak_voice_segment(
    speech_id: String,
    text: String,
    voice_name: Option<String>,
    on_frame: Channel<InvokeResponseBody>,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    voice: State<'_, VoiceAi>,
) -> Result<(), String> {
    // VOP-FR-DWSB: registered first, so that a cancel during the read of the key
    // is not lost.
    let mut cancel = voice.register_speech(&speech_id);
    let mut emit = |bytes: Vec<u8>| {
        let _ = on_frame.send(InvokeResponseBody::Raw(bytes));
    };
    let chars = text.chars().count() as u64;
    let result = match resolve_active_voice(&store, &voice) {
        Ok(active) => {
            speak_segment(
                &voice,
                &active,
                &text,
                voice_name.as_deref(),
                &mut cancel,
                &mut emit,
                &app,
            )
            .await
        }
        Err(code) => Err(code),
    };
    voice.release_speech(&speech_id);
    match &result {
        Ok(()) => logging::log_debug(
            &app,
            &logging::BUFFER,
            SPEECH_DOMAINS,
            "voice segment spoken",
            log_fields! { "chars" => chars },
        ),
        Err(code) => log_failure(&app, "voice speech failed", code),
    }
    result
}

#[tauri::command]
pub fn cancel_voice_speech(speech_id: String, voice: State<'_, VoiceAi>) -> Result<(), String> {
    voice.cancel_speech(&speech_id);
    Ok(())
}

/// VOP-FR-BSNY: the task that sweeps idle sessions, for the life of the
/// application.
pub fn start_watchdog(app: tauri::AppHandle) {
    use tauri::Manager;
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(WATCHDOG_TICK).await;
            let voice = app.state::<VoiceAi>();
            if matches!(voice.sweep_idle(Instant::now()), Ok(true)) {
                logging::log_warn(
                    &app,
                    &logging::BUFFER,
                    SPEECH_DOMAINS,
                    "voice capture lost its webview",
                    log_fields! {},
                );
            }
        }
    });
}
