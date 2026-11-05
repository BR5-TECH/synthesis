//! Chunks of a capture (`specifications/core/VOP-voice-ai-providers.md`,
//! "Chunks").
//!
//! A chunked capture ends a chunk at a short pause that follows a sentence end,
//! and transcribes the chunk in the background while the capture receives the
//! audio of the next chunk. The ledger puts the events of one capture in index
//! order. Audio and chunk text stay in memory (VOP-FR-YEFK).

use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;

use super::activity::detector::{
    tail_samples, GapTracker, PauseTracker, Settings, SilenceEdge, FRAME_SAMPLES,
};
use super::activity::silero::SpeechModel;
use super::pcm::StreamResampler;
use super::{SharedSink, ERR_SESSION_LOST};

/// VOP-FR-WNQW: the chunk audio that stands for a sentence end on a route with
/// no live text.
pub(crate) const NO_TEXT_SENTENCE_SECONDS: usize = 2;
const DETECTOR_RATE: u32 = 16_000;

/// VOP "Records": the step of a chunk that `voice-chunk-changed` reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChunkState {
    End,
    Transcript,
    Failed,
}

impl ChunkState {
    fn name(self) -> &'static str {
        match self {
            Self::End => "chunk_end",
            Self::Transcript => "chunk_transcript",
            Self::Failed => "chunk_failed",
        }
    }
}

/// VOP-FR-DSSU: what `voice-chunk-changed` carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChunkChange {
    pub(crate) capture_id: String,
    pub(crate) index: usize,
    pub(crate) state: ChunkState,
    pub(crate) text: Option<String>,
    pub(crate) code: Option<String>,
}

impl ChunkChange {
    pub(crate) fn payload(&self) -> serde_json::Value {
        serde_json::json!({
            "captureId": self.capture_id,
            "index": self.index,
            "state": self.state.name(),
            "text": self.text,
            "code": self.code,
        })
    }
}

/// VOP-FR-WNQW: text that ends with `.`, `?`, `!`, or `…`, with closing quotes
/// and brackets ignored.
pub(crate) fn is_sentence_end(text: &str) -> bool {
    const CLOSING: &[char] = &['"', '\'', '”', '’', '»', '›', ')', ']', '}', '」', '』'];
    let core =
        text.trim_end().trim_end_matches(|c: char| CLOSING.contains(&c) || c.is_whitespace());
    core.ends_with(['.', '?', '!', '…'])
}

/// VOP-FR-DSSU, VOP-FR-GLDR: the chunks of one capture and their results. It
/// emits the events in index order: the events of a chunk come after all
/// events of the chunks before it.
pub(crate) struct ChunkLedger {
    capture_id: String,
    sink: SharedSink,
    results: Vec<Option<Result<String, String>>>,
    /// The chunks whose `chunk_end` went out.
    ends_sent: usize,
    /// The chunks whose result went out.
    results_sent: usize,
    /// A cancelled capture emits nothing more.
    closed: bool,
}

impl ChunkLedger {
    pub(crate) fn new(capture_id: &str, sink: SharedSink) -> Self {
        Self {
            capture_id: capture_id.to_string(),
            sink,
            results: Vec::new(),
            ends_sent: 0,
            results_sent: 0,
            closed: false,
        }
    }

    pub(crate) fn count(&self) -> usize {
        self.results.len()
    }

    /// A chunk ended. Gives its index.
    pub(crate) fn end(&mut self) -> usize {
        self.results.push(None);
        self.flush();
        self.results.len() - 1
    }

    pub(crate) fn resolve(&mut self, index: usize, result: Result<String, String>) {
        if let Some(slot) = self.results.get_mut(index) {
            *slot = Some(result.map(|t| t.trim().to_string()));
        }
        self.flush();
    }

    pub(crate) fn close(&mut self) {
        self.closed = true;
    }

    fn emit(&self, index: usize, state: ChunkState, text: Option<String>, code: Option<String>) {
        if self.closed {
            return;
        }
        let change = ChunkChange { capture_id: self.capture_id.clone(), index, state, text, code };
        self.sink.chunk_changed(&change);
    }

    fn flush(&mut self) {
        loop {
            if self.ends_sent < self.results.len() && self.ends_sent == self.results_sent {
                self.emit(self.ends_sent, ChunkState::End, None, None);
                self.ends_sent += 1;
            }
            let Some(Some(result)) = self.results.get(self.results_sent).cloned() else {
                return;
            };
            if self.results_sent >= self.ends_sent {
                return;
            }
            match result {
                Ok(text) => self.emit(self.results_sent, ChunkState::Transcript, Some(text), None),
                Err(code) => self.emit(self.results_sent, ChunkState::Failed, None, Some(code)),
            }
            self.results_sent += 1;
        }
    }

    /// VOP-FR-GLDR: the texts that succeeded, in order, joined by one space. With
    /// no success, the code of the last failure.
    pub(crate) fn outcome(&self) -> Result<String, String> {
        let mut texts: Vec<&str> = Vec::new();
        let mut succeeded = false;
        let mut last_failure: Option<String> = None;
        for result in &self.results {
            match result {
                Some(Ok(text)) => {
                    succeeded = true;
                    if !text.is_empty() {
                        texts.push(text);
                    }
                }
                Some(Err(code)) => last_failure = Some(code.clone()),
                // A chunk with no result went with its task.
                None => last_failure = Some(ERR_SESSION_LOST.to_string()),
            }
        }
        match (succeeded, last_failure) {
            (false, Some(code)) => Err(code),
            _ => Ok(texts.join(" ")),
        }
    }
}

