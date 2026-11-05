//! The spoken language of the author on the transcription routes
//! (VOP-FR-BYZC, VOP-FR-IRZC, VOP-FR-TSUS, VOP-FR-KBPZ, GSS-FR-DNAW).

use std::sync::Arc;

use serde_json::json;

use super::support::*;
use crate::global_settings::AppPreferences;
use crate::voice_ai::http::hint_fields;
use crate::voice_ai::languages::*;
use crate::voice_ai::*;

fn http_config(provider: &str, base_url: &str, language: Option<&'static str>) -> ActiveVoice {
    let mut record = VoiceAiRecord::seeded(descriptor(provider).unwrap());
    record.base_url = base_url.to_string();
    record.stt_model = "stt".to_string();
    record.transcription_route = TranscriptionRoute::Http;
    record.capabilities =
        Some(VoiceCapabilities { http_transcription: true, ..VoiceCapabilities::default() });
    ActiveVoice { record, key: Some("k".into()), language }
}

// VOP-FR-IRZC: the table holds fifteen languages, each a code and a name.
#[test]
fn the_table_holds_fifteen_languages() {
    assert_eq!(LANGUAGES.len(), 15);
    assert!(LANGUAGES.contains(&("ru", "Russian")));
    assert!(LANGUAGES.contains(&("ko", "Korean")));
}

// VOP-FR-BYZC: a code or a name of the table, any case, and a free name of
// letters, spaces, hyphens, and brackets. Each other value is none.
#[test]
fn a_spoken_language_is_a_code_a_name_or_a_free_name() {
    let ru = spoken_language("RU").unwrap();
    assert_eq!((ru.name.as_str(), ru.code), ("Russian", Some("ru")));
    assert_eq!(spoken_language(" german ").unwrap().code, Some("de"));
    let free = spoken_language("Swiss German (Zürich)").unwrap();
    assert_eq!((free.name.as_str(), free.code), ("Swiss German (Zürich)", None));
    assert!(spoken_language(&"x".repeat(40)).is_some(), "40 characters is the limit");
    for bad in ["", "   ", "<b>", "fr; rm -rf", "Klingon 2", &"x".repeat(41)] {
        assert_eq!(spoken_language(bad), None, "{bad:?}");
    }
}

// VOP-FR-TSUS: a code goes to `openai` and `custom` as `language`, and
// OpenRouter and a free name get no hint.
#[test]
fn the_hint_goes_to_openai_and_custom_only() {
    assert_eq!(transcription_hint("custom", "ru"), Some("ru"));
    assert_eq!(transcription_hint("openai", "Russian"), Some("ru"));
    assert_eq!(transcription_hint("openrouter", "ru"), None);
    assert_eq!(transcription_hint("custom", "Basque"), None);
    let none: Vec<String> = Vec::new();
    assert_eq!(hint_fields("custom", &none, Some("ru")), vec![("language", "ru".to_string())]);
    assert_eq!(hint_fields("openrouter", &none, Some("ru")), Vec::new());
    assert_eq!(hint_fields("openai", &none, Some("ru")), vec![("language", "ru".to_string())]);
}

