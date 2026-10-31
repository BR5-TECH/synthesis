//! A VAD session, with a scripted model and a host that records each call. The
//! capture rate is 16 kHz, so a frame is 512 samples of the feed, and each
//! sample holds its own position.

use std::collections::VecDeque;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use tokio::sync::oneshot;

use super::support::Bench;
use crate::global_settings::VoiceVadSensitivity;
use crate::voice_ai::activity::detector::Settings;
use crate::voice_ai::activity::silero::SpeechModel;
use crate::voice_ai::activity::{
    ActivityChange, ActivityHost, ActivityState, Gate, Message, Worker, ERR_DETECTOR_FAILED,
    ERR_NO_AUDIO, FEED_LIMIT,
};
use crate::voice_ai::{
    TranscriptionRoute, VoiceCaptureStart, ERR_NO_ACTIVE_PROVIDER, ERR_SESSION_ACTIVE,
    ERR_SESSION_LOST,
};

const RATE: u32 = 16_000;
const FRAME: usize = 512;
const SPEECH: f32 = 0.9;
const QUIET: f32 = 0.05;
/// 320 ms at 16 kHz.
const PRE_ROLL: usize = 5120;

#[derive(Default)]
struct Calls {
    started: Vec<Vec<String>>,
    appended: Vec<(String, Vec<i16>)>,
    cancelled: Vec<String>,
    changes: Vec<ActivityChange>,
    /// The order of appends and changes, by name.
    order: Vec<String>,
    refusal: Option<String>,
    append_refusal: Option<String>,
    settings: Option<Settings>,
}

#[derive(Clone, Default)]
struct Host(Arc<Mutex<Calls>>);

impl ActivityHost for Host {
    fn settings(&self) -> Settings {
        self.0.lock().unwrap().settings.unwrap_or(Settings::new(500, VoiceVadSensitivity::Medium))
    }

    fn start_capture(&self, _: u32, keyterms: Vec<String>) -> Result<VoiceCaptureStart, String> {
        let mut calls = self.0.lock().unwrap();
        if let Some(code) = calls.refusal.clone() {
            return Err(code);
        }
        calls.started.push(keyterms);
        Ok(VoiceCaptureStart {
            session_id: format!("capture-{}", calls.started.len()),
            route: TranscriptionRoute::Realtime,
            live: true,
        })
    }

    fn append_capture(&self, capture_id: &str, samples: Vec<i16>) -> Result<(), String> {
        let mut calls = self.0.lock().unwrap();
        if let Some(code) = calls.append_refusal.clone() {
            return Err(code);
        }
        calls.order.push("append".to_string());
        calls.appended.push((capture_id.to_string(), samples));
        Ok(())
    }

    fn cancel_capture(&self, capture_id: &str) {
        self.0.lock().unwrap().cancelled.push(capture_id.to_string());
    }

    fn emit(&self, change: &ActivityChange) {
        let mut calls = self.0.lock().unwrap();
        calls.order.push(format!("{:?}", change.state));
        calls.changes.push(change.clone());
    }

    fn note_failure(&self, _: &str, _: &str) {}
}

impl Host {
    fn states(&self) -> Vec<ActivityState> {
        self.0.lock().unwrap().changes.iter().map(|c| c.state).collect()
    }

    fn last(&self) -> ActivityChange {
        self.0.lock().unwrap().changes.last().cloned().unwrap()
    }

    fn set(&self, change: impl FnOnce(&mut Calls)) {
        change(&mut self.0.lock().unwrap());
    }

    fn audio(&self) -> Vec<i16> {
        self.0.lock().unwrap().appended.iter().flat_map(|(_, s)| s.clone()).collect()
    }
}

#[derive(Clone, Default)]
struct Script(Arc<Mutex<VecDeque<Result<f32, String>>>>);

impl Script {
    fn queue(&self, probabilities: &[f32]) {
        self.0.lock().unwrap().extend(probabilities.iter().map(|p| Ok(*p)));
    }

    fn fail(&self) {
        self.0.lock().unwrap().push_back(Err("broken".to_string()));
    }
}

impl SpeechModel for Script {
    fn probability(&mut self, _: &[f32]) -> Result<f32, String> {
        self.0.lock().unwrap().pop_front().unwrap_or(Ok(QUIET))
    }
}

