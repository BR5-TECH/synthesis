//! Verification, detection, and the records.

use serde_json::json;

use super::support::*;
use crate::voice_ai::registry::*;
use crate::voice_ai::verify::{verify_integration, VerifyRequest};
use crate::voice_ai::*;

fn request<'a>(provider: &'a str, base: &'a str, key: Option<&'a str>) -> VerifyRequest<'a> {
    VerifyRequest { provider, base_url: base, api_key: key, stt_model: "stt", tts_model: "tts" }
}

/// A llama.cpp server: live transcription, and speech on Realtime.
async fn llama_server() -> Fake {
    fake(
        |path, _| match path {
            "/v1/audio/transcriptions" => transcribed(""),
            _ => HttpReply::status(404),
        },
        ws_handler(|uri, mut socket| async move {
            if uri.contains("intent=transcription") {
                send(&mut socket, llama_created("transcription", 16_000, None)).await;
                recv_type(&mut socket, "input_audio_buffer.commit").await;
                send(
                    &mut socket,
                    json!({ "type": "input_audio_buffer.committed", "item_id": "i" }),
                )
                .await;
            } else {
                send(&mut socket, llama_created("realtime", 16_000, Some(24_000))).await;
                recv_type(&mut socket, "response.create").await;
                send(
                    &mut socket,
                    json!({ "type": "response.output_audio.delta", "delta": "AAA=" }),
                )
                .await;
                recv_type(&mut socket, "response.cancel").await;
            }
        }),
    )
    .await
}

// VOP-FR-GYMG, VOP-FR-FHDE, VOP-FR-WUEI, VOP-FR-USQZ,
// VOP-FR-IKUE, VOP-FR-UWUE: a llama.cpp server with no key gets the
// Realtime routes for transcription and for speech.
#[tokio::test]
async fn llama_cpp_gets_realtime_for_both_directions() {
    let bench = Bench::new();
    let server = llama_server().await;
    let record =
        verify_integration(&bench.store, &bench.voice, request("custom", &server.url, None))
            .await
            .unwrap();
    let capabilities = record.capabilities.clone().unwrap();
    assert_eq!(capabilities.realtime_transcription, Some(RealtimeTranscription::Live));
    assert_eq!(capabilities.realtime_input_rate, Some(16_000));
    assert_eq!(capabilities.dialect, Some(Dialect::Llamacpp));
    assert!(capabilities.realtime_speech);
    assert!(capabilities.http_transcription);
    assert_eq!(record.transcription_route, TranscriptionRoute::Realtime);
    assert_eq!(record.speech_route, Some(SpeechRoute::Realtime));
    let ws = server.ws.lock().unwrap().clone();
    assert!(ws[0].uri.contains("intent=transcription&model=stt"), "{}", ws[0].uri);
    assert!(ws.iter().all(|w| w.authorization.is_none()), "no key means no header");
}

// VOP-FR-IQAY, VOP-FR-OMTM, VOP-FR-IKUE: a server with no
// Realtime route, as Ollama is, verifies on HTTP.
#[tokio::test]
async fn a_server_with_no_realtime_route_verifies_on_http() {
    let bench = Bench::new();
    let server = fake(
        |path, _| match path {
            "/v1/audio/transcriptions" => transcribed(""),
            "/v1/audio/speech" => HttpReply::audio("audio/wav", vec![0; 8]),
            _ => HttpReply::status(404),
        },
        None,
    )
    .await;
    let record =
        verify_integration(&bench.store, &bench.voice, request("custom", &server.url, None))
            .await
            .unwrap();
    let capabilities = record.capabilities.clone().unwrap();
    assert_eq!(capabilities.realtime_transcription, None);
    assert!(!capabilities.realtime_speech);
    assert!(capabilities.openai_speech);
    assert_eq!(record.transcription_route, TranscriptionRoute::Http);
    assert_eq!(record.speech_route, Some(SpeechRoute::OpenaiSpeech));
}

