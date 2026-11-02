//! VAD sessions (`specifications/core/VOP-voice-ai-providers.md`, "Voice
//! activity").
//!
//! The WebView streams the microphone here. A worker thread for each session
//! resamples the audio, runs the detector, and owns each utterance: it opens a
//! capture from confirmed speech and sends the audio to it. All state of a
//! session changes on that thread, through messages, so a command cannot race
//! with a frame.

use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use tokio::sync::oneshot;

use super::pcm::StreamResampler;
use super::{VoiceAi, VoiceCaptureStart, ERR_SESSION_ACTIVE, ERR_SESSION_LOST};

pub(crate) mod commands;
pub(crate) mod detector;
pub(crate) mod ring;
pub(crate) mod silero;

use detector::{Action, Conditions, Detector, GapTracker, PauseTracker, Settings, FRAME_SAMPLES};
use ring::AudioRing;
use silero::SpeechModel;

/// VOP-FR-ADHQ.
pub const ERR_DETECTOR_UNAVAILABLE: &str = "voice_detector_unavailable";
/// VOP-FR-BTBM.
pub const ERR_DETECTOR_FAILED: &str = "voice_detector_failed";
/// VOP-FR-KUGV.
pub const ERR_NO_AUDIO: &str = "voice_no_audio";

const DETECTOR_RATE: u32 = 16_000;
/// VOP-FR-FAQA.
const PRE_ROLL_MS: u64 = 320;
/// The ring holds the pre-roll (320 ms), the speech up to the confirmation
/// (320 ms), and one slice of a feed (250 ms).
const RING_SECONDS: usize = 1;
/// VOP-FR-KUGV.
pub(crate) const FEED_LIMIT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityState {
    SpeechStart,
    SpeechConfirmed,
    SpeechEnd,
    Misfire,
    Failed,
}

impl ActivityState {
    fn name(self) -> &'static str {
        match self {
            Self::SpeechStart => "speech_start",
            Self::SpeechConfirmed => "speech_confirmed",
            Self::SpeechEnd => "speech_end",
            Self::Misfire => "misfire",
            Self::Failed => "failed",
        }
    }
}

/// VOP "Records": what `voice-activity-changed` carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityChange {
    pub(crate) activity_id: String,
    pub(crate) state: ActivityState,
    pub(crate) capture_id: Option<String>,
    pub(crate) live: Option<bool>,
    pub(crate) code: Option<String>,
    pub(crate) fatal: Option<bool>,
}

impl ActivityChange {
    pub(crate) fn payload(&self) -> serde_json::Value {
        serde_json::json!({
            "activityId": self.activity_id,
            "state": self.state.name(),
            "captureId": self.capture_id,
            "live": self.live,
            "code": self.code,
            "fatal": self.fatal,
        })
    }
}

/// VOP-FR-QZCE: one whole record, so that a lost or a late one leaves no wrong
/// value behind it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Gate {
    pub(crate) seq: u64,
    pub(crate) blocked: bool,
    pub(crate) playing: bool,
    /// VOP-FR-ERDS: the next capture opens chunked.
    pub(crate) chunked: bool,
    pub(crate) keyterms: Vec<String>,
}

pub(crate) enum Message {
    /// The sender waits for the answer, so a slow worker slows the feed
    /// (`VOM-voice-mode.md` non-functional: a backlog merges in the WebView).
    Feed(Vec<i16>, oneshot::Sender<()>),
    Gate(Gate),
    Close {
        discard: bool,
        reply: Option<oneshot::Sender<Option<String>>>,
    },
}