struct Session {
    worker: Worker<Host>,
    host: Host,
    script: Script,
    next_sample: usize,
    seq: u64,
}

impl Session {
    fn new() -> Self {
        Self::at_rate(RATE)
    }

    fn at_rate(rate: u32) -> Self {
        let host = Host::default();
        let script = Script::default();
        let worker = Worker::new(
            "activity-1".to_string(),
            rate,
            vec!["arch".to_string()],
            host.clone(),
            Box::new(script.clone()),
        )
        .unwrap();
        Self { worker, host, script, next_sample: 0, seq: 0 }
    }

    /// Feed one frame for each probability, as a single batch.
    fn feed(&mut self, probabilities: &[f32]) {
        self.script.queue(probabilities);
        let count = probabilities.len() * FRAME;
        let samples: Vec<i16> = (0..count).map(|i| (self.next_sample + i) as i16).collect();
        self.next_sample += count;
        let (done, _) = oneshot::channel();
        self.worker.handle(Message::Feed(samples, done));
    }

    fn gate(&mut self, change: impl FnOnce(&mut Gate)) {
        self.seq += 1;
        let mut gate = Gate { seq: self.seq, ..Gate::default() };
        change(&mut gate);
        self.worker.handle(Message::Gate(gate));
    }

    fn close(&mut self, discard: bool) -> Option<String> {
        let (reply, mut answer) = oneshot::channel();
        self.worker.handle(Message::Close { discard, reply: Some(reply) });
        answer.try_recv().unwrap()
    }

    /// Up to a confirmed utterance: quiet audio for the pre-roll, then speech.
    fn confirm(&mut self) {
        self.feed(&[QUIET; 20]);
        self.feed(&[SPEECH; 11]);
    }

    /// `QUIET` frames that end a confirmed utterance.
    fn end(&mut self) {
        self.feed(&[QUIET; 22]);
    }
}

// VOP-FR-XILN, VOP-FR-QFRF: audio goes only to the detector while no capture is
// open, and a misfire opens none.
#[test]
fn a_misfire_opens_no_capture_and_sends_no_audio() {
    let mut session = Session::new();
    session.feed(&[QUIET, SPEECH, QUIET, QUIET, QUIET]);
    assert_eq!(session.host.states(), vec![ActivityState::SpeechStart, ActivityState::Misfire]);
    assert!(session.host.0.lock().unwrap().started.is_empty());
    assert!(session.host.audio().is_empty());
}

// VOP-FR-GZPY: the capture opens at 10 frames, with its id and `live`.
#[test]
fn confirmed_speech_opens_one_capture() {
    let mut session = Session::new();
    session.confirm();
    assert_eq!(
        session.host.states(),
        vec![ActivityState::SpeechStart, ActivityState::SpeechConfirmed]
    );
    let change = session.host.last();
    assert_eq!(change.capture_id.as_deref(), Some("capture-1"));
    assert_eq!(change.live, Some(true));
    assert_eq!(change.activity_id, "activity-1");
}

// VOP-FR-FAQA: the capture gets the pre-roll, then all later audio, in order, and
// no sample comes twice or goes missing.
#[test]
fn the_capture_receives_the_pre_roll_and_the_live_audio_in_order() {
    let mut session = Session::new();
    session.confirm();
    session.feed(&[SPEECH; 3]);
    session.end();

    let audio = session.host.audio();
    let start = 20 * FRAME - PRE_ROLL;
    let expected: Vec<i16> = (start..session.next_sample).map(|i| i as i16).collect();
    assert_eq!(audio.len(), expected.len());
    assert_eq!(audio, expected);
}

// VOP-FR-FAQA: a backlog can merge into a chunk longer than the ring, and no
// sample of an open capture is lost to it.
#[test]
fn a_chunk_longer_than_the_ring_loses_no_audio() {
    let mut session = Session::new();
    session.confirm();
    session.feed(&[SPEECH; 200]);
    let audio = session.host.audio();
    let start = 20 * FRAME - PRE_ROLL;
    assert_eq!(audio.len(), session.next_sample - start);
    let expected: Vec<i16> = (start..session.next_sample).map(|i| i as i16).collect();
    assert_eq!(audio, expected);
}

