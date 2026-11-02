//! The chunks of a capture session (VOP-FR-JRVF, VOP-FR-VOZD, VOP-FR-GLRN,
//! VOP-FR-GLDR).

use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;

use super::super::chunks::{
    detect_with, is_sentence_end, spawn_detector, ChunkLedger, Chunking, DetectorAnswer,
    DetectorMessage, SharedLedger, NO_TEXT_SENTENCE_SECONDS,
};
use super::super::live::{commit_live, finish_live, gap_live, spawn_live};
use super::*;

/// The chunk state of one capture.
pub(super) struct ChunkRun {
    ledger: SharedLedger,
    /// The position in the held audio where the open chunk starts.
    start: usize,
    /// The transcriptions that run, one for each chunk that ended.
    tasks: Vec<tauri::async_runtime::JoinHandle<()>>,
    /// VOP-FR-WNQW: the live text of the open chunk, on a Realtime route.
    text: Arc<Mutex<String>>,
    /// VOP-FR-YDVH: the detector of a Push-to-Talk capture.
    detector: Option<Sender<DetectorMessage>>,
}

impl ChunkRun {
    pub(super) fn open(
        capture_id: &str,
        capture_rate: u32,
        chunking: Chunking,
        sink: &SharedSink,
    ) -> Option<Self> {
        let detector = match chunking {
            Chunking::Off | Chunking::Gaps { .. } => return None,
            Chunking::Session => None,
            Chunking::Own { settings, load } => {
                spawn_detector(capture_rate, settings, load, sink.clone())
            }
        };
        Some(Self {
            ledger: Arc::new(Mutex::new(ChunkLedger::new(capture_id, sink.clone()))),
            start: 0,
            tasks: Vec::new(),
            text: Arc::new(Mutex::new(String::new())),
            detector,
        })
    }

    /// The store of the live text of the open chunk.
    pub(super) fn new_text(&self) -> Arc<Mutex<String>> {
        self.text.clone()
    }

    /// Send audio to the detector, and give the wait for its answer.
    pub(super) fn detect(&self, samples: &[i16]) -> Option<oneshot::Receiver<DetectorAnswer>> {
        detect_with(self.detector.as_ref()?, samples)
    }

    fn open_text(&self) -> String {
        self.text.lock().map(|t| t.clone()).unwrap_or_default()
    }
}

impl Drop for ChunkRun {
    /// A cancelled capture stops its transcriptions and emits nothing more.
    fn drop(&mut self) {
        if let Ok(mut ledger) = self.ledger.lock() {
            ledger.close();
        }
        for task in &self.tasks {
            task.abort();
        }
    }
}

/// VOP-FR-VOZD: transcribe one chunk in the background, and give the result
/// to the ledger.
fn spawn_chunk<F>(
    ledger: &SharedLedger,
    index: usize,
    transcription: F,
) -> tauri::async_runtime::JoinHandle<()>
where
    F: std::future::Future<Output = Result<String, String>> + Send + 'static,
{
    let ledger = ledger.clone();
    tauri::async_runtime::spawn(async move {
        let result = transcription.await;
        if let Ok(mut ledger) = ledger.lock() {
            ledger.resolve(index, result);
        }
    })
}