/// What a session needs from the rest of the application. A trait, so a test
/// runs a session with no `AppHandle`.
pub(crate) trait ActivityHost: Send {
    fn settings(&self) -> Settings;
    fn start_capture(
        &self,
        capture_rate: u32,
        keyterms: Vec<String>,
        chunked: bool,
    ) -> Result<VoiceCaptureStart, String>;
    fn append_capture(&self, capture_id: &str, samples: Vec<i16>) -> Result<(), String>;
    /// VOP-FR-JRVF: a short pause in a chunked capture. Gives whether a chunk
    /// ended.
    fn chunk_pause(&self, capture_id: &str) -> bool;
    /// VOP-FR-KIMU: a word gap in a capture, where an interim commit can go.
    fn word_gap(&self, capture_id: &str);
    /// VOP-FR-WCSB: the utterance of the capture ended.
    fn end_capture(&self, capture_id: &str);
    /// Whether the capture still exists, so that it has not settled.
    fn capture_alive(&self, capture_id: &str) -> bool;
    fn cancel_capture(&self, capture_id: &str);
    fn emit(&self, change: &ActivityChange);
    /// `detail` comes from the runtime or the resampler. It holds no audio.
    fn note_failure(&self, code: &str, detail: &str);
}

/// One VAD session. It lives on its worker thread.
pub(crate) struct Worker<H: ActivityHost> {
    id: String,
    capture_rate: u32,
    host: H,
    model: Box<dyn SpeechModel>,
    detector: Detector,
    settings: Settings,
    conditions: Conditions,
    keyterms: Vec<String>,
    /// VOP-FR-ERDS: what the last applied gate says.
    chunked: bool,
    last_seq: u64,
    resampler: StreamResampler,
    ring: AudioRing,
    /// Audio at 16 kHz that does not yet fill a frame.
    pending: Vec<i16>,
    frames: u64,
    /// The position of the frame that gave `speech_start`.
    start_position: u64,
    /// The position after the last sample that the capture received.
    cursor: u64,
    /// The capture of the utterance in progress.
    capture: Option<String>,
    /// VOP-FR-YDVH: the pause finder of a chunked capture in progress.
    pauses: Option<PauseTracker>,
    /// VOP-FR-KIMU: the word gap finder of the capture in progress.
    gaps: Option<GapTracker>,
    /// VOP-FR-KIMU: a word gap that the frames of this slice found.
    gap_found: bool,
    /// The captures that have `speech_end` and have not settled, oldest first
    /// (VOP-FR-WCSB, VOP-FR-FAAB).
    settling: Vec<String>,
    ended: bool,
}

impl<H: ActivityHost> Worker<H> {
    pub(crate) fn new(
        id: String,
        capture_rate: u32,
        keyterms: Vec<String>,
        host: H,
        model: Box<dyn SpeechModel>,
    ) -> Result<Self, String> {
        let resampler = StreamResampler::new(capture_rate, DETECTOR_RATE)?;
        let settings = host.settings();
        Ok(Self {
            id,
            capture_rate,
            host,
            model,
            detector: Detector::new(),
            settings,
            conditions: Conditions::default(),
            keyterms,
            chunked: false,
            last_seq: 0,
            resampler,
            ring: AudioRing::new(capture_rate as usize * RING_SECONDS),
            pending: Vec::new(),
            frames: 0,
            start_position: 0,
            cursor: 0,
            capture: None,
            pauses: None,
            gaps: None,
            gap_found: false,
            settling: Vec::new(),
            ended: false,
        })
    }

