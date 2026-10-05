//! A run's two durable log streams (`GRS-graduation-run-log-storage.md`).
//!
//! Every run owns `logs/source.jsonl` and `logs/structured.jsonl` under its own
//! directory (GRD-FR-OSCG). They are what the author reads a run back from
//! after it has stopped, which is the whole reason a failing run is debuggable
//! rather than opaque.
//!
//! **Persistence is mandatory** (GRS-FR-EIXS). A stream that cannot be written
//! stops the run before it performs another agent action or another state
//! transition, and the author's Continue retries the pending writes first
//! (GRD-FR-IKVE). That is why [`emit`] answers whether the run may go on, and
//! why every caller reads the answer.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use tauri::Manager;

use crate::fs::FsAccess;
use crate::log_fields;
use crate::logging::{self, Domain};

use super::events::announce_log_records;
use super::observability::GraduationVisualStage;
use super::{GraduationInterruptionReason, GraduationRun};

pub mod index;
pub mod read;
pub mod records;
pub mod writer;

pub use index::{
    GraduationLogFailure, GraduationLogIndexes, GraduationLogPersistence, GraduationLogSegment,
    GraduationStreamIndex,
};
pub use read::{
    GraduationLogCursor, GraduationLogPage, GraduationLogPageEntry, GraduationLogPassScope,
    GraduationLogPresentation,
};
pub use records::{
    GraduationLogLevel, GraduationLogOrigin, GraduationLogProducer, GraduationLogRecord,
    GraduationLogStream, GraduationSourceChunk, GraduationStructuredRecord,
};
pub use writer::LogPaths;

/// What one caller wants written, before this module attributes it.
pub struct StructuredEvent {
    pub producer: GraduationLogProducer,
    pub level: GraduationLogLevel,
    pub event: String,
    pub fields: serde_json::Map<String, serde_json::Value>,
    /// The phase a stage transition is **leaving** (GRS-FR-EPPM). Every other
    /// producer settles its own phase and ignores this.
    pub leaving: Option<GraduationVisualStage>,
}

impl StructuredEvent {
    pub fn new(producer: GraduationLogProducer, event: impl Into<String>) -> Self {
        Self {
            producer,
            level: GraduationLogLevel::Info,
            event: event.into(),
            fields: serde_json::Map::new(),
            leaving: None,
        }
    }

    pub fn at_level(mut self, level: GraduationLogLevel) -> Self {
        self.level = level;
        self
    }

    pub fn leaving(mut self, stage: GraduationVisualStage) -> Self {
        self.leaving = Some(stage);
        self
    }

    /// Add one field. Values are what a surface renders and what a search
    /// matches, so they carry no credential and no byte of a project file.
    pub fn with(mut self, key: &str, value: impl Into<serde_json::Value>) -> Self {
        self.fields.insert(key.to_string(), value.into());
        self
    }
}

/// The log indexes of every run this process is writing.
///
/// A run's structured records come from the loop's own thread and its source
/// chunks come from the executor's output path, so **two threads write one
/// run's streams**. One shared index under one hold is what keeps their
/// sequences ascending and keeps either from discarding what the other wrote
/// (GRS-FR-SXNY).
#[derive(Default)]
pub struct LogRegistry {
    held: Mutex<BTreeMap<String, Arc<Mutex<GraduationLogIndexes>>>>,
}

impl LogRegistry {
    /// The indexes of one run, seeded from what the store holds.
    fn handle(
        &self,
        run_id: &str,
        seed: impl FnOnce() -> GraduationLogIndexes,
    ) -> Arc<Mutex<GraduationLogIndexes>> {
        let mut held = match self.held.lock() {
            Ok(held) => held,
            Err(poisoned) => poisoned.into_inner(),
        };
        held.entry(run_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(seed())))
            .clone()
    }

    /// Forget a run this process has stopped writing.
    pub fn release(&self, run_id: &str) {
        if let Ok(mut held) = self.held.lock() {
            held.remove(run_id);
        }
    }
}

