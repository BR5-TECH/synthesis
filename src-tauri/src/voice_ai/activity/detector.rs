//! The state machine of a VAD session (VOP-FR-DICU, VOP-FR-QFRF, VOP-FR-GZPY,
//! VOP-FR-OFDY, VOP-FR-GWZP, VOP-FR-FMRV, VOP-FR-DAZW, VOP-FR-NDEH).
//!
//! It takes one speech probability for each frame and gives what happened. It
//! has no clock: time is a count of frames, so a test scripts a whole utterance
//! with no wait.

use crate::global_settings::VoiceVadSensitivity;

/// VOP-FR-SMSX: the samples of one frame at 16 kHz.
pub(crate) const FRAME_SAMPLES: usize = 512;
const FRAME_MS: u32 = 32;
/// VOP-FR-GZPY: 320 ms, the first whole frame at or over 300 ms.
const CONFIRM_FRAMES: u32 = 10;
/// VOP-FR-GZPY: a paused reply leaves the model reporting speech for a time, so
/// only speech that outlasts this is the author's.
const CONFIRM_FRAMES_DURING_PLAYBACK: u32 = 20;
/// VOP-FR-QFRF.
const MISFIRE_FRAMES: u32 = 3;
/// VOP-FR-NDEH.
const COOLDOWN_FRAMES: u32 = 16;
const COOLDOWN_MAX_FRAMES: u32 = 256;
/// VOP-FR-OFDY: the tail of `VOM-voice-mode.md` VOM-FR-FTFU.
const TAIL_MS: u32 = 200;
/// VOP-FR-GWZP.
const LONGEST_FRAMES: u32 = 10 * 60 * 1000 / FRAME_MS;
/// VOP-FR-JRVF.
const SHORT_PAUSE_MS: u32 = 400;
/// VOP-FR-DSNZ.
pub(crate) const SILENCE_MIN_MS: u32 = 500;
pub(crate) const SILENCE_MAX_MS: u32 = 5000;
/// VOP-FR-OORS.
const END_BELOW_START: f32 = 0.15;
/// VOP-FR-DBEE.
const PLAYBACK_MARGIN: f32 = 0.20;
const PLAYBACK_START_CAP: f32 = 0.90;

/// The two preferences that shape an utterance, as read from the store.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    silence_ms: u32,
    sensitivity: VoiceVadSensitivity,
}

impl Settings {
    /// VOP-FR-DSNZ: a stored value out of range is clamped.
    pub(crate) fn new(silence_ms: u32, sensitivity: VoiceVadSensitivity) -> Self {
        Self { silence_ms: silence_ms.clamp(SILENCE_MIN_MS, SILENCE_MAX_MS), sensitivity }
    }

    /// VOP-FR-OORS.
    pub(crate) fn start(self) -> f32 {
        match self.sensitivity {
            VoiceVadSensitivity::Low => 0.70,
            VoiceVadSensitivity::Medium => 0.50,
            VoiceVadSensitivity::High => 0.35,
        }
    }

    /// VOP-FR-OORS.
    pub(crate) fn end(self) -> f32 {
        self.start() - END_BELOW_START
    }

    /// VOP-FR-DBEE.
    fn start_while_playing(self) -> f32 {
        (self.start() + PLAYBACK_MARGIN).min(PLAYBACK_START_CAP)
    }

    /// VOP-FR-JRVF: the frames of the short pause that ends a chunk: 400 ms, or
    /// half of the silence timeout when that is less.
    pub(crate) fn short_pause_frames(self) -> u32 {
        SHORT_PAUSE_MS.min(self.silence_ms / 2).div_ceil(FRAME_MS)
    }

    /// VOP-FR-OFDY: the frames of the silence timeout and the tail.
    fn end_frames(self) -> u32 {
        (self.silence_ms + TAIL_MS).div_ceil(FRAME_MS)
    }
}

/// What the gate says about this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Conditions {
    pub(crate) blocked: bool,
    pub(crate) playing: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    SpeechStart,
    Confirm,
    Misfire,
    SpeechEnd,
}