// VOP-FR-FAQA: the pre-roll also survives such a chunk.
#[test]
fn speech_inside_a_long_chunk_keeps_its_pre_roll() {
    let mut session = Session::new();
    let mut script = vec![QUIET; 100];
    script.extend([SPEECH; 100]);
    session.feed(&script);
    let start = 100 * FRAME - PRE_ROLL;
    assert_eq!(session.host.audio()[0], start as i16);
}

// VOP-FR-GZOC, VOP-FR-FAQA: at a capture rate of 48 kHz, the detector gets 16 kHz,
// and the capture keeps the 48 kHz audio with its pre-roll. This is the rate of a
// real microphone, where the resampler runs.
#[test]
fn a_capture_rate_of_48_khz_keeps_its_audio_and_its_pre_roll() {
    const FRAME_48K: usize = FRAME * 3;
    let mut session = Session::at_rate(48_000);
    let mut script = vec![QUIET; 20];
    script.extend([SPEECH; 11]);
    session.script.queue(&script);
    let count = 40 * FRAME_48K;
    let samples: Vec<i16> = (0..count).map(|i| (i % 30_000) as i16).collect();
    let (done, _) = oneshot::channel();
    session.worker.handle(Message::Feed(samples, done));

    assert_eq!(
        session.host.states(),
        vec![ActivityState::SpeechStart, ActivityState::SpeechConfirmed]
    );
    let audio = session.host.audio();
    let start = 20 * FRAME_48K - 15_360;
    assert_eq!(audio.len(), count - start);
    assert_eq!(audio[0], (start % 30_000) as i16);
    assert_eq!(audio[audio.len() - 1], ((count - 1) % 30_000) as i16);
}

// VOP-FR-KUGV: the feed limit, as the session thread runs it.
#[test]
fn the_session_thread_ends_when_the_feed_stops() {
    let session = Session::new();
    let host = session.host.clone();
    let (_keep, messages) = mpsc::channel::<Message>();
    session.worker.run(messages, Duration::ZERO);
    let change = host.last();
    assert_eq!(change.code.as_deref(), Some(ERR_NO_AUDIO));
    assert_eq!(change.fatal, Some(true));
}

// VOP-FR-KUGV: a gate does not move the limit, or gate pushes would hide lost audio.
#[test]
fn a_gate_does_not_move_the_feed_limit() {
    let session = Session::new();
    let host = session.host.clone();
    let (sender, messages) = mpsc::channel::<Message>();
    sender.send(Message::Gate(Gate { seq: 1, ..Gate::default() })).unwrap();
    session.worker.run(messages, Duration::ZERO);
    assert_eq!(host.last().code.as_deref(), Some(ERR_NO_AUDIO));
}

// VOP-FR-KUGV: the limit is 30 s.
#[test]
fn the_feed_limit_is_thirty_seconds() {
    assert_eq!(FEED_LIMIT, Duration::from_secs(30));
}

// VOP-FR-WYKO: a sender that went ends the session and its capture.
#[test]
fn a_session_with_no_sender_ends_and_cancels_its_capture() {
    let mut session = Session::new();
    session.confirm();
    let host = session.host.clone();
    let (sender, messages) = mpsc::channel::<Message>();
    drop(sender);
    session.worker.run(messages, FEED_LIMIT);
    assert_eq!(host.0.lock().unwrap().cancelled, vec!["capture-1".to_string()]);
}

// VOP-FR-BTBM: the last append of an utterance can also fail, and no `speech_end`
// follows it.
#[test]
fn a_lost_capture_at_the_end_of_an_utterance_gives_no_speech_end() {
    let mut session = Session::new();
    session.confirm();
    session.host.set(|c| c.append_refusal = Some(ERR_SESSION_LOST.to_string()));
    session.end();
    assert_eq!(session.host.last().state, ActivityState::Failed);
    assert!(!session.host.states().contains(&ActivityState::SpeechEnd));
}

// VOP-FR-FAQA: speech early in a session has less pre-roll than the full length.
#[test]
fn speech_at_the_start_of_a_session_has_the_audio_that_exists() {
    let mut session = Session::new();
    session.feed(&[SPEECH; 11]);
    assert_eq!(session.host.audio()[0], 0);
}

// VOP-FR-OFDY: `speech_end` follows the last audio of the utterance.
#[test]
fn speech_end_comes_after_the_last_append() {
    let mut session = Session::new();
    session.confirm();
    session.end();
    let order = session.host.0.lock().unwrap().order.clone();
    assert_eq!(order.last().map(String::as_str), Some("SpeechEnd"));
    assert_eq!(session.host.last().capture_id.as_deref(), Some("capture-1"));
}

