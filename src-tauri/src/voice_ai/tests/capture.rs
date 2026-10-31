//! Capture sessions, on both routes.

use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine;
use serde_json::json;

use super::support::*;
use crate::voice_ai::capture::WATCHDOG_LIMIT;
use crate::voice_ai::*;

fn config(
    provider: &str,
    base_url: &str,
    route: TranscriptionRoute,
    http: bool,
    key: Option<&str>,
) -> ActiveVoice {
    let mut record = VoiceAiRecord::seeded(descriptor(provider).unwrap());
    record.base_url = base_url.to_string();
    record.stt_model = "stt".to_string();
    record.transcription_route = route;
    record.capabilities = Some(VoiceCapabilities {
        realtime_transcription: (route == TranscriptionRoute::Realtime)
            .then_some(RealtimeTranscription::Live),
        http_transcription: http,
        ..VoiceCapabilities::default()
    });
    ActiveVoice { record, key: key.map(str::to_string) }
}

fn pcm_of(event: &serde_json::Value) -> usize {
    base64::engine::general_purpose::STANDARD
        .decode(event["audio"].as_str().unwrap())
        .unwrap()
        .len()
}

// VOP-FR-GYKP, VOP-FR-JAOB, VOP-FR-VZDI, VOP-FR-GZOC, VOP-FR-CSWH,
// VOP-FR-HOGR: a whole live session. Audio sent before the socket is ready
// reaches the server, at the session's rate.
#[tokio::test]
async fn a_live_session_streams_audio_and_text() {
    let bench = Bench::new();
    let received = Arc::new(std::sync::Mutex::new(0usize));
    let total = received.clone();
    let server = fake(
        |_, _| HttpReply::status(404),
        ws_handler(move |_, mut socket| {
            let total = total.clone();
            async move {
                send(&mut socket, llama_created("transcription", 16_000, None)).await;
                send(
                    &mut socket,
                    json!({
                        "type": "conversation.item.input_audio_transcription.delta",
                        "item_id": "a", "delta": "hello ",
                    }),
                )
                .await;
                send(
                    &mut socket,
                    json!({
                        "type": "conversation.item.input_audio_transcription.delta",
                        "item_id": "a", "delta": "there",
                    }),
                )
                .await;
                loop {
                    let event = recv(&mut socket).await.unwrap();
                    match event["type"].as_str().unwrap() {
                        "input_audio_buffer.append" => *total.lock().unwrap() += pcm_of(&event),
                        "input_audio_buffer.commit" => {
                            assert_eq!(event["final"], true);
                            break;
                        }
                        other => panic!("unexpected {other}"),
                    }
                }
                send(
                    &mut socket,
                    json!({
                        "type": "conversation.item.input_audio_transcription.completed",
                        "item_id": "a", "transcript": "Hello there.",
                    }),
                )
                .await;
            }
        }),
    )
    .await;
    let sink = Arc::new(RecordingSink::default());
    let voice = &bench.voice;
    let start = voice
        .start_capture(
            config("custom", &server.url, TranscriptionRoute::Realtime, false, None),
            48_000,
            vec![],
            sink.clone(),
        )
        .unwrap();
    assert!(start.live);
    assert_eq!(start.route, TranscriptionRoute::Realtime);
    voice.append_capture(&start.session_id, vec![0; 48_000]).unwrap();
    let text = voice.finish_capture(&start.session_id).await.unwrap();
    assert_eq!(text, "Hello there.");
    let shown = sink.transcripts.lock().unwrap().clone();
    assert_eq!(shown.first().map(String::as_str), Some("hello"));
    assert_eq!(shown.last().map(String::as_str), Some("Hello there."));
    let bytes = *received.lock().unwrap();
    assert!((bytes as i64 - 32_000).abs() < 2_100, "one second at 16 kHz, got {bytes} bytes");
    assert!(voice.capture.lock().unwrap().is_none(), "a success ends the session");
}