pub(crate) type SharedLedger = Arc<Mutex<ChunkLedger>>;

/// What gives a model to the detector of a Push-to-Talk capture.
pub(crate) type ModelLoader = Box<dyn FnOnce() -> Result<Box<dyn SpeechModel>, String> + Send>;

/// VOP-FR-ERDS, VOP-FR-YDVH: how a capture finds its short pauses.
pub(crate) enum Chunking {
    /// One transcript (VOP-FR-VZDI).
    Off,
    /// The detector of a VAD session reports the pauses.
    Session,
    /// The capture runs the local detector over its own audio.
    Own { settings: Settings, load: ModelLoader },
    /// One transcript (VOP-FR-VZDI). A live `openai` capture runs the local
    /// detector over its own audio for its word gaps alone (VOP-FR-KIMU).
    Gaps { settings: Settings, load: ModelLoader },
}

/// Send audio to a detector, and give the wait for its answer.
pub(crate) fn detect_with(
    detector: &Sender<DetectorMessage>,
    samples: &[i16],
) -> Option<oneshot::Receiver<DetectorAnswer>> {
    let (reply, answer) = oneshot::channel();
    detector.send(DetectorMessage::Audio(samples.to_vec(), reply)).ok()?;
    Some(answer)
}

/// What the detector of a Push-to-Talk capture found in one piece of audio.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DetectorAnswer {
    /// VOP-FR-JRVF: the capture is in a short pause after the audio.
    pub(crate) paused: bool,
    /// VOP-FR-KIMU: the audio holds a word gap.
    pub(crate) gap: bool,
    /// VOP-FR-HSJU: the speech end in the held audio, when the capture is in a
    /// silence after speech.
    pub(crate) speech_until: Option<usize>,
}

pub(crate) enum DetectorMessage {
    /// Audio at the capture rate. The reply tells what the audio holds.
    Audio(Vec<i16>, oneshot::Sender<DetectorAnswer>),
    ChunkEnded,
}

/// The name of the thread of a Push-to-Talk detector.
pub(crate) const DETECTOR_THREAD: &str = "voice-chunk-detector";

/// VOP-FR-YDVH and the non-functional requirement: the detector of a
/// Push-to-Talk capture runs on its own thread, not on an async worker. It ends
/// when the capture drops its sender.
pub(crate) fn spawn_detector(
    capture_rate: u32,
    settings: Settings,
    load: ModelLoader,
    sink: SharedSink,
) -> Option<Sender<DetectorMessage>> {
    let (sender, messages) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name(DETECTOR_THREAD.to_string())
        .spawn(move || run_detector(capture_rate, settings, load, messages, sink));
    match spawned {
        Ok(_) => Some(sender),
        Err(_) => None,
    }
}

fn run_detector(
    capture_rate: u32,
    settings: Settings,
    load: ModelLoader,
    messages: Receiver<DetectorMessage>,
    sink: SharedSink,
) {
    // With no model or no resampler, the capture has one chunk.
    let model = load().inspect_err(|_| sink.diagnostic("chunk_detector", "model_unavailable"));
    let resampler = StreamResampler::new(capture_rate, DETECTOR_RATE);
    let (Ok(mut model), Ok(mut resampler)) = (model, resampler) else {
        for message in messages {
            if let DetectorMessage::Audio(_, reply) = message {
                let _ = reply.send(DetectorAnswer::default());
            }
        }
        return;
    };
    let mut tracker = PauseTracker::new(settings);
    let mut gaps = GapTracker::new(settings);
    let mut edge = SilenceEdge::new(settings);
    // The frames that the model took. A frame position comes from this total,
    // so no rounding adds up.
    let mut frames: u64 = 0;
    let mut pending: Vec<i16> = Vec::new();
    let mut broken = false;
    for message in messages {
        match message {
            DetectorMessage::ChunkEnded => tracker.chunk_ended(),
            DetectorMessage::Audio(samples, reply) => {
                if !broken {
                    match resampler.push(&samples) {
                        Ok(out) => pending.extend(out),
                        Err(_) => broken = true,
                    }
                }
                let mut at = 0;
                let mut gap = false;
                while !broken && pending.len() - at >= FRAME_SAMPLES {
                    let frame: Vec<f32> = pending[at..at + FRAME_SAMPLES]
                        .iter()
                        .map(|s| f32::from(*s) / 32768.0)
                        .collect();
                    at += FRAME_SAMPLES;
                    match model.probability(&frame) {
                        Ok(probability) => {
                            tracker.step(probability);
                            gap |= gaps.step(probability);
                            edge.step(probability, frame_position(frames, capture_rate));
                            frames += 1;
                        }
                        Err(_) => {
                            sink.diagnostic("chunk_detector", "model_failed");
                            broken = true;
                        }
                    }
                }
                pending.drain(..at.min(pending.len()));
                let speech_until = (!broken)
                    .then(|| edge.silence_from())
                    .flatten()
                    .map(|from| (from + tail_samples(capture_rate)) as usize);
                let _ = reply.send(DetectorAnswer {
                    paused: !broken && tracker.paused(),
                    gap,
                    speech_until,
                });
            }
        }
    }
}

/// VOP-FR-HSJU: the position at `capture_rate` of the first sample of a frame.
/// The resampler drops its filter delay, so the frames line up with the held
/// audio.
pub(crate) fn frame_position(frame: u64, capture_rate: u32) -> u64 {
    frame * u64::from(capture_rate) * FRAME_SAMPLES as u64 / u64::from(DETECTOR_RATE)
}
