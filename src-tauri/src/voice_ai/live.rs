//! One Realtime transcription session of a capture (VOP-FR-GYKP, VOP-FR-JAOB,
//! VOP-FR-XXAB, VOP-FR-HOGR, VOP-FR-QQYG).
//!
//! A chunked capture of the `openai` dialect keeps one session and commits the
//! input buffer at each chunk end (VOP-FR-GLRN). The other dialects end their
//! session at a commit, so the capture opens one session for each chunk.
//!
//! The `openai` dialect transcribes only committed audio. So while the input
//! continues, the session also sends interim commits at word gaps, and the
//! live text comes while the author speaks (VOP-FR-KIMU, VOP-FR-TWEZ).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};

use super::pcm::StreamResampler;
use super::realtime::{self, ServerEvent, Socket, Transcript};
use super::*;
use commits::CommitLedger;

mod commits;

const OPEN_LIMIT: Duration = Duration::from_secs(15);
const CUSTOM_OPEN_LIMIT: Duration = Duration::from_secs(60);
const FINISH_LIMIT: Duration = Duration::from_secs(60);

pub(crate) type Reply = oneshot::Sender<Result<String, String>>;

pub(crate) enum LiveCommand {
    Audio(Vec<i16>),
    /// VOP-FR-GLRN: the end of one chunk in a session that continues.
    Commit(Reply),
    /// VOP-FR-KIMU: a word gap, where an interim commit can go.
    Gap,
    Finish(Reply),
}