// VOP-FR-FMRV, VOP-FR-QZCE: only the gate that names this capture settles it.
#[test]
fn only_the_gate_that_names_the_capture_settles_it() {
    let mut session = Session::new();
    session.confirm();
    session.end();

    session.gate(|g| g.settled = Some("capture-0".to_string()));
    session.feed(&[SPEECH; 20]);
    assert_eq!(session.host.states().len(), 3);

    session.gate(|g| g.settled = Some("capture-1".to_string()));
    session.feed(&[SPEECH]);
    assert_eq!(session.host.last().state, ActivityState::SpeechStart);
}

// VOP-FR-QZCE: a gate that arrives late has no effect.
#[test]
fn an_older_gate_is_ignored() {
    let mut session = Session::new();
    session.gate(|g| g.blocked = true);
    session.gate(|g| g.blocked = true);
    session.worker.handle(Message::Gate(Gate { seq: 1, ..Gate::default() }));
    session.feed(&[SPEECH]);
    assert!(session.host.states().is_empty());
}

// VOP-FR-KYFC: a blocked gate starts no utterance, and an open one does.
#[test]
fn a_blocked_gate_starts_nothing() {
    let mut session = Session::new();
    session.gate(|g| g.blocked = true);
    session.feed(&[SPEECH; 5]);
    assert!(session.host.states().is_empty());
    session.gate(|g| g.blocked = false);
    session.feed(&[SPEECH]);
    assert_eq!(session.host.states(), vec![ActivityState::SpeechStart]);
}

// VOP-FR-DBEE: the gate's `playing` raises the start threshold.
#[test]
fn a_gate_that_reports_a_reply_raises_the_threshold() {
    let mut session = Session::new();
    session.gate(|g| g.playing = true);
    session.feed(&[0.6]);
    assert!(session.host.states().is_empty());
    session.gate(|g| g.playing = false);
    session.feed(&[0.6]);
    assert_eq!(session.host.states(), vec![ActivityState::SpeechStart]);
}

// VOP-FR-QZCE, `VOM-voice-mode.md` VOM-FR-LNUH: a capture takes the newest roster.
#[test]
fn a_capture_takes_the_keyterms_of_the_newest_gate() {
    let mut session = Session::new();
    session.confirm();
    assert_eq!(session.host.0.lock().unwrap().started[0], vec!["arch".to_string()]);
    session.end();
    session.gate(|g| {
        g.settled = Some("capture-1".to_string());
        g.keyterms = vec!["helga".to_string()];
    });
    session.confirm();
    assert_eq!(session.host.0.lock().unwrap().started[1], vec!["helga".to_string()]);
}

// VOP-FR-FYQX, VOP-FR-DAZW: a capture that is still open gives a misfire, and
// speech that goes on starts nothing.
#[test]
fn a_refusal_for_an_open_capture_gives_a_misfire() {
    let mut session = Session::new();
    session.host.set(|c| c.refusal = Some(ERR_SESSION_ACTIVE.to_string()));
    session.feed(&[SPEECH; 40]);
    assert_eq!(session.host.states(), vec![ActivityState::SpeechStart, ActivityState::Misfire]);
}

// VOP-FR-FYQX: another refusal reports its code, and the session keeps listening.
#[test]
fn another_refusal_reports_its_code_and_the_session_listens() {
    let mut session = Session::new();
    session.host.set(|c| c.refusal = Some(ERR_NO_ACTIVE_PROVIDER.to_string()));
    session.feed(&[SPEECH; 11]);
    let change = session.host.last();
    assert_eq!(change.state, ActivityState::Failed);
    assert_eq!(change.code.as_deref(), Some(ERR_NO_ACTIVE_PROVIDER));
    assert_eq!(change.fatal, Some(false));

    session.host.set(|c| c.refusal = None);
    session.feed(&[QUIET, SPEECH]);
    assert_eq!(session.host.last().state, ActivityState::SpeechStart);
}