// VOP-FR-KBPZ, GSS-FR-DNAW: the active configuration copies the spoken
// language of the preferences.
#[test]
fn the_active_configuration_copies_the_spoken_language() {
    let bench = Bench::new();
    let mut record = VoiceAiRecord::seeded(descriptor("custom").unwrap());
    record.base_url = "http://localhost:1".into();
    record.verified_at = Some("2026-11-04T00:00:00Z".into());
    bench.store.save_voice_ai_registry(vec![record], Some("custom".into())).unwrap();
    assert_eq!(resolve_active_voice(&bench.store, &bench.voice).unwrap().language, None);
    bench
        .store
        .save_app_preferences(AppPreferences {
            voice_spoken_language: "ru".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(resolve_active_voice(&bench.store, &bench.voice).unwrap().language, Some("ru"));
}

// VOP-FR-TSUS: the HTTP route of `custom` sends the language, and a 400 for
// it repeats once without it.
#[tokio::test]
async fn a_refused_language_repeats_without_it() {
    let bench = Bench::new();
    let server = fake(
        |_, body| {
            if String::from_utf8_lossy(body).contains("name=\"language\"") {
                HttpReply::status(400)
            } else {
                transcribed("hello")
            }
        },
        None,
    )
    .await;
    let start = bench
        .voice
        .start_capture(
            http_config("custom", &server.url, Some("ru")),
            48_000,
            Vec::new(),
            Arc::new(RecordingSink::default()),
        )
        .unwrap();
    bench.voice.append_capture(&start.session_id, vec![100; 4_800]).unwrap();
    assert_eq!(bench.voice.finish_capture(&start.session_id).await.unwrap(), "hello");
    let seen = server.http.lock().unwrap().clone();
    assert_eq!(seen.len(), 2);
    assert!(String::from_utf8_lossy(&seen[0].body).contains("name=\"language\""));
    assert!(!String::from_utf8_lossy(&seen[1].body).contains("name=\"language\""));
}

// VOP-FR-TSUS: the Realtime route of `openai` sends the language in its
// transcription settings.
#[tokio::test]
async fn openai_realtime_transcription_carries_the_language() {
    let bench = Bench::new();
    let server = fake(
        |_, _| HttpReply::status(404),
        ws_handler(|_, mut socket| async move {
            send(
                &mut socket,
                json!({ "type": "session.created", "session": { "type": "transcription" } }),
            )
            .await;
            let update = recv_type(&mut socket, "session.update").await.unwrap();
            assert_eq!(update["session"]["audio"]["input"]["transcription"]["language"], "ja");
            recv_type(&mut socket, "input_audio_buffer.commit").await;
            send(&mut socket, json!({ "type": "input_audio_buffer.committed", "item_id": "a" }))
                .await;
            send(
                &mut socket,
                json!({
                    "type": "conversation.item.input_audio_transcription.completed",
                    "item_id": "a", "transcript": "ok",
                }),
            )
            .await;
        }),
    )
    .await;
    let mut config = http_config("openai", &format!("{}/v1", server.url), Some("ja"));
    config.record.transcription_route = TranscriptionRoute::Realtime;
    config.record.capabilities = Some(VoiceCapabilities {
        realtime_transcription: Some(RealtimeTranscription::Live),
        ..VoiceCapabilities::default()
    });
    let start = bench
        .voice
        .start_capture(config, 24_000, Vec::new(), Arc::new(RecordingSink::default()))
        .unwrap();
    assert_eq!(bench.voice.finish_capture(&start.session_id).await.unwrap(), "ok");
}

// GSS-FR-DNAW: no value, a value that is not a string, and a value over 40
// characters read as no spoken language.
#[test]
fn the_spoken_language_preference_reads_leniently() {
    let read = |toml_text: &str| -> String {
        toml::from_str::<AppPreferences>(toml_text).unwrap().voice_spoken_language
    };
    assert_eq!(read(""), "");
    assert_eq!(read("voiceSpokenLanguage = \"ru\"\n"), "ru");
    assert_eq!(read("voiceSpokenLanguage = 7\n"), "");
    assert_eq!(read(&format!("voiceSpokenLanguage = \"{}\"\n", "x".repeat(41))), "");
    assert_eq!(read(&format!("voiceSpokenLanguage = \"{}\"\n", "x".repeat(40))), "x".repeat(40));
    // The other fields of the file stay.
    let both: AppPreferences =
        toml::from_str("voiceSpokenLanguage = 7\nvoiceInputMode = \"vad\"\n").unwrap();
    assert_eq!(both.voice_input_mode, crate::global_settings::VoiceInputMode::Vad);
}

// VOP-FR-TSUS: only a 400 to a `custom` request with a language repeats, and
// it repeats once.
#[tokio::test]
async fn only_a_custom_language_refusal_repeats_and_only_once() {
    for (provider, language, requests) in
        [("openai", Some("ru"), 1), ("custom", None, 1), ("custom", Some("ru"), 2)]
    {
        let bench = Bench::new();
        let server = fake(|_, _| HttpReply::status(400), None).await;
        let base =
            if provider == "openai" { format!("{}/v1", server.url) } else { server.url.clone() };
        let start = bench
            .voice
            .start_capture(
                http_config(provider, &base, language),
                48_000,
                Vec::new(),
                Arc::new(RecordingSink::default()),
            )
            .unwrap();
        bench.voice.append_capture(&start.session_id, vec![100; 4_800]).unwrap();
        assert!(bench.voice.finish_capture(&start.session_id).await.is_err());
        assert_eq!(server.http.lock().unwrap().len(), requests, "{provider} {language:?}");
    }
}
