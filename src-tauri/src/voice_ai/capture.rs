//! Capture sessions: audio in, a transcript out (VOP-FR-KBPZ, VOP-FR-VZDI).
//!
//! One session exists at a time. It holds its audio at the capture rate for all
//! of its life, because a retry can need another route at another rate
//! (VOP-FR-YEFK, VOP-FR-IAFL). Audio never leaves memory for the disk.

use std::time::{Duration, Instant};

use tokio::sync::{mpsc, oneshot};

use super::detect::http_stt_model;
use super::http::{self, HTTP_RATE};
use super::pcm::{resample_all, StreamResampler};
use super::realtime::{self, ServerEvent, Socket, Transcript};
use super::*;

/// VOP-FR-BSNY.
pub(crate) const WATCHDOG_LIMIT: Duration = Duration::from_secs(30);
pub(crate) const WATCHDOG_TICK: Duration = Duration::from_secs(5);
const OPEN_LIMIT: Duration = Duration::from_secs(15);
const CUSTOM_OPEN_LIMIT: Duration = Duration::from_secs(60);
const FINISH_LIMIT: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Recording,
    Finishing,
    Failed,
}

enum LiveCommand {
    Audio(Vec<i16>),
    Finish(oneshot::Sender<Result<String, String>>),
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
    live: Option<mpsc::UnboundedSender<LiveCommand>>,
    /// VOP-FR-IAFL: the code of the last failure.
    failure: Option<String>,
    sink: SharedSink,
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

/// A socket that failed at open or in the session. What the failure means for
/// the author depends on the time, so each stage names its own code.
struct LiveFailure(&'static str);

struct LiveConfig {
    provider: String,
    base_url: String,
    key: Option<String>,
    stt_model: String,
    capture_rate: u32,
    keyterms: Vec<String>,
    session_id: String,
}

async fn open_session(
    config: &LiveConfig,
    sink: &SharedSink,
) -> Result<(Socket, realtime::SessionInfo), LiveFailure> {
    let url = realtime::transcription_url(&config.provider, &config.base_url, &config.stt_model);
    let limit = if config.provider == "custom" { CUSTOM_OPEN_LIMIT } else { OPEN_LIMIT };
    let mut socket =
        realtime::connect(&url, config.key.as_deref(), limit).await.map_err(|kind| {
            sink.diagnostic("realtime_open", &kind);
            LiveFailure(ERR_PROVIDER_UNREACHABLE)
        })?;
    loop {
        match realtime::next_event_within(&mut socket, limit).await {
            Ok(Some(ServerEvent::SessionCreated(info))) => return Ok((socket, info)),
            Ok(Some(ServerEvent::Error { code, .. })) => {
                sink.diagnostic("realtime_open", code.as_deref().unwrap_or("error"));
                return Err(LiveFailure(ERR_PROVIDER_REJECTED));
            }
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => {
                sink.diagnostic("realtime_open", "closed_before_session");
                return Err(LiveFailure(ERR_PROVIDER_UNREACHABLE));
            }
        }
    }
}

/// The end of a session that failed: answer the finish, and drop the audio that
/// still comes. The session holds its own copy for a retry.
async fn drain_until_finish(
    mut commands: mpsc::UnboundedReceiver<LiveCommand>,
    failure: LiveFailure,
) {
    while let Some(command) = commands.recv().await {
        if let LiveCommand::Finish(reply) = command {
            let _ = reply.send(Err(failure.0.to_string()));
            return;
        }
    }
}

/// VOP-FR-QQYG: the session has all of its text. `llamacpp` says so by closing
/// the socket, so a live `llamacpp` session is not done here.
fn session_ended(
    dialect: Dialect,
    turn_based: bool,
    final_item: Option<&str>,
    transcript: &Transcript,
) -> bool {
    match dialect {
        Dialect::Openai => final_item.is_some_and(|item| transcript.is_completed(item)),
        Dialect::Vllm => transcript.all_completed(),
        Dialect::Llamacpp => turn_based && transcript.all_completed(),
    }
}

/// VOP-FR-GYKP, VOP-FR-JAOB, VOP-FR-XXAB, VOP-FR-HOGR: one Realtime
/// session. The channel holds the audio that comes before the socket is ready.
async fn run_live(
    config: LiveConfig,
    mut commands: mpsc::UnboundedReceiver<LiveCommand>,
    sink: SharedSink,
) {
    let (mut socket, session) = match open_session(&config, &sink).await {
        Ok(opened) => opened,
        Err(failure) => return drain_until_finish(commands, failure).await,
    };
    let dialect = realtime::dialect_of(&config.provider, &session);
    let rate = realtime::input_rate_of(dialect, &session);
    let configured = realtime::configure_transcription(
        &mut socket,
        dialect,
        &session,
        &config.stt_model,
        &config.keyterms,
    )
    .await;
    let resampler = StreamResampler::new(config.capture_rate, rate);
    let (Ok(()), Ok(mut resampler)) = (configured, resampler) else {
        return drain_until_finish(commands, LiveFailure(ERR_TRANSCRIPTION_FAILED)).await;
    };

    let mut transcript = Transcript::default();
    let mut shown = String::new();
    let mut finish: Option<oneshot::Sender<Result<String, String>>> = None;
    let mut deadline = None::<tokio::time::Instant>;
    let mut failure: Option<LiveFailure> = None;
    // VOP-FR-QQYG: the item that the final commit named.
    let mut final_item: Option<String> = None;
    let turn_based = realtime::is_turn_based(&session);

    loop {
        let expired = async {
            match deadline {
                Some(at) => tokio::time::sleep_until(at).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            command = commands.recv(), if finish.is_none() => match command {
                // The session was cancelled.
                None => return,
                Some(LiveCommand::Audio(samples)) => {
                    let sent = match resampler.push(&samples) {
                        Ok(out) => realtime::send_audio(&mut socket, &out).await,
                        Err(e) => Err(e),
                    };
                    if sent.is_err() {
                        failure = Some(LiveFailure(ERR_SESSION_LOST));
                        break;
                    }
                }
                Some(LiveCommand::Finish(reply)) => {
                    let ended = match resampler.finish() {
                        Ok(out) => match realtime::send_audio(&mut socket, &out).await {
                            Ok(()) => realtime::end_input(&mut socket, dialect).await,
                            Err(e) => Err(e),
                        },
                        Err(e) => Err(e),
                    };
                    finish = Some(reply);
                    deadline = Some(tokio::time::Instant::now() + FINISH_LIMIT);
                    if ended.is_err() {
                        failure = Some(LiveFailure(ERR_SESSION_LOST));
                        break;
                    }
                }
            },
            event = realtime::next_event(&mut socket) => {
                let ending = finish.is_some();
                match event {
                    Ok(Some(ServerEvent::Committed { item_id, previous_item_id })) => {
                        transcript.committed(item_id.as_deref(), previous_item_id.as_deref());
                        if ending {
                            final_item = item_id;
                        }
                    }
                    Ok(Some(ServerEvent::Delta { item_id, text })) => {
                        transcript.delta(item_id.as_deref(), &text);
                    }
                    Ok(Some(ServerEvent::Completed { item_id, text })) => {
                        transcript.completed(item_id.as_deref(), &text);
                    }
                    Ok(Some(ServerEvent::TranscriptionFailed)) => {
                        sink.diagnostic("realtime_transcription", "item_failed");
                        failure = Some(LiveFailure(ERR_TRANSCRIPTION_FAILED));
                        break;
                    }
                    Ok(Some(ServerEvent::Error { code, .. })) => {
                        // A commit with no audio has no transcript to wait for.
                        let empty = code.as_deref().is_some_and(|c| c.contains("commit_empty"));
                        if !(ending && empty) {
                            sink.diagnostic(
                                "realtime_transcription",
                                code.as_deref().unwrap_or("error"),
                            );
                            failure = Some(LiveFailure(ERR_TRANSCRIPTION_FAILED));
                        }
                        break;
                    }
                    Ok(Some(_)) => {}
                    // VOP-FR-QQYG: only `openai` does not end on a close.
                    Ok(None) => {
                        if !(ending && dialect != Dialect::Openai) {
                            sink.diagnostic("realtime_transcription", "closed_early");
                            failure = Some(LiveFailure(ERR_SESSION_LOST));
                        }
                        break;
                    }
                    Err(_) => {
                        if !(ending && transcript.all_completed()) {
                            sink.diagnostic("realtime_transcription", "socket_failed");
                            failure = Some(LiveFailure(ERR_SESSION_LOST));
                        }
                        break;
                    }
                }
                if ending && session_ended(dialect, turn_based, final_item.as_deref(), &transcript)
                {
                    break;
                }
                let text = transcript.text();
                if text != shown {
                    sink.transcript_changed(&config.session_id, &text);
                    shown = text;
                }
            }
            _ = expired => {
                failure = Some(LiveFailure(ERR_TRANSCRIPTION_FAILED));
                break;
            }
        }
    }
    let _ = socket.close(None).await;
    // The final text can end the loop before the check above sees it.
    let text = transcript.text();
    if text != shown {
        sink.transcript_changed(&config.session_id, &text);
    }
    match (finish, failure) {
        (Some(reply), None) => {
            let _ = reply.send(Ok(transcript.text().trim().to_string()));
        }
        (Some(reply), Some(failure)) => {
            let _ = reply.send(Err(failure.0.to_string()));
        }
        (None, Some(failure)) => drain_until_finish(commands, failure).await,
        (None, None) => {}
    }
}

impl VoiceAi {
    fn session_guard(&self) -> Result<std::sync::MutexGuard<'_, Option<CaptureSession>>, String> {
        self.capture.lock().map_err(|e| format!("voice capture poisoned: {e}"))
    }

    /// VOP-FR-KBPZ: copy the configuration and open one session.
    pub(crate) fn start_capture(
        &self,
        config: ActiveVoice,
        capture_rate: u32,
        keyterms: Vec<String>,
        sink: SharedSink,
    ) -> Result<VoiceCaptureStart, String> {
        let mut slot = self.session_guard()?;
        if slot.as_ref().is_some_and(|s| s.phase != Phase::Failed) {
            return Err(ERR_SESSION_ACTIVE.to_string());
        }
        let keyterms = usable_keyterms(keyterms);
        let route = config.record.transcription_route;
        let id = self.new_session_id();
        let live = (route == TranscriptionRoute::Realtime)
            .then(|| spawn_live(&config, capture_rate, &keyterms, &id, sink.clone()));
        let is_live = route == TranscriptionRoute::Realtime
            && config.record.capabilities.as_ref().and_then(|c| c.realtime_transcription)
                == Some(RealtimeTranscription::Live);
        *slot = Some(CaptureSession {
            id: id.clone(),
            config,
            capture_rate,
            keyterms,
            pcm: Vec::new(),
            phase: Phase::Recording,
            last_audio: Instant::now(),
            route,
            live,
            failure: None,
            sink,
        });
        Ok(VoiceCaptureStart { session_id: id, route, live: is_live })
    }

    /// VOP-FR-DXPK.
    pub(crate) fn append_capture(&self, session_id: &str, samples: Vec<i16>) -> Result<(), String> {
        let mut slot = self.session_guard()?;
        let session = slot
            .as_mut()
            .filter(|s| s.id == session_id && s.phase == Phase::Recording)
            .ok_or(ERR_SESSION_LOST)?;
        session.last_audio = Instant::now();
        session.pcm.extend_from_slice(&samples);
        if let Some(live) = &session.live {
            // A closed channel means that the socket failed. The audio is held
            // here for a retry, and the finish reports the failure.
            let _ = live.send(LiveCommand::Audio(samples));
        }
        Ok(())
    }

    /// Mark the session as finishing, and give what the end of input needs.
    fn begin_finish(
        &self,
        session_id: &str,
        from: Phase,
    ) -> Result<(TranscriptionRoute, Option<mpsc::UnboundedSender<LiveCommand>>), String> {
        let mut slot = self.session_guard()?;
        let session = slot
            .as_mut()
            .filter(|s| s.id == session_id && s.phase == from)
            .ok_or(ERR_SESSION_LOST)?;
        session.phase = Phase::Finishing;
        Ok((session.route, session.live.take()))
    }

    fn settle(&self, session_id: &str, result: &Result<String, String>) {
        let Ok(mut slot) = self.session_guard() else { return };
        if slot.as_ref().is_none_or(|s| s.id != session_id) {
            return;
        }
        match result {
            // VOP-FR-YEFK: a success ends the session and drops its audio.
            Ok(_) => *slot = None,
            // VOP-FR-UOYP: a failed session stays for a retry.
            Err(code) => {
                if let Some(session) = slot.as_mut() {
                    session.phase = Phase::Failed;
                    session.failure = Some(code.clone());
                }
            }
        }
    }

    async fn transcribe_held(&self, session_id: &str) -> Result<String, String> {
        let (config, capture_rate, keyterms, pcm, sink) = {
            let slot = self.session_guard()?;
            let session = slot.as_ref().filter(|s| s.id == session_id).ok_or(ERR_SESSION_LOST)?;
            (
                session.config.clone(),
                session.capture_rate,
                session.keyterms.clone(),
                session.pcm.clone(),
                session.sink.clone(),
            )
        };
        let record = &config.record;
        let samples = resample_all(&pcm, capture_rate, HTTP_RATE)
            .map_err(|_| ERR_TRANSCRIPTION_FAILED.to_string())?;
        let (client, _) = http::client().map_err(|f| f.transcription_code().to_string())?;
        let hints = http::hint_fields(&record.provider, &keyterms);
        http::transcribe(
            &client,
            &record.provider,
            &record.base_url,
            config.key.as_deref(),
            &http_stt_model(&record.provider, &record.stt_model),
            &samples,
            &hints,
        )
        .await
        .map_err(|f| {
            sink.diagnostic("http_transcription", &f.detail());
            f.transcription_code().to_string()
        })
    }

    /// VOP-FR-VZDI, VOP-FR-EYHP.
    pub(crate) async fn finish_capture(&self, session_id: &str) -> Result<String, String> {
        let (route, live) = self.begin_finish(session_id, Phase::Recording)?;
        let result = match (route, live) {
            (TranscriptionRoute::Realtime, Some(live)) => finish_live(live).await,
            _ => self.transcribe_held(session_id).await,
        };
        self.settle(session_id, &result);
        result
    }

    /// VOP-FR-IAFL: the same route, or the HTTP route where a Realtime socket
    /// failed and the capabilities hold it.
    pub(crate) async fn retry_capture(
        &self,
        session_id: &str,
        sink: SharedSink,
    ) -> Result<String, String> {
        self.begin_finish(session_id, Phase::Failed)?;
        let replay = {
            let mut slot = self.session_guard()?;
            let session = slot.as_mut().filter(|s| s.id == session_id).ok_or(ERR_SESSION_LOST)?;
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
                    sink,
                );
                let _ = live.send(LiveCommand::Audio(session.pcm.clone()));
                Some(live)
            } else {
                None
            }
        };
        let result = match replay {
            Some(live) => finish_live(live).await,
            None => self.transcribe_held(session_id).await,
        };
        self.settle(session_id, &result);
        result
    }

