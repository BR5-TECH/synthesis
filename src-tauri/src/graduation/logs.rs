//! A run's two durable log streams (`GRS-graduation-run-log-storage.md`).
//!
//! Every run owns `logs/activity.jsonl` and `logs/structured.jsonl` under its own
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
    GraduationActivityRecord, GraduationLogLevel, GraduationLogOrigin, GraduationLogProducer,
    GraduationLogRecord, GraduationLogStream, GraduationStructuredRecord,
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
/// A run's structured records come from the loop's own thread and its activity
/// records come from the executor's output path, so **two threads write one
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
    // GRS-FR-EIXS: **either** required stream is mandatory. An activity record
    // the executor's own thread could not store is found here, which is still
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
/// from the loop and an activity record from the executor's output path never take
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

/// GRS-FR-SXNY / GRS-FR-RGPN: append the activity of a turn, from the thread
/// that read it, and answer only once it is durable.
///
/// The run record is written by the loop's thread alone, so a failure here is
/// recorded on the shared indexes. The executor stops the turn on the failure
/// (EAC-FR-DUTR), and the loop rests the run before it performs another agent
/// action (GRS-FR-EIXS).
pub fn append_activity_records<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run_id: &str,
    seed: GraduationLogIndexes,
    activity: Vec<GraduationActivityRecord>,
) -> Result<(), GraduationLogFailure> {
    if activity.is_empty() {
        return Ok(());
    }
    let records = activity.into_iter().map(GraduationLogRecord::Activity).collect();
    match append_shared(app, run_id, seed, GraduationLogStream::Activity, records) {
        // GRS-FR-UCZL: the activity stream announces that it grew exactly as the
        // structured one does. A surface following a working turn reads the
        // activity back under its own cursor on this, and nothing else tells it.
        Ok(indexes) => {
            announce_log_records(
                app,
                run_id,
                GraduationLogStream::Activity,
                indexes.activity.latest_sequence,
            );
            Ok(())
        }
        Err(indexes) => {
            logging::log_error(
                app,
                &crate::logging::BUFFER,
                &[Domain::Backend],
                "graduation could not write a turn's activity to the run's activity stream",
                log_fields! { "run_id" => run_id.to_string() },
            );
            Err(indexes
                .persistence
                .failure
                .unwrap_or_else(|| {
                    GraduationLogFailure::write(
                        index::CODE_STORAGE_UNAVAILABLE,
                        GraduationLogStream::Activity,
                        "The run's log storage could not be reached.",
                        Vec::new(),
                    )
                }))
        }
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
    let before = (run.logs.activity.record_count, run.logs.structured.record_count);
    writer::reconcile(&fs, &paths, &mut run.logs);
    let after = (run.logs.activity.record_count, run.logs.structured.record_count);
    if before != after {
        logging::log_info(
            app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "graduation brought a stopped run's saved log index up to its files",
            log_fields! {
                "run_id" => run.id.clone(),
                "saved_activity_records" => before.0,
                "activity_records" => after.0,
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

/// GRS-FR-JAPO / GRS-FR-WJIA: one safe activity, attributed to the turn that
/// produced it.
///
/// `None` for a kind the stream excludes (GRS-FR-VZUZ). The summary is cut to
/// one line again here, so a record never holds a line break whatever reached
/// the sink.
pub fn activity_record(
    run_id: &str,
    producer: GraduationLogProducer,
    pass: Option<u32>,
    activity: &crate::tools::agent_exec::DurableActivity,
) -> Option<GraduationActivityRecord> {
    if !crate::tools::agent_exec::is_safe_kind(activity.kind) || !producer.writes_activity() {
        return None;
    }
    let pass = pass.filter(|_| producer.carries_pass());
    Some(GraduationActivityRecord {
        attribution: records::GraduationLogAttribution {
            schema_version: 1,
            record_id: activity.record_id.clone(),
            sequence: 0,
            at: activity.at.clone(),
            run_id: run_id.to_string(),
            phase_id: producer.phase_id(None).as_str().to_string(),
            pass,
            origin: producer.activity_origin(pass, activity.channel),
            producer: producer.as_str().to_string(),
        },
        channel: activity.channel.to_string(),
        kind: activity.kind.to_string(),
        summary: crate::tools::agent_exec::descriptor::summary_line(&activity.summary),
    })
}

/// The durable sink that carries a turn's safe activity into `activity.jsonl`.
///
/// It stands beside `AGV-agent-activity.md`'s in-memory sink rather than in
/// place of it: that one is the live panel and is evicted, and this one is the
/// record the author reads a finished run back from (GRS-FR-JQOO).
///
/// GRS-FR-RGPN / GRS-FR-ITWJ: it answers only once the record is durable, and
/// it answers a failed append with the typed failure, which makes the executor
/// stop the turn (EAC-FR-DUTR).
///
/// GRS-FR-YXVY / GRS-FR-JWBW: what arrives has already passed the executor's
/// masking (`../tools/EAC-execute-agent-cli.md` EAC-FR-FKCN). This sink applies
/// no second rule and holds no payload to apply one to.
pub struct GraduationActivitySink<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    run_id: String,
    producer: GraduationLogProducer,
    /// GLG-FR-GZUM: the pass the run stood at when the turn was dispatched.
    pass: u32,
    /// Seeds the shared indexes where this process holds none yet.
    seed: GraduationLogIndexes,
}

impl<R: tauri::Runtime> GraduationActivitySink<R> {
    pub fn new(
        app: &tauri::AppHandle<R>,
        run: &GraduationRun,
        producer: GraduationLogProducer,
    ) -> Self {
        Self {
            app: app.clone(),
            run_id: run.id.clone(),
            producer,
            pass: run.pass(),
            seed: run.logs.clone(),
        }
    }
}

impl<R: tauri::Runtime> crate::tools::agent_exec::DurableOutputSink for GraduationActivitySink<R> {
    fn record(
        &self,
        activity: crate::tools::agent_exec::DurableActivity,
    ) -> Result<(), crate::tools::agent_exec::DurableOutputFailure> {
        let Some(record) = activity_record(&self.run_id, self.producer, Some(self.pass), &activity)
        else {
            // GRS-FR-VZUZ: an excluded kind is refused before it reaches the
            // file. The executor never sends one, so this is the second defence.
            logging::log_warn(
                &self.app,
                &crate::logging::BUFFER,
                &[Domain::Backend],
                "graduation refused an activity of an excluded kind",
                log_fields! {
                    "run_id" => self.run_id.clone(),
                    "kind" => activity.kind,
                },
            );
            return Ok(());
        };
        append_activity_records(&self.app, &self.run_id, self.seed.clone(), vec![record])
            .map_err(|failure| crate::tools::agent_exec::DurableOutputFailure {
                code: failure.code,
                message: failure.message,
            })
    }
}

/// GRS-FR-EIXS / GRS-FR-DDSB: the run record learns of a durable failure the
/// executor reported, so the interruption names it.
///
/// The shared indexes already hold the failure where the sink wrote it. A
/// failure that is not there yet is noted here, so the run never rests on
/// `log_persistence_failed` with no failure to name.
pub fn note_durable_failure<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    failure: &crate::tools::agent_exec::DurableOutputFailure,
) {
    refresh(app, run);
    if run.logs.persistence.is_healthy() {
        run.logs.note_failed(
            GraduationLogFailure::write(
                failure.code.as_str(),
                GraduationLogStream::Activity,
                failure.message.clone(),
                Vec::new(),
            ),
            0,
        );
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