// VOP-FR-IKUE, VOP-FR-TSVV: with no other speech route, `/tts` serves.
#[tokio::test]
async fn llama_tts_is_the_last_speech_route() {
    let bench = Bench::new();
    let server = fake(
        |path, _| match path {
            "/v1/audio/transcriptions" => transcribed(""),
            "/tts" => HttpReply::audio("audio/wav", vec![0; 8]),
            _ => HttpReply::status(404),
        },
        None,
    )
    .await;
    let record =
        verify_integration(&bench.store, &bench.voice, request("custom", &server.url, None))
            .await
            .unwrap();
    assert_eq!(record.speech_route, Some(SpeechRoute::LlamaTts));
}

// VOP-FR-AVXY: the server refuses the model. The record keeps its message,
// and the HTTP route still verifies.
#[tokio::test]
async fn a_refused_model_keeps_the_servers_message() {
    let bench = Bench::new();
    let server = fake(
        |path, _| {
            if path == "/v1/audio/transcriptions" {
                transcribed("")
            } else {
                HttpReply::status(404)
            }
        },
        ws_handler(|_, mut socket| async move {
            send(
                &mut socket,
                json!({ "type": "error", "error": {
                    "message": "The current model does not support realtime transcription."
                }}),
            )
            .await;
        }),
    )
    .await;
    let mut body = request("custom", &server.url, None);
    body.tts_model = "";
    let record = verify_integration(&bench.store, &bench.voice, body).await.unwrap();
    assert_eq!(record.capabilities.clone().unwrap().realtime_transcription, None);
    assert_eq!(
        record.capability_note.as_deref(),
        Some("The current model does not support realtime transcription.")
    );
    assert_eq!(record.transcription_route, TranscriptionRoute::Http);
}

// VOP-FR-BTDW, VOP-FR-WUEI: a conversation session becomes a transcription
// session, and the capability says that it is not live.
#[tokio::test]
async fn a_conversation_session_is_turn_based() {
    let bench = Bench::new();
    let server = fake(
        |_, _| HttpReply::status(404),
        ws_handler(|uri, mut socket| async move {
            send(&mut socket, llama_created("realtime", 16_000, None)).await;
            if uri.contains("intent=transcription") {
                let update = recv_type(&mut socket, "session.update").await.unwrap();
                assert_eq!(update["session"]["type"], "transcription");
                recv_type(&mut socket, "input_audio_buffer.commit").await;
                send(
                    &mut socket,
                    json!({
                        "type": "conversation.item.input_audio_transcription.completed",
                        "item_id": "i", "transcript": "",
                    }),
                )
                .await;
            }
        }),
    )
    .await;
    let mut body = request("custom", &server.url, None);
    body.tts_model = "";
    let record = verify_integration(&bench.store, &bench.voice, body).await.unwrap();
    assert_eq!(
        record.capabilities.unwrap().realtime_transcription,
        Some(RealtimeTranscription::Turn)
    );
}

// VOP-FR-TLGM, VOP-FR-YRQN: `session.created` with no audio block is `vllm`.
#[tokio::test]
async fn a_session_with_no_audio_block_is_vllm() {
    let bench = Bench::new();
    let server = fake(
        |_, _| HttpReply::status(404),
        ws_handler(|_, mut socket| async move {
            send(&mut socket, json!({ "type": "session.created", "id": "s" })).await;
            let update = recv_type(&mut socket, "session.update").await.unwrap();
            assert_eq!(update["model"], "stt");
            assert!(update.get("session").is_none());
            // The first commit marks readiness, and the last one holds `final`.
            let ready = recv_type(&mut socket, "input_audio_buffer.commit").await.unwrap();
            assert!(ready.get("final").is_none());
            let last = recv_type(&mut socket, "input_audio_buffer.commit").await.unwrap();
            assert_eq!(last["final"], true);
            send(&mut socket, json!({ "type": "transcription.done", "text": "" })).await;
        }),
    )
    .await;
    let mut body = request("custom", &server.url, None);
    body.tts_model = "";
    let record = verify_integration(&bench.store, &bench.voice, body).await.unwrap();
    let capabilities = record.capabilities.unwrap();
    assert_eq!(capabilities.dialect, Some(Dialect::Vllm));
    assert_eq!(capabilities.realtime_input_rate, Some(16_000));
}