// VOP-FR-BTBM: a lost capture ends the utterance, and the session stays.
#[test]
fn a_lost_capture_ends_the_utterance_and_the_session_stays() {
    let mut session = Session::new();
    session.confirm();
    session.host.set(|c| c.append_refusal = Some(ERR_SESSION_LOST.to_string()));
    session.feed(&[SPEECH]);
    let change = session.host.last();
    assert_eq!(change.state, ActivityState::Failed);
    assert_eq!(change.code.as_deref(), Some(ERR_SESSION_LOST));
    assert_eq!(change.fatal, Some(false));

    session.host.set(|c| c.append_refusal = None);
    session.feed(&[QUIET, SPEECH]);
    assert_eq!(session.host.last().state, ActivityState::SpeechStart);
}

// VOP-FR-BTBM: a failure of the model ends the session.
#[test]
fn a_detector_failure_ends_the_session() {
    let mut session = Session::new();
    session.confirm();
    session.script.fail();
    let (done, _) = oneshot::channel();
    session.worker.handle(Message::Feed(vec![0; FRAME], done));
    let change = session.host.last();
    assert_eq!(change.code.as_deref(), Some(ERR_DETECTOR_FAILED));
    assert_eq!(change.fatal, Some(true));
    assert_eq!(session.host.0.lock().unwrap().cancelled, vec!["capture-1".to_string()]);
}

// VOP-FR-KUGV: no feed ends the session and cancels its capture.
#[test]
fn a_session_with_no_feed_ends_and_cancels_its_capture() {
    let mut session = Session::new();
    session.confirm();
    session.worker.feed_stopped();
    let change = session.host.last();
    assert_eq!(change.state, ActivityState::Failed);
    assert_eq!(change.code.as_deref(), Some(ERR_NO_AUDIO));
    assert_eq!(change.fatal, Some(true));
    assert_eq!(session.host.0.lock().unwrap().cancelled, vec!["capture-1".to_string()]);
}

// VOP-FR-KUGV: a capture that waits for its finish goes with the session too.
#[test]
fn no_feed_also_cancels_a_capture_that_waits_for_its_finish() {
    let mut session = Session::new();
    session.confirm();
    session.end();
    session.worker.feed_stopped();
    assert_eq!(session.host.0.lock().unwrap().cancelled, vec!["capture-1".to_string()]);
}

// VOP-FR-FAAB: a close with `discard` cancels the capture.
#[test]
fn a_discarding_close_cancels_the_capture() {
    let mut session = Session::new();
    session.confirm();
    assert_eq!(session.close(true), None);
    assert_eq!(session.host.0.lock().unwrap().cancelled, vec!["capture-1".to_string()]);
}

// VOP-FR-FAAB: a close returns the open capture for "finish voice capture".
#[test]
fn a_close_returns_the_capture_of_an_utterance_in_progress() {
    let mut session = Session::new();
    session.confirm();
    assert_eq!(session.close(false).as_deref(), Some("capture-1"));
    assert!(session.host.0.lock().unwrap().cancelled.is_empty());
}

// VOP-FR-FAAB: unconfirmed speech is dropped, and it needs no event.
#[test]
fn a_close_during_unconfirmed_speech_returns_nothing() {
    let mut session = Session::new();
    session.feed(&[SPEECH; 3]);
    assert_eq!(session.close(false), None);
}

// VOP-FR-FAAB: the reply can reach the WebView before `speech_end`, so a capture
// that has not settled still comes back, and it is not cancelled.
#[test]
fn a_close_returns_a_capture_that_has_not_settled() {
    let mut session = Session::new();
    session.confirm();
    session.end();
    assert_eq!(session.close(false).as_deref(), Some("capture-1"));
    assert!(session.host.0.lock().unwrap().cancelled.is_empty());
}

// VOP-FR-FAAB: a discarding close also cancels a capture that has not settled.
#[test]
fn a_discarding_close_cancels_a_capture_that_has_not_settled() {
    let mut session = Session::new();
    session.confirm();
    session.end();
    assert_eq!(session.close(true), None);
    assert_eq!(session.host.0.lock().unwrap().cancelled, vec!["capture-1".to_string()]);
}

// VOP-FR-FAAB: a settled capture belongs to nobody here.
#[test]
fn a_close_after_the_settle_returns_nothing() {
    let mut session = Session::new();
    session.confirm();
    session.end();
    session.gate(|g| g.settled = Some("capture-1".to_string()));
    assert_eq!(session.close(false), None);
}

