//! The state machine of a VAD session, on scripted probabilities. Time is a
//! count of frames, so no test waits.

use crate::global_settings::VoiceVadSensitivity;
use crate::voice_ai::activity::detector::{Action, Conditions, Detector, Settings};

const SPEECH: f32 = 0.9;
const QUIET: f32 = 0.05;
const IDLE: Conditions = Conditions { blocked: false, playing: false };
const PLAYING: Conditions = Conditions { blocked: false, playing: true };
/// 500 ms of silence and the 200 ms tail, in frames of 32 ms.
const SHORTEST_END: usize = 22;

fn settings() -> Settings {
    Settings::new(500, VoiceVadSensitivity::Medium)
}

fn run(detector: &mut Detector, probabilities: &[f32], conditions: Conditions) -> Vec<Action> {
    probabilities.iter().filter_map(|p| detector.step(*p, settings(), conditions)).collect()
}

fn confirmed() -> Detector {
    let mut detector = Detector::new();
    let actions = run(&mut detector, &[SPEECH; 11], IDLE);
    assert_eq!(actions, vec![Action::SpeechStart, Action::Confirm]);
    detector
}

// VOP-FR-DICU: the event comes at the first frame, with no wait for 300 ms.
#[test]
fn speech_start_comes_at_the_first_frame_over_the_threshold() {
    let mut detector = Detector::new();
    assert_eq!(detector.step(QUIET, settings(), IDLE), None);
    assert_eq!(detector.step(SPEECH, settings(), IDLE), Some(Action::SpeechStart));
}

// VOP-FR-OORS: each level sets the start threshold.
#[test]
fn each_sensitivity_level_sets_the_start_threshold() {
    for (level, under, over) in [
        (VoiceVadSensitivity::Low, 0.69, 0.70),
        (VoiceVadSensitivity::Medium, 0.49, 0.50),
        (VoiceVadSensitivity::High, 0.34, 0.35),
    ] {
        let chosen = Settings::new(1500, level);
        assert_eq!(Detector::new().step(under, chosen, IDLE), None, "{level:?}");
        let start = Detector::new().step(over, chosen, IDLE);
        assert_eq!(start, Some(Action::SpeechStart), "{level:?}");
    }
}

// VOP-FR-DBEE: the start threshold rises while a reply plays, to at most 0.90.
#[test]
fn a_reply_that_plays_raises_the_start_threshold() {
    assert_eq!(Detector::new().step(0.69, settings(), PLAYING), None);
    assert_eq!(Detector::new().step(0.70, settings(), PLAYING), Some(Action::SpeechStart));

    let low = Settings::new(500, VoiceVadSensitivity::Low);
    assert_eq!(Detector::new().step(0.89, low, PLAYING), None);
    assert_eq!(Detector::new().step(0.90, low, PLAYING), Some(Action::SpeechStart));
}

// VOP-FR-DBEE: the end threshold does not rise, so a word between the raised
// and the base start threshold does not end an utterance.
#[test]
fn the_end_threshold_stays_when_a_reply_plays() {
    let mut detector = Detector::new();
    assert_eq!(detector.step(0.8, settings(), PLAYING), Some(Action::SpeechStart));
    let actions = run(&mut detector, &[0.4; 10], PLAYING);
    assert_eq!(actions, vec![Action::Confirm]);
}

// VOP-FR-OORS: the end threshold of each level is 0.15 under its start.
#[test]
fn each_sensitivity_level_sets_the_end_threshold() {
    for (level, over, under) in [
        (VoiceVadSensitivity::Low, 0.55, 0.54),
        (VoiceVadSensitivity::Medium, 0.35, 0.34),
        (VoiceVadSensitivity::High, 0.20, 0.19),
    ] {
        let chosen = Settings::new(500, level);
        let mut stays = Detector::new();
        stays.step(SPEECH, chosen, IDLE);
        let low: Vec<_> = (0..3).filter_map(|_| stays.step(over, chosen, IDLE)).collect();
        assert_eq!(low, vec![], "{level:?}");

        let mut ends = Detector::new();
        ends.step(SPEECH, chosen, IDLE);
        let low: Vec<_> = (0..3).filter_map(|_| ends.step(under, chosen, IDLE)).collect();
        assert_eq!(low, vec![Action::Misfire], "{level:?}");
    }
}

// VOP-FR-DBEE: the margin at the High level.
#[test]
fn the_margin_applies_at_the_high_level() {
    let high = Settings::new(500, VoiceVadSensitivity::High);
    assert_eq!(Detector::new().step(0.54, high, PLAYING), None);
    assert_eq!(Detector::new().step(0.55, high, PLAYING), Some(Action::SpeechStart));
}

