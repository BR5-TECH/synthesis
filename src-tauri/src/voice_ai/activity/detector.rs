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
/// VOP-FR-QFRF.
const MISFIRE_FRAMES: u32 = 3;
/// VOP-FR-NDEH.
const COOLDOWN_FRAMES: u32 = 16;
/// VOP-FR-OFDY: the tail of `VOM-voice-mode.md` VOM-FR-FTFU.
const TAIL_MS: u32 = 200;
/// VOP-FR-GWZP.
const LONGEST_FRAMES: u32 = 10 * 60 * 1000 / FRAME_MS;
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
    fn start(self) -> f32 {
        match self.sensitivity {
            VoiceVadSensitivity::Low => 0.70,
            VoiceVadSensitivity::Medium => 0.50,
            VoiceVadSensitivity::High => 0.35,
        }
    }

    /// VOP-FR-OORS.
    fn end(self) -> f32 {
        self.start() - END_BELOW_START
    }

    /// VOP-FR-DBEE.
    fn start_while_playing(self) -> f32 {
        (self.start() + PLAYBACK_MARGIN).min(PLAYBACK_START_CAP)
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
    Settling,
    Rearm,
}

pub(crate) struct Detector {
    state: State,
    cooldown: u32,
}

impl Detector {
    pub(crate) fn new() -> Self {
        Self { state: State::Idle, cooldown: 0 }
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
            State::Settling => None,
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
            self.cooldown = if utterance.during_playback { COOLDOWN_FRAMES } else { 0 };
            return Some(Action::Misfire);
        }
        // A frame that is low can be the end of speech under 300 ms, so the
        // confirmation waits to see it (`VOM-voice-mode.md` VOM-FR-DGOG).
        if frames >= CONFIRM_FRAMES && low == 0 {
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
            self.state = State::Settling;
            return Some(Action::SpeechEnd);
        }
        self.state = State::Active { utterance, frames, silent };
        None
    }

    /// VOP-FR-FMRV: the gate named the capture as settled.
    pub(crate) fn settled(&mut self) {
        if matches!(self.state, State::Settling) {
            self.state = State::Idle;
        }
    }

    /// VOP-FR-DAZW: the capture of this utterance did not open or went away.
    pub(crate) fn rearm(&mut self) {
        self.state = State::Rearm;
    }
}