// VOP-FR-HAXX, VOP-FR-XONB, VOP-FR-GDVD, VOP-FR-HOGR: the first
// messages of an `openai` session.
#[tokio::test]
async fn an_openai_session_has_the_documented_shape() {
    let bench = Bench::new();
    let server = fake(
        |path, _| match path {
            "/v1/audio/transcriptions" => transcribed(""),
            _ => HttpReply::status(404),
        },
        ws_handler(|_, mut socket| async move {
            send(
                &mut socket,
                json!({ "type": "session.created", "session": {
                    "type": "transcription"
                }}),
            )
            .await;
            let update = recv_type(&mut socket, "session.update").await.unwrap();
            let session = &update["session"];
            assert_eq!(session["type"], "transcription");
            assert_eq!(session["audio"]["input"]["format"]["rate"], 24_000);
            assert_eq!(session["audio"]["input"]["format"]["type"], "audio/pcm");
            assert_eq!(session["audio"]["input"]["transcription"]["model"], "stt");
            assert!(session["audio"]["input"]["turn_detection"].is_null());
            let commit = recv_type(&mut socket, "input_audio_buffer.commit").await.unwrap();
            assert!(commit.get("final").is_none());
            send(&mut socket, json!({ "type": "input_audio_buffer.committed", "item_id": "i" }))
                .await;
        }),
    )
    .await;
    let base = format!("{}/v1", server.url);
    let body = VerifyRequest { tts_model: "", ..request("openai", &base, Some("sk-test")) };
    let record = verify_integration(&bench.store, &bench.voice, body).await.unwrap();
    assert_eq!(record.transcription_route, TranscriptionRoute::Realtime);
    let ws = server.ws.lock().unwrap().clone();
    assert!(ws[0].uri.ends_with("/v1/realtime?intent=transcription"), "{}", ws[0].uri);
    assert_eq!(ws[0].authorization.as_deref(), Some("Bearer sk-test"));
}

// VOP-FR-IAEN: `openai` is not probed for an HTTP speech route.
#[tokio::test]
async fn openai_has_no_http_speech_probe() {
    let bench = Bench::new();
    let server = fake(
        |path, _| {
            if path.ends_with("/audio/transcriptions") {
                transcribed("")
            } else {
                HttpReply::audio("audio/wav", vec![])
            }
        },
        None,
    )
    .await;
    let base = format!("{}/v1", server.url);
    let error = verify_integration(&bench.store, &bench.voice, request("openai", &base, Some("k")))
        .await
        .unwrap_err();
    assert_eq!(error, ERR_NO_SPEECH_ROUTE);
    assert!(server.http.lock().unwrap().iter().all(|h| !h.path.contains("/audio/speech")));
}

// VOP-FR-KAUL, VOP-FR-PGPO: no route at all fails, and nothing is written.
#[tokio::test]
async fn no_transcription_route_fails_and_writes_nothing() {
    let bench = Bench::new();
    let server = fake(|_, _| HttpReply::status(404), None).await;
    let error =
        verify_integration(&bench.store, &bench.voice, request("custom", &server.url, Some("k")))
            .await
            .unwrap_err();
    assert_eq!(error, ERR_NO_TRANSCRIPTION_ROUTE);
    assert!(bench.store.load_voice_ai_registry().unwrap().0.is_empty());
    assert!(bench.secrets.0.lock().unwrap().is_empty());
}

// VOP-FR-KAUL: a TTS model with no speech route fails.
#[tokio::test]
async fn a_tts_model_with_no_route_fails() {
    let bench = Bench::new();
    let server = fake(
        |path, _| {
            if path.ends_with("/audio/transcriptions") {
                transcribed("")
            } else {
                HttpReply::status(404)
            }
        },
        None,
    )
    .await;
    let error =
        verify_integration(&bench.store, &bench.voice, request("custom", &server.url, None))
            .await
            .unwrap_err();
    assert_eq!(error, ERR_NO_SPEECH_ROUTE);
}

