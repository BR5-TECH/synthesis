//! Interim commits of the `openai` Realtime dialect (VOP-FR-KIMU,
//! VOP-FR-TWEZ, VOP-FR-QQYG). The fake server names each commit and
//! transcribes it, so the live text comes before the finish. No result here
//! depends on time: each wait is for an event, and its limit only stops a
//! broken test.

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use tokio::sync::mpsc;

use super::support::*;
use crate::global_settings::VoiceVadSensitivity;
use crate::voice_ai::activity::detector::Settings;
use crate::voice_ai::activity::silero::SpeechModel;
use crate::voice_ai::chunks::{ChunkState, Chunking};
use crate::voice_ai::*;

const RATE: usize = 16_000;
/// The rate of the `openai` dialect, at which the server counts samples.
const SERVER_RATE: usize = 24_000;

fn config(provider: &str, base_url: &str) -> ActiveVoice {
    let mut record = VoiceAiRecord::seeded(descriptor(provider).unwrap());
    record.base_url = base_url.to_string();
    record.stt_model = "stt".to_string();
    record.transcription_route = TranscriptionRoute::Realtime;
    record.capabilities = Some(VoiceCapabilities {
        realtime_transcription: Some(RealtimeTranscription::Live),
        ..VoiceCapabilities::default()
    });
    ActiveVoice { record, key: Some("k".to_string()), language: None }
}

fn speech(seconds: f32) -> Vec<i16> {
    vec![10_000; (RATE as f32 * seconds) as usize]
}

fn samples_of(event: &serde_json::Value) -> usize {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(event["audio"].as_str().unwrap_or_default())
        .map(|bytes| bytes.len() / 2)
        .unwrap_or(0)
}

/// An `openai` server. It names each commit with the next item of `items` and
/// completes it with its text. A commit after the last item gets the error of
/// an empty buffer. At the close, it reports the samples that came before
/// each commit, at its own rate.
async fn openai_server(items: &'static [(&'static str, &'static str)]) -> (Fake, Report) {
    let (report, reports) = mpsc::unbounded_channel();
    let server = fake(
        |_, _| HttpReply::status(404),
        ws_handler(move |_, mut socket| {
            let report = report.clone();
            async move {
                send(
                    &mut socket,
                    json!({ "type": "session.created", "session": { "type": "transcription" } }),
                )
                .await;
                let mut commits: Vec<usize> = Vec::new();
                let mut since = 0;
                let mut previous: Option<&str> = None;
                let mut next = items.iter();
                while let Some(event) = recv(&mut socket).await {
                    match event["type"].as_str().unwrap_or_default() {
                        "input_audio_buffer.append" => since += samples_of(&event),
                        "input_audio_buffer.commit" => {
                            commits.push(since);
                            since = 0;
                            let Some((item, text)) = next.next() else {
                                send(
                                    &mut socket,
                                    json!({ "type": "error", "error": {
                                        "code": "input_audio_buffer_commit_empty",
                                        "message": "the buffer is empty",
                                    }}),
                                )
                                .await;
                                continue;
                            };
                            send(
                                &mut socket,
                                json!({
                                    "type": "input_audio_buffer.committed",
                                    "item_id": item, "previous_item_id": previous,
                                }),
                            )
                            .await;
                            send(
                                &mut socket,
                                json!({
                                    "type": "conversation.item.input_audio_transcription.completed",
                                    "item_id": item, "transcript": text,
                                }),
                            )
                            .await;
                            previous = Some(item);
                        }
                        _ => {}
                    }
                }
                let _ = report.send(commits);
            }
        }),
    )
    .await;
    (server, Report(reports))
}

struct Report(mpsc::UnboundedReceiver<Vec<usize>>);

impl Report {
    /// The samples before each commit, once the client closed the socket.
    async fn commits(mut self) -> Vec<usize> {
        tokio::time::timeout(Duration::from_secs(60), self.0.recv())
            .await
            .expect("the socket did not close")
            .expect("a report")
    }
}