/// What one utterance took from the settings when it started, so that a change
/// applies only from the next utterance (VOP-FR-DSNZ).
///
/// `start` is the threshold of the sensitivity and not the raised one, because
/// the reply is paused by then and only the author's voice reaches the
/// microphone.
#[derive(Clone, Copy, Debug)]
struct Utterance {
    start: f32,
    end: f32,
    end_frames: u32,
    during_playback: bool,
}

#[derive(Clone, Copy, Debug)]
enum State {
    Idle,
    Pending { utterance: Utterance, frames: u32, low: u32 },
    Active { utterance: Utterance, frames: u32, silent: Option<u32> },
    Rearm,
}

/// VOP-FR-NDEH: 16 frames, doubling at each misfire in a row.
fn cooldown_after(misfires: u32) -> u32 {
    COOLDOWN_FRAMES.saturating_mul(1 << misfires.min(4)).min(COOLDOWN_MAX_FRAMES)
}

pub(crate) struct Detector {
    state: State,
    cooldown: u32,
    /// VOP-FR-NDEH: misfires in a row during playback.
    misfires: u32,
    /// Frames with no reply playing, which reset `misfires`.
    quiet: u32,
}

impl Detector {
    pub(crate) fn new() -> Self {
        Self { state: State::Idle, cooldown: 0, misfires: 0, quiet: 0 }
    }

    /// Whether no utterance is in progress, so that the settings can be read.
    pub(crate) fn waiting(&self) -> bool {
        matches!(self.state, State::Idle | State::Rearm)
    }

    /// One frame. At most one action comes from one frame.
    pub(crate) fn step(
        &mut self,
        probability: f32,
        settings: Settings,
        conditions: Conditions,
    ) -> Option<Action> {
        match self.state {
            State::Idle => self.idle(probability, settings, conditions),
            State::Pending { utterance, frames, low } => {
                self.pending(probability, utterance, frames + 1, low)
            }
            State::Active { utterance, frames, silent } => {
                self.active(probability, utterance, frames + 1, silent)
            }
            State::Rearm => {
                if probability < settings.end() {
                    self.state = State::Idle;
                }
                None
            }
        }
    }

    fn idle(
        &mut self,
        probability: f32,
        settings: Settings,
        conditions: Conditions,
    ) -> Option<Action> {
        // A pause also clears `playing`, and the gate that follows a misfire comes
        // late. So one frame with no reply does not reset the count.
        self.quiet = if conditions.playing { 0 } else { self.quiet + 1 };
        if self.quiet >= COOLDOWN_FRAMES {
            self.misfires = 0;
        }
        if self.cooldown > 0 {
            self.cooldown -= 1;
            return None;
        }
        let threshold =
            if conditions.playing { settings.start_while_playing() } else { settings.start() };
        if conditions.blocked || probability < threshold {
            return None;
        }
        let utterance = Utterance {
            start: settings.start(),
            end: settings.end(),
            end_frames: settings.end_frames(),
            during_playback: conditions.playing,
        };
        self.state = State::Pending { utterance, frames: 0, low: 0 };
        Some(Action::SpeechStart)
    }

    fn pending(
        &mut self,
        probability: f32,
        utterance: Utterance,
        frames: u32,
        low: u32,
    ) -> Option<Action> {
        let low = if probability < utterance.end { low + 1 } else { 0 };
        if low >= MISFIRE_FRAMES {
            self.state = State::Idle;
            self.cooldown = 0;
            if utterance.during_playback {
                self.cooldown = cooldown_after(self.misfires);
                self.misfires += 1;
            }
            return Some(Action::Misfire);
        }
        // A frame that is low can be the end of speech under 300 ms, so the
        // confirmation waits to see it (`VOM-voice-mode.md` VOM-FR-DGOG).
        let confirm =
            if utterance.during_playback { CONFIRM_FRAMES_DURING_PLAYBACK } else { CONFIRM_FRAMES };
        if frames >= confirm && low == 0 {
            self.misfires = 0;
            self.state = State::Active { utterance, frames, silent: None };
            return Some(Action::Confirm);
        }
        self.state = State::Pending { utterance, frames, low };
        None
    }