// VOP-FR-GGBG: an empty TTS model has no speech route and still verifies.
#[tokio::test]
async fn an_empty_tts_model_verifies_with_no_speech_route() {
    let bench = Bench::new();
    let server = fake(
        |path, _| {
            if path.ends_with("/audio/transcriptions") {
                transcribed("")
            } else {
                HttpReply::status(404)
            }
        },
        None,
    )
    .await;
    let body = VerifyRequest { tts_model: "", ..request("custom", &server.url, None) };
    let record = verify_integration(&bench.store, &bench.voice, body).await.unwrap();
    assert_eq!(record.speech_route, None);
    assert!(record.verified_at.is_some());
}

// VOP-FR-DBDI: a refused key is `rejected`, not a missing route.
#[tokio::test]
async fn a_refused_key_is_rejected() {
    let bench = Bench::new();
    let server = fake(|_, _| HttpReply::status(401), None).await;
    let base = format!("{}/v1", server.url);
    let body = VerifyRequest { tts_model: "", ..request("openai", &base, Some("bad")) };
    let error = verify_integration(&bench.store, &bench.voice, body).await.unwrap_err();
    assert_eq!(error, ERR_REJECTED);
}

// VOP-FR-OMTM: OpenRouter's `GET /key` answers first.
#[tokio::test]
async fn openrouter_checks_its_key_first() {
    let bench = Bench::new();
    let server = fake(|_, _| HttpReply::status(401), None).await;
    let error = verify_integration(
        &bench.store,
        &bench.voice,
        request("openrouter", &server.url, Some("k")),
    )
    .await
    .unwrap_err();
    assert_eq!(error, ERR_REJECTED);
    let seen = server.http.lock().unwrap().clone();
    assert_eq!(seen.len(), 1, "no other probe runs");
    assert_eq!(seen[0].path, "/key");
    assert_eq!(seen[0].authorization.as_deref(), Some("Bearer k"));
}

// VOP-FR-DBDI: nothing listens on the port.
#[tokio::test]
async fn a_closed_port_is_unreachable() {
    let bench = Bench::new();
    // Port 1 is not served, and no parallel test can take it.
    let url = "http://127.0.0.1:1".to_string();
    let body = VerifyRequest { tts_model: "", ..request("custom", &url, None) };
    let error = verify_integration(&bench.store, &bench.voice, body).await.unwrap_err();
    assert_eq!(error, ERR_UNREACHABLE);
}

// VOP-FR-TLVI, VOP-FR-DBDI: refusals that need no network.
#[tokio::test]
async fn bad_input_is_refused_before_any_request() {
    let bench = Bench::new();
    let verify = |base: &'static str, provider: &'static str, key: Option<&'static str>| {
        verify_integration(
            &bench.store,
            &bench.voice,
            VerifyRequest { tts_model: "", ..request(provider, base, key) },
        )
    };
    assert_eq!(verify("", "custom", None).await.unwrap_err(), ERR_BASE_URL_EMPTY);
    assert_eq!(verify("ftp://h", "custom", None).await.unwrap_err(), ERR_BASE_URL_INVALID);
    assert_eq!(verify("http://u:p@h", "custom", None).await.unwrap_err(), ERR_BASE_URL_INVALID);
    assert_eq!(verify("http://h:1/v1", "custom", None).await.unwrap_err(), ERR_BASE_URL_INVALID);
    assert_eq!(verify("http://h:1", "openai", None).await.unwrap_err(), ERR_KEY_MISSING);
    assert_eq!(verify("http://h:1", "openrouter", None).await.unwrap_err(), ERR_KEY_MISSING);
    assert_eq!(verify("http://h:1", "other", None).await.unwrap_err(), ERR_UNKNOWN_PROVIDER);
}

