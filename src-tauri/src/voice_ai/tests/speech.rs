//! Speech, on each route.

use serde_json::json;
use tokio::sync::watch;

use super::support::*;
use crate::voice_ai::pcm::{wav_of, FRAME_FILE, FRAME_PCM};
use crate::voice_ai::speech::speak_segment;
use crate::voice_ai::*;

fn active(provider: &str, base_url: &str, route: SpeechRoute) -> ActiveVoice {
    let mut record = VoiceAiRecord::seeded(descriptor(provider).unwrap());
    record.base_url = base_url.to_string();
    record.tts_model = "tts".to_string();
    record.default_voice = "default-voice".to_string();
    if provider == "openai" {
        record.default_voice = "marin".to_string();
    }
    record.speech_route = Some(route);
    ActiveVoice { record, key: None }
}

async fn speak(
    voice: &VoiceAi,
    config: &ActiveVoice,
    text: &str,
    name: Option<&str>,
) -> (Result<(), String>, Vec<Vec<u8>>) {
    let (_stop, mut cancel) = watch::channel(false);
    let mut frames = Vec::new();
    let mut emit = |frame: Vec<u8>| frames.push(frame);
    let sink = RecordingSink::default();
    let result = speak_segment(voice, config, text, name, &mut cancel, &mut emit, &sink).await;
    (result, frames)
}

// VOP-FR-UPRG, VOP-FR-TZDV, VOP-FR-XZET, VOP-FR-MOSY: the shape of an
// `openai` request, and each delta as one PCM frame at once.
#[tokio::test]
async fn openai_speech_is_a_realtime_response_out_of_band() {
    let bench = Bench::new();
    let server = fake(
        |_, _| HttpReply::status(404),
        ws_handler(|uri, mut socket| async move {
            assert!(uri.ends_with("/v1/realtime?model=tts"), "{uri}");
            send(
                &mut socket,
                json!({ "type": "session.created", "session": { "type": "realtime" } }),
            )
            .await;
            let update = recv_type(&mut socket, "session.update").await.unwrap();
            assert_eq!(update["session"]["audio"]["output"]["voice"], "cedar");
            let request = recv_type(&mut socket, "response.create").await.unwrap();
            let response = &request["response"];
            assert_eq!(response["conversation"], "none");
            assert_eq!(response["input"], json!([]));
            assert_eq!(response["output_modalities"], json!(["audio"]));
            assert!(response["instructions"].as_str().unwrap().contains("Hello there"));
            send(&mut socket, json!({ "type": "response.output_audio.delta", "delta": "AQI=" }))
                .await;
            send(&mut socket, json!({ "type": "response.output_audio.delta", "delta": "AwQ=" }))
                .await;
            send(&mut socket, json!({ "type": "response.done" })).await;
        }),
    )
    .await;
    let config = active("openai", &format!("{}/v1", server.url), SpeechRoute::Realtime);
    let (result, frames) = speak(&bench.voice, &config, "Hello there", Some("cedar")).await;
    result.unwrap();
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0], vec![FRAME_PCM, 0xC0, 0x5D, 0, 0, 1, 2]);
    assert_eq!(frames[1], vec![FRAME_PCM, 0xC0, 0x5D, 0, 0, 3, 4]);
}

// VOP-FR-TOFP, VOP-FR-INDT: llama.cpp gets its own message order, its own
// output rate, and no voice.
#[tokio::test]
async fn llama_speech_uses_a_new_session_with_its_own_order() {
    let bench = Bench::new();
    let server = fake(
        |_, _| HttpReply::status(404),
        ws_handler(|_, mut socket| async move {
            send(&mut socket, llama_created("realtime", 16_000, Some(22_050))).await;
            let update = recv(&mut socket).await.unwrap();
            assert_eq!(update["type"], "session.update");
            assert_eq!(update["session"]["output_modalities"], json!(["audio"]));
            assert!(update["session"]["audio"].is_null(), "no voice is sent");
            let item = recv(&mut socket).await.unwrap();
            assert_eq!(item["type"], "conversation.item.create");
            assert_eq!(item["item"]["content"][0]["type"], "input_text");
            assert_eq!(item["item"]["content"][0]["text"], "Read this");
            let create = recv(&mut socket).await.unwrap();
            assert_eq!(create["type"], "response.create");
            send(&mut socket, json!({ "type": "response.output_audio.delta", "delta": "AQI=" }))
                .await;
            send(&mut socket, json!({ "type": "response.done" })).await;
        }),
    )
    .await;
    let config = active("custom", &server.url, SpeechRoute::Realtime);
    let (result, frames) = speak(&bench.voice, &config, "Read this", Some("ignored")).await;
    result.unwrap();
    let rate = u32::from_le_bytes([frames[0][1], frames[0][2], frames[0][3], frames[0][4]]);
    assert_eq!(rate, 22_050);
}