    /// VOP-FR-KUGV: only a feed moves the limit, so a gate cannot hide lost audio.
    pub(crate) fn run(mut self, messages: Receiver<Message>, feed_limit: Duration) {
        let mut deadline = Instant::now() + feed_limit;
        while !self.ended {
            match messages.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(message) => {
                    if matches!(message, Message::Feed(..)) {
                        deadline = Instant::now() + feed_limit;
                    }
                    self.handle(message);
                }
                Err(RecvTimeoutError::Timeout) => self.feed_stopped(),
                Err(RecvTimeoutError::Disconnected) => {
                    self.close(true);
                }
            }
        }
    }

    pub(crate) fn handle(&mut self, message: Message) {
        match message {
            Message::Feed(samples, done) => {
                self.feed(&samples);
                let _ = done.send(());
            }
            Message::Gate(gate) => self.apply_gate(gate),
            Message::Close { discard, reply } => {
                let open = self.close(discard);
                if let Some(reply) = reply {
                    let _ = reply.send(open);
                }
            }
        }
    }

    pub(crate) fn feed_stopped(&mut self) {
        self.cancel_all();
        self.fail(ERR_NO_AUDIO, "", true);
    }

    fn change(&self, state: ActivityState) -> ActivityChange {
        ActivityChange {
            activity_id: self.id.clone(),
            state,
            capture_id: None,
            live: None,
            code: None,
            fatal: None,
        }
    }

    fn fail(&mut self, code: &str, detail: &str, fatal: bool) {
        self.host.note_failure(code, detail);
        let change = ActivityChange {
            code: Some(code.to_string()),
            fatal: Some(fatal),
            ..self.change(ActivityState::Failed)
        };
        self.host.emit(&change);
        if fatal {
            self.ended = true;
        }
    }

    /// A backlog can merge into a chunk that is longer than the ring, so each
    /// slice is sent on before the next one enters the ring (VOP-FR-FAQA).
    fn feed(&mut self, samples: &[i16]) {
        let slice = (self.capture_rate as usize * RING_SECONDS / 4).max(1);
        for part in samples.chunks(slice) {
            self.feed_slice(part);
        }
    }

    fn feed_slice(&mut self, samples: &[i16]) {
        if self.ended {
            return;
        }
        // VOP-FR-DSNZ: a change applies from the next utterance.
        if self.detector.waiting() {
            self.settings = self.host.settings();
        }
        self.ring.push(samples);
        match self.resampler.push(samples) {
            Ok(resampled) => self.pending.extend(resampled),
            Err(detail) => return self.fatal_detector_failure(&detail),
        }
        let mut at = 0;
        while self.pending.len() - at >= FRAME_SAMPLES && !self.ended {
            let frame: Vec<f32> = self.pending[at..at + FRAME_SAMPLES]
                .iter()
                .map(|s| f32::from(*s) / 32768.0)
                .collect();
            at += FRAME_SAMPLES;
            self.frame(&frame);
        }
        self.pending.drain(..at);
        self.send_audio();
        self.check_gap();
        self.check_pause();
    }

    /// VOP-FR-KIMU: a word gap goes to the capture after the audio of the slice
    /// reached it, so an interim commit holds the audio before the gap.
    fn check_gap(&mut self) {
        if !std::mem::take(&mut self.gap_found) {
            return;
        }
        if let Some(capture_id) = self.capture.as_deref() {
            self.host.word_gap(capture_id);
        }
    }

    /// VOP-FR-JRVF, VOP-FR-YDVH: the session's detector finds the short pauses
    /// of a chunked capture, after the audio of the slice reached it.
    fn check_pause(&mut self) {
        let (Some(capture_id), Some(pauses)) = (self.capture.as_deref(), self.pauses.as_mut())
        else {
            return;
        };
        if pauses.paused() && self.host.chunk_pause(capture_id) {
            pauses.chunk_ended();
        }
    }

    fn fatal_detector_failure(&mut self, detail: &str) {
        self.cancel_all();
        self.fail(ERR_DETECTOR_FAILED, detail, true);
    }

    fn frame(&mut self, frame: &[f32]) {
        let position = self.frames * u64::from(self.capture_rate) * FRAME_SAMPLES as u64
            / u64::from(DETECTOR_RATE);
        self.frames += 1;
        let probability = match self.model.probability(frame) {
            Ok(probability) => probability,
            Err(detail) => return self.fatal_detector_failure(&detail),
        };
        if let Some(pauses) = self.pauses.as_mut() {
            pauses.step(probability);
        }
        if let Some(gaps) = self.gaps.as_mut() {
            self.gap_found |= gaps.step(probability);
        }
        match self.detector.step(probability, self.settings, self.conditions) {
            Some(Action::SpeechStart) => {
                self.start_position = position;
                self.host.emit(&self.change(ActivityState::SpeechStart));
            }
            Some(Action::Confirm) => self.open_capture(),
            Some(Action::Misfire) => self.host.emit(&self.change(ActivityState::Misfire)),
            Some(Action::SpeechEnd) => self.end_utterance(),
            None => {}
        }
    }

    fn pre_roll_samples(&self) -> u64 {
        u64::from(self.capture_rate) * PRE_ROLL_MS / 1000
    }

    /// VOP-FR-GZPY, VOP-FR-FYQX, VOP-FR-FAQA.
    fn open_capture(&mut self) {
        match self.host.start_capture(self.capture_rate, self.keyterms.clone(), self.chunked) {
            Ok(start) => {
                self.pauses = self.chunked.then(|| PauseTracker::after_speech(self.settings));
                self.gaps = Some(GapTracker::after_speech(self.settings));
                self.cursor = self.start_position.saturating_sub(self.pre_roll_samples());
                let change = ActivityChange {
                    capture_id: Some(start.session_id.clone()),
                    live: Some(start.live),
                    ..self.change(ActivityState::SpeechConfirmed)
                };
                self.capture = Some(start.session_id);
                self.host.emit(&change);
                self.send_audio();
            }
            Err(code) if code == ERR_SESSION_ACTIVE => {
                self.detector.rearm();
                self.host.emit(&self.change(ActivityState::Misfire));
            }
            Err(code) => {
                self.detector.rearm();
                self.fail(&code, "capture did not open", false);
            }
        }
    }

    /// VOP-FR-FAQA: all audio that the ring holds and the capture does not.
    fn send_audio(&mut self) {
        let Some(capture_id) = self.capture.clone() else { return };
        let samples = self.ring.since(self.cursor);
        self.cursor = self.ring.written();
        if samples.is_empty() {
            return;
        }
        if let Err(detail) = self.host.append_capture(&capture_id, samples) {
            // VOP-FR-BTBM.
            self.capture = None;
            self.pauses = None;
            self.gaps = None;
            self.detector.rearm();
            self.fail(ERR_SESSION_LOST, &detail, false);
        }
    }

    /// VOP-FR-OFDY: `speech_end` follows the last audio of the utterance.
    fn end_utterance(&mut self) {
        self.send_audio();
        // The last audio can fail, and that failure ended the utterance.
        let Some(capture_id) = self.capture.take() else { return };
        self.pauses = None;
        self.gaps = None;
        // VOP-FR-WCSB: the capture finishes, and the next utterance can open one.
        self.host.end_capture(&capture_id);
        let host = &self.host;
        self.settling.retain(|id| host.capture_alive(id));
        self.settling.push(capture_id.clone());
        let change = ActivityChange {
            capture_id: Some(capture_id),
            ..self.change(ActivityState::SpeechEnd)
        };
        self.host.emit(&change);
    }

    /// VOP-FR-QZCE, VOP-FR-KYFC, VOP-FR-ERDS.
    fn apply_gate(&mut self, gate: Gate) {
        if gate.seq < self.last_seq {
            return;
        }
        self.last_seq = gate.seq;
        self.conditions = Conditions { blocked: gate.blocked, playing: gate.playing };
        self.keyterms = gate.keyterms;
        self.chunked = gate.chunked;
    }

    fn cancel_all(&mut self) {
        self.pauses = None;
        self.gaps = None;
        let open: Vec<String> =
            self.capture.take().into_iter().chain(self.settling.drain(..)).collect();
        for capture_id in open {
            self.host.cancel_capture(&capture_id);
        }
    }

    /// VOP-FR-FAAB.
    fn close(&mut self, discard: bool) -> Option<String> {
        self.ended = true;
        if discard {
            self.cancel_all();
            return None;
        }
        self.send_audio();
        // A capture that has `speech_end` counts too: the reply can reach the
        // WebView before that event does.
        let host = &self.host;
        self.settling.retain(|id| host.capture_alive(id));
        self.capture.take().or_else(|| self.settling.pop())
    }
}