// VOP-FR-XXAB, VOP-FR-HOGR: `openai` waits for `completed` of the item that
// the commit named, and a late item joins in commit order.
#[tokio::test]
async fn an_openai_session_ends_with_its_committed_item() {
    let bench = Bench::new();
    let server = fake(
        |_, _| HttpReply::status(404),
        ws_handler(|_, mut socket| async move {
            send(
                &mut socket,
                json!({ "type": "session.created", "session": { "type": "transcription" } }),
            )
            .await;
            let commit = recv_type(&mut socket, "input_audio_buffer.commit").await.unwrap();
            assert!(commit.get("final").is_none());
            send(&mut socket, json!({
                "type": "input_audio_buffer.committed", "item_id": "b", "previous_item_id": null,
            }))
            .await;
            send(
                &mut socket,
                json!({
                    "type": "conversation.item.input_audio_transcription.completed",
                    "item_id": "b", "transcript": "Done.",
                }),
            )
            .await;
        }),
    )
    .await;
    let base = format!("{}/v1", server.url);
    let start = bench
        .voice
        .start_capture(
            config("openai", &base, TranscriptionRoute::Realtime, false, Some("k")),
            24_000,
            vec![],
            Arc::new(RecordingSink::default()),
        )
        .unwrap();
    bench.voice.append_capture(&start.session_id, vec![0; 2_400]).unwrap();
    assert_eq!(bench.voice.finish_capture(&start.session_id).await.unwrap(), "Done.");
}

// VOP-FR-VZDI: a commit with no audio is an empty transcript, not a failure.
#[tokio::test]
async fn a_commit_with_no_audio_is_an_empty_transcript() {
    let bench = Bench::new();
    let server = fake(
        |_, _| HttpReply::status(404),
        ws_handler(|_, mut socket| async move {
            send(&mut socket, llama_created("transcription", 16_000, None)).await;
            recv_type(&mut socket, "input_audio_buffer.commit").await;
            send(
                &mut socket,
                json!({ "type": "error", "error": {
                    "code": "input_audio_buffer_commit_empty", "message": "the buffer is empty",
                }}),
            )
            .await;
            recv(&mut socket).await;
        }),
    )
    .await;
    let start = bench
        .voice
        .start_capture(
            config("custom", &server.url, TranscriptionRoute::Realtime, false, None),
            16_000,
            vec![],
            Arc::new(RecordingSink::default()),
        )
        .unwrap();
    assert_eq!(bench.voice.finish_capture(&start.session_id).await.unwrap(), "");
}

// VOP-FR-HOGR: after the final commit, a clean close ends the session.
#[tokio::test]
async fn a_clean_close_after_the_final_commit_ends_the_session() {
    let bench = Bench::new();
    let server = fake(
        |_, _| HttpReply::status(404),
        ws_handler(|_, mut socket| async move {
            send(&mut socket, llama_created("transcription", 16_000, None)).await;
            send(
                &mut socket,
                json!({
                    "type": "conversation.item.input_audio_transcription.delta",
                    "item_id": "a", "delta": "partial",
                }),
            )
            .await;
            recv_type(&mut socket, "input_audio_buffer.commit").await;
            let _ = socket.close(None).await;
        }),
    )
    .await;
    let start = bench
        .voice
        .start_capture(
            config("custom", &server.url, TranscriptionRoute::Realtime, false, None),
            16_000,
            vec![],
            Arc::new(RecordingSink::default()),
        )
        .unwrap();
    assert_eq!(bench.voice.finish_capture(&start.session_id).await.unwrap(), "partial");
}