    /// VOP-FR-YEFK: a cancel drops the audio. Dropping the sender ends the
    /// task of a live session, and the task closes its socket.
    pub(crate) fn cancel_capture(&self, session_id: &str) -> Result<(), String> {
        let mut slot = self.session_guard()?;
        if slot.as_ref().is_some_and(|s| s.id == session_id) {
            *slot = None;
        }
        Ok(())
    }

    /// VOP-FR-BSNY: cancel a recording that has stopped to send audio.
    pub(crate) fn sweep_idle(&self, now: Instant) -> Result<bool, String> {
        let mut slot = self.session_guard()?;
        let idle = slot.as_ref().is_some_and(|s| {
            s.phase == Phase::Recording && now.duration_since(s.last_audio) >= WATCHDOG_LIMIT
        });
        if idle {
            *slot = None;
        }
        Ok(idle)
    }
}

fn spawn_live(
    config: &ActiveVoice,
    capture_rate: u32,
    keyterms: &[String],
    session_id: &str,
    sink: SharedSink,
) -> mpsc::UnboundedSender<LiveCommand> {
    let (sender, receiver) = mpsc::unbounded_channel();
    let live = LiveConfig {
        provider: config.record.provider.clone(),
        base_url: config.record.base_url.clone(),
        key: config.key.clone(),
        stt_model: config.record.stt_model.clone(),
        capture_rate,
        keyterms: keyterms.to_vec(),
        session_id: session_id.to_string(),
    };
    tauri::async_runtime::spawn(run_live(live, receiver, sink));
    sender
}

async fn finish_live(live: mpsc::UnboundedSender<LiveCommand>) -> Result<String, String> {
    let (reply, answer) = oneshot::channel();
    live.send(LiveCommand::Finish(reply)).map_err(|_| ERR_SESSION_LOST.to_string())?;
    answer.await.map_err(|_| ERR_SESSION_LOST.to_string())?
}