/// The shared indexes of one run, seeded from the record the caller holds.
///
/// GRS-FR-IOHF: the first use of a run's indexes in this process reconciles
/// the seed against the files, whichever path uses them first. The record is
/// saved at turn boundaries and a turn may run for hours, so a process that
/// stopped inside one left a record naming fewer records than the files hold.
/// A seed left behind would make the next append reuse a sequence the files
/// already hold (GRS-FR-SXNY).
fn handle_of<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run_id: &str,
    seed: impl FnOnce() -> GraduationLogIndexes,
) -> Option<Arc<Mutex<GraduationLogIndexes>>> {
    let registry = app.try_state::<LogRegistry>()?;
    Some(registry.handle(run_id, || {
        let mut seeded = seed();
        if let (Ok(fs), Ok(paths)) = (super::store_fs(app), paths_for(app, run_id)) {
            writer::reconcile(&fs, &paths, &mut seeded);
        }
        seeded
    }))
}

fn locked(handle: &Mutex<GraduationLogIndexes>) -> std::sync::MutexGuard<'_, GraduationLogIndexes> {
    match handle.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Forget a run this process has stopped driving.
pub fn release<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run_id: &str) {
    if let Some(registry) = app.try_state::<LogRegistry>() {
        registry.release(run_id);
    }
}

/// Where one run's streams stand.
pub fn paths_for<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run_id: &str,
) -> Result<LogPaths, String> {
    Ok(LogPaths::new(super::store_base(app)?.logs(run_id)))
}

/// GRS-FR-KDOY: create both streams for a run that is about to exist.
pub fn initialize<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run: &mut GraduationRun) {
    let (Ok(fs), Ok(paths)) = (super::store_fs(app), paths_for(app, &run.id)) else {
        return;
    };
    if let Err(failure) = writer::initialize(&fs, &paths) {
        run.logs.note_failed(failure, 0);
    }
}

/// GRS-FR-EIXS / GRD-FR-IKVE: write one structured record, and answer whether
/// the run may go on.
///
/// `false` means the record could not be stored. The run has been rested
/// `interrupted` with the reason `log_persistence_failed`, and the caller must
/// stop rather than perform another agent action or another state transition.
#[must_use]
pub fn emit<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    event: StructuredEvent,
) -> bool {
    // GRS-FR-EIXS: **either** required stream is mandatory. A source chunk the
    // executor's own thread could not store is found here, which is still
    // before the run performs another agent action.
    if stream_failed(app, &run.id) {
        refresh(app, run);
        if let Some(failure) = run.logs.persistence.failure.clone() {
            rest_on_log_failure(app, run, failure);
            return false;
        }
    }
    let record = compose(run, &event);
    write_records(app, run, GraduationLogStream::Structured, vec![record])
}

/// GRD-FR-IKVE: retry the pending writes, before anything else is attempted.
///
/// Answers whether the streams are healthy afterwards. A run whose retry still
/// fails stays interrupted and Continue stays available.
pub fn retry_pending<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run: &mut GraduationRun) -> bool {
    if run.logs.persistence.is_healthy() {
        return true;
    }
    let (Ok(fs), Ok(paths)) = (super::store_fs(app), paths_for(app, &run.id)) else {
        return false;
    };
    let handle = handle_of(app, &run.id, || run.logs.clone());
    let mut held = handle.as_ref().map(|handle| locked(handle));
    let indexes: &mut GraduationLogIndexes = match held.as_mut() {
        Some(guard) => guard,
        None => &mut run.logs,
    };
    match writer::replay_pending(&fs, &paths, indexes) {
        Ok(pending) => {
            indexes.note_healthy(pending);
            let settled = indexes.clone();
            drop(held);
            run.logs = settled;
            logging::log_info(
                app,
                &crate::logging::BUFFER,
                &[Domain::Backend],
                "graduation replayed the log writes a run was resting on",
                log_fields! { "run_id" => run.id.clone() },
            );
            let _ = super::save_run(app, run);
            true
        }
        Err(failure) => {
            let pending = failure.pending_record_ids.len() as u32;
            indexes.note_failed(failure, pending);
            let settled = indexes.clone();
            drop(held);
            run.logs = settled;
            let _ = super::save_run(app, run);
            false
        }
    }
}