// VOP-FR-EYHP, VOP-FR-INCC: the multipart request of the HTTP route, at 16 kHz,
// with the model of that route.
#[tokio::test]
async fn the_http_route_sends_one_wav() {
    let bench = Bench::new();
    let server = fake(
        |path, _| {
            if path == "/v1/audio/transcriptions" {
                transcribed(" It works. ")
            } else {
                HttpReply::status(404)
            }
        },
        None,
    )
    .await;
    let mut active =
        config("openai", &format!("{}/v1", server.url), TranscriptionRoute::Http, true, Some("k"));
    active.record.stt_model = "gpt-live-transcribe".into();
    let start = bench
        .voice
        .start_capture(active, 48_000, vec!["arch".into()], Arc::new(RecordingSink::default()))
        .unwrap();
    assert!(!start.live);
    bench.voice.append_capture(&start.session_id, vec![100; 48_000]).unwrap();
    assert_eq!(bench.voice.finish_capture(&start.session_id).await.unwrap(), "It works.");
    let seen = server.http.lock().unwrap().clone();
    let body = String::from_utf8_lossy(&seen[0].body);
    assert!(body.contains("name=\"response_format\""));
    assert!(body.contains("json"));
    assert!(body.contains("gpt-transcribe"), "the HTTP model");
    assert!(!body.contains("gpt-live-transcribe"));
    assert!(body.contains("name=\"prompt\""));
    assert!(body.contains("arch"));
    assert!(body.contains("audio/wav"));
    assert!(body.contains("RIFF"));
    assert!(seen[0].body.len() > 32_000 && seen[0].body.len() < 34_000, "{}", seen[0].body.len());
}

// VOP-FR-INCC: OpenRouter has `keyterms`, Custom has none, and a nickname that a
// provider refuses is left out.
#[test]
fn hints_follow_the_provider() {
    use crate::voice_ai::http::hint_fields;
    let terms = vec!["arch".to_string()];
    assert_eq!(hint_fields("openrouter", &terms), vec![("keyterms[]", "arch".to_string())]);
    assert!(hint_fields("custom", &terms).is_empty());
    assert_eq!(hint_fields("openai", &terms), vec![("prompt", "arch".to_string())]);
}

// VOP-FR-INCC: the Realtime route of `openai` sends `keywords`, with no nickname
// that the provider refuses.
#[tokio::test]
async fn openai_realtime_sends_usable_keywords() {
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
            assert_eq!(
                update["session"]["audio"]["input"]["transcription"]["keywords"],
                json!(["arch"])
            );
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
    let start = bench
        .voice
        .start_capture(
            config(
                "openai",
                &format!("{}/v1", server.url),
                TranscriptionRoute::Realtime,
                false,
                Some("k"),
            ),
            24_000,
            vec!["arch".into(), "a<b".into(), "c\nd".into(), " ".into()],
            Arc::new(RecordingSink::default()),
        )
        .unwrap();
    assert_eq!(bench.voice.finish_capture(&start.session_id).await.unwrap(), "ok");
}

// VOP-FR-KBPZ, VOP-FR-YEFK: a second start is refused, and a start after a
// failure replaces the failed session.
#[tokio::test]
async fn one_session_at_a_time() {
    let bench = Bench::new();
    let server = fake(|_, _| HttpReply::status(500), None).await;
    let active = || config("custom", &server.url, TranscriptionRoute::Http, true, None);
    let sink = || -> SharedSinkAlias { Arc::new(RecordingSink::default()) };
    let start = bench.voice.start_capture(active(), 16_000, vec![], sink()).unwrap();
    assert_eq!(
        bench.voice.start_capture(active(), 16_000, vec![], sink()).unwrap_err(),
        ERR_SESSION_ACTIVE
    );
    bench.voice.append_capture(&start.session_id, vec![1; 100]).unwrap();
    assert_eq!(
        bench.voice.finish_capture(&start.session_id).await.unwrap_err(),
        ERR_TRANSCRIPTION_FAILED
    );
    let next = bench.voice.start_capture(active(), 16_000, vec![], sink()).unwrap();
    assert_ne!(next.session_id, start.session_id);
}

type SharedSinkAlias = Arc<dyn VoiceSink>;