// VOP-FR-USQZ: a server that reports no output rate cannot speak.
#[tokio::test]
async fn a_server_with_no_output_rate_cannot_speak() {
    let bench = Bench::new();
    let server = fake(
        |_, _| HttpReply::status(404),
        ws_handler(|_, mut socket| async move {
            send(&mut socket, llama_created("realtime", 16_000, None)).await;
            recv(&mut socket).await;
        }),
    )
    .await;
    let config = active("custom", &server.url, SpeechRoute::Realtime);
    assert_eq!(speak(&bench.voice, &config, "x", None).await.0.unwrap_err(), ERR_SPEECH_FAILED);
}

// VOP-FR-QTZJ: `mp3` first.
#[tokio::test]
async fn openrouter_speaks_mp3() {
    let bench = Bench::new();
    let server = fake(|_, _| HttpReply::audio("audio/mpeg", vec![0xFF, 0xFB]), None).await;
    let config = active("openrouter", &server.url, SpeechRoute::OpenaiSpeech);
    let (result, frames) = speak(&bench.voice, &config, "x", None).await;
    result.unwrap();
    assert_eq!(frames, vec![vec![FRAME_FILE, 0, 0, 0, 0, 0xFF, 0xFB]]);
    let seen = server.http.lock().unwrap().clone();
    let body: serde_json::Value = serde_json::from_slice(&seen[0].body).unwrap();
    assert_eq!(body["response_format"], "mp3");
    assert_eq!(seen[0].path, "/audio/speech");
}

// VOP-FR-QTZJ: a model that refuses `mp3` gets `pcm`, at the rate of the
// `Content-Type`, or else at 24 kHz.
#[tokio::test]
async fn openrouter_falls_back_to_pcm() {
    for (content_type, rate) in [("audio/pcm;rate=48000", 48_000u32), ("audio/pcm", 24_000)] {
        let bench = Bench::new();
        let content_type = content_type.to_string();
        let server = fake(
            move |_, body| {
                let body: serde_json::Value = serde_json::from_slice(body).unwrap();
                if body["response_format"] == "mp3" {
                    HttpReply::status(400)
                } else {
                    HttpReply::audio(&content_type, vec![1, 2])
                }
            },
            None,
        )
        .await;
        let config = active("openrouter", &server.url, SpeechRoute::OpenaiSpeech);
        let (result, frames) = speak(&bench.voice, &config, "x", None).await;
        result.unwrap();
        assert_eq!(frames[0][0], FRAME_PCM);
        assert_eq!(
            u32::from_le_bytes([frames[0][1], frames[0][2], frames[0][3], frames[0][4]]),
            rate
        );
    }
}

// VOP-FR-TSVV: `wav` from Ollama, with its sizes rewritten.
#[tokio::test]
async fn custom_speech_rewrites_the_wav_sizes() {
    let bench = Bench::new();
    let server = fake(
        |_, _| {
            let mut wav = wav_of(&[1i16; 8], 24_000);
            wav[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
            wav[40..44].copy_from_slice(&u32::MAX.to_le_bytes());
            HttpReply::audio("audio/wav", wav)
        },
        None,
    )
    .await;
    let config = active("custom", &server.url, SpeechRoute::OpenaiSpeech);
    let (result, frames) = speak(&bench.voice, &config, "x", None).await;
    result.unwrap();
    assert_eq!(frames[0][0], FRAME_FILE);
    let wav = &frames[0][5..];
    assert_eq!(u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]), 16);
    let seen = server.http.lock().unwrap().clone();
    assert_eq!(seen[0].path, "/v1/audio/speech");
    let body: serde_json::Value = serde_json::from_slice(&seen[0].body).unwrap();
    assert_eq!(body["response_format"], "wav");
}

