//! Chunked capture, on both routes. The capture rate is 16 kHz. A model that
//! reads the loudness of each frame stands in for Silero, so the audio itself
//! says where the speech and the pauses are, and no test waits for a time.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;

use super::support::*;
use crate::global_settings::VoiceVadSensitivity;
use crate::voice_ai::activity::detector::{PauseTracker, Settings};
use crate::voice_ai::activity::silero::SpeechModel;
use crate::voice_ai::chunks::{
    is_sentence_end, ChunkChange, ChunkLedger, ChunkState, Chunking, DETECTOR_THREAD,
};
use crate::voice_ai::*;

const RATE: usize = 16_000;

/// Speech is loud, and a pause is silent.
struct Loudness {
    threads: Arc<Mutex<Vec<String>>>,
}

impl SpeechModel for Loudness {
    fn probability(&mut self, frame: &[f32]) -> Result<f32, String> {
        let name = std::thread::current().name().unwrap_or_default().to_string();
        self.threads.lock().unwrap().push(name);
        let loud = frame.iter().map(|s| s.abs()).sum::<f32>() / frame.len() as f32;
        Ok(if loud > 0.1 { 0.9 } else { 0.05 })
    }
}

fn own_detector(threads: &Arc<Mutex<Vec<String>>>) -> Chunking {
    let threads = threads.clone();
    Chunking::Own {
        settings: Settings::new(1500, VoiceVadSensitivity::Medium),
        load: Box::new(move || Ok(Box::new(Loudness { threads }) as Box<dyn SpeechModel>)),
    }
}

fn speech(seconds: f32) -> Vec<i16> {
    vec![10_000; (RATE as f32 * seconds) as usize]
}

/// 500 ms: longer than the short pause of 400 ms, and shorter than the silence
/// timeout.
fn pause() -> Vec<i16> {
    vec![0; RATE / 2]
}

fn config(provider: &str, base_url: &str, route: TranscriptionRoute) -> ActiveVoice {
    let mut record = VoiceAiRecord::seeded(descriptor(provider).unwrap());
    record.base_url = base_url.to_string();
    record.stt_model = "stt".to_string();
    record.transcription_route = route;
    record.capabilities = Some(VoiceCapabilities {
        realtime_transcription: (route == TranscriptionRoute::Realtime)
            .then_some(RealtimeTranscription::Live),
        http_transcription: true,
        ..VoiceCapabilities::default()
    });
    ActiveVoice { record, key: Some("k".to_string()), language: None }
}

fn events(sink: &RecordingSink) -> Vec<(usize, ChunkState, Option<String>, Option<String>)> {
    sink.chunks
        .lock()
        .unwrap()
        .iter()
        .map(|c| (c.index, c.state, c.text.clone(), c.code.clone()))
        .collect()
}

fn transcript(index: usize, text: &str) -> (usize, ChunkState, Option<String>, Option<String>) {
    (index, ChunkState::Transcript, Some(text.to_string()), None)
}

fn end(index: usize) -> (usize, ChunkState, Option<String>, Option<String>) {
    (index, ChunkState::End, None, None)
}