// VOP-FR-BTDW: a change of the record does not reach an open session.
#[tokio::test]
async fn a_session_keeps_its_configuration() {
    let bench = Bench::new();
    let server = fake(
        |path, _| {
            if path.ends_with("/audio/transcriptions") {
                transcribed("kept")
            } else {
                HttpReply::status(404)
            }
        },
        None,
    )
    .await;
    let mut held = config("custom", &server.url, TranscriptionRoute::Http, true, None);
    held.record.verified_at = Some("t".into());
    bench.store.save_voice_ai_registry(vec![held.record.clone()], Some("custom".into())).unwrap();
    let active = crate::voice_ai::resolve_active_voice(&bench.store, &bench.voice).unwrap();
    let start = bench
        .voice
        .start_capture(active, 16_000, vec![], Arc::new(RecordingSink::default()))
        .unwrap();
    held.record.base_url = "http://127.0.0.1:1".into();
    bench.store.save_voice_ai_registry(vec![held.record], Some("custom".into())).unwrap();
    assert_eq!(bench.voice.finish_capture(&start.session_id).await.unwrap(), "kept");
}

// VOP-FR-UOYP, VOP-FR-IAFL, VOP-FR-YEFK: a failed recording stays, and a
// retry on the same route sends the same audio.
#[tokio::test]
async fn a_failed_recording_retries_with_the_same_audio() {
    let bench = Bench::new();
    let calls = Arc::new(std::sync::Mutex::new(0));
    let count = calls.clone();
    let server = fake(
        move |_, _| {
            let mut calls = count.lock().unwrap();
            *calls += 1;
            if *calls == 1 {
                HttpReply::status(401)
            } else {
                transcribed("second try")
            }
        },
        None,
    )
    .await;
    let start = bench
        .voice
        .start_capture(
            config("custom", &server.url, TranscriptionRoute::Http, true, None),
            16_000,
            vec![],
            Arc::new(RecordingSink::default()),
        )
        .unwrap();
    bench.voice.append_capture(&start.session_id, vec![5; 800]).unwrap();
    assert_eq!(
        bench.voice.finish_capture(&start.session_id).await.unwrap_err(),
        ERR_PROVIDER_REJECTED
    );
    let text = bench
        .voice
        .retry_capture(&start.session_id, Arc::new(RecordingSink::default()))
        .await
        .unwrap();
    assert_eq!(text, "second try");
    let seen = server.http.lock().unwrap().clone();
    let audio = |body: &[u8]| {
        let at = body.windows(4).position(|w| w == b"RIFF").unwrap();
        body[at..at + 44 + 800 * 2].to_vec()
    };
    assert_eq!(audio(&seen[0].body), audio(&seen[1].body), "the same audio");
    assert!(bench.voice.capture.lock().unwrap().is_none());
}

// VOP-FR-UOYP: an unreachable provider has its own code.
#[tokio::test]
async fn an_unreachable_provider_has_its_code() {
    let bench = Bench::new();
    let start = bench
        .voice
        .start_capture(
            config("custom", "http://127.0.0.1:1", TranscriptionRoute::Http, true, None),
            16_000,
            vec![],
            Arc::new(RecordingSink::default()),
        )
        .unwrap();
    assert_eq!(
        bench.voice.finish_capture(&start.session_id).await.unwrap_err(),
        ERR_PROVIDER_UNREACHABLE
    );
}

// VOP-FR-IAFL, VOP-FR-UOYP: a socket that drops fails the finish, and the
// retry takes the HTTP route with the whole recording.
#[tokio::test]
async fn a_dropped_socket_retries_on_http() {
    let bench = Bench::new();
    let server = fake(
        |path, _| {
            if path.ends_with("/audio/transcriptions") {
                transcribed("from http")
            } else {
                HttpReply::status(404)
            }
        },
        ws_handler(|_, mut socket| async move {
            send(&mut socket, llama_created("transcription", 16_000, None)).await;
            recv_type(&mut socket, "input_audio_buffer.append").await;
            // The task ends and the socket drops with no close frame.
            drop(socket);
        }),
    )
    .await;
    let start = bench
        .voice
        .start_capture(
            config("custom", &server.url, TranscriptionRoute::Realtime, true, None),
            16_000,
            vec![],
            Arc::new(RecordingSink::default()),
        )
        .unwrap();
    bench.voice.append_capture(&start.session_id, vec![3; 16_000]).unwrap();
    let error = bench.voice.finish_capture(&start.session_id).await.unwrap_err();
    assert_eq!(error, ERR_SESSION_LOST);
    let text = bench
        .voice
        .retry_capture(&start.session_id, Arc::new(RecordingSink::default()))
        .await
        .unwrap();
    assert_eq!(text, "from http");
}

