//! Capture sessions: audio in, a transcript out (VOP-FR-KBPZ, VOP-FR-VZDI).
//!
//! One session records at a time. Sessions that finish do not count, so the
//! next utterance records while earlier ones transcribe (VOP-FR-WCSB). A
//! session holds its audio at the capture rate for all of its life, because a
//! retry can need another route at another rate (VOP-FR-YEFK, VOP-FR-IAFL).
//! Audio never leaves memory for the disk.

use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use super::chunks::{self, Chunking};
use super::detect::http_stt_model;
use super::http::{self, HTTP_RATE};
use super::live::{finish_live, spawn_live, LiveCommand};
use super::pcm::resample_all;
use super::*;

mod chunked;

use chunked::ChunkRun;

/// What the end of input of a capture needs: the route, the live session, and
/// the speech end (VOP-FR-HSJU).
type Finishing = (TranscriptionRoute, Option<mpsc::UnboundedSender<LiveCommand>>, Option<usize>);

/// VOP-FR-BSNY.
pub(crate) const WATCHDOG_LIMIT: Duration = Duration::from_secs(30);
pub(crate) const WATCHDOG_TICK: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Recording,
    /// VOP-FR-WCSB: a VAD utterance had `speech_end`, and "finish voice
    /// capture" did not come yet. The input is over.
    Ended,
    Finishing,
    Failed,
}

impl Phase {
    /// VOP-FR-BSNY: a session that waits for its finish.
    fn unfinished(self) -> bool {
        matches!(self, Phase::Recording | Phase::Ended)
    }
}

pub(crate) struct CaptureSession {
    pub(crate) id: String,
    config: ActiveVoice,
    capture_rate: u32,
    keyterms: Vec<String>,
    pcm: Vec<i16>,
    pub(crate) phase: Phase,
    pub(crate) last_audio: Instant,
    route: TranscriptionRoute,
    /// VOP-FR-JAOB: the route gives live text.
    is_live: bool,
    live: Option<mpsc::UnboundedSender<LiveCommand>>,
    /// VOP-FR-ERDS: set for a chunked capture.
    chunks: Option<ChunkRun>,
    /// VOP-FR-KIMU: the word gap detector of a live `openai` capture that is
    /// not chunked.
    gaps: Option<std::sync::mpsc::Sender<chunks::DetectorMessage>>,
    /// VOP-FR-IAFL: the code of the last failure.
    failure: Option<String>,
    /// VOP-FR-HSJU: the speech end in `pcm`, when the detector of the capture
    /// found the capture in a silence after speech.
    speech_until: Option<usize>,
    sink: SharedSink,
}

/// VOP-FR-HSJU: the held audio ends at the speech end. `floor` is the first
/// sample that must stay, the start of the open chunk.
fn trim_to_speech(session: &mut CaptureSession, floor: usize) {
    let Some(until) = session.speech_until else { return };
    let keep = until.max(floor);
    if keep >= session.pcm.len() {
        return;
    }
    let cut = session.pcm.len() - keep;
    session.pcm.truncate(keep);
    // VOP-FR-YEFK: counts only.
    session.sink.live_step(
        "voice capture trimmed at the speech end",
        &[("keptSamples", keep), ("cutSamples", cut)],
        false,
    );
}

/// VOP-FR-INCC: a nickname that holds one of these characters is refused by a
/// provider, and such a refusal stops the whole update.
fn usable_keyterms(keyterms: Vec<String>) -> Vec<String> {
    keyterms
        .into_iter()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty() && !k.contains(['<', '>', '\r', '\n']))
        .collect()
}

/// What an HTTP transcription needs. It holds a key, so it does not derive
/// `Debug`.
#[derive(Clone)]
struct HttpJob {
    config: ActiveVoice,
    capture_rate: u32,
    keyterms: Vec<String>,
    sink: SharedSink,
}

impl HttpJob {
    fn of(session: &CaptureSession) -> Self {
        Self {
            config: session.config.clone(),
            capture_rate: session.capture_rate,
            keyterms: session.keyterms.clone(),
            sink: session.sink.clone(),
        }
    }