    fn active(
        &mut self,
        probability: f32,
        utterance: Utterance,
        frames: u32,
        silent: Option<u32>,
    ) -> Option<Action> {
        let silent = if probability >= utterance.start {
            None
        } else if let Some(count) = silent {
            Some(count + 1)
        } else if probability < utterance.end {
            Some(1)
        } else {
            None
        };
        if frames >= LONGEST_FRAMES || silent.is_some_and(|count| count >= utterance.end_frames) {
            // VOP-FR-FMRV: the next utterance can start at once.
            self.state = State::Idle;
            return Some(Action::SpeechEnd);
        }
        self.state = State::Active { utterance, frames, silent };
        None
    }

    /// VOP-FR-DAZW: the capture of this utterance did not open or went away.
    pub(crate) fn rearm(&mut self) {
        self.state = State::Rearm;
    }
}

/// VOP-FR-JRVF, VOP-FR-YDVH: finds the short pauses of a chunked capture.
///
/// The silence count works as VOP-FR-OFDY states: a frame under the end
/// threshold starts it, and only a frame at or above the start threshold clears
/// it. A pause counts only after speech since the last chunk end, so silence
/// alone does not make a chunk.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PauseTracker {
    start: f32,
    end: f32,
    short_frames: u32,
    silent: Option<u32>,
    heard: bool,
}

impl PauseTracker {
    pub(crate) fn new(settings: Settings) -> Self {
        Self {
            start: settings.start(),
            end: settings.end(),
            short_frames: settings.short_pause_frames(),
            silent: None,
            heard: false,
        }
    }

    /// A tracker for a capture that opens from confirmed speech (VOP-FR-GZPY).
    pub(crate) fn after_speech(settings: Settings) -> Self {
        Self { heard: true, ..Self::new(settings) }
    }

    pub(crate) fn step(&mut self, probability: f32) {
        if probability >= self.start {
            self.heard = true;
            self.silent = None;
        } else if let Some(count) = self.silent {
            self.silent = Some(count + 1);
        } else if probability < self.end {
            self.silent = Some(1);
        }
    }

    /// Whether the capture is in a short pause that follows speech.
    pub(crate) fn paused(&self) -> bool {
        self.heard && self.silent.is_some_and(|count| count >= self.short_frames)
    }

    /// A chunk ended: the next pause needs new speech.
    pub(crate) fn chunk_ended(&mut self) {
        self.heard = false;
    }
}

/// VOP-FR-KIMU: the detector frames under the end threshold that make a word
/// gap.
pub(crate) const WORD_GAP_FRAMES: u32 = 3;

/// VOP-FR-KIMU: finds the word gaps of a capture, where an interim commit can
/// cut the audio between two words.
///
/// The silence count works as in [`PauseTracker`]. A gap counts only after
/// speech, and it reports one time: the next gap needs new speech.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GapTracker {
    start: f32,
    end: f32,
    silent: Option<u32>,
    heard: bool,
    reported: bool,
}

impl GapTracker {
    /// A tracker for a capture that opens from confirmed speech.
    pub(crate) fn after_speech(settings: Settings) -> Self {
        Self {
            start: settings.start(),
            end: settings.end(),
            silent: None,
            heard: true,
            reported: false,
        }
    }

    /// A tracker for a capture that has heard no speech yet.
    pub(crate) fn new(settings: Settings) -> Self {
        Self { heard: false, ..Self::after_speech(settings) }
    }

    /// Takes one frame. Gives `true` at the frame that completes a word gap.
    pub(crate) fn step(&mut self, probability: f32) -> bool {
        if probability >= self.start {
            self.heard = true;
            self.reported = false;
            self.silent = None;
            return false;
        }
        if let Some(count) = self.silent {
            self.silent = Some(count + 1);
        } else if probability < self.end {
            self.silent = Some(1);
        }
        let gap = self.heard
            && !self.reported
            && self.silent.is_some_and(|count| count >= WORD_GAP_FRAMES);
        if gap {
            self.reported = true;
        }
        gap
    }
}