// VOP-FR-IAFL: with no HTTP route, a retry replays the audio on a new socket.
#[tokio::test]
async fn a_failed_socket_with_no_http_route_replays_on_realtime() {
    let bench = Bench::new();
    let opened = Arc::new(std::sync::Mutex::new(0));
    let count = opened.clone();
    let server = fake(
        |_, _| HttpReply::status(404),
        ws_handler(move |_, mut socket| {
            let count = count.clone();
            async move {
                let attempt = {
                    let mut opened = count.lock().unwrap();
                    *opened += 1;
                    *opened
                };
                send(&mut socket, llama_created("transcription", 16_000, None)).await;
                if attempt == 1 {
                    return;
                }
                let audio = recv_type(&mut socket, "input_audio_buffer.append").await.unwrap();
                assert_eq!(pcm_of(&audio), 2_000);
                recv_type(&mut socket, "input_audio_buffer.commit").await;
                send(
                    &mut socket,
                    json!({
                        "type": "conversation.item.input_audio_transcription.completed",
                        "item_id": "a", "transcript": "replayed",
                    }),
                )
                .await;
            }
        }),
    )
    .await;
    let start = bench
        .voice
        .start_capture(
            config("custom", &server.url, TranscriptionRoute::Realtime, false, None),
            16_000,
            vec![],
            Arc::new(RecordingSink::default()),
        )
        .unwrap();
    bench.voice.append_capture(&start.session_id, vec![3; 1_000]).unwrap();
    assert!(bench.voice.finish_capture(&start.session_id).await.is_err());
    let text = bench
        .voice
        .retry_capture(&start.session_id, Arc::new(RecordingSink::default()))
        .await
        .unwrap();
    assert_eq!(text, "replayed");
}

// VOP-FR-YEFK: a cancel drops the audio and closes the socket.
#[tokio::test]
async fn a_cancel_closes_the_socket() {
    let bench = Bench::new();
    let (closed, wait_closed) = tokio::sync::oneshot::channel();
    let closed = Arc::new(std::sync::Mutex::new(Some(closed)));
    let server = fake(
        |_, _| HttpReply::status(404),
        ws_handler(move |_, mut socket| {
            let closed = closed.clone();
            async move {
                send(&mut socket, llama_created("transcription", 16_000, None)).await;
                while recv(&mut socket).await.is_some() {}
                if let Some(closed) = closed.lock().unwrap().take() {
                    let _ = closed.send(());
                }
            }
        }),
    )
    .await;
    let start = bench
        .voice
        .start_capture(
            config("custom", &server.url, TranscriptionRoute::Realtime, false, None),
            16_000,
            vec![],
            Arc::new(RecordingSink::default()),
        )
        .unwrap();
    bench.voice.cancel_capture(&start.session_id).unwrap();
    assert!(bench.voice.capture.lock().unwrap().is_none());
    tokio::time::timeout(Duration::from_secs(30), wait_closed).await.unwrap().unwrap();
    assert_eq!(
        bench.voice.append_capture(&start.session_id, vec![1]).unwrap_err(),
        ERR_SESSION_LOST
    );
}

// VOP-FR-BSNY: a recording with no audio for 30 s is cancelled. A failed
// session waits for its retry and is not swept.
#[tokio::test]
async fn the_watchdog_sweeps_only_idle_recordings() {
    let bench = Bench::new();
    let server = fake(|_, _| HttpReply::status(500), None).await;
    let start = bench
        .voice
        .start_capture(
            config("custom", &server.url, TranscriptionRoute::Http, true, None),
            16_000,
            vec![],
            Arc::new(RecordingSink::default()),
        )
        .unwrap();
    let now = Instant::now();
    assert!(!bench.voice.sweep_idle(now).unwrap());
    let late = now + WATCHDOG_LIMIT + Duration::from_secs(1);
    let _ = bench.voice.finish_capture(&start.session_id).await;
    assert!(!bench.voice.sweep_idle(late).unwrap(), "a failed session stays");

    let next = bench
        .voice
        .start_capture(
            config("custom", &server.url, TranscriptionRoute::Http, true, None),
            16_000,
            vec![],
            Arc::new(RecordingSink::default()),
        )
        .unwrap();
    assert!(bench
        .voice
        .sweep_idle(Instant::now() + WATCHDOG_LIMIT + Duration::from_secs(1))
        .unwrap());
    assert!(bench.voice.append_capture(&next.session_id, vec![1]).is_err());
}