    /// VOP-FR-EYHP: one request with `pcm`.
    async fn run(self, pcm: Vec<i16>) -> Result<String, String> {
        let record = &self.config.record;
        let samples = resample_all(&pcm, self.capture_rate, HTTP_RATE)
            .map_err(|_| ERR_TRANSCRIPTION_FAILED.to_string())?;
        let (client, _) = http::client().map_err(|f| f.transcription_code().to_string())?;
        let model = http_stt_model(&record.provider, &record.stt_model);
        let key = self.config.key.as_deref();
        let hints = http::hint_fields(&record.provider, &self.keyterms, self.config.language);
        let send = |hints: Vec<(&'static str, String)>| {
            let (client, model, samples) = (&client, &model, &samples);
            async move {
                http::transcribe(
                    client,
                    &record.provider,
                    &record.base_url,
                    key,
                    model,
                    samples,
                    &hints,
                )
                .await
            }
        };
        let with_language = self.config.language.is_some() && record.provider == "custom";
        let transcribed = match send(hints).await {
            // VOP-FR-TSUS: on `custom`, a 400 for a language repeats once
            // without it.
            Err(http::HttpFailure::BadRequest) if with_language => {
                send(http::hint_fields(&record.provider, &self.keyterms, None)).await
            }
            other => other,
        };
        transcribed.map_err(|f| {
            self.sink.diagnostic("http_transcription", &f.detail());
            f.transcription_code().to_string()
        })
    }
}