// VOP-FR-TSVV: `llama_tts` posts `input` to `/tts`.
#[tokio::test]
async fn llama_tts_posts_to_tts() {
    let bench = Bench::new();
    let server = fake(|_, _| HttpReply::audio("audio/wav", wav_of(&[1i16; 2], 24_000)), None).await;
    let config = active("custom", &server.url, SpeechRoute::LlamaTts);
    let (result, _) = speak(&bench.voice, &config, "say it", Some("female")).await;
    result.unwrap();
    let seen = server.http.lock().unwrap().clone();
    assert_eq!(seen[0].path, "/tts");
    let body: serde_json::Value = serde_json::from_slice(&seen[0].body).unwrap();
    assert_eq!(body["input"], "say it");
    assert_eq!(body["voice"], "female");
}

// VOP-FR-RABZ: a voice that `openai` does not offer becomes the default voice.
#[tokio::test]
async fn an_unknown_voice_becomes_the_default_voice() {
    let bench = Bench::new();
    let server = fake(
        |_, _| HttpReply::status(404),
        ws_handler(|_, mut socket| async move {
            send(&mut socket, json!({ "type": "session.created", "session": {} })).await;
            let update = recv_type(&mut socket, "session.update").await.unwrap();
            assert_eq!(update["session"]["audio"]["output"]["voice"], "marin");
            send(&mut socket, json!({ "type": "response.done" })).await;
        }),
    )
    .await;
    let config = active("openai", &format!("{}/v1", server.url), SpeechRoute::Realtime);
    speak(&bench.voice, &config, "x", Some("george")).await.0.unwrap();
}

// VOP-FR-RABZ: on `custom`, a 400 repeats once with the default voice.
#[tokio::test]
async fn custom_repeats_once_with_the_default_voice() {
    let bench = Bench::new();
    let server = fake(
        |_, body| {
            let body: serde_json::Value = serde_json::from_slice(body).unwrap();
            if body["voice"] == "default-voice" {
                HttpReply::audio("audio/wav", wav_of(&[1i16; 2], 24_000))
            } else {
                HttpReply::status(400)
            }
        },
        None,
    )
    .await;
    let config = active("custom", &server.url, SpeechRoute::OpenaiSpeech);
    speak(&bench.voice, &config, "x", Some("unknown")).await.0.unwrap();
    assert_eq!(server.http.lock().unwrap().len(), 2);
}