// VOP-FR-DXPK: a session id that is not open is refused.
#[test]
fn audio_for_an_unknown_session_is_refused() {
    let bench = Bench::new();
    assert_eq!(bench.voice.append_capture("none", vec![1]).unwrap_err(), ERR_SESSION_LOST);
}

// VOP-FR-QQYG, VOP-FR-XXAB: after the final commit, llama.cpp sends no `committed`
// and can complete an item before another one. The session ends at the close.
#[tokio::test]
async fn a_llama_session_ends_at_the_close_and_keeps_late_items() {
    let bench = Bench::new();
    let server = fake(
        |_, _| HttpReply::status(404),
        ws_handler(|_, mut socket| async move {
            send(&mut socket, llama_created("transcription", 16_000, None)).await;
            recv_type(&mut socket, "input_audio_buffer.commit").await;
            send(
                &mut socket,
                json!({
                    "type": "conversation.item.input_audio_transcription.completed",
                    "item_id": "a", "transcript": "First line.",
                }),
            )
            .await;
            send(
                &mut socket,
                json!({
                    "type": "conversation.item.input_audio_transcription.delta",
                    "item_id": "b", "delta": "Second",
                }),
            )
            .await;
            send(
                &mut socket,
                json!({
                    "type": "conversation.item.input_audio_transcription.completed",
                    "item_id": "b", "transcript": "Second line.",
                }),
            )
            .await;
            let _ = socket.close(None).await;
        }),
    )
    .await;
    let start = bench
        .voice
        .start_capture(
            config("custom", &server.url, TranscriptionRoute::Realtime, false, None),
            16_000,
            vec![],
            Arc::new(RecordingSink::default()),
        )
        .unwrap();
    assert_eq!(
        bench.voice.finish_capture(&start.session_id).await.unwrap(),
        "First line.\nSecond line."
    );
}

// VOP-FR-QQYG: a turn-based session does not close, so it ends with its items.
#[tokio::test]
async fn a_turn_based_session_ends_with_its_items_and_is_not_live() {
    let bench = Bench::new();
    let server = fake(
        |_, _| HttpReply::status(404),
        ws_handler(|_, mut socket| async move {
            send(&mut socket, llama_created("realtime", 16_000, None)).await;
            recv_type(&mut socket, "input_audio_buffer.commit").await;
            send(
                &mut socket,
                json!({
                    "type": "conversation.item.input_audio_transcription.completed",
                    "item_id": "a", "transcript": "A turn.",
                }),
            )
            .await;
            recv(&mut socket).await;
        }),
    )
    .await;
    let mut active = config("custom", &server.url, TranscriptionRoute::Realtime, false, None);
    if let Some(c) = active.record.capabilities.as_mut() {
        c.realtime_transcription = Some(RealtimeTranscription::Turn);
    }
    let start = bench
        .voice
        .start_capture(active, 16_000, vec![], Arc::new(RecordingSink::default()))
        .unwrap();
    assert!(!start.live);
    assert_eq!(bench.voice.finish_capture(&start.session_id).await.unwrap(), "A turn.");
}