// `VOM-voice-mode.md` VOM-FR-DGOG: speech of 288 ms that ends at the confirmation
// frame stays under 300 ms, so it opens no capture.
#[test]
fn speech_that_ends_at_the_confirmation_is_a_misfire() {
    let mut detector = Detector::new();
    let mut script = vec![SPEECH; 9];
    script.extend([QUIET; 3]);
    assert_eq!(run(&mut detector, &script, IDLE), vec![Action::SpeechStart, Action::Misfire]);
}

// VOP-FR-KYFC: a blocked session starts no utterance.
#[test]
fn a_blocked_session_starts_nothing() {
    let blocked = Conditions { blocked: true, playing: false };
    assert_eq!(Detector::new().step(SPEECH, settings(), blocked), None);
}

// VOP-FR-KYFC: an utterance in progress continues.
#[test]
fn a_block_does_not_end_an_utterance_in_progress() {
    let mut detector = confirmed();
    let blocked = Conditions { blocked: true, playing: false };
    assert_eq!(run(&mut detector, &[SPEECH; 20], blocked), vec![]);
}

// VOP-FR-QFRF: 3 frames in a row under the end threshold end unconfirmed speech.
#[test]
fn three_low_frames_in_a_row_before_the_confirmation_give_a_misfire() {
    let mut detector = Detector::new();
    let script = [SPEECH, QUIET, QUIET, QUIET];
    assert_eq!(run(&mut detector, &script, IDLE), vec![Action::SpeechStart, Action::Misfire]);
    assert!(detector.waiting());
}

// VOP-FR-QFRF: only frames in a row count.
#[test]
fn a_speech_frame_clears_the_low_count_before_the_confirmation() {
    let mut detector = Detector::new();
    let script =
        [SPEECH, QUIET, QUIET, SPEECH, QUIET, QUIET, SPEECH, SPEECH, SPEECH, SPEECH, SPEECH];
    assert_eq!(run(&mut detector, &script, IDLE), vec![Action::SpeechStart, Action::Confirm]);
}

// VOP-FR-GZPY: 10 frames after `speech_start`, not 9.
#[test]
fn the_confirmation_comes_at_ten_frames() {
    let mut detector = Detector::new();
    assert_eq!(run(&mut detector, &[SPEECH; 10], IDLE), vec![Action::SpeechStart]);
    assert_eq!(detector.step(SPEECH, settings(), IDLE), Some(Action::Confirm));
}

// VOP-FR-OFDY: the utterance ends at the silence timeout plus the tail, not
// earlier. 700 ms is 21.9 frames, so the 22nd frame ends it.
#[test]
fn silence_ends_an_utterance_at_the_timeout_plus_the_tail() {
    let mut detector = confirmed();
    assert_eq!(run(&mut detector, &[QUIET; SHORTEST_END - 1], IDLE), vec![]);
    assert_eq!(detector.step(QUIET, settings(), IDLE), Some(Action::SpeechEnd));
}

// VOP-FR-OFDY: speech clears the silence count.
#[test]
fn a_frame_of_speech_clears_the_silence_count() {
    let mut detector = confirmed();
    assert_eq!(run(&mut detector, &[QUIET; SHORTEST_END - 1], IDLE), vec![]);
    assert_eq!(detector.step(SPEECH, settings(), IDLE), None);
    assert_eq!(run(&mut detector, &[QUIET; SHORTEST_END - 1], IDLE), vec![]);
    assert_eq!(detector.step(QUIET, settings(), IDLE), Some(Action::SpeechEnd));
}

// VOP-FR-OFDY: a noise floor between the thresholds does not clear the count,
// so it cannot hold an utterance open.
#[test]
fn a_noise_floor_between_the_thresholds_does_not_hold_an_utterance_open() {
    let mut detector = confirmed();
    assert_eq!(detector.step(QUIET, settings(), IDLE), None);
    assert_eq!(run(&mut detector, &[0.4; SHORTEST_END - 2], IDLE), vec![]);
    assert_eq!(detector.step(0.4, settings(), IDLE), Some(Action::SpeechEnd));
}

// VOP-FR-OFDY: a neutral frame alone starts no silence count.
#[test]
fn a_neutral_frame_starts_no_silence_count() {
    let mut detector = confirmed();
    assert_eq!(run(&mut detector, &[0.4; 100], IDLE), vec![]);
}