/// The one place a record reaches a file, and the one place a failure rests the
/// run.
///
/// The indexes it advances are the run's **shared** ones, so a structured record
/// from the loop and a source chunk from the executor's output path never take
/// the same sequence and never discard each other's work.
fn write_records<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    stream: GraduationLogStream,
    records: Vec<GraduationLogRecord>,
) -> bool {
    match append_shared(app, &run.id, run.logs.clone(), stream, records) {
        Ok(indexes) => {
            run.logs = indexes;
            // GRS-FR-UCZL: told after the records are durable, so a consumer
            // that re-reads on it reads the result rather than racing it.
            announce_log_records(
                app,
                &run.id,
                stream,
                run.logs.stream(stream).latest_sequence,
            );
            true
        }
        Err(indexes) => {
            run.logs = indexes;
            let failure = run
                .logs
                .persistence
                .failure
                .clone()
                .unwrap_or_else(|| {
                    GraduationLogFailure::write(
                        index::CODE_STORAGE_UNAVAILABLE,
                        stream,
                        "The run's log storage could not be reached.",
                        Vec::new(),
                    )
                });
            rest_on_log_failure(app, run, failure);
            false
        }
    }
}

/// Append to one stream under the run's shared indexes.
///
/// Answers the indexes as they now stand, whichever way it went, so the caller's
/// own copy of the run record is never left behind what the files hold.
fn append_shared<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run_id: &str,
    seed: GraduationLogIndexes,
    stream: GraduationLogStream,
    records: Vec<GraduationLogRecord>,
) -> Result<GraduationLogIndexes, GraduationLogIndexes> {
    let ids: Vec<String> = records.iter().map(|r| r.record_id().to_string()).collect();
    let unavailable = |mut indexes: GraduationLogIndexes| {
        let count = ids.len() as u32;
        indexes.note_failed(
            GraduationLogFailure::write(
                index::CODE_STORAGE_UNAVAILABLE,
                stream,
                "The run's log storage could not be reached.",
                ids.clone(),
            ),
            count,
        );
        indexes
    };
    let (Ok(fs), Ok(paths)) = (super::store_fs(app), paths_for(app, run_id)) else {
        return Err(unavailable(seed));
    };
    let Some(handle) = handle_of(app, run_id, || seed.clone()) else {
        return Err(unavailable(seed));
    };
    let mut indexes = locked(&handle);
    match writer::append(&fs, &paths, &mut indexes, stream, records) {
        Ok(()) => {
            if !indexes.persistence.is_healthy() {
                indexes.note_healthy(0);
            }
            Ok(indexes.clone())
        }
        Err(failure) => {
            let count = failure.pending_record_ids.len() as u32;
            indexes.note_failed(failure, count);
            Err(indexes.clone())
        }
    }
}

/// GRS-FR-SXNY: append the executor output of a turn, from the thread that read
/// it.
///
/// The run record is written by the loop's thread alone, so a failure here is
/// recorded on the shared indexes and the loop rests the run at its next record
/// — which is still before it performs another agent action (GRS-FR-EIXS).
pub fn append_source_chunks<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run_id: &str,
    seed: GraduationLogIndexes,
    chunks: Vec<GraduationSourceChunk>,
) {
    if chunks.is_empty() {
        return;
    }
    let records = chunks.into_iter().map(GraduationLogRecord::Source).collect();
    match append_shared(app, run_id, seed, GraduationLogStream::Source, records) {
        // GRS-FR-UCZL: the source stream announces that it grew exactly as the
        // structured one does. A surface following a working turn reads the raw
        // output back under its own cursor on this, and nothing else tells it.
        Ok(indexes) => announce_log_records(
            app,
            run_id,
            GraduationLogStream::Source,
            indexes.source.latest_sequence,
        ),
        Err(_) => logging::log_error(
            app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "graduation could not write a turn's output to the run's source stream",
            log_fields! { "run_id" => run_id.to_string() },
        ),
    }
}

/// Bring a run record up to the shared indexes before it is written.
///
/// The loop calls this before it saves, so the record names what both threads
/// have written rather than what its own thread has.
pub fn refresh<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run: &mut GraduationRun) {
    let Some(handle) = handle_of(app, &run.id, || run.logs.clone()) else {
        return;
    };
    run.logs = locked(&handle).clone();
}