fn has_transcript(text: &'static str) -> impl FnMut(&RecordingSink) -> bool {
    move |sink| sink.transcripts.lock().unwrap().iter().any(|t| t == text)
}

// VOP-FR-KIMU, VOP-FR-QQYG, VOP-FR-XXAB: 4 s of audio that no commit holds
// sends an interim commit, its text shows before the finish, and the finish
// waits for the final item and joins both with one space.
#[tokio::test]
async fn four_seconds_of_audio_send_an_interim_commit_and_live_text() {
    let bench = Bench::new();
    let (server, report) = openai_server(&[("a", "First part"), ("b", "and the end.")]).await;
    let sink = Arc::new(RecordingSink::default());
    let voice = &bench.voice;
    let start = voice
        .start_capture(
            config("openai", &format!("{}/v1", server.url)),
            RATE as u32,
            vec![],
            sink.clone(),
        )
        .unwrap();
    let id = &start.session_id;
    voice.append_capture(id, speech(4.0)).unwrap();
    sink.wait_for(has_transcript("First part")).await;
    voice.append_capture(id, speech(0.5)).unwrap();
    assert_eq!(voice.finish_capture(id).await.unwrap(), "First part and the end.");
    assert_eq!(report.commits().await.len(), 2);
    assert_eq!(server.ws.lock().unwrap().len(), 1, "one socket");
}

// VOP-FR-QQYG: exactly 4 s of audio, then the finish: the interim commit is
// the only commit, and the finish returns its text.
#[tokio::test]
async fn a_finish_after_an_interim_commit_sends_no_final_commit() {
    let bench = Bench::new();
    let (server, report) = openai_server(&[("a", "All of it.")]).await;
    let sink = Arc::new(RecordingSink::default());
    let voice = &bench.voice;
    let start = voice
        .start_capture(
            config("openai", &format!("{}/v1", server.url)),
            RATE as u32,
            vec![],
            sink.clone(),
        )
        .unwrap();
    let id = &start.session_id;
    voice.append_capture(id, speech(4.0)).unwrap();
    assert_eq!(voice.finish_capture(id).await.unwrap(), "All of it.");
    assert_eq!(report.commits().await.len(), 1, "one commit");
}

// VOP-FR-QQYG: a final commit that holds no audio is named with no item, and
// the finish returns the text of the interim items.
#[tokio::test]
async fn an_empty_final_commit_keeps_the_text_of_the_interim_items() {
    let bench = Bench::new();
    let (server, report) = openai_server(&[("a", "All of it.")]).await;
    let sink = Arc::new(RecordingSink::default());
    let voice = &bench.voice;
    let start = voice
        .start_capture(
            config("openai", &format!("{}/v1", server.url)),
            RATE as u32,
            vec![],
            sink.clone(),
        )
        .unwrap();
    let id = &start.session_id;
    voice.append_capture(id, speech(4.0)).unwrap();
    voice.append_capture(id, vec![10_000; 10]).unwrap();
    assert_eq!(voice.finish_capture(id).await.unwrap(), "All of it.");
    assert_eq!(report.commits().await.len(), 2, "the final commit got the empty error");
}

// VOP-FR-KIMU: a word gap commits only after 1 s of audio that no commit holds.
#[tokio::test]
async fn a_word_gap_commits_after_one_second_and_not_before() {
    let bench = Bench::new();
    let (server, report) = openai_server(&[("a", "One.")]).await;
    let sink = Arc::new(RecordingSink::default());
    let voice = &bench.voice;
    let start = voice
        .start_capture(
            config("openai", &format!("{}/v1", server.url)),
            RATE as u32,
            vec![],
            sink.clone(),
        )
        .unwrap();
    let id = &start.session_id;
    voice.append_capture(id, speech(0.5)).unwrap();
    voice.word_gap(id);
    voice.append_capture(id, speech(0.6)).unwrap();
    voice.word_gap(id);
    sink.wait_for(has_transcript("One.")).await;
    assert_eq!(voice.finish_capture(id).await.unwrap(), "One.");
    let commits = report.commits().await;
    assert_eq!(commits.len(), 1, "{commits:?}");
    assert!(commits[0] > SERVER_RATE, "the gap at 0.5 s sent no commit: {commits:?}");
}