// VOP-FR-YRQN, VOP-FR-QQYG: a whole `vllm` capture at 16 kHz, and it ends at
// `transcription.done`.
#[tokio::test]
async fn a_vllm_session_streams_at_16_khz() {
    let bench = Bench::new();
    let received = Arc::new(std::sync::Mutex::new(0usize));
    let total = received.clone();
    let server = fake(
        |_, _| HttpReply::status(404),
        ws_handler(move |_, mut socket| {
            let total = total.clone();
            async move {
                send(&mut socket, json!({ "type": "session.created", "id": "s" })).await;
                recv_type(&mut socket, "session.update").await;
                let ready = recv_type(&mut socket, "input_audio_buffer.commit").await.unwrap();
                assert!(ready.get("final").is_none());
                send(&mut socket, json!({ "type": "transcription.delta", "delta": "hel" })).await;
                loop {
                    let event = recv(&mut socket).await.unwrap();
                    match event["type"].as_str().unwrap() {
                        "input_audio_buffer.append" => *total.lock().unwrap() += pcm_of(&event),
                        "input_audio_buffer.commit" => {
                            assert_eq!(event["final"], true);
                            break;
                        }
                        other => panic!("unexpected {other}"),
                    }
                }
                send(&mut socket, json!({ "type": "transcription.done", "text": "hello" })).await;
                recv(&mut socket).await;
            }
        }),
    )
    .await;
    let start = bench
        .voice
        .start_capture(
            config("custom", &server.url, TranscriptionRoute::Realtime, false, None),
            48_000,
            vec![],
            Arc::new(RecordingSink::default()),
        )
        .unwrap();
    bench.voice.append_capture(&start.session_id, vec![0; 48_000]).unwrap();
    assert_eq!(bench.voice.finish_capture(&start.session_id).await.unwrap(), "hello");
    let bytes = *received.lock().unwrap();
    assert!((bytes as i64 - 32_000).abs() < 2_100, "one second at 16 kHz, got {bytes} bytes");
}

// VOP-FR-QQYG, VOP-FR-UOYP: `openai` does not end on a close, so a close after
// the commit is a lost session.
#[tokio::test]
async fn an_openai_close_after_the_commit_is_a_lost_session() {
    let bench = Bench::new();
    let sink = Arc::new(RecordingSink::default());
    let server = fake(
        |_, _| HttpReply::status(404),
        ws_handler(|_, mut socket| async move {
            send(
                &mut socket,
                json!({ "type": "session.created", "session": { "type": "transcription" } }),
            )
            .await;
            recv_type(&mut socket, "input_audio_buffer.commit").await;
            let _ = socket.close(None).await;
        }),
    )
    .await;
    let start = bench
        .voice
        .start_capture(
            config(
                "openai",
                &format!("{}/v1", server.url),
                TranscriptionRoute::Realtime,
                false,
                Some("k"),
            ),
            24_000,
            vec![],
            sink.clone(),
        )
        .unwrap();
    bench.voice.append_capture(&start.session_id, vec![0; 2_400]).unwrap();
    assert_eq!(bench.voice.finish_capture(&start.session_id).await.unwrap_err(), ERR_SESSION_LOST);
    assert!(sink.diagnostics.lock().unwrap().iter().any(|d| d.contains("closed_early")));
}

// VOP-FR-UOYP: a refused handshake and an `error` before the session have
// their codes, and each says why in the diagnostics.
#[tokio::test]
async fn a_session_that_does_not_open_has_its_code() {
    let bench = Bench::new();
    let sink = Arc::new(RecordingSink::default());
    let no_route = fake(|_, _| HttpReply::status(404), None).await;
    let start = bench
        .voice
        .start_capture(
            config("custom", &no_route.url, TranscriptionRoute::Realtime, false, None),
            16_000,
            vec![],
            sink.clone(),
        )
        .unwrap();
    assert_eq!(
        bench.voice.finish_capture(&start.session_id).await.unwrap_err(),
        ERR_PROVIDER_UNREACHABLE
    );
    assert!(sink.diagnostics.lock().unwrap().iter().any(|d| d.contains("handshake_failed")));

    let refused = fake(
        |_, _| HttpReply::status(404),
        ws_handler(|_, mut socket| async move {
            send(
                &mut socket,
                json!({ "type": "error", "error": { "code": "bad_model", "message": "no" } }),
            )
            .await;
        }),
    )
    .await;
    let start = bench
        .voice
        .start_capture(
            config("custom", &refused.url, TranscriptionRoute::Realtime, false, None),
            16_000,
            vec![],
            sink.clone(),
        )
        .unwrap();
    assert_eq!(
        bench.voice.finish_capture(&start.session_id).await.unwrap_err(),
        ERR_PROVIDER_REJECTED
    );
    assert!(sink.diagnostics.lock().unwrap().iter().any(|d| d.contains("bad_model")));
}