/// GRS-FR-IOHF: bring the saved copy of a run's indexes up to its files.
///
/// For a run the process stopped inside a turn, which no live index will ever
/// describe. Without it, the record of that run keeps naming fewer records
/// than its files hold, and the stages of its last turn stay unopenable.
pub fn reconcile_saved<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run: &mut GraduationRun) {
    if held_handle(app, &run.id).is_some() {
        return;
    }
    let (Ok(fs), Ok(paths)) = (super::store_fs(app), paths_for(app, &run.id)) else {
        logging::log_warn(
            app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "graduation could not reach a stopped run's log storage to reconcile its index",
            log_fields! { "run_id" => run.id.clone() },
        );
        return;
    };
    let before = (run.logs.source.record_count, run.logs.structured.record_count);
    writer::reconcile(&fs, &paths, &mut run.logs);
    let after = (run.logs.source.record_count, run.logs.structured.record_count);
    if before != after {
        logging::log_info(
            app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "graduation brought a stopped run's saved log index up to its files",
            log_fields! {
                "run_id" => run.id.clone(),
                "saved_source_records" => before.0,
                "source_records" => after.0,
                "saved_structured_records" => before.1,
                "structured_records" => after.1,
            },
        );
    }
}

/// The shared indexes of a run, only where this process already holds them.
///
/// Unlike [`handle_of`], it seeds nothing, so a reader leaves the registry as it
/// found it.
fn held_handle<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run_id: &str,
) -> Option<Arc<Mutex<GraduationLogIndexes>>> {
    let registry = app.try_state::<LogRegistry>()?;
    let held = match registry.held.lock() {
        Ok(held) => held,
        Err(poisoned) => poisoned.into_inner(),
    };
    held.get(run_id).cloned()
}

/// Whether the shared indexes say a stream stopped being written.
pub fn stream_failed<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run_id: &str) -> bool {
    held_handle(app, run_id)
        .map(|handle| !locked(&handle).persistence.is_healthy())
        .unwrap_or(false)
}

/// GRS-FR-ZTCF: answer a read of a run record with the live indexes.
///
/// The saved copy names only what the last save saw, and a turn can run for
/// hours (GRS-FR-FCRC). Where this process writes the run, the live indexes
/// replace that copy. Where it does not, the record stays as it was read.
///
/// It runs for every chunk the executor writes, so it logs nothing. The record
/// of a damaged read (GRS-FR-EYNU) is kept from the saved copy, because only a
/// read writes it and the live indexes never hold it.
pub fn overlay_live<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run: &mut GraduationRun) {
    let Some(handle) = held_handle(app, &run.id) else {
        return;
    };
    let mut live = locked(&handle).clone();
    live.last_read_failure = run.logs.last_read_failure.take();
    run.logs = live;
}

/// GRS-FR-EIXS / GRS-FR-DDSB: stop the run, naming the failure and what it left.
fn rest_on_log_failure<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    failure: GraduationLogFailure,
) {
    let pending = failure.pending_record_ids.len() as u32;
    logging::log_error(
        app,
        &crate::logging::BUFFER,
        &[Domain::Backend],
        "graduation could not write a run's log stream",
        log_fields! {
            "run_id" => run.id.clone(),
            "stream" => failure.stream.as_str(),
            "code" => failure.code.clone(),
            "pending" => pending as i64,
        },
    );
    run.logs.note_failed(failure, pending);
    let _ = super::transitions::interrupt(
        app,
        run,
        GraduationInterruptionReason::LogPersistenceFailed,
        "The run's log stream could not be written.",
    );
}

/// GRS-FR-KJVN: attribute one record by what produced it, and by nothing else.
fn compose(run: &GraduationRun, event: &StructuredEvent) -> GraduationLogRecord {
    let phase = event.producer.phase_id(event.leaving);
    GraduationLogRecord::Structured(GraduationStructuredRecord {
        attribution: records::GraduationLogAttribution {
            schema_version: 1,
            record_id: crate::notes::new_note_id(),
            // Assigned by the writer.
            sequence: 0,
            at: crate::notes::now_rfc3339(),
            run_id: run.id.clone(),
            phase_id: phase.as_str().to_string(),
            pass: event.producer.carries_pass().then(|| run.pass()),
            origin: event.producer.origin(),
            producer: event.producer.as_str().to_string(),
        },
        level: event.level,
        event: event.event.clone(),
        fields: event.fields.clone(),
    })
}