// VOP-FR-DSNZ: a change applies from the next utterance.
#[test]
fn a_change_of_settings_applies_from_the_next_utterance() {
    let mut session = Session::new();
    session.host.set(|c| c.settings = Some(Settings::new(500, VoiceVadSensitivity::High)));
    session.feed(&[0.4]);
    assert_eq!(session.host.states(), vec![ActivityState::SpeechStart]);
}

// VOP "Records": the wire names of the event, which the WebView reads.
#[test]
fn the_event_payload_has_the_names_of_the_webview() {
    let change = ActivityChange {
        activity_id: "activity-1".to_string(),
        state: ActivityState::SpeechConfirmed,
        capture_id: Some("capture-1".to_string()),
        live: Some(true),
        code: None,
        fatal: None,
    };
    assert_eq!(
        change.payload(),
        serde_json::json!({
            "activityId": "activity-1",
            "state": "speech_confirmed",
            "captureId": "capture-1",
            "live": true,
            "code": null,
            "fatal": null,
        })
    );
    for (state, name) in [
        (ActivityState::SpeechStart, "speech_start"),
        (ActivityState::SpeechEnd, "speech_end"),
        (ActivityState::Misfire, "misfire"),
        (ActivityState::Failed, "failed"),
    ] {
        let payload = ActivityChange { state, ..change.clone() }.payload();
        assert_eq!(payload["state"], name);
    }
}

// VOP-FR-ADHQ: a model that does not load opens no session.
#[tokio::test]
async fn a_model_that_does_not_load_opens_no_session() {
    let bench = Bench::new();
    let opened = bench
        .voice
        .open_activity(RATE, vec![], Host::default(), || Err("no runtime".to_string()))
        .await;
    assert_eq!(opened, Err("no runtime".to_string()));
    let fed = bench.voice.feed_activity("activity-1", vec![0; FRAME]).await;
    assert_eq!(fed, Err(ERR_SESSION_LOST.to_string()));
}

// VOP-FR-WYKO, VOP-FR-TADX: "open" replaces an older session, and the older id
// no longer feeds.
#[tokio::test]
async fn opening_replaces_an_older_session_and_cancels_its_capture() {
    let bench = Bench::new();
    let host = Host::default();
    let script = Script::default();
    script.queue(&[SPEECH; 11]);
    let model = script.clone();
    let first = bench
        .voice
        .open_activity(RATE, vec![], host.clone(), move || {
            Ok(Box::new(model) as Box<dyn SpeechModel>)
        })
        .await
        .unwrap();
    bench.voice.feed_activity(&first, vec![0; 11 * FRAME]).await.unwrap();
    assert_eq!(host.last().state, ActivityState::SpeechConfirmed);

    let second = bench
        .voice
        .open_activity(RATE, vec![], Host::default(), || {
            Ok(Box::new(Script::default()) as Box<dyn SpeechModel>)
        })
        .await
        .unwrap();
    assert_ne!(first, second);
    assert_eq!(host.0.lock().unwrap().cancelled, vec!["capture-1".to_string()]);
    let fed = bench.voice.feed_activity(&first, vec![0; FRAME]).await;
    assert_eq!(fed, Err(ERR_SESSION_LOST.to_string()));
    assert_eq!(bench.voice.feed_activity(&second, vec![0; FRAME]).await, Ok(()));
    bench.voice.end_activity().unwrap();
}

// VOP-FR-QZCE, VOP-FR-FAAB: a late command does not reach the next session.
#[tokio::test]
async fn a_command_for_a_session_that_went_has_no_effect() {
    let bench = Bench::new();
    let id = bench
        .voice
        .open_activity(RATE, vec![], Host::default(), || {
            Ok(Box::new(Script::default()) as Box<dyn SpeechModel>)
        })
        .await
        .unwrap();
    assert_eq!(bench.voice.set_activity_gate("activity-0", Gate::default()), Ok(()));
    assert_eq!(bench.voice.close_activity("activity-0", false).await, Ok(None));
    assert_eq!(bench.voice.feed_activity(&id, vec![0; FRAME]).await, Ok(()));
    assert_eq!(bench.voice.close_activity(&id, true).await, Ok(None));
    let fed = bench.voice.feed_activity(&id, vec![0; FRAME]).await;
    assert_eq!(fed, Err(ERR_SESSION_LOST.to_string()));
}