// VOP-FR-OFDY: the threshold that clears the count is the base one. The reply
// is paused then, so the author's voice is all that reaches the microphone.
#[test]
fn the_base_start_threshold_clears_silence_for_an_utterance_that_began_during_playback() {
    let mut detector = Detector::new();
    assert_eq!(detector.step(0.8, settings(), PLAYING), Some(Action::SpeechStart));
    assert_eq!(run(&mut detector, &[0.6; 10], IDLE), vec![Action::Confirm]);
    assert_eq!(run(&mut detector, &[QUIET; SHORTEST_END - 1], IDLE), vec![]);
    assert_eq!(detector.step(0.6, settings(), IDLE), None);
    assert_eq!(run(&mut detector, &[QUIET; SHORTEST_END - 1], IDLE), vec![]);
}

// VOP-FR-GWZP: 10 minutes is 18750 frames, counted from `speech_start`.
#[test]
fn an_utterance_ends_at_ten_minutes_with_no_silence() {
    let mut detector = confirmed();
    let actions = run(&mut detector, &vec![SPEECH; 18739], IDLE);
    assert_eq!(actions, vec![]);
    assert_eq!(detector.step(SPEECH, settings(), IDLE), Some(Action::SpeechEnd));
}

// VOP-FR-FMRV: no utterance starts until the capture settled.
#[test]
fn no_utterance_starts_before_the_capture_settled() {
    let mut detector = confirmed();
    run(&mut detector, &[QUIET; SHORTEST_END], IDLE);
    assert_eq!(run(&mut detector, &[SPEECH; 40], IDLE), vec![]);
    detector.settled();
    assert_eq!(detector.step(SPEECH, settings(), IDLE), Some(Action::SpeechStart));
}

// VOP-FR-DAZW: after a failed open, speech starts nothing until a low frame.
#[test]
fn a_failed_open_waits_for_a_low_frame() {
    let mut detector = confirmed();
    detector.rearm();
    assert_eq!(run(&mut detector, &[SPEECH; 40], IDLE), vec![]);
    assert_eq!(detector.step(QUIET, settings(), IDLE), None);
    assert_eq!(detector.step(SPEECH, settings(), IDLE), Some(Action::SpeechStart));
}

// VOP-FR-NDEH: a misfire during playback refuses a start for 16 frames.
#[test]
fn a_misfire_during_playback_refuses_a_start_for_sixteen_frames() {
    let mut detector = Detector::new();
    assert_eq!(detector.step(0.8, settings(), PLAYING), Some(Action::SpeechStart));
    assert_eq!(run(&mut detector, &[QUIET; 3], PLAYING), vec![Action::Misfire]);
    assert_eq!(run(&mut detector, &[SPEECH; 16], PLAYING), vec![]);
    assert_eq!(detector.step(SPEECH, settings(), PLAYING), Some(Action::SpeechStart));
}

// VOP-FR-NDEH: with no reply, a misfire refuses nothing.
#[test]
fn a_misfire_with_no_reply_refuses_nothing() {
    let mut detector = Detector::new();
    let script = [SPEECH, QUIET, QUIET, QUIET, SPEECH];
    let expected = vec![Action::SpeechStart, Action::Misfire, Action::SpeechStart];
    assert_eq!(run(&mut detector, &script, IDLE), expected);
}

// VOP-FR-DSNZ: a stored value out of range is clamped.
#[test]
fn a_silence_out_of_range_is_clamped() {
    assert_eq!(
        Settings::new(10, VoiceVadSensitivity::Medium),
        Settings::new(500, VoiceVadSensitivity::Medium)
    );
    assert_eq!(
        Settings::new(90_000, VoiceVadSensitivity::Medium),
        Settings::new(5000, VoiceVadSensitivity::Medium)
    );
}

// VOP-FR-DSNZ: an utterance keeps the settings it started with.
#[test]
fn an_utterance_keeps_the_settings_it_started_with() {
    let mut detector = Detector::new();
    let long = Settings::new(5000, VoiceVadSensitivity::Medium);
    assert_eq!(detector.step(SPEECH, settings(), IDLE), Some(Action::SpeechStart));
    for _ in 0..10 {
        detector.step(SPEECH, long, IDLE);
    }
    for _ in 0..SHORTEST_END - 1 {
        assert_eq!(detector.step(QUIET, long, IDLE), None);
    }
    assert_eq!(detector.step(QUIET, long, IDLE), Some(Action::SpeechEnd));
}