/// One chunk of executor output, attributed to the turn that produced it.
///
/// The channel the executor reported is the record's `source`, which is why the
/// three names agree byte-for-byte: `stdout`, `stderr`, and `executor`.
pub fn source_chunk(
    run: &GraduationRun,
    producer: GraduationLogProducer,
    channel: &str,
    payload: &str,
) -> GraduationSourceChunk {
    use base64::Engine as _;
    let bytes = payload.as_bytes();
    GraduationSourceChunk {
        attribution: records::GraduationLogAttribution {
            schema_version: 1,
            record_id: crate::notes::new_note_id(),
            sequence: 0,
            at: crate::notes::now_rfc3339(),
            run_id: run.id.clone(),
            phase_id: producer.phase_id(None).as_str().to_string(),
            pass: producer.carries_pass().then(|| run.pass()),
            origin: GraduationLogOrigin::Executor,
            producer: producer.as_str().to_string(),
        },
        agent: None,
        container: None,
        source: channel.to_string(),
        encoding: "base64".to_string(),
        data_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
        byte_length: bytes.len() as u64,
    }
}

/// The sink that carries a turn's executor output into `source.jsonl`.
///
/// It stands beside `AGV-agent-activity.md`'s in-memory sink rather than in
/// place of it: that one is the live panel and is evicted, and this one is the
/// durable copy the author reads a finished run back from (GRS-FR-JQOO).
///
/// **What it delivers is the executor's masked activity events** rather than
/// byte-exact stdout chunks. `AgentActivityEvent` carries a summary and a
/// payload string, and the executor exposes no byte-exact durable seam
/// (GRS-FR-QDVH), so a record here holds the masked text of an event and not
/// the raw bytes of the stream it came from.
///
/// GRS-FR-YXVY / GRS-FR-JWBW: what arrives has already passed the executor's
/// own credential and session-identity masking (`../tools/EAC-execute-agent-cli.md`
/// EAC-FR-29). This module applies no second rule of its own and inspects
/// nothing: it accepts chunks from that masked delivery path and from no other.
pub struct GraduationSourceSink<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    run_id: String,
    producer: GraduationLogProducer,
}

impl<R: tauri::Runtime> GraduationSourceSink<R> {
    pub fn new(app: &tauri::AppHandle<R>, run_id: &str, producer: GraduationLogProducer) -> Self {
        Self {
            app: app.clone(),
            run_id: run_id.to_string(),
            producer,
        }
    }
}

impl<R: tauri::Runtime> crate::tools::agent_exec::AgentActivitySink for GraduationSourceSink<R> {
    fn activity(&self, event: crate::tools::agent_exec::AgentActivityEvent) {
        // The run is reloaded per chunk so the record is attributed to the pass
        // the run stands at now, and so two sinks never hold one record apart.
        let Ok(run) = super::load_run(&self.app, &self.run_id) else {
            return;
        };
        let text = if event.payload.is_empty() {
            event.summary.clone()
        } else {
            event.payload.clone()
        };
        let chunk = source_chunk(&run, self.producer, event.channel, &text);
        append_source_chunks(&self.app, &self.run_id, run.logs.clone(), vec![chunk]);
    }
}

/// Both sinks a graduation turn reports through, as one.
pub struct CompositeSink {
    sinks: Vec<std::sync::Arc<dyn crate::tools::agent_exec::AgentActivitySink>>,
}

impl CompositeSink {
    pub fn of(sinks: Vec<std::sync::Arc<dyn crate::tools::agent_exec::AgentActivitySink>>) -> Self {
        Self { sinks }
    }
}

impl crate::tools::agent_exec::AgentActivitySink for CompositeSink {
    fn activity(&self, event: crate::tools::agent_exec::AgentActivityEvent) {
        for sink in &self.sinks {
            sink.activity(event.clone());
        }
    }
}

/// The store the run's own filesystem access answers for, for a caller that
/// holds neither.
pub fn store_fs_of<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Option<std::sync::Arc<FsAccess>> {
    app.try_state::<crate::fs::FsAccessState>()
        .and_then(|state| state.get())
}