/// What the module keeps of the open session.
pub(crate) struct ActivityHandle {
    id: String,
    sender: Sender<Message>,
    thread: std::thread::JoinHandle<()>,
}

impl VoiceAi {
    fn activity_guard(&self) -> Result<std::sync::MutexGuard<'_, Option<ActivityHandle>>, String> {
        self.activity.lock().map_err(|e| format!("voice activity poisoned: {e}"))
    }

    /// VOP-FR-WYKO, VOP-FR-ADHQ: `load` runs on the worker thread, and a failure
    /// to load opens no session.
    pub(crate) async fn open_activity<H, L>(
        &self,
        capture_rate: u32,
        keyterms: Vec<String>,
        host: H,
        load: L,
    ) -> Result<String, String>
    where
        H: ActivityHost + 'static,
        L: FnOnce() -> Result<Box<dyn SpeechModel>, String> + Send + 'static,
    {
        self.end_activity()?;
        let id = format!("activity-{}", self.next_session.fetch_add(1, Ordering::Relaxed));
        let worker_id = id.clone();
        let (ready, is_ready) = oneshot::channel::<Result<(), String>>();
        let (sender, messages) = std::sync::mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("voice-activity".to_string())
            .spawn(move || {
                let worker = load()
                    .and_then(|model| Worker::new(worker_id, capture_rate, keyterms, host, model));
                match worker {
                    Ok(worker) => {
                        let _ = ready.send(Ok(()));
                        worker.run(messages, FEED_LIMIT);
                    }
                    Err(detail) => {
                        let _ = ready.send(Err(detail));
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        is_ready.await.map_err(|_| "the detector thread stopped".to_string())??;
        *self.activity_guard()? = Some(ActivityHandle { id: id.clone(), sender, thread });
        Ok(id)
    }

    /// VOP-FR-WYKO: a session of a reloaded webview cannot close itself, so the
    /// next open ends it. Application exit ends a session in the same way.
    pub(crate) fn end_activity(&self) -> Result<(), String> {
        let open = self.activity_guard()?.take();
        if let Some(handle) = open {
            let _ = handle.sender.send(Message::Close { discard: true, reply: None });
            let _ = handle.thread.join();
        }
        Ok(())
    }

    fn activity_sender(&self, activity_id: &str) -> Result<Sender<Message>, String> {
        let open = self.activity_guard()?;
        open.as_ref()
            .filter(|handle| handle.id == activity_id)
            .map(|handle| handle.sender.clone())
            .ok_or_else(|| ERR_SESSION_LOST.to_string())
    }

    /// VOP-FR-TADX.
    pub(crate) async fn feed_activity(
        &self,
        activity_id: &str,
        samples: Vec<i16>,
    ) -> Result<(), String> {
        let sender = self.activity_sender(activity_id)?;
        let (done, is_done) = oneshot::channel();
        sender.send(Message::Feed(samples, done)).map_err(|_| ERR_SESSION_LOST.to_string())?;
        is_done.await.map_err(|_| ERR_SESSION_LOST.to_string())
    }

    /// VOP-FR-QZCE: a gate for a session that went has no effect.
    pub(crate) fn set_activity_gate(&self, activity_id: &str, gate: Gate) -> Result<(), String> {
        if let Ok(sender) = self.activity_sender(activity_id) {
            let _ = sender.send(Message::Gate(gate));
        }
        Ok(())
    }

    /// VOP-FR-FAAB: a close for a session that went has no effect.
    pub(crate) async fn close_activity(
        &self,
        activity_id: &str,
        discard: bool,
    ) -> Result<Option<String>, String> {
        let handle = {
            let mut open = self.activity_guard()?;
            if open.as_ref().is_none_or(|handle| handle.id != activity_id) {
                return Ok(None);
            }
            open.take()
        };
        let Some(handle) = handle else { return Ok(None) };
        let (reply, answer) = oneshot::channel();
        let _ = handle.sender.send(Message::Close { discard, reply: Some(reply) });
        Ok(answer.await.unwrap_or(None))
    }
}