// VOP-FR-ZSXI, VOP-FR-PGPO, VOP-FR-HTZP, VOP-FR-EBQI: the key goes to
// the vault, the record holds only its hint, and an empty field reuses it.
#[tokio::test]
async fn the_key_goes_to_the_vault_and_an_empty_field_reuses_it() {
    let bench = Bench::new();
    let server = fake(
        |path, _| {
            if path.ends_with("/audio/transcriptions") {
                transcribed("")
            } else {
                HttpReply::status(404)
            }
        },
        None,
    )
    .await;
    let body =
        VerifyRequest { tts_model: "", ..request("custom", &server.url, Some("sk-secret-1234")) };
    let record = verify_integration(&bench.store, &bench.voice, body).await.unwrap();
    assert_eq!(record.masked_hint.as_deref(), Some("1234"));
    assert_eq!(bench.secrets.0.lock().unwrap().get("custom").unwrap(), "sk-secret-1234");
    let stored = toml::to_string(&bench.store.load_voice_ai_registry().unwrap().0[0]).unwrap();
    assert!(!stored.contains("sk-secret"), "the record holds no key");

    let body = VerifyRequest { tts_model: "", ..request("custom", &server.url, Some("  ")) };
    verify_integration(&bench.store, &bench.voice, body).await.unwrap();
    let last = server.http.lock().unwrap().last().cloned().unwrap();
    assert_eq!(last.authorization.as_deref(), Some("Bearer sk-secret-1234"));
}

// VOP-FR-IKUE: a choice of the author stands, and it gives way when the
// capabilities no longer hold it.
#[tokio::test]
async fn the_authors_route_stands_while_it_is_detected() {
    let bench = Bench::new();
    let server = llama_server().await;
    let body = || request("custom", &server.url, None);
    let record = verify_integration(&bench.store, &bench.voice, body()).await.unwrap();
    assert_eq!(record.transcription_route, TranscriptionRoute::Realtime);
    set_routes(
        &bench.store,
        &bench.voice,
        "custom",
        TranscriptionRoute::Http,
        Some(SpeechRoute::Realtime),
    )
    .unwrap();
    let record = verify_integration(&bench.store, &bench.voice, body()).await.unwrap();
    assert_eq!(record.transcription_route, TranscriptionRoute::Http, "the choice stands");

    let gone = fake(
        |_, _| HttpReply::status(404),
        ws_handler(|_, mut socket| async move {
            send(&mut socket, llama_created("transcription", 16_000, None)).await;
            recv_type(&mut socket, "input_audio_buffer.commit").await;
            send(&mut socket, json!({ "type": "input_audio_buffer.committed", "item_id": "i" }))
                .await;
        }),
    )
    .await;
    let body = VerifyRequest { tts_model: "", ..request("custom", &gone.url, None) };
    let record = verify_integration(&bench.store, &bench.voice, body).await.unwrap();
    assert_eq!(record.transcription_route, TranscriptionRoute::Realtime, "it gives way");
}

fn verified(bench: &Bench, provider: &str, capabilities: VoiceCapabilities) {
    let mut record = VoiceAiRecord::seeded(descriptor(provider).unwrap());
    record.verified_at = Some("2026-10-31T00:00:00Z".into());
    record.capabilities = Some(capabilities);
    bench.store.save_voice_ai_registry(vec![record], None).unwrap();
}

// VOP-FR-SURK, VOP-FR-CVLB: each provider has a record, stored or seeded.
#[test]
fn each_provider_has_a_record_from_its_descriptor() {
    let bench = Bench::new();
    let list = list_integrations(&bench.store).unwrap();
    assert_eq!(list.active, None);
    let ids: Vec<&str> = list.records.iter().map(|r| r.provider.as_str()).collect();
    assert_eq!(ids, ["openai", "openrouter", "custom"]);
    assert_eq!(list.records[0].base_url, "https://api.openai.com/v1");
    assert_eq!(list.records[0].stt_model, "gpt-live-transcribe");
    assert_eq!(list.records[0].tts_model, "gpt-realtime-2.1-mini");
    assert_eq!(list.records[0].default_voice, "marin");
    assert_eq!(list.records[1].base_url, "https://openrouter.ai/api/v1");
    assert_eq!(list.records[1].stt_model, "openai/gpt-transcribe");
    assert_eq!(list.records[1].default_voice, "george");
    assert_eq!(list.records[2].base_url, "");
}