/// A socket that failed at open or in the session. What the failure means for
/// the author depends on the time, so each stage names its own code.
#[derive(Clone, Copy)]
struct LiveFailure(&'static str);

struct LiveConfig {
    provider: String,
    base_url: String,
    key: Option<String>,
    stt_model: String,
    capture_rate: u32,
    keyterms: Vec<String>,
    session_id: String,
    /// Set for a chunked capture: the live text of the open chunk
    /// (VOP-FR-WNQW).
    chunk_text: Option<Arc<Mutex<String>>>,
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

/// The end of a session that failed: answer each commit and the finish, and
/// drop the audio that still comes. The session holds its own copy for a retry.
async fn drain_until_finish(
    mut commands: mpsc::UnboundedReceiver<LiveCommand>,
    failure: LiveFailure,
) {
    while let Some(command) = commands.recv().await {
        match command {
            LiveCommand::Audio(_) | LiveCommand::Gap => {}
            LiveCommand::Commit(reply) => {
                let _ = reply.send(Err(failure.0.to_string()));
            }
            LiveCommand::Finish(reply) => {
                let _ = reply.send(Err(failure.0.to_string()));
                return;
            }
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

/// VOP-FR-KIMU: the audio that no commit holds, at least, before a word gap
/// sends an interim commit.
pub(crate) const INTERIM_MIN_SECONDS: u32 = 1;
/// VOP-FR-KIMU: the audio that no commit holds, at most, before an interim
/// commit goes with no word gap.
pub(crate) const INTERIM_MAX_SECONDS: u32 = 4;

/// VOP-FR-KIMU: whether an interim commit goes now. `uncommitted` is the
/// count of samples at the capture rate that no commit holds.
pub(crate) fn interim_due(uncommitted: usize, capture_rate: u32, at_gap: bool) -> bool {
    let seconds = if at_gap { INTERIM_MIN_SECONDS } else { INTERIM_MAX_SECONDS };
    uncommitted > 0 && uncommitted >= capture_rate as usize * seconds as usize
}

/// VOP-FR-KIMU: log an interim commit, with counts alone.
fn note_interim(sink: &SharedSink, ledger: &CommitLedger, uncommitted: usize, at_gap: bool) {
    sink.live_step(
        "voice interim commit sent",
        &[("group", ledger.open_group()), ("samples", uncommitted), ("atGap", usize::from(at_gap))],
        false,
    );
}

/// The live text: the whole transcript, or for a chunked capture the text of
/// the open chunk (VOP-FR-JAOB, VOP-FR-WNQW).
fn shown_text(config: &LiveConfig, transcript: &Transcript, ledger: &CommitLedger) -> String {
    if config.chunk_text.is_some() {
        transcript.text_except(ledger.closed())
    } else {
        transcript.text()
    }
}

impl LiveConfig {
    /// Keep the live text of the open chunk for the sentence end check.
    fn keep_chunk_text(&self, text: &str) {
        if let Some(shared) = &self.chunk_text {
            if let Ok(mut open) = shared.lock() {
                *open = text.to_string();
            }
        }
    }
}

/// The text that the session shows, and its change.
struct Shown {
    text: String,
}

impl Shown {
    /// VOP-FR-JAOB: emit the live text when it changed.
    fn update(&mut self, config: &LiveConfig, sink: &SharedSink, text: String) {
        if text != self.text {
            // The sentence end check reads the kept text, so it is kept before
            // anyone hears of it.
            config.keep_chunk_text(&text);
            sink.transcript_changed(&config.session_id, &text);
            self.text = text;
        }
    }
}

/// One Realtime session. The channel holds the audio that comes before the
/// socket is ready.
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

    let chunked = config.chunk_text.is_some();
    // VOP-FR-KIMU: only the `openai` dialect continues after a commit.
    let interims = dialect == Dialect::Openai;
    let mut transcript = Transcript::default();
    let mut ledger = CommitLedger::default();
    let mut shown = Shown { text: String::new() };
    // VOP-FR-KIMU: the samples at the capture rate that no commit holds.
    let mut uncommitted: usize = 0;
    let mut finish: Option<Reply> = None;
    let mut deadline = None::<tokio::time::Instant>;
    let mut failure: Option<LiveFailure> = None;
    // VOP-FR-QQYG: the item that the final commit named.
    let mut final_item: Option<String> = None;
    let turn_based = realtime::is_turn_based(&session);

    loop {
        // VOP-FR-QQYG: the session has all of its text.
        if finish.is_some() {
            let done = if interims {
                ledger.idle(&transcript)
            } else {
                session_ended(dialect, turn_based, final_item.as_deref(), &transcript)
            };
            if done {
                break;
            }
        }
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
                    let count = samples.len();
                    let sent = match resampler.push(&samples) {
                        Ok(out) => realtime::send_audio(&mut socket, &out).await,
                        Err(e) => Err(e),
                    };
                    if sent.is_err() {
                        failure = Some(LiveFailure(ERR_SESSION_LOST));
                        break;
                    }
                    uncommitted += count;
                    if interims && interim_due(uncommitted, config.capture_rate, false) {
                        if realtime::commit_input(&mut socket).await.is_err() {
                            failure = Some(LiveFailure(ERR_SESSION_LOST));
                            break;
                        }
                        note_interim(&sink, &ledger, uncommitted, false);
                        ledger.sent();
                        uncommitted = 0;
                    }
                }
                Some(LiveCommand::Gap) => {
                    if interims && interim_due(uncommitted, config.capture_rate, true) {
                        if realtime::commit_input(&mut socket).await.is_err() {
                            failure = Some(LiveFailure(ERR_SESSION_LOST));
                            break;
                        }
                        note_interim(&sink, &ledger, uncommitted, true);
                        ledger.sent();
                        uncommitted = 0;
                    }
                }
                Some(LiveCommand::Commit(reply)) => {
                    // VOP-FR-TWEZ: a chunk end with no audio after the last
                    // commit sends no commit.
                    if uncommitted > 0 {
                        if realtime::commit_input(&mut socket).await.is_err() {
                            let _ = reply.send(Err(ERR_SESSION_LOST.to_string()));
                            failure = Some(LiveFailure(ERR_SESSION_LOST));
                            break;
                        }
                        ledger.sent();
                        uncommitted = 0;
                    }
                    sink.live_step(
                        "voice chunk sealed",
                        &[("group", ledger.open_group()), ("waiting", ledger.waiting_count())],
                        false,
                    );
                    ledger.seal(reply, transcript.known_items());
                    ledger.settle(&transcript);
                    // The live text of the next chunk starts now.
                    shown.update(&config, &sink, shown_text(&config, &transcript, &ledger));
                }
                Some(LiveCommand::Finish(reply)) => {
                    let ended = match resampler.finish() {
                        Ok(out) => {
                            // VOP-FR-QQYG: after an interim commit, the final
                            // commit goes only for audio that no commit holds.
                            // The drained filter tail is not such audio: it
                            // is a few milliseconds of the audio that the last
                            // commit cut.
                            let skip = interims && ledger.any_sent() && uncommitted == 0;
                            match realtime::send_audio(&mut socket, &out).await {
                                Ok(()) if skip => {
                                    sink.live_step(
                                        "voice finish sent no final commit",
                                        &[("group", ledger.open_group())],
                                        false,
                                    );
                                    Ok(())
                                }
                                Ok(()) => {
                                    if interims {
                                        ledger.sent();
                                    }
                                    realtime::end_input(&mut socket, dialect).await
                                }
                                Err(e) => Err(e),
                            }
                        }
                        Err(e) => Err(e),
                    };
                    uncommitted = 0;
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
                        if ledger.named(item_id.as_deref()) {
                            if let Some(item) = item_id.as_deref() {
                                transcript.mark_ours(item);
                            }
                        } else if ending {
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
                        if empty && ledger.named(None) {
                            ledger.settle(&transcript);
                            continue;
                        }
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
                ledger.settle(&transcript);
                // VOP-FR-DSSU: in a chunked capture, the chunk events carry the
                // text after the finish.
                if !(chunked && ending) {
                    shown.update(&config, &sink, shown_text(&config, &transcript, &ledger));
                }
            }
            _ = expired => {
                sink.live_step(
                    "voice finish deadline expired",
                    &[
                        ("waiting", ledger.waiting_count()),
                        ("incomplete", ledger.incomplete_count(&transcript)),
                    ],
                    true,
                );
                failure = Some(LiveFailure(ERR_TRANSCRIPTION_FAILED));
                break;
            }
        }
    }
    let _ = socket.close(None).await;
    if !chunked {
        // The final text can end the loop before the check above sees it.
        let text = transcript.text();
        if text != shown.text {
            sink.transcript_changed(&config.session_id, &text);
        }
    }
    let code = failure.map_or(ERR_TRANSCRIPTION_FAILED, |f| f.0);
    ledger.fail_all(code);
    match (finish, failure) {
        (Some(reply), None) => {
            let text = shown_text(&config, &transcript, &ledger);
            let _ = reply.send(Ok(text.trim().to_string()));
        }
        (Some(reply), Some(failure)) => {
            let _ = reply.send(Err(failure.0.to_string()));
        }
        (None, Some(failure)) => drain_until_finish(commands, failure).await,
        (None, None) => {}
    }
}

/// Open one session. `chunk_text` is set for a chunked capture.
pub(crate) fn spawn_live(
    config: &ActiveVoice,
    capture_rate: u32,
    keyterms: &[String],
    session_id: &str,
    chunk_text: Option<Arc<Mutex<String>>>,
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
        chunk_text,
    };
    tauri::async_runtime::spawn(run_live(live, receiver, sink));
    sender
}

/// Send a command that takes a reply, and wait for the reply.
async fn ask(
    live: &mpsc::UnboundedSender<LiveCommand>,
    command: impl FnOnce(Reply) -> LiveCommand,
) -> Result<String, String> {
    let (reply, answer) = oneshot::channel();
    live.send(command(reply)).map_err(|_| ERR_SESSION_LOST.to_string())?;
    answer.await.map_err(|_| ERR_SESSION_LOST.to_string())?
}

pub(crate) async fn finish_live(
    live: mpsc::UnboundedSender<LiveCommand>,
) -> Result<String, String> {
    ask(&live, LiveCommand::Finish).await
}

/// VOP-FR-KIMU: a word gap of the capture.
pub(crate) fn gap_live(live: &mpsc::UnboundedSender<LiveCommand>) {
    // A closed channel means that the socket failed, and the finish reports it.
    let _ = live.send(LiveCommand::Gap);
}

/// VOP-FR-GLRN: commit the open chunk, and give a wait for its text. The commit
/// goes out now, so that it stays in order with the audio.
pub(crate) fn commit_live(
    live: &mpsc::UnboundedSender<LiveCommand>,
) -> oneshot::Receiver<Result<String, String>> {
    let (reply, answer) = oneshot::channel();
    // A closed channel drops the reply, and the wait then fails.
    let _ = live.send(LiveCommand::Commit(reply));
    answer
}