/// VOP-FR-JRVF, VOP-FR-GLRN: end the open chunk of `session`. `last` is the
/// end of the capture, so no session opens for a next chunk.
fn end_chunk(session: &mut CaptureSession, last: bool) {
    let job = HttpJob::of(session);
    let Some(run) = session.chunks.as_mut() else { return };
    let index = match run.ledger.lock() {
        Ok(mut ledger) => ledger.end(),
        Err(_) => return,
    };
    let audio = session.pcm[run.start.min(session.pcm.len())..].to_vec();
    run.start = session.pcm.len();
    let task = match session.route {
        TranscriptionRoute::Http => spawn_chunk(&run.ledger, index, job.run(audio)),
        TranscriptionRoute::Realtime if last => {
            let live = session.live.take();
            spawn_chunk(&run.ledger, index, async move {
                match live {
                    Some(live) => finish_live(live).await,
                    None => Err(ERR_SESSION_LOST.to_string()),
                }
            })
        }
        // VOP-FR-GLRN: the `openai` session continues after a commit.
        TranscriptionRoute::Realtime if session.config.record.provider == "openai" => {
            let answer = session.live.as_ref().map(commit_live);
            if let Ok(mut text) = run.text.lock() {
                text.clear();
            }
            spawn_chunk(&run.ledger, index, async move {
                match answer {
                    Some(answer) => {
                        answer.await.unwrap_or_else(|_| Err(ERR_SESSION_LOST.to_string()))
                    }
                    None => Err(ERR_SESSION_LOST.to_string()),
                }
            })
        }
        // VOP-FR-GLRN: the other dialects end their session at a commit, so the
        // next chunk has a new session.
        TranscriptionRoute::Realtime => {
            let ended = session.live.take();
            run.text = Arc::new(Mutex::new(String::new()));
            session.live = Some(spawn_live(
                &session.config,
                session.capture_rate,
                &session.keyterms,
                &session.id,
                Some(run.text.clone()),
                session.sink.clone(),
            ));
            spawn_chunk(&run.ledger, index, async move {
                match ended {
                    Some(live) => finish_live(live).await,
                    None => Err(ERR_SESSION_LOST.to_string()),
                }
            })
        }
    };
    run.tasks.push(task);
    if let Some(detector) = &run.detector {
        let _ = detector.send(DetectorMessage::ChunkEnded);
    }
}

/// VOP-FR-WNQW: whether the open chunk stands at a sentence end.
fn at_sentence_end(session: &CaptureSession, run: &ChunkRun) -> bool {
    if session.is_live {
        is_sentence_end(&run.open_text())
    } else {
        let audio = session.pcm.len().saturating_sub(run.start);
        audio >= session.capture_rate as usize * NO_TEXT_SENTENCE_SECONDS
    }
}

impl VoiceAi {
    pub(super) fn is_chunked(&self, session_id: &str) -> bool {
        self.session_guard()
            .is_ok_and(|slot| slot.iter().any(|s| s.id == session_id && s.chunks.is_some()))
    }

    /// VOP-FR-KIMU: the detector found a word gap. The live session decides
    /// whether an interim commit goes.
    pub(crate) fn word_gap(&self, session_id: &str) {
        let Ok(slot) = self.session_guard() else { return };
        let live = slot
            .iter()
            .find(|s| s.id == session_id && s.phase == Phase::Recording)
            .and_then(|s| s.live.as_ref());
        if let Some(live) = live {
            gap_live(live);
        }
    }

    /// VOP-FR-JRVF: the detector found a short pause. The chunk ends when the
    /// pause follows a sentence end. Gives whether a chunk ended.
    pub(crate) fn chunk_pause(&self, session_id: &str) -> Result<bool, String> {
        let mut slot = self.session_guard()?;
        let Some(session) =
            slot.iter_mut().find(|s| s.id == session_id && s.phase == Phase::Recording)
        else {
            return Ok(false);
        };
        let Some(run) = session.chunks.as_ref() else { return Ok(false) };
        if !at_sentence_end(session, run) {
            return Ok(false);
        }
        end_chunk(session, false);
        Ok(true)
    }

    /// VOP-FR-GLDR: end the last chunk, wait for every chunk, and join the texts
    /// that succeeded.
    pub(super) async fn finish_chunked(&self, session_id: &str) -> Result<String, String> {
        let (tasks, ledger) = {
            let mut slot = self.session_guard()?;
            let session = slot
                .iter_mut()
                .find(|s| s.id == session_id && s.phase.unfinished())
                .ok_or(ERR_SESSION_LOST)?;
            session.phase = Phase::Finishing;
            let Some(run) = session.chunks.as_mut() else { return Err(ERR_SESSION_LOST.into()) };
            run.detector = None;
            let open_audio = session.pcm.len() > run.start;
            let none_yet = run.ledger.lock().map(|l| l.count() == 0).unwrap_or(true);
            if open_audio || none_yet {
                end_chunk(session, true);
            } else {
                // The open chunk has no audio: its socket closes with no request.
                session.live = None;
            }
            let Some(run) = session.chunks.as_mut() else { return Err(ERR_SESSION_LOST.into()) };
            (std::mem::take(&mut run.tasks), run.ledger.clone())
        };
        for task in tasks {
            let _ = task.await;
        }
        let result = ledger.lock().map_err(|e| format!("voice chunks poisoned: {e}"))?.outcome();
        self.settle(session_id, &result);
        result
    }
}