// VOP-FR-IHLG: the ids are this module's own.
#[test]
fn the_provider_ids_are_stable() {
    let ids: Vec<&str> = PROVIDERS.iter().map(|d| d.id).collect();
    assert_eq!(ids, ["openai", "openrouter", "custom"]);
    assert!(descriptor("anthropic").is_none(), "an AI API provider is not a voice provider");
}

// VOP-FR-KRMY: only a verified provider becomes active.
#[test]
fn only_a_verified_provider_becomes_active() {
    let bench = Bench::new();
    assert_eq!(set_active(&bench.store, &bench.voice, "custom").unwrap_err(), ERR_NOT_VERIFIED);
    verified(&bench, "custom", VoiceCapabilities::default());
    let list = set_active(&bench.store, &bench.voice, "custom").unwrap();
    assert_eq!(list.active.as_deref(), Some("custom"));
    assert_eq!(set_active(&bench.store, &bench.voice, "other").unwrap_err(), ERR_UNKNOWN_PROVIDER);
}

// VOP-FR-ZSXI: clearing removes the record, the key, and the active choice.
#[test]
fn clearing_removes_the_record_the_key_and_the_active_choice() {
    let bench = Bench::new();
    verified(&bench, "custom", VoiceCapabilities::default());
    set_active(&bench.store, &bench.voice, "custom").unwrap();
    bench.secrets.0.lock().unwrap().insert("custom".into(), "k".into());
    let list = clear_integration(&bench.store, &bench.voice, "custom").unwrap();
    assert_eq!(list.active, None);
    assert!(list.records[2].verified_at.is_none());
    assert!(bench.secrets.0.lock().unwrap().is_empty());
}

// VOP-FR-SURK: a route that detection did not find is refused.
#[test]
fn a_route_that_is_not_detected_is_refused() {
    let bench = Bench::new();
    let refused = |t, s| set_routes(&bench.store, &bench.voice, "custom", t, s).unwrap_err();
    assert_eq!(refused(TranscriptionRoute::Http, None), ERR_ROUTE_NOT_DETECTED);
    verified(
        &bench,
        "custom",
        VoiceCapabilities { http_transcription: true, ..Default::default() },
    );
    assert_eq!(refused(TranscriptionRoute::Realtime, None), ERR_ROUTE_NOT_DETECTED);
    assert_eq!(
        refused(TranscriptionRoute::Http, Some(SpeechRoute::LlamaTts)),
        ERR_ROUTE_NOT_DETECTED
    );
    let record =
        set_routes(&bench.store, &bench.voice, "custom", TranscriptionRoute::Http, None).unwrap();
    assert!(record.routes_chosen);
}

// VOP-FR-SURK: the default voice is stored, and the records survive TOML.
#[test]
fn the_record_survives_the_store() {
    let bench = Bench::new();
    let capabilities = VoiceCapabilities {
        dialect: Some(Dialect::Llamacpp),
        realtime_transcription: Some(RealtimeTranscription::Live),
        realtime_input_rate: Some(16_000),
        ..Default::default()
    };
    verified(&bench, "custom", capabilities.clone());
    set_default_voice(&bench.store, &bench.voice, "custom", " female ").unwrap();
    let text = toml::to_string(&bench.store.load_voice_ai_registry().unwrap().0[0]).unwrap();
    let back: VoiceAiRecord = toml::from_str(&text).unwrap();
    assert_eq!(back.default_voice, "female");
    assert_eq!(back.capabilities, Some(capabilities));
}