// VOP-FR-IAFL: a failed transcription is not a failed socket, so a retry stays on
// Realtime, even with an HTTP route.
#[tokio::test]
async fn a_failed_transcription_retries_on_realtime() {
    let bench = Bench::new();
    let opened = Arc::new(std::sync::Mutex::new(0));
    let count = opened.clone();
    let server = fake(
        |_, _| panic!("no HTTP request is expected"),
        ws_handler(move |_, mut socket| {
            let count = count.clone();
            async move {
                let attempt = {
                    let mut opened = count.lock().unwrap();
                    *opened += 1;
                    *opened
                };
                send(&mut socket, llama_created("transcription", 16_000, None)).await;
                recv_type(&mut socket, "input_audio_buffer.commit").await;
                if attempt == 1 {
                    send(
                        &mut socket,
                        json!({
                            "type": "conversation.item.input_audio_transcription.failed",
                            "item_id": "a",
                        }),
                    )
                    .await;
                    recv(&mut socket).await;
                } else {
                    send(
                        &mut socket,
                        json!({
                            "type": "conversation.item.input_audio_transcription.completed",
                            "item_id": "a", "transcript": "second",
                        }),
                    )
                    .await;
                }
            }
        }),
    )
    .await;
    let start = bench
        .voice
        .start_capture(
            config("custom", &server.url, TranscriptionRoute::Realtime, true, None),
            16_000,
            vec![],
            Arc::new(RecordingSink::default()),
        )
        .unwrap();
    bench.voice.append_capture(&start.session_id, vec![1; 100]).unwrap();
    assert_eq!(
        bench.voice.finish_capture(&start.session_id).await.unwrap_err(),
        ERR_TRANSCRIPTION_FAILED
    );
    let text = bench
        .voice
        .retry_capture(&start.session_id, Arc::new(RecordingSink::default()))
        .await
        .unwrap();
    assert_eq!(text, "second");
}

// VOP-FR-UOYP: an HTTP failure records its status, and the code stays typed.
#[tokio::test]
async fn an_http_failure_records_its_status() {
    let bench = Bench::new();
    let sink = Arc::new(RecordingSink::default());
    let server = fake(|_, _| HttpReply::status(503), None).await;
    let start = bench
        .voice
        .start_capture(
            config("custom", &server.url, TranscriptionRoute::Http, true, None),
            16_000,
            vec![],
            sink.clone(),
        )
        .unwrap();
    assert_eq!(
        bench.voice.finish_capture(&start.session_id).await.unwrap_err(),
        ERR_TRANSCRIPTION_FAILED
    );
    assert_eq!(sink.diagnostics.lock().unwrap()[0], "http_transcription: refused status=503");
}

// VOP-FR-KBPZ: each session has its own id.
#[test]
fn each_session_has_its_own_id() {
    let bench = Bench::new();
    assert_ne!(bench.voice.new_session_id(), bench.voice.new_session_id());
}

// VOP-FR-BSNY: the limit is 30 seconds.
#[test]
fn the_watchdog_limit_is_30_seconds() {
    assert_eq!(WATCHDOG_LIMIT, Duration::from_secs(30));
}

// VOP-FR-DXPK: a raw body and a JSON array of bytes give the same samples.
#[test]
fn raw_and_json_bodies_give_the_same_samples() {
    use crate::voice_ai::commands::body_samples;
    use tauri::ipc::InvokeBody;
    let raw = InvokeBody::Raw(vec![1, 0, 0xFF, 0xFF]);
    let json = InvokeBody::Json(json!([1, 0, 255, 255]));
    assert_eq!(body_samples(&raw), vec![1, -1]);
    assert_eq!(body_samples(&json), vec![1, -1]);
    assert!(body_samples(&InvokeBody::Json(json!({ "a": 1 }))).is_empty());
}
