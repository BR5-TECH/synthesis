//! The speech end of a capture (VOP-FR-HSJU): a transcription gets no audio
//! after 200 ms past the last speech, so a speech-to-text model cannot write a
//! stock phrase for the silence. The capture rate is 16 kHz, and a WAV holds 2
//! bytes for each sample. No result depends on time.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;
use tokio::sync::{mpsc, Notify};

use super::chunks::{
    by_length, config, end, events, own_detector, pause, speech, transcript, RATE,
};
use super::support::*;
use crate::voice_ai::capture::Phase;
use crate::voice_ai::chunks::Chunking;
use crate::voice_ai::*;

/// 200 ms at 16 kHz.
const TAIL: usize = 3_200;

fn silence(seconds: f32) -> Vec<i16> {
    vec![0; (RATE as f32 * seconds) as usize]
}

/// The size of the body of each HTTP request, in order.
fn bodies(server: &Fake) -> Vec<usize> {
    server.http.lock().unwrap().iter().map(|seen| seen.body.len()).collect()
}

/// A request whose WAV holds `samples` of audio. The multipart form adds less
/// than 1 kB to the PCM.
fn holds(body: usize, samples: usize) -> bool {
    body > samples * 2 && body < samples * 2 + 1_000
}

/// The first frame that is silent after `seconds` of speech. A frame is 512
/// samples, and a frame that holds a little speech reads as silent.
fn first_quiet_frame(seconds: f32) -> usize {
    let speech = (RATE as f32 * seconds) as usize;
    let partial = speech % 512;
    let loud_enough = partial * 10_000 / 512 > 3_277;
    (speech / 512 + usize::from(loud_enough)) * 512
}

// VOP-FR-ZIHM, VOP-FR-HSJU: on the HTTP route, the audio of a chunk ends 200 ms
// after its last speech, and the rest of the pause starts the next chunk.
#[tokio::test]
async fn a_chunk_ends_at_the_speech_before_its_pause() {
    let bench = Bench::new();
    let server = by_length(false, false).await;
    let sink = Arc::new(RecordingSink::default());
    let threads = Arc::new(Mutex::new(Vec::new()));
    let voice = &bench.voice;
    let start = voice
        .start_chunked_capture(
            config("custom", &server.url, TranscriptionRoute::Http),
            RATE as u32,
            vec![],
            own_detector(&threads),
            sink.clone(),
        )
        .unwrap();
    let id = &start.session_id;
    voice.append_and_detect(id, speech(2.5)).await.unwrap();
    voice.append_and_detect(id, pause()).await.unwrap();
    voice.append_and_detect(id, speech(1.0)).await.unwrap();
    assert_eq!(voice.finish_capture(id).await.unwrap(), "One. Two.");

    let cut = first_quiet_frame(2.5) + TAIL;
    // The chunks transcribe in parallel, so the requests come in any order.
    let mut sizes = bodies(&server);
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(sizes.len(), 2, "{sizes:?}");
    assert!(holds(sizes[0], cut), "the first chunk ends at the speech end: {sizes:?}");
    let rest = RATE * 4 - cut;
    assert!(holds(sizes[1], rest), "the rest of the pause opens the next chunk: {sizes:?}");
}

// VOP-FR-WNQW, VOP-FR-HSJU: with no live text, the 2 s of a sentence end count
// only to the speech end. A short utterance with a long silence after it is one
// request, and that request holds no silence after the tail.
#[tokio::test]
async fn a_short_utterance_is_one_request_up_to_its_speech_end() {
    let bench = Bench::new();
    let server = by_length(false, false).await;
    let sink = Arc::new(RecordingSink::default());
    let threads = Arc::new(Mutex::new(Vec::new()));
    let voice = &bench.voice;
    let start = voice
        .start_chunked_capture(
            config("custom", &server.url, TranscriptionRoute::Http),
            RATE as u32,
            vec![],
            own_detector(&threads),
            sink.clone(),
        )
        .unwrap();
    let id = &start.session_id;
    voice.append_and_detect(id, speech(1.0)).await.unwrap();
    voice.append_and_detect(id, silence(1.7)).await.unwrap();
    assert!(events(&sink).is_empty(), "the silence does not make 2 s of chunk audio");
    assert_eq!(voice.finish_capture(id).await.unwrap(), "Two.");

    assert_eq!(events(&sink), vec![end(0), transcript(0, "Two.")]);
    let sizes = bodies(&server);
    assert_eq!(sizes.len(), 1, "{sizes:?}");
    assert!(holds(sizes[0], first_quiet_frame(1.0) + TAIL), "{sizes:?}");
}