// VOP-FR-RABZ: a voice that OpenRouter's model does not offer becomes the default.
#[tokio::test]
async fn openrouter_checks_the_voices_of_its_model() {
    let bench = Bench::new();
    let server = fake(
        |path, _| match path {
            "/models?output_modalities=speech" => {
                HttpReply::json(r#"{"data":[{"id":"tts","supported_voices":["sarah"]}]}"#)
            }
            _ => HttpReply::audio("audio/mpeg", vec![1]),
        },
        None,
    )
    .await;
    let config = active("openrouter", &server.url, SpeechRoute::OpenaiSpeech);
    speak(&bench.voice, &config, "x", Some("george")).await.0.unwrap();
    speak(&bench.voice, &config, "y", Some("sarah")).await.0.unwrap();
    let seen = server.http.lock().unwrap().clone();
    let speech: Vec<serde_json::Value> = seen
        .iter()
        .filter(|h| h.path == "/audio/speech")
        .map(|h| serde_json::from_slice(&h.body).unwrap())
        .collect();
    assert_eq!(speech[0]["voice"], "default-voice");
    assert_eq!(speech[1]["voice"], "sarah");
    assert_eq!(
        seen.iter().filter(|h| h.path.starts_with("/models")).count(),
        1,
        "one catalogue call"
    );
}

// VOP-FR-MOSY: the segment limit counts characters, not bytes.
#[tokio::test]
async fn a_segment_over_the_limit_is_refused() {
    let bench = Bench::new();
    let config = active("custom", "http://127.0.0.1:1", SpeechRoute::OpenaiSpeech);
    let text = "a".repeat(4097);
    assert_eq!(
        speak(&bench.voice, &config, &text, None).await.0.unwrap_err(),
        ERR_SEGMENT_TOO_LONG
    );
    let wide = "é".repeat(4096);
    assert_ne!(
        speak(&bench.voice, &config, &wide, None).await.0.unwrap_err(),
        ERR_SEGMENT_TOO_LONG
    );
}

// VOP-FR-GQJS: each failure has its code.
#[tokio::test]
async fn a_failed_segment_has_its_code() {
    let bench = Bench::new();
    let unreachable = active("custom", "http://127.0.0.1:1", SpeechRoute::OpenaiSpeech);
    assert_eq!(
        speak(&bench.voice, &unreachable, "x", None).await.0.unwrap_err(),
        ERR_PROVIDER_UNREACHABLE
    );
    let server =
        fake(|path, _| HttpReply::status(if path == "/tts" { 401 } else { 500 }), None).await;
    let refused = active("custom", &server.url, SpeechRoute::LlamaTts);
    assert_eq!(
        speak(&bench.voice, &refused, "x", None).await.0.unwrap_err(),
        ERR_PROVIDER_REJECTED
    );
    let failed = active("custom", &server.url, SpeechRoute::OpenaiSpeech);
    assert_eq!(speak(&bench.voice, &failed, "x", None).await.0.unwrap_err(), ERR_SPEECH_FAILED);
    let error_event = fake(
        |_, _| HttpReply::status(404),
        ws_handler(|_, mut socket| async move {
            send(&mut socket, llama_created("realtime", 16_000, Some(24_000))).await;
            recv_type(&mut socket, "response.create").await;
            send(&mut socket, json!({ "type": "error", "error": { "message": "no" } })).await;
            recv(&mut socket).await;
        }),
    )
    .await;
    let realtime = active("custom", &error_event.url, SpeechRoute::Realtime);
    assert_eq!(speak(&bench.voice, &realtime, "x", None).await.0.unwrap_err(), ERR_SPEECH_FAILED);
}

// VOP-FR-DWSB: a cancel sends `response.cancel` and sends no more frames.
#[tokio::test]
async fn a_cancel_stops_the_frames() {
    let bench = Bench::new();
    let server = fake(
        |_, _| HttpReply::status(404),
        ws_handler(|_, mut socket| async move {
            send(&mut socket, llama_created("realtime", 16_000, Some(24_000))).await;
            recv_type(&mut socket, "response.create").await;
            send(&mut socket, json!({ "type": "response.output_audio.delta", "delta": "AQI=" }))
                .await;
            assert!(recv_type(&mut socket, "response.cancel").await.is_some());
            send(&mut socket, json!({ "type": "response.output_audio.delta", "delta": "AwQ=" }))
                .await;
        }),
    )
    .await;
    let voice = &bench.voice;
    let mut cancel = voice.register_speech("s");
    let config = active("custom", &server.url, SpeechRoute::Realtime);
    let mut frames = 0;
    let mut emit = |_: Vec<u8>| {
        frames += 1;
        voice.cancel_speech("s");
    };
    speak_segment(voice, &config, "x", None, &mut cancel, &mut emit, &RecordingSink::default())
        .await
        .unwrap();
    voice.release_speech("s");
    assert_eq!(frames, 1);
    assert!(voice.speeches.lock().unwrap().is_empty());
}

// VOP-FR-DWSB: a cancel aborts an HTTP request that is in flight, and its audio
// is not sent.
#[tokio::test]
async fn a_cancel_aborts_an_http_request() {
    let bench = Bench::new();
    let (seen_tx, seen_rx) = tokio::sync::oneshot::channel::<()>();
    let seen_tx = std::sync::Mutex::new(Some(seen_tx));
    let (gate_tx, gate_rx) = std::sync::mpsc::channel::<()>();
    let gate_rx = std::sync::Mutex::new(gate_rx);
    let server = fake(
        move |_, _| {
            if let Some(seen) = seen_tx.lock().unwrap().take() {
                let _ = seen.send(());
            }
            let _ = gate_rx.lock().unwrap().recv();
            HttpReply::audio("audio/wav", wav_of(&[1i16; 2], 24_000))
        },
        None,
    )
    .await;
    let config = active("custom", &server.url, SpeechRoute::OpenaiSpeech);
    let (stop, mut cancel) = watch::channel(false);
    let mut frames = 0;
    let mut emit = |_: Vec<u8>| frames += 1;
    let sink = RecordingSink::default();
    let speaking = speak_segment(&bench.voice, &config, "x", None, &mut cancel, &mut emit, &sink);
    let stopper = async {
        seen_rx.await.unwrap();
        stop.send(true).unwrap();
    };
    let (result, ()) = tokio::join!(speaking, stopper);
    result.unwrap();
    assert_eq!(frames, 0);
    let _ = gate_tx.send(());
}

// VOP-FR-DWSB: a cancel goes first, so a burst of audio cannot hide it.
#[tokio::test]
async fn a_cancel_is_not_hidden_by_a_burst_of_audio() {
    let bench = Bench::new();
    let server = fake(
        |_, _| HttpReply::status(404),
        ws_handler(|_, mut socket| async move {
            send(&mut socket, llama_created("realtime", 16_000, Some(24_000))).await;
            recv_type(&mut socket, "response.create").await;
            for _ in 0..200 {
                send(
                    &mut socket,
                    json!({ "type": "response.output_audio.delta", "delta": "AQI=" }),
                )
                .await;
            }
            recv_type(&mut socket, "response.cancel").await;
        }),
    )
    .await;
    let config = active("custom", &server.url, SpeechRoute::Realtime);
    let voice = &bench.voice;
    let mut cancel = voice.register_speech("s");
    let mut frames = 0;
    let mut emit = |_: Vec<u8>| {
        frames += 1;
        voice.cancel_speech("s");
    };
    let sink = RecordingSink::default();
    speak_segment(voice, &config, "x", None, &mut cancel, &mut emit, &sink).await.unwrap();
    assert_eq!(frames, 1, "the burst stops at the cancel");
}

// VOP-FR-DWSB: a cancel between two segments of one reply stops the next one,
// and a later reply is not affected.
#[tokio::test]
async fn a_cancel_between_segments_stops_the_next_segment() {
    let bench = Bench::new();
    let voice = &bench.voice;
    voice.cancel_speech("reply-1");
    let cancelled = voice.register_speech("reply-1");
    assert!(*cancelled.borrow());
    voice.release_speech("reply-1");
    assert!(!*voice.register_speech("reply-2").borrow());

    let server = fake(|_, _| HttpReply::audio("audio/wav", wav_of(&[1i16; 2], 24_000)), None).await;
    let config = active("custom", &server.url, SpeechRoute::OpenaiSpeech);
    let mut cancel = voice.register_speech("reply-1");
    let mut frames = 0;
    let mut emit = |_: Vec<u8>| frames += 1;
    let sink = RecordingSink::default();
    speak_segment(voice, &config, "x", None, &mut cancel, &mut emit, &sink).await.unwrap();
    assert_eq!(frames, 0);
    assert!(server.http.lock().unwrap().is_empty(), "no request goes out");
}

// VOP-FR-QTZJ: only a 400 tries `pcm`. A 500 is a failure.
#[tokio::test]
async fn openrouter_retries_only_a_400() {
    let bench = Bench::new();
    let server = fake(|_, _| HttpReply::status(500), None).await;
    let config = active("openrouter", &server.url, SpeechRoute::OpenaiSpeech);
    assert_eq!(speak(&bench.voice, &config, "x", None).await.0.unwrap_err(), ERR_SPEECH_FAILED);
    assert_eq!(server.http.lock().unwrap().len(), 1);
}

// VOP-FR-RABZ: on `custom`, only a 400 repeats with the default voice.
#[tokio::test]
async fn custom_repeats_only_a_400() {
    let bench = Bench::new();
    let server = fake(|_, _| HttpReply::status(500), None).await;
    let config = active("custom", &server.url, SpeechRoute::OpenaiSpeech);
    assert_eq!(
        speak(&bench.voice, &config, "x", Some("other")).await.0.unwrap_err(),
        ERR_SPEECH_FAILED
    );
    assert_eq!(server.http.lock().unwrap().len(), 1);
}

// VOP-FR-UPRG: each segment opens its own session with its voice.
#[tokio::test]
async fn each_openai_segment_opens_its_own_session() {
    let bench = Bench::new();
    let server = fake(
        |_, _| HttpReply::status(404),
        ws_handler(|_, mut socket| async move {
            send(&mut socket, json!({ "type": "session.created", "session": {} })).await;
            recv_type(&mut socket, "response.create").await;
            send(&mut socket, json!({ "type": "response.done" })).await;
        }),
    )
    .await;
    let config = active("openai", &format!("{}/v1", server.url), SpeechRoute::Realtime);
    speak(&bench.voice, &config, "one", Some("marin")).await.0.unwrap();
    speak(&bench.voice, &config, "two", Some("cedar")).await.0.unwrap();
    assert_eq!(server.ws.lock().unwrap().len(), 2);
}