/// Wait for a state that a background task makes. The limit only stops a
/// broken test, and no result depends on it.
async fn until(mut done: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(30), async {
        while !done() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the state did not come");
}

/// An HTTP server that answers by the size of the WAV: a chunk of 3 s is
/// "One.", and a shorter one is "Two.".
async fn by_length(fail_long: bool, fail_short: bool) -> Fake {
    fake(
        move |path, body| {
            if !path.ends_with("/audio/transcriptions") {
                return HttpReply::status(404);
            }
            let long = body.len() > 64_000;
            match (long, fail_long, fail_short) {
                (true, true, _) | (false, _, true) => HttpReply::status(500),
                (true, false, _) => transcribed(" One. "),
                (false, _, false) => transcribed("Two."),
            }
        },
        None,
    )
    .await
}

// VOP-FR-ERDS, VOP-FR-JRVF, VOP-FR-WNQW, VOP-FR-YDVH, VOP-FR-VOZD, VOP-FR-GLRN,
// VOP-FR-DSSU, VOP-FR-GLDR: a Push-to-Talk capture on the HTTP route. Its own
// detector finds the pause after 3 s, the chunk goes as one request, and the
// capture keeps its audio for the next chunk. The finish joins the two texts.
#[tokio::test]
async fn a_push_to_talk_capture_sends_one_request_for_each_chunk() {
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
    assert!(!start.live);
    voice.append_and_detect(&start.session_id, speech(2.5)).await.unwrap();
    assert!(events(&sink).is_empty());
    voice.append_and_detect(&start.session_id, pause()).await.unwrap();
    assert_eq!(events(&sink).first(), Some(&end(0)), "the pause ends the chunk");
    // VOP-FR-VOZD: the capture still takes audio while the chunk transcribes.
    voice.append_and_detect(&start.session_id, speech(1.0)).await.unwrap();

    assert_eq!(voice.finish_capture(&start.session_id).await.unwrap(), "One. Two.");
    assert_eq!(events(&sink), vec![end(0), transcript(0, "One."), end(1), transcript(1, "Two.")]);
    assert_eq!(server.http.lock().unwrap().len(), 2, "one request for each chunk");
    assert!(voice.capture.lock().unwrap().is_empty(), "a success ends the session");
}

// VOP-FR-WNQW: with no live text, a pause before 2 s of chunk audio is not a
// sentence end, and silence with no speech makes no chunk.
#[tokio::test]
async fn a_pause_before_two_seconds_of_audio_ends_no_chunk() {
    let bench = Bench::new();
    let server = by_length(false, false).await;
    let sink = Arc::new(RecordingSink::default());
    let threads = Arc::new(Mutex::new(Vec::new()));
    let start = bench
        .voice
        .start_chunked_capture(
            config("custom", &server.url, TranscriptionRoute::Http),
            RATE as u32,
            vec![],
            own_detector(&threads),
            sink.clone(),
        )
        .unwrap();
    let id = &start.session_id;
    bench.voice.append_and_detect(id, vec![0; RATE * 3]).await.unwrap();
    assert!(events(&sink).is_empty(), "silence alone is not a chunk");
    bench.voice.append_and_detect(id, speech(0.5)).await.unwrap();
    bench.voice.append_and_detect(id, pause()).await.unwrap();
    // 4 s of chunk audio now, after the speech: the pause ends it.
    assert_eq!(events(&sink).first(), Some(&end(0)));
    bench.voice.cancel_capture(id).unwrap();

    let sink = Arc::new(RecordingSink::default());
    let start = bench
        .voice
        .start_chunked_capture(
            config("custom", &server.url, TranscriptionRoute::Http),
            RATE as u32,
            vec![],
            own_detector(&threads),
            sink.clone(),
        )
        .unwrap();
    bench.voice.append_and_detect(&start.session_id, speech(1.0)).await.unwrap();
    bench.voice.append_and_detect(&start.session_id, pause()).await.unwrap();
    assert!(events(&sink).is_empty(), "1.5 s of chunk audio");
}

// VOP-FR-YDVH and the non-functional requirement: the detector of a
// Push-to-Talk capture runs on its own thread, not on an async worker.
#[tokio::test]
async fn the_push_to_talk_detector_runs_on_its_own_thread() {
    let bench = Bench::new();
    let server = by_length(false, false).await;
    let threads = Arc::new(Mutex::new(Vec::new()));
    let start = bench
        .voice
        .start_chunked_capture(
            config("custom", &server.url, TranscriptionRoute::Http),
            RATE as u32,
            vec![],
            own_detector(&threads),
            Arc::new(RecordingSink::default()),
        )
        .unwrap();
    bench.voice.append_and_detect(&start.session_id, speech(0.5)).await.unwrap();
    let names = threads.lock().unwrap().clone();
    assert!(!names.is_empty());
    assert!(names.iter().all(|n| n == DETECTOR_THREAD), "{names:?}");
}

// VOP-FR-GLDR, VOP-FR-DSSU, VOP-FR-UOYP, VOP-FR-SIKM: with no chunk that
// succeeded, the finish fails with the code of the last failure, and the
// session stays. A retry sends all held audio as one request, with no chunk
// event.
#[tokio::test]
async fn a_capture_with_no_good_chunk_fails_and_retries_as_one_request() {
    let bench = Bench::new();
    let healthy = Arc::new(AtomicBool::new(false));
    let flag = healthy.clone();
    let server = fake(
        move |_, _| {
            if flag.load(Ordering::SeqCst) {
                transcribed("All of it.")
            } else {
                HttpReply::status(500)
            }
        },
        None,
    )
    .await;
    let sink = Arc::new(RecordingSink::default());
    let threads = Arc::new(Mutex::new(Vec::new()));
    let start = bench
        .voice
        .start_chunked_capture(
            config("custom", &server.url, TranscriptionRoute::Http),
            RATE as u32,
            vec![],
            own_detector(&threads),
            sink.clone(),
        )
        .unwrap();
    let id = &start.session_id;
    bench.voice.append_and_detect(id, speech(2.5)).await.unwrap();
    bench.voice.append_and_detect(id, pause()).await.unwrap();
    bench.voice.append_and_detect(id, speech(1.0)).await.unwrap();
    assert_eq!(bench.voice.finish_capture(id).await.unwrap_err(), ERR_TRANSCRIPTION_FAILED);
    let failed = (ChunkState::Failed, Some(ERR_TRANSCRIPTION_FAILED.to_string()));
    let seen: Vec<_> = events(&sink).into_iter().map(|(i, s, _, c)| (i, s, c)).collect();
    assert_eq!(
        seen,
        vec![
            (0, ChunkState::End, None),
            (0, failed.0, failed.1.clone()),
            (1, ChunkState::End, None),
            (1, failed.0, failed.1),
        ]
    );

    healthy.store(true, Ordering::SeqCst);
    let before = events(&sink).len();
    let text = bench.voice.retry_capture(id, sink.clone()).await.unwrap();
    assert_eq!(text, "All of it.");
    assert_eq!(events(&sink).len(), before, "a retry emits no chunk event");
    let seen = server.http.lock().unwrap().clone();
    let last = seen.last().unwrap().body.len();
    // 4 s at 16 kHz is 128 000 bytes of PCM in the WAV.
    assert!(last > 128_000 && last < 130_000, "{last}");
}

// VOP-FR-GLDR, VOP-FR-DSSU: a chunk that failed is left out, and the texts that
// succeeded make the result.
#[tokio::test]
async fn a_failed_chunk_is_left_out_of_the_result() {
    let bench = Bench::new();
    let server = by_length(true, false).await;
    let sink = Arc::new(RecordingSink::default());
    let threads = Arc::new(Mutex::new(Vec::new()));
    let start = bench
        .voice
        .start_chunked_capture(
            config("custom", &server.url, TranscriptionRoute::Http),
            RATE as u32,
            vec![],
            own_detector(&threads),
            sink.clone(),
        )
        .unwrap();
    let id = &start.session_id;
    bench.voice.append_and_detect(id, speech(2.5)).await.unwrap();
    bench.voice.append_and_detect(id, pause()).await.unwrap();
    bench.voice.append_and_detect(id, speech(1.0)).await.unwrap();
    assert_eq!(bench.voice.finish_capture(id).await.unwrap(), "Two.");
    let states: Vec<_> = events(&sink).into_iter().map(|(i, s, _, _)| (i, s)).collect();
    assert_eq!(
        states,
        vec![
            (0, ChunkState::End),
            (0, ChunkState::Failed),
            (1, ChunkState::End),
            (1, ChunkState::Transcript),
        ]
    );
}

// VOP-FR-GLRN, VOP-FR-WNQW, VOP-FR-JAOB: the `openai` dialect keeps one socket
// and commits at each chunk end. The live text that ends a sentence lets the
// pause end the chunk, the chunk text is the text of the item that the commit
// names, and the live text restarts for the open chunk.
#[tokio::test]
async fn an_openai_capture_commits_each_chunk_on_one_socket() {
    let bench = Bench::new();
    let server = fake(
        |_, _| HttpReply::status(404),
        ws_handler(|_, mut socket| async move {
            send(
                &mut socket,
                json!({ "type": "session.created", "session": { "type": "transcription" } }),
            )
            .await;
            let delta = |item: &str, text: &str| {
                json!({
                    "type": "conversation.item.input_audio_transcription.delta",
                    "item_id": item, "delta": text,
                })
            };
            send(&mut socket, delta("a", "Hello.")).await;
            for (item, text) in [("a", "Hello."), ("b", "Bye.")] {
                recv_type(&mut socket, "input_audio_buffer.commit").await;
                send(
                    &mut socket,
                    json!({ "type": "input_audio_buffer.committed", "item_id": item }),
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
                if item == "a" {
                    send(&mut socket, delta("b", "Bye")).await;
                }
            }
            recv(&mut socket).await;
        }),
    )
    .await;
    let sink = Arc::new(RecordingSink::default());
    let threads = Arc::new(Mutex::new(Vec::new()));
    let voice = &bench.voice;
    let start = voice
        .start_chunked_capture(
            config("openai", &format!("{}/v1", server.url), TranscriptionRoute::Realtime),
            RATE as u32,
            vec![],
            own_detector(&threads),
            sink.clone(),
        )
        .unwrap();
    assert!(start.live);
    let id = &start.session_id;
    voice.append_and_detect(id, speech(0.5)).await.unwrap();
    until(|| sink.transcripts.lock().unwrap().iter().any(|t| t == "Hello.")).await;
    voice.append_and_detect(id, pause()).await.unwrap();
    assert_eq!(events(&sink).first(), Some(&end(0)));
    until(|| sink.transcripts.lock().unwrap().last().is_some_and(|t| t == "Bye")).await;
    voice.append_and_detect(id, speech(0.5)).await.unwrap();

    assert_eq!(voice.finish_capture(id).await.unwrap(), "Hello. Bye.");
    assert_eq!(events(&sink), vec![end(0), transcript(0, "Hello."), end(1), transcript(1, "Bye.")]);
    assert_eq!(server.ws.lock().unwrap().len(), 1, "one socket");
    let shown = sink.transcripts.lock().unwrap().clone();
    assert!(shown.iter().all(|t| !t.contains("Hello.\nBye")), "{shown:?}");
}

// VOP-FR-GLRN: a dialect whose session ends at a commit opens a new session for
// the next chunk.
#[tokio::test]
async fn a_llama_capture_opens_a_session_for_each_chunk() {
    let bench = Bench::new();
    let connections = Arc::new(AtomicUsize::new(0));
    let count = connections.clone();
    let server = fake(
        |_, _| HttpReply::status(404),
        ws_handler(move |_, mut socket| {
            let index = count.fetch_add(1, Ordering::SeqCst);
            async move {
                let text = ["Part one.", "Part two."][index.min(1)];
                send(&mut socket, llama_created("transcription", 16_000, None)).await;
                send(
                    &mut socket,
                    json!({
                        "type": "conversation.item.input_audio_transcription.delta",
                        "item_id": "a", "delta": text,
                    }),
                )
                .await;
                let commit = recv_type(&mut socket, "input_audio_buffer.commit").await.unwrap();
                assert_eq!(commit["final"], true);
                send(
                    &mut socket,
                    json!({
                        "type": "conversation.item.input_audio_transcription.completed",
                        "item_id": "a", "transcript": text,
                    }),
                )
                .await;
                let _ = socket.close(None).await;
            }
        }),
    )
    .await;
    let sink = Arc::new(RecordingSink::default());
    let threads = Arc::new(Mutex::new(Vec::new()));
    let voice = &bench.voice;
    let start = voice
        .start_chunked_capture(
            config("custom", &server.url, TranscriptionRoute::Realtime),
            RATE as u32,
            vec![],
            own_detector(&threads),
            sink.clone(),
        )
        .unwrap();
    let id = &start.session_id;
    voice.append_and_detect(id, speech(0.5)).await.unwrap();
    until(|| sink.transcripts.lock().unwrap().iter().any(|t| t == "Part one.")).await;
    voice.append_and_detect(id, pause()).await.unwrap();
    until(|| sink.transcripts.lock().unwrap().iter().any(|t| t == "Part two.")).await;
    voice.append_and_detect(id, speech(0.5)).await.unwrap();
    assert_eq!(voice.finish_capture(id).await.unwrap(), "Part one. Part two.");
    assert_eq!(server.ws.lock().unwrap().len(), 2, "one session for each chunk");
    assert_eq!(
        events(&sink),
        vec![end(0), transcript(0, "Part one."), end(1), transcript(1, "Part two.")]
    );
}

// VOP-FR-WCSB, VOP-FR-KBPZ: a session that had `speech_end` does not count as
// open, so the next one starts. A session that records still refuses a start.
#[tokio::test]
async fn an_ended_session_does_not_block_the_next_start() {
    let bench = Bench::new();
    let server = by_length(false, false).await;
    let active = || config("custom", &server.url, TranscriptionRoute::Http);
    let sink = || -> Arc<dyn VoiceSink> { Arc::new(RecordingSink::default()) };
    let first = bench.voice.start_capture(active(), 16_000, vec![], sink()).unwrap();
    bench.voice.append_capture(&first.session_id, speech(1.0)).unwrap();
    bench.voice.end_capture_input(&first.session_id).unwrap();
    let second = bench.voice.start_capture(active(), 16_000, vec![], sink()).unwrap();
    assert_eq!(
        bench.voice.start_capture(active(), 16_000, vec![], sink()).unwrap_err(),
        ERR_SESSION_ACTIVE
    );
    assert_eq!(
        bench.voice.append_capture(&first.session_id, speech(0.1)).unwrap_err(),
        ERR_SESSION_LOST,
        "the input of an ended session is over"
    );
    assert_eq!(bench.voice.finish_capture(&first.session_id).await.unwrap(), "Two.");
    bench.voice.append_capture(&second.session_id, speech(0.1)).unwrap();
}

// VOP-FR-WCSB: a session is finishing from the call of "finish voice capture",
// and the next start does not wait for its result.
#[tokio::test]
async fn a_finishing_session_does_not_block_the_next_start() {
    let bench = Bench::new();
    let committed = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let (flag, gate) = (committed.clone(), release.clone());
    let server = fake(
        |_, _| HttpReply::status(404),
        ws_handler(move |_, mut socket| {
            let (flag, gate) = (flag.clone(), gate.clone());
            async move {
                send(&mut socket, llama_created("transcription", 16_000, None)).await;
                recv_type(&mut socket, "input_audio_buffer.commit").await;
                flag.store(true, Ordering::SeqCst);
                // The transcript waits until the next start was tried.
                until(|| gate.load(Ordering::SeqCst)).await;
                send(
                    &mut socket,
                    json!({
                        "type": "conversation.item.input_audio_transcription.completed",
                        "item_id": "a", "transcript": "Late.",
                    }),
                )
                .await;
                let _ = socket.close(None).await;
            }
        }),
    )
    .await;
    let active = || config("custom", &server.url, TranscriptionRoute::Realtime);
    let sink = || -> Arc<dyn VoiceSink> { Arc::new(RecordingSink::default()) };
    let first = bench.voice.start_capture(active(), 16_000, vec![], sink()).unwrap();
    bench.voice.append_capture(&first.session_id, speech(0.2)).unwrap();
    let finish = bench.voice.finish_capture(&first.session_id);
    let next = async {
        until(|| committed.load(Ordering::SeqCst)).await;
        let second = bench.voice.start_capture(active(), 16_000, vec![], sink());
        release.store(true, Ordering::SeqCst);
        second
    };
    let (finished, second) = tokio::join!(finish, next);
    assert!(second.is_ok(), "the next start opens while the first finishes");
    assert_eq!(finished.unwrap(), "Late.");
}

// VOP-FR-DSSU: the events of one capture come in index order, whatever order
// the transcriptions end in.
#[test]
fn chunk_events_come_in_index_order() {
    let sink = Arc::new(RecordingSink::default());
    let mut ledger = ChunkLedger::new("voice-1", sink.clone());
    assert_eq!(ledger.end(), 0);
    assert_eq!(ledger.end(), 1);
    ledger.resolve(1, Ok(" second ".to_string()));
    assert_eq!(events(&sink), vec![end(0)]);
    ledger.resolve(0, Err(ERR_PROVIDER_UNREACHABLE.to_string()));
    assert_eq!(
        events(&sink),
        vec![
            end(0),
            (0, ChunkState::Failed, None, Some(ERR_PROVIDER_UNREACHABLE.to_string())),
            end(1),
            transcript(1, "second"),
        ]
    );
    assert_eq!(ledger.outcome().unwrap(), "second");
}

// VOP-FR-GLDR: empty chunk texts are a success, and they add no space.
#[test]
fn empty_chunk_texts_succeed_with_no_extra_space() {
    let sink = Arc::new(RecordingSink::default());
    let mut ledger = ChunkLedger::new("voice-1", sink);
    for text in ["One.", "", "Two."] {
        let index = ledger.end();
        ledger.resolve(index, Ok(text.to_string()));
    }
    assert_eq!(ledger.outcome().unwrap(), "One. Two.");
    let mut failed = ChunkLedger::new("voice-2", Arc::new(RecordingSink::default()));
    let index = failed.end();
    failed.resolve(index, Err(ERR_PROVIDER_REJECTED.to_string()));
    let index = failed.end();
    failed.resolve(index, Err(ERR_TRANSCRIPTION_FAILED.to_string()));
    assert_eq!(failed.outcome().unwrap_err(), ERR_TRANSCRIPTION_FAILED, "the last failure");
}

// VOP-FR-DSSU: the wire names of the event, which the WebView reads.
#[test]
fn the_chunk_event_payload_has_the_names_of_the_webview() {
    let change = ChunkChange {
        capture_id: "voice-1".to_string(),
        index: 2,
        state: ChunkState::Transcript,
        text: Some("Hi.".to_string()),
        code: None,
    };
    assert_eq!(
        change.payload(),
        json!({ "captureId": "voice-1", "index": 2, "state": "chunk_transcript", "text": "Hi.", "code": null })
    );
    for (state, name) in [(ChunkState::End, "chunk_end"), (ChunkState::Failed, "chunk_failed")] {
        assert_eq!(ChunkChange { state, ..change.clone() }.payload()["state"], name);
    }
    assert_eq!(VOICE_CHUNK_CHANGED, "voice-chunk-changed");
}

// VOP-FR-WNQW: a sentence end, with closing quotes and brackets ignored.
#[test]
fn a_sentence_end_ignores_closing_quotes_and_brackets() {
    for text in ["Done.", "Why?", "Stop!", "Wait…", "He said \"yes.\"", "(fine!) ", "«oui.»"] {
        assert!(is_sentence_end(text), "{text}");
    }
    for text in ["", "Hello", "a, b", "(open", "1.5 m"] {
        assert!(!is_sentence_end(text), "{text}");
    }
}

// VOP-FR-JRVF: the short pause is 400 ms, or half of the silence timeout when
// that is less, and it is shorter than the silence timeout.
#[test]
fn the_short_pause_is_400_ms_or_half_the_silence() {
    assert_eq!(Settings::new(1500, VoiceVadSensitivity::Medium).short_pause_frames(), 13);
    assert_eq!(Settings::new(500, VoiceVadSensitivity::Medium).short_pause_frames(), 8);
    assert_eq!(Settings::new(5000, VoiceVadSensitivity::Medium).short_pause_frames(), 13);
}

// VOP-FR-JRVF: the pause count works as the silence count of VOP-FR-OFDY, and
// a pause counts only after speech since the last chunk end.
#[test]
fn a_pause_follows_speech_and_noise_does_not_clear_it() {
    let mut pauses = PauseTracker::new(Settings::new(500, VoiceVadSensitivity::Medium));
    for _ in 0..20 {
        pauses.step(0.05);
    }
    assert!(!pauses.paused(), "no speech yet");
    pauses.step(0.9);
    for _ in 0..7 {
        pauses.step(0.05);
    }
    assert!(!pauses.paused());
    pauses.step(0.4);
    assert!(pauses.paused(), "a frame between the thresholds does not clear the count");
    pauses.chunk_ended();
    assert!(!pauses.paused());
    pauses.step(0.9);
    assert!(!pauses.paused());
}

// VOP-FR-KIMU: a word gap needs speech before it, and it reports one time.
#[test]
fn a_word_gap_follows_speech_and_reports_one_time() {
    use crate::voice_ai::activity::detector::{GapTracker, WORD_GAP_FRAMES};
    assert_eq!(WORD_GAP_FRAMES, 3, "VOP-FR-KIMU: 3 detector frames");
    let mut gaps = GapTracker::new(Settings::new(500, VoiceVadSensitivity::Medium));
    assert!((0..10).all(|_| !gaps.step(0.05)), "silence alone is no gap");
    assert!(!gaps.step(0.9));
    let found: Vec<bool> = (0..WORD_GAP_FRAMES + 3).map(|_| gaps.step(0.05)).collect();
    assert_eq!(found.iter().filter(|g| **g).count(), 1);
    assert!(found[WORD_GAP_FRAMES as usize - 1]);
}