// VOP-FR-TWEZ, VOP-FR-KIMU, VOP-FR-GLRN: in a chunked capture, an interim
// commit emits no chunk event, and a chunk end with no audio after it sends
// no commit. The chunk text is the text of the interim item.
#[tokio::test]
async fn a_chunk_end_after_an_interim_commit_sends_no_commit() {
    let bench = Bench::new();
    let (server, report) = openai_server(&[("a", "Hello there.")]).await;
    let sink = Arc::new(RecordingSink::default());
    let voice = &bench.voice;
    let start = voice
        .start_chunked_capture(
            config("openai", &format!("{}/v1", server.url)),
            RATE as u32,
            vec![],
            Chunking::Session,
            sink.clone(),
        )
        .unwrap();
    let id = &start.session_id;
    voice.append_capture(id, speech(4.0)).unwrap();
    sink.wait_for(has_transcript("Hello there.")).await;
    assert!(sink.chunks.lock().unwrap().is_empty(), "no chunk event before the chunk end");
    assert!(voice.chunk_pause(id, None).unwrap(), "the live text ends a sentence");
    sink.wait_for(|s| s.chunks.lock().unwrap().len() == 2).await;
    let chunks: Vec<(usize, ChunkState, Option<String>)> =
        sink.chunks.lock().unwrap().iter().map(|c| (c.index, c.state, c.text.clone())).collect();
    assert_eq!(
        chunks,
        vec![
            (0, ChunkState::End, None),
            (0, ChunkState::Transcript, Some("Hello there.".to_string())),
        ]
    );
    assert_eq!(voice.finish_capture(id).await.unwrap(), "Hello there.");
    assert_eq!(report.commits().await.len(), 1, "one commit");
}

/// Speech is loud, and a gap is silent.
struct Loudness;

impl SpeechModel for Loudness {
    fn probability(&mut self, frame: &[f32]) -> Result<f32, String> {
        let loud = frame.iter().map(|s| s.abs()).sum::<f32>() / frame.len() as f32;
        Ok(if loud > 0.1 { 0.9 } else { 0.05 })
    }
}

// VOP-FR-KIMU, VOP-FR-IXDV: a live `openai` Push-to-Talk capture that is not
// chunked runs the local detector for its word gaps, and a gap after 1 s sends
// an interim commit. Speech after that commit still gets the final commit.
#[tokio::test]
async fn a_push_to_talk_capture_commits_at_the_word_gap_of_its_own_detector() {
    let bench = Bench::new();
    let (server, report) = openai_server(&[("a", "Hello"), ("b", "there.")]).await;
    let sink = Arc::new(RecordingSink::default());
    let voice = &bench.voice;
    let start = voice
        .start_chunked_capture(
            config("openai", &format!("{}/v1", server.url)),
            RATE as u32,
            vec![],
            Chunking::Gaps {
                settings: Settings::new(1500, VoiceVadSensitivity::Medium),
                load: Box::new(|| Ok(Box::new(Loudness) as Box<dyn SpeechModel>)),
            },
            sink.clone(),
        )
        .unwrap();
    let id = &start.session_id;
    voice.append_and_detect(id, speech(1.2)).await.unwrap();
    voice.append_and_detect(id, vec![0; RATE / 5]).await.unwrap();
    sink.wait_for(has_transcript("Hello")).await;
    voice.append_and_detect(id, speech(0.5)).await.unwrap();
    assert_eq!(voice.finish_capture(id).await.unwrap(), "Hello there.");
    assert_eq!(report.commits().await.len(), 2);
}