// VOP-FR-CWWL, VOP-FR-HSJU, VOP-FR-GLDR: in a Push-to-Talk capture, the silence
// after the last chunk sends no request and emits no chunk event.
#[tokio::test]
async fn a_push_to_talk_capture_sends_no_chunk_of_trailing_silence() {
    let bench = Bench::new();
    let server = by_length(false, false).await;
    let sink = Arc::new(RecordingSink::default());
    let threads = Arc::new(Mutex::new(Vec::new()));
    let voice = &bench.voice;
    let start = voice
        .start_chunked_capture(
            config("custom", &server.url, TranscriptionRoute::Http),
            RATE as u32,
            vec![],
            own_detector(&threads),
            sink.clone(),
        )
        .unwrap();
    let id = &start.session_id;
    voice.append_and_detect(id, speech(2.5)).await.unwrap();
    voice.append_and_detect(id, pause()).await.unwrap();
    voice.append_and_detect(id, silence(1.0)).await.unwrap();
    assert_eq!(voice.finish_capture(id).await.unwrap(), "One.");

    assert_eq!(events(&sink), vec![end(0), transcript(0, "One.")]);
    assert_eq!(bodies(&server).len(), 1, "no request for the silence");
}

// VOP-FR-CWWL, VOP-FR-HSJU: in a VAD session, the speech end of `speech_end`
// leaves the last chunk with no audio, so it sends no request.
#[tokio::test]
async fn a_last_chunk_with_no_speech_sends_no_request_on_the_vad_route() {
    let bench = Bench::new();
    let server = by_length(false, false).await;
    let sink = Arc::new(RecordingSink::default());
    let voice = &bench.voice;
    let start = voice
        .start_chunked_capture(
            config("custom", &server.url, TranscriptionRoute::Http),
            RATE as u32,
            vec![],
            Chunking::Session,
            sink.clone(),
        )
        .unwrap();
    let id = &start.session_id;
    let cut = RATE * 5 / 2 + TAIL;
    voice.append_capture(id, speech(2.5)).unwrap();
    voice.append_capture(id, pause()).unwrap();
    assert!(voice.chunk_pause(id, Some(cut)).unwrap());
    voice.append_capture(id, silence(1.0)).unwrap();
    voice.end_capture_input(id, Some(cut)).unwrap();
    assert_eq!(voice.finish_capture(id).await.unwrap(), "One.");

    assert_eq!(events(&sink), vec![end(0), transcript(0, "One.")]);
    let sizes = bodies(&server);
    assert_eq!(sizes.len(), 1, "{sizes:?}");
    assert!(holds(sizes[0], cut), "{sizes:?}");
}

// VOP-FR-IAFL, VOP-FR-SIKM, VOP-FR-HSJU: a retry sends the held audio, and that
// audio ends at the speech end.
#[tokio::test]
async fn a_retry_sends_the_audio_up_to_the_speech_end() {
    let bench = Bench::new();
    let healthy = Arc::new(AtomicBool::new(false));
    let flag = healthy.clone();
    let server = fake(
        move |_, _| {
            if flag.load(Ordering::SeqCst) {
                transcribed("Hello.")
            } else {
                HttpReply::status(500)
            }
        },
        None,
    )
    .await;
    let sink = Arc::new(RecordingSink::default());
    let threads = Arc::new(Mutex::new(Vec::new()));
    let voice = &bench.voice;
    let start = voice
        .start_chunked_capture(
            config("custom", &server.url, TranscriptionRoute::Http),
            RATE as u32,
            vec![],
            own_detector(&threads),
            sink.clone(),
        )
        .unwrap();
    let id = &start.session_id;
    voice.append_and_detect(id, speech(1.0)).await.unwrap();
    voice.append_and_detect(id, silence(1.7)).await.unwrap();
    assert_eq!(voice.finish_capture(id).await.unwrap_err(), ERR_TRANSCRIPTION_FAILED);

    healthy.store(true, Ordering::SeqCst);
    assert_eq!(voice.retry_capture(id, sink.clone()).await.unwrap(), "Hello.");
    let sizes = bodies(&server);
    let keep = first_quiet_frame(1.0) + TAIL;
    assert!(holds(*sizes.last().unwrap(), keep), "{sizes:?}");
}

// VOP-FR-HSJU, VOP-FR-EYHP: a capture with no detector keeps all of its audio.
#[tokio::test]
async fn a_capture_with_no_detector_keeps_all_of_its_audio() {
    let bench = Bench::new();
    let server = by_length(false, false).await;
    let sink = Arc::new(RecordingSink::default());
    let voice = &bench.voice;
    let start = voice
        .start_capture(
            config("custom", &server.url, TranscriptionRoute::Http),
            RATE as u32,
            vec![],
            sink.clone(),
        )
        .unwrap();
    let id = &start.session_id;
    voice.append_capture(id, speech(1.0)).unwrap();
    voice.append_capture(id, silence(1.7)).unwrap();
    voice.finish_capture(id).await.unwrap();
    let sizes = bodies(&server);
    assert!(holds(sizes[0], RATE * 27 / 10), "{sizes:?}");
}