impl VoiceAi {
    fn session_guard(&self) -> Result<std::sync::MutexGuard<'_, Vec<CaptureSession>>, String> {
        self.capture.lock().map_err(|e| format!("voice capture poisoned: {e}"))
    }

    /// VOP-FR-KBPZ: copy the configuration and open one session. The capture
    /// is not chunked.
    #[cfg(test)]
    pub(crate) fn start_capture(
        &self,
        config: ActiveVoice,
        capture_rate: u32,
        keyterms: Vec<String>,
        sink: SharedSink,
    ) -> Result<VoiceCaptureStart, String> {
        self.start_chunked_capture(config, capture_rate, keyterms, Chunking::Off, sink)
    }

    /// VOP-FR-KBPZ, VOP-FR-WCSB, VOP-FR-ERDS: a session that records refuses a
    /// start. A start replaces a failed session.
    pub(crate) fn start_chunked_capture(
        &self,
        config: ActiveVoice,
        capture_rate: u32,
        keyterms: Vec<String>,
        chunking: Chunking,
        sink: SharedSink,
    ) -> Result<VoiceCaptureStart, String> {
        let mut slot = self.session_guard()?;
        if slot.iter().any(|s| s.phase == Phase::Recording) {
            return Err(ERR_SESSION_ACTIVE.to_string());
        }
        slot.retain(|s| s.phase != Phase::Failed);
        let keyterms = usable_keyterms(keyterms);
        let route = config.record.transcription_route;
        let id = self.new_session_id();
        let is_live = route == TranscriptionRoute::Realtime
            && config.record.capabilities.as_ref().and_then(|c| c.realtime_transcription)
                == Some(RealtimeTranscription::Live);
        let (chunking, gaps) = match chunking {
            // VOP-FR-KIMU: only the `openai` dialect sends interim commits.
            Chunking::Gaps { settings, load } => {
                let gaps = (is_live && config.record.provider == "openai")
                    .then(|| chunks::spawn_detector(capture_rate, settings, load, sink.clone()))
                    .flatten();
                (Chunking::Off, gaps)
            }
            other => (other, None),
        };
        let chunks = ChunkRun::open(&id, capture_rate, chunking, &sink);
        let chunk_text = chunks.as_ref().map(ChunkRun::new_text);
        let live = (route == TranscriptionRoute::Realtime)
            .then(|| spawn_live(&config, capture_rate, &keyterms, &id, chunk_text, sink.clone()));
        slot.push(CaptureSession {
            id: id.clone(),
            config,
            capture_rate,
            keyterms,
            pcm: Vec::new(),
            phase: Phase::Recording,
            last_audio: Instant::now(),
            route,
            is_live,
            live,
            chunks,
            gaps,
            failure: None,
            speech_until: None,
            sink,
        });
        Ok(VoiceCaptureStart { session_id: id, route, live: is_live })
    }

    /// VOP-FR-DXPK.
    pub(crate) fn append_capture(&self, session_id: &str, samples: Vec<i16>) -> Result<(), String> {
        self.append_inner(session_id, samples).map(|_| ())
    }

    /// Hold the audio and send it on. Gives the wait for the pause check of a
    /// capture that runs its own detector (VOP-FR-YDVH).
    fn append_inner(
        &self,
        session_id: &str,
        samples: Vec<i16>,
    ) -> Result<Option<tokio::sync::oneshot::Receiver<chunks::DetectorAnswer>>, String> {
        let mut slot = self.session_guard()?;
        let session = slot
            .iter_mut()
            .find(|s| s.id == session_id && s.phase == Phase::Recording)
            .ok_or(ERR_SESSION_LOST)?;
        session.last_audio = Instant::now();
        session.pcm.extend_from_slice(&samples);
        let detection = match (&session.chunks, &session.gaps) {
            (Some(chunks), _) => chunks.detect(&samples),
            (None, Some(gaps)) => chunks::detect_with(gaps, &samples),
            (None, None) => None,
        };
        if let Some(live) = &session.live {
            // A closed channel means that the socket failed. The audio is held
            // here for a retry, and the finish reports the failure.
            let _ = live.send(LiveCommand::Audio(samples));
        }
        Ok(detection)
    }

    /// "append voice capture": VOP-FR-DXPK, and VOP-FR-YDVH for a capture that
    /// runs its own detector. The detector thread answers before the command
    /// returns, so a chunk ends in the order of the audio.
    pub(crate) async fn append_and_detect(
        &self,
        session_id: &str,
        samples: Vec<i16>,
    ) -> Result<(), String> {
        let Some(detection) = self.append_inner(session_id, samples)? else { return Ok(()) };
        // A detector that went keeps the last speech end that it found.
        let Ok(answer) = detection.await else { return Ok(()) };
        self.note_speech_until(session_id, answer.speech_until)?;
        if answer.gap {
            self.word_gap(session_id);
        }
        if answer.paused {
            self.chunk_pause(session_id, answer.speech_until)?;
        }
        Ok(())
    }

    /// VOP-FR-HSJU: the latest speech end that the own detector of a capture
    /// found.
    fn note_speech_until(&self, session_id: &str, until: Option<usize>) -> Result<(), String> {
        let mut slot = self.session_guard()?;
        if let Some(session) =
            slot.iter_mut().find(|s| s.id == session_id && s.phase == Phase::Recording)
        {
            session.speech_until = until;
        }
        Ok(())
    }

    /// VOP-FR-WCSB, VOP-FR-HSJU: the utterance of a VAD session ended at
    /// `speech_until`, and the session no longer counts as open.
    pub(crate) fn end_capture_input(
        &self,
        session_id: &str,
        speech_until: Option<usize>,
    ) -> Result<(), String> {
        let mut slot = self.session_guard()?;
        if let Some(session) =
            slot.iter_mut().find(|s| s.id == session_id && s.phase == Phase::Recording)
        {
            session.phase = Phase::Ended;
            session.gaps = None;
            session.speech_until = speech_until;
        }
        Ok(())
    }

    /// Whether a session with this id exists.
    pub(crate) fn capture_exists(&self, session_id: &str) -> bool {
        self.session_guard().is_ok_and(|slot| slot.iter().any(|s| s.id == session_id))
    }

    /// Mark the session as finishing, and give what the end of input needs:
    /// the route, the live session, and the speech end (VOP-FR-HSJU).
    fn begin_finish(&self, session_id: &str, from: &[Phase]) -> Result<Finishing, String> {
        let mut slot = self.session_guard()?;
        let session = slot
            .iter_mut()
            .find(|s| s.id == session_id && from.contains(&s.phase))
            .ok_or(ERR_SESSION_LOST)?;
        session.phase = Phase::Finishing;
        // The detector thread ends with its sender.
        session.gaps = None;
        trim_to_speech(session, 0);
        Ok((session.route, session.live.take(), session.speech_until))
    }

    fn settle(&self, session_id: &str, result: &Result<String, String>) {
        let Ok(mut slot) = self.session_guard() else { return };
        match result {
            // VOP-FR-YEFK: a success ends the session and drops its audio.
            Ok(_) => slot.retain(|s| s.id != session_id),
            // VOP-FR-UOYP: a failed session stays for a retry.
            Err(code) => {
                if let Some(session) = slot.iter_mut().find(|s| s.id == session_id) {
                    session.phase = Phase::Failed;
                    session.failure = Some(code.clone());
                }
            }
        }
    }

    async fn transcribe_held(&self, session_id: &str) -> Result<String, String> {
        let (job, pcm) = {
            let slot = self.session_guard()?;
            let session = slot.iter().find(|s| s.id == session_id).ok_or(ERR_SESSION_LOST)?;
            (HttpJob::of(session), session.pcm.clone())
        };
        job.run(pcm).await
    }

    /// VOP-FR-VZDI, VOP-FR-EYHP, VOP-FR-GLDR.
    pub(crate) async fn finish_capture(&self, session_id: &str) -> Result<String, String> {
        if self.is_chunked(session_id) {
            return self.finish_chunked(session_id).await;
        }
        let (route, live, until) =
            self.begin_finish(session_id, &[Phase::Recording, Phase::Ended])?;
        let result = match (route, live) {
            (TranscriptionRoute::Realtime, Some(live)) => finish_live(live, until).await,
            _ => self.transcribe_held(session_id).await,
        };
        self.settle(session_id, &result);
        result
    }

    /// VOP-FR-IAFL, VOP-FR-SIKM: the same route, or the HTTP route where a
    /// Realtime socket failed and the capabilities hold it. All held audio goes
    /// as one request, and a chunked capture emits no chunk event.
    pub(crate) async fn retry_capture(
        &self,
        session_id: &str,
        sink: SharedSink,
    ) -> Result<String, String> {
        self.begin_finish(session_id, &[Phase::Failed])?;
        let replay = {
            let mut slot = self.session_guard()?;
            let session = slot.iter_mut().find(|s| s.id == session_id).ok_or(ERR_SESSION_LOST)?;
            let has_http =
                session.config.record.capabilities.as_ref().is_some_and(|c| c.http_transcription);
            let socket_failed = matches!(
                session.failure.as_deref(),
                Some(ERR_SESSION_LOST | ERR_PROVIDER_UNREACHABLE)
            );
            if session.route == TranscriptionRoute::Realtime && socket_failed && has_http {
                session.route = TranscriptionRoute::Http;
                None
            } else if session.route == TranscriptionRoute::Realtime {
                let live = spawn_live(
                    &session.config,
                    session.capture_rate,
                    &session.keyterms,
                    session_id,
                    None,
                    sink,
                );
                let _ = live.send(LiveCommand::Audio(session.pcm.clone()));
                Some(live)
            } else {
                None
            }
        };
        let result = match replay {
            // The held audio is already trimmed, so no commit is skipped.
            Some(live) => finish_live(live, None).await,
            None => self.transcribe_held(session_id).await,
        };
        self.settle(session_id, &result);
        result
    }

    /// VOP-FR-YEFK: a cancel drops the audio. Dropping the sender ends the
    /// task of a live session, and the task closes its socket.
    pub(crate) fn cancel_capture(&self, session_id: &str) -> Result<(), String> {
        self.session_guard()?.retain(|s| s.id != session_id);
        Ok(())
    }

    /// VOP-FR-BSNY: cancel each unfinished session that has stopped to send
    /// audio.
    pub(crate) fn sweep_idle(&self, now: Instant) -> Result<bool, String> {
        let mut slot = self.session_guard()?;
        let before = slot.len();
        slot.retain(|s| {
            !(s.phase.unfinished() && now.duration_since(s.last_audio) >= WATCHDOG_LIMIT)
        });
        Ok(slot.len() != before)
    }
}