// VOP-FR-IXDV, VOP-FR-HSJU, VOP-FR-QQYG: when the detector heard no speech
// after the interim commit, the finish sends no final commit for the silence,
// and the transcript is the text of the interim item.
#[tokio::test]
async fn a_finish_with_no_speech_after_the_interim_commit_sends_no_final_commit() {
    let bench = Bench::new();
    let (server, report) = openai_server(&[("a", "Hello"), ("b", "Thank you.")]).await;
    let sink = Arc::new(RecordingSink::default());
    let voice = &bench.voice;
    let start = voice
        .start_chunked_capture(
            config("openai", &format!("{}/v1", server.url)),
            RATE as u32,
            vec![],
            Chunking::Gaps {
                settings: Settings::new(1500, VoiceVadSensitivity::Medium),
                load: Box::new(|| Ok(Box::new(Loudness) as Box<dyn SpeechModel>)),
            },
            sink.clone(),
        )
        .unwrap();
    let id = &start.session_id;
    voice.append_and_detect(id, speech(1.2)).await.unwrap();
    // The silence comes in frames of 32 ms, as a microphone gives it, so the word
    // gap commits about 100 ms into the silence, before the speech end.
    for _ in 0..10 {
        voice.append_and_detect(id, vec![0; 512]).await.unwrap();
    }
    sink.wait_for(has_transcript("Hello")).await;
    for _ in 0..45 {
        voice.append_and_detect(id, vec![0; 512]).await.unwrap();
    }
    assert_eq!(voice.finish_capture(id).await.unwrap(), "Hello");
    assert_eq!(report.commits().await.len(), 1, "no commit for the silence");
}

/// A server of the `llamacpp` or the `vllm` dialect. It reports each commit
/// that came after audio and has no `"final": true`.
async fn final_only_server(vllm: bool) -> (Fake, Report) {
    let (report, reports) = mpsc::unbounded_channel();
    let server = fake(
        |_, _| HttpReply::status(404),
        ws_handler(move |_, mut socket| {
            let report = report.clone();
            async move {
                if vllm {
                    send(&mut socket, json!({ "type": "session.created", "id": "s" })).await;
                } else {
                    send(&mut socket, llama_created("transcription", 16_000, None)).await;
                }
                let mut heard = false;
                let mut interim: Vec<usize> = Vec::new();
                while let Some(event) = recv(&mut socket).await {
                    match event["type"].as_str().unwrap_or_default() {
                        "input_audio_buffer.append" => heard = true,
                        "input_audio_buffer.commit" if event["final"] == true => {
                            if vllm {
                                send(
                                    &mut socket,
                                    json!({ "type": "transcription.done", "text": "done" }),
                                )
                                .await;
                            } else {
                                send(
                                    &mut socket,
                                    json!({
                                        "type": "conversation.item.input_audio_transcription.completed",
                                        "item_id": "a", "transcript": "done",
                                    }),
                                )
                                .await;
                                break;
                            }
                        }
                        "input_audio_buffer.commit" if heard => interim.push(1),
                        _ => {}
                    }
                }
                let _ = report.send(interim);
            }
        }),
    )
    .await;
    (server, Report(reports))
}

// VOP-FR-KIMU: only the `openai` dialect sends interim commits.
#[tokio::test]
async fn llamacpp_and_vllm_send_no_interim_commit() {
    for vllm in [false, true] {
        let bench = Bench::new();
        let (server, report) = final_only_server(vllm).await;
        let sink = Arc::new(RecordingSink::default());
        let voice = &bench.voice;
        let start = voice
            .start_capture(config("custom", &server.url), RATE as u32, vec![], sink.clone())
            .unwrap();
        let id = &start.session_id;
        voice.append_capture(id, speech(4.5)).unwrap();
        voice.word_gap(id);
        assert_eq!(voice.finish_capture(id).await.unwrap(), "done", "vllm: {vllm}");
        assert!(report.commits().await.is_empty(), "vllm: {vllm}");
    }
}