/// An `openai` server that names each commit and completes it only after
/// `release`. At the close, it reports the number of commits.
async fn held_openai_server(release: Arc<Notify>) -> (Fake, mpsc::UnboundedReceiver<usize>) {
    let (report, reports) = mpsc::unbounded_channel();
    let server = fake(
        |_, _| HttpReply::status(404),
        ws_handler(move |_, mut socket| {
            let (report, release) = (report.clone(), release.clone());
            async move {
                send(
                    &mut socket,
                    json!({ "type": "session.created", "session": { "type": "transcription" } }),
                )
                .await;
                let mut commits = 0;
                while let Some(event) = recv(&mut socket).await {
                    if event["type"] != "input_audio_buffer.commit" {
                        continue;
                    }
                    commits += 1;
                    let item = format!("item-{commits}");
                    send(
                        &mut socket,
                        json!({ "type": "input_audio_buffer.committed", "item_id": item }),
                    )
                    .await;
                    release.notified().await;
                    send(
                        &mut socket,
                        json!({
                            "type": "conversation.item.input_audio_transcription.completed",
                            "item_id": item, "transcript": "One.",
                        }),
                    )
                    .await;
                }
                let _ = report.send(commits);
            }
        }),
    )
    .await;
    (server, reports)
}

// VOP-FR-IXDV, VOP-FR-CWWL, VOP-FR-QQYG: on the `openai` dialect, a last chunk
// with no speech sends no commit, and the session still gives the text of the
// chunk before it, also when that text comes after the finish.
#[tokio::test]
async fn a_last_openai_chunk_with_no_speech_still_gives_the_chunk_before() {
    let bench = Bench::new();
    let release = Arc::new(Notify::new());
    let (server, mut report) = held_openai_server(release.clone()).await;
    let sink = Arc::new(RecordingSink::default());
    let mut voice_config =
        config("openai", &format!("{}/v1", server.url), TranscriptionRoute::Realtime);
    // With no live text, the 2 s rule finds the sentence end.
    voice_config.record.capabilities = Some(VoiceCapabilities {
        realtime_transcription: Some(RealtimeTranscription::Turn),
        ..VoiceCapabilities::default()
    });
    let voice = &bench.voice;
    let start = voice
        .start_chunked_capture(voice_config, RATE as u32, vec![], Chunking::Session, sink.clone())
        .unwrap();
    let id = start.session_id.clone();
    let cut = RATE * 5 / 2 + TAIL;
    voice.append_capture(&id, speech(2.5)).unwrap();
    voice.append_capture(&id, pause()).unwrap();
    assert!(voice.chunk_pause(&id, Some(cut)).unwrap());
    voice.append_capture(&id, silence(1.0)).unwrap();
    voice.end_capture_input(&id, Some(cut)).unwrap();

    let finishing =
        || voice.capture.lock().unwrap().iter().any(|s| s.id == id && s.phase == Phase::Finishing);
    let release_after_finish = async {
        tokio::time::timeout(Duration::from_secs(30), async {
            while !finishing() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("the finish did not start");
        release.notify_one();
    };
    let (text, ()) = tokio::join!(voice.finish_capture(&id), release_after_finish);
    assert_eq!(text.unwrap(), "One.");
    assert_eq!(events(&sink), vec![end(0), transcript(0, "One.")]);
    let commits = tokio::time::timeout(Duration::from_secs(30), report.recv())
        .await
        .expect("the socket did not close")
        .expect("a report");
    assert_eq!(commits, 1, "the last chunk sends no commit");
}

// VOP-FR-HSJU, VOP-FR-IAFL: a capture that is not chunked, as a spoken answer
// is, ends at the speech end of `speech_end`, and so does its retry.
#[tokio::test]
async fn a_capture_that_is_not_chunked_ends_at_the_speech_end() {
    let bench = Bench::new();
    let healthy = Arc::new(AtomicBool::new(false));
    let flag = healthy.clone();
    let server = fake(
        move |_, _| {
            if flag.load(Ordering::SeqCst) {
                transcribed("Yes.")
            } else {
                HttpReply::status(500)
            }
        },
        None,
    )
    .await;
    let sink = Arc::new(RecordingSink::default());
    let voice = &bench.voice;
    let start = voice
        .start_capture(
            config("custom", &server.url, TranscriptionRoute::Http),
            RATE as u32,
            vec![],
            sink.clone(),
        )
        .unwrap();
    let id = &start.session_id;
    let cut = RATE + TAIL;
    voice.append_capture(id, speech(1.0)).unwrap();
    voice.append_capture(id, silence(1.7)).unwrap();
    voice.end_capture_input(id, Some(cut)).unwrap();
    assert_eq!(voice.finish_capture(id).await.unwrap_err(), ERR_TRANSCRIPTION_FAILED);
    healthy.store(true, Ordering::SeqCst);
    assert_eq!(voice.retry_capture(id, sink.clone()).await.unwrap(), "Yes.");
    let sizes = bodies(&server);
    assert_eq!(sizes.len(), 2, "{sizes:?}");
    assert!(sizes.iter().all(|size| holds(*size, cut)), "{sizes:?}");
}