// VOP-FR-YPEY: a `wss` connection offers `http/1.1` only.
#[test]
fn a_websocket_offers_http_1_1_only() {
    let config = crate::tls::websocket_client_config().unwrap();
    assert_eq!(config.alpn_protocols, vec![b"http/1.1".to_vec()]);
}

// VOP-FR-XMKU: the bundle declares the microphone.
#[test]
fn the_bundle_declares_the_microphone() {
    let plist = include_str!("../../../Info.plist");
    assert!(plist.contains("<key>NSMicrophoneUsageDescription</key>"));
    let entitlements = include_str!("../../../Entitlements.plist");
    assert!(entitlements.contains("<key>com.apple.security.device.audio-input</key>"));
    let config: serde_json::Value =
        serde_json::from_str(include_str!("../../../tauri.conf.json")).unwrap();
    assert_eq!(config["bundle"]["macOS"]["entitlements"], "Entitlements.plist");
}

// VOP-FR-MUFH: a failed list gives the descriptor list, and no error.
#[tokio::test]
async fn model_lists_come_from_the_server_or_the_descriptor() {
    let openai =
        crate::voice_ai::models::list_models("openai", "http://127.0.0.1:1", None).await.unwrap();
    assert_eq!(openai.stt_models, ["gpt-live-transcribe", "gpt-transcribe"]);
    assert_eq!(openai.tts_models, ["gpt-realtime-2.1-mini"]);

    let server = fake(
        |path, _| match path {
            "/v1/models" => HttpReply::json(r#"{"data":[{"id":"voxtral"}]}"#),
            "/models?output_modalities=transcription" => {
                HttpReply::json(r#"{"data":[{"id":"s"}]}"#)
            }
            "/models?output_modalities=speech" => HttpReply::json(r#"{"data":[{"id":"t"}]}"#),
            _ => HttpReply::status(404),
        },
        None,
    )
    .await;
    let custom = crate::voice_ai::models::list_models("custom", &server.url, None).await.unwrap();
    assert_eq!(custom.stt_models, ["voxtral"]);
    let router =
        crate::voice_ai::models::list_models("openrouter", &server.url, None).await.unwrap();
    assert_eq!(router.stt_models, ["s"]);
    assert_eq!(router.tts_models, ["t"]);
    let dead = crate::voice_ai::models::list_models("openrouter", "http://127.0.0.1:1", None)
        .await
        .unwrap();
    assert_eq!(dead.stt_models, ["openai/gpt-transcribe"]);
}

// VOP-FR-XJRF: the voices of the active provider, by kind.
#[tokio::test]
async fn voices_follow_the_active_provider() {
    let bench = Bench::new();
    let list = crate::voice_ai::models::list_voices(&bench.store, &bench.voice, None);
    assert_eq!(list.await.unwrap(), None, "no active provider");

    verified(&bench, "openai", VoiceCapabilities::default());
    set_active(&bench.store, &bench.voice, "openai").unwrap();
    let voices = crate::voice_ai::models::list_voices(&bench.store, &bench.voice, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(voices.len(), 13);
    assert!(voices.iter().any(|v| v == "marin"));

    verified(&bench, "custom", VoiceCapabilities::default());
    set_active(&bench.store, &bench.voice, "custom").unwrap();
    let voices =
        crate::voice_ai::models::list_voices(&bench.store, &bench.voice, None).await.unwrap();
    assert_eq!(voices, None, "free text");

    let server = fake(
        |path, _| match path {
            "/models?output_modalities=speech" => HttpReply::json(
                r#"{"data":[{"id":"elevenlabs/eleven-v4-turbo","supported_voices":["george","sarah"]}]}"#,
            ),
            _ => HttpReply::status(404),
        },
        None,
    )
    .await;
    let mut record = VoiceAiRecord::seeded(descriptor("openrouter").unwrap());
    record.base_url = server.url.clone();
    record.verified_at = Some("t".into());
    bench.store.save_voice_ai_registry(vec![record], Some("openrouter".into())).unwrap();
    let voices =
        crate::voice_ai::models::list_voices(&bench.store, &bench.voice, None).await.unwrap();
    assert_eq!(voices, Some(vec!["george".to_string(), "sarah".to_string()]));
}

struct BrokenSecrets;

impl crate::ai_shared::SecretStore for BrokenSecrets {
    fn set(&self, _: &str, _: &str) -> Result<(), crate::ai_shared::SecretUnavailable> {
        Err(crate::ai_shared::SecretUnavailable("vault_write_failed".into()))
    }
    fn get(&self, _: &str) -> Result<Option<String>, crate::ai_shared::SecretUnavailable> {
        Err(crate::ai_shared::SecretUnavailable("vault_unavailable".into()))
    }
    fn delete(&self, _: &str) -> Result<(), crate::ai_shared::SecretUnavailable> {
        Ok(())
    }
}

// VOP-FR-DBDI, VOP-FR-PGPO: a vault that refuses is `keychain_unavailable`,
// and the record is not written.
#[tokio::test]
async fn a_vault_that_refuses_writes_nothing() {
    let bench = Bench::new();
    let voice = VoiceAi::new(Box::new(BrokenSecrets));
    let server = fake(
        |path, _| {
            if path.ends_with("/audio/transcriptions") {
                transcribed("")
            } else {
                HttpReply::status(404)
            }
        },
        None,
    )
    .await;
    let reading = VerifyRequest { tts_model: "", ..request("custom", &server.url, None) };
    assert_eq!(
        verify_integration(&bench.store, &voice, reading).await.unwrap_err(),
        ERR_KEYCHAIN_UNAVAILABLE
    );
    let writing = VerifyRequest { tts_model: "", ..request("custom", &server.url, Some("k")) };
    assert_eq!(
        verify_integration(&bench.store, &voice, writing).await.unwrap_err(),
        ERR_KEYCHAIN_UNAVAILABLE
    );
    assert!(bench.store.load_voice_ai_registry().unwrap().0.is_empty());
}

// VOP-FR-TOCX: a stored key goes only to the saved base URL.
#[test]
fn a_stored_key_goes_only_to_the_saved_base_url() {
    use crate::voice_ai::models::listing_target;
    let bench = Bench::new();
    let mut record = VoiceAiRecord::seeded(descriptor("custom").unwrap());
    record.base_url = "http://localhost:8080".into();
    bench.store.save_voice_ai_registry(vec![record], None).unwrap();
    bench.secrets.0.lock().unwrap().insert("custom".into(), "sk-stored".into());
    let target = |url: &str, key: Option<&str>| {
        listing_target(&bench.store, &bench.voice, "custom", url, key)
    };
    assert_eq!(
        target("http://localhost:8080/", None),
        Some(("http://localhost:8080".to_string(), Some("sk-stored".to_string())))
    );
    assert_eq!(
        target("http://localhost:80", None),
        Some(("http://localhost:80".to_string(), None))
    );
    assert_eq!(
        target("http://other:1", Some("typed")),
        Some(("http://other:1".to_string(), Some("typed".to_string())))
    );
    assert_eq!(target("http://l", Some("  ")), Some(("http://l".to_string(), None)));
    assert_eq!(target("not a url", None), None);
    assert_eq!(target("http://h:1/v1", None), None, "a Custom URL holds no /v1");
    assert_eq!(target("", None), None);
}

// VOP-FR-XJRF: a named provider answers, whichever provider is active.
#[tokio::test]
async fn voices_of_a_named_provider() {
    use crate::voice_ai::models::list_voices;
    let bench = Bench::new();
    let voices = list_voices(&bench.store, &bench.voice, Some("openai")).await.unwrap().unwrap();
    assert!(voices.iter().any(|v| v == "cedar"));
    assert_eq!(list_voices(&bench.store, &bench.voice, Some("custom")).await.unwrap(), None);
    assert_eq!(
        list_voices(&bench.store, &bench.voice, Some("other")).await.unwrap_err(),
        ERR_UNKNOWN_PROVIDER
    );
}
