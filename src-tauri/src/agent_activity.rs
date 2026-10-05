//! Agent activity (`AGV-agent-activity.md`).
//!
//! What an agent CLI did while it was doing it, kept per run and readable while
//! the run is still under way. It exists because a turn that takes a quarter of
//! an hour used to be a quarter of an hour of silence: the executor captured
//! both streams and said nothing about either until the container exited, so a
//! run that was working and a run that was wedged looked identical from outside.
//!
//! ## Why this is not the log buffer
//!
//! `../core/LGC-logging.md`'s buffer is the *application's* account of itself —
//! bounded at 20,000 records for the whole session, cleared when the project
//! changes, and held in memory alone. One graduation turn can emit more events
//! than that on its own, and evicting a session's diagnostics to make room for
//! one agent's narration would cost more than it gives. So the whole stream is
//! kept here, per run, with a file behind it that outlives the session, and the
//! log carries the same events at `DEBUG` for a reader who is searching rather
//! than watching (EAC-FR-32).
//!
//! ## Nothing here masks anything
//!
//! Every record arrives already masked by the executor, which is the only place
//! that holds the credentials to mask against (EAC-FR-29). This module stores
//! what it is given, exactly as `LGC-logging.md` does, and a caller that wrote
//! an unmasked value into it would be the fault.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::Emitter;

use crate::fs::FsAccess;
use crate::tools::agent_exec::{AgentActivityEvent, AgentActivitySink};

/// AGV-FR-11: one or more records were appended to a run's activity, or a run's
/// activity was cleared. Carries no record content, so no consumer can render
/// the stream from the event alone — it re-reads, exactly as the log panel does.
pub const AGENT_ACTIVITY_APPENDED: &str = "agent-activity-appended";

/// AGV-FR-05: the newest records one run keeps in memory.
///
/// Enough that a reader arriving mid-run sees the shape of what has happened,
/// and few enough that a chatty agent cannot grow the process without bound. The
/// file behind it holds every record either way, so this bounds what is *fast*
/// rather than what is *kept*.
pub const MEMORY_EVENTS_PER_RUN: usize = 4096;

/// AGV-FR-05: and the bytes those records may occupy, whichever bound is
/// reached first. A thousand records carrying a large tool result each are worth
/// more memory than a thousand carrying a word.
pub const MEMORY_BYTES_PER_RUN: usize = 8 * 1024 * 1024;

/// AGV-FR-06: how many runs keep records in memory at once.
///
/// A project's queue holds several runs and exactly one of them works
/// (`GRD-graduation.md` GRD-FR-BNTC), so this is well above what is ever live —
/// it bounds the accumulation of finished runs nobody has looked at.
pub const RUNS_IN_MEMORY: usize = 16;

/// AGV-FR-09: how many runs keep their *counters* — far more than keep their
/// records.
///
/// A run whose records were reclaimed keeps the sequence it had reached, so a
/// line arriving for it afterwards continues its numbering instead of starting
/// again at one. Restarting would be the worst kind of failure here: a consumer
/// holding a cursor would be told the run's newest sequence is *below* what it
/// already has, and would then ignore everything that followed for as long as
/// the run lasted. A counter costs a few bytes against the records' megabytes,
/// which is why many more runs can keep one.
pub const RUNS_TRACKED: usize = 256;

/// AGV-FR-07: the largest a run's activity file grows before it stops being
/// appended to.
pub const FILE_BYTES_PER_RUN: usize = 128 * 1024 * 1024;

/// AGV-FR-07: where activity files live, under `app_data_dir()`.
pub const ACTIVITY_DIRNAME: &str = "agent-activity";

/// AGV-FR-10: a page returned no larger than this, whatever was asked for.
pub const MAX_PAGE: usize = 1000;
const DEFAULT_PAGE: usize = 500;

// ---------------------------------------------------------------------------
// The record
// ---------------------------------------------------------------------------

/// One thing an agent did (AGV-FR-02).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityRecord {
    /// Assigned here, increasing within a run and never reused, so a consumer
    /// can ask for everything after what it already holds.
    pub seq: u64,
    /// RFC 3339 UTC, taken when the line arrived rather than when it was stored.
    pub at: String,
    /// `stdout`, `stderr`, or `executor`.
    pub channel: String,
    /// The normalized kind (`EAC-FR-33`).
    pub kind: String,
    /// One line, for a row.
    pub summary: String,
    /// The whole event, verbatim, as the vendor wrote it and masked.
    pub payload: String,
    /// Whether `payload` is a prefix of what arrived.
    pub payload_truncated: bool,
}

impl ActivityRecord {
    /// What this record costs the memory bound: its text, not its overhead.
    fn weight(&self) -> usize {
        self.summary.len() + self.payload.len() + self.at.len() + self.kind.len()
    }
}

/// AGV-FR-11: what a consumer is told when a run's activity changed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityState {
    pub run_id: String,
    /// Every record ever appended for this run, including those no longer in
    /// memory.
    pub total: u64,
    /// Records dropped from memory to stay inside the bounds of AGV-FR-05. They
    /// are still in the file.
    pub dropped: u64,
    /// The newest sequence, or zero for a run with no records.
    pub latest_seq: u64,
}

/// AGV-FR-10: one page of a run's activity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityPage {
    pub run_id: String,
    /// Ascending by `seq`, whichever cursor produced them.
    pub records: Vec<ActivityRecord>,
    pub total: u64,
    pub dropped: u64,
    pub latest_seq: u64,
}

// ---------------------------------------------------------------------------
// The store
// ---------------------------------------------------------------------------

#[derive(Default)]
struct RunActivity {
    /// AGV-FR-12: this run was thrown away, and nothing more is recorded for it.
    ///
    /// A tombstone rather than an absent entry, because discarding a run and the
    /// run stopping are not the same instant: a discard is permitted while a run
    /// is still working (`GRD-graduation.md` GRD-FR-GMTX), cancellation only sets
    /// a flag the loop notices on its next poll, and the container's pipes go on
    /// being drained until the child actually dies. A late line arriving in that
    /// window would otherwise recreate the entry the discard had just removed —
    /// numbering from one again — and put the run back in the eviction order, so
    /// a run the author threw away would start answering reads again.
    forgotten: bool,
    records: VecDeque<ActivityRecord>,
    bytes: usize,
    next_seq: u64,
    total: u64,
    dropped: u64,
    /// How many bytes this run's file holds, so the file bound is enforced
    /// without asking the filesystem before every append.
    file_bytes: usize,
    /// Set once the file bound is reached, so the notice about it is written
    /// once rather than per record.
    file_full: bool,
}

/// AGV-FR-01: the process-wide store, one entry per run.
pub struct ActivityStore {
    /// AGV-FR-07: how large a run's file may grow.
    ///
    /// A field rather than the constant directly, so the bound's own behaviour —
    /// the notice, and the records that follow it — is reachable by a test
    /// without writing a hundred and twenty-eight megabytes to disk.
    file_bound: usize,
    runs: Mutex<HashMap<String, RunActivity>>,
    /// Least-recently-appended first. Both bounds are taken from this: the
    /// oldest beyond `RUNS_IN_MEMORY` lose their records, and the oldest beyond
    /// `RUNS_TRACKED` are forgotten outright.
    order: Mutex<VecDeque<String>>,
}

impl Default for ActivityStore {
    fn default() -> Self {
        ActivityStore {
            file_bound: FILE_BYTES_PER_RUN,
            runs: Mutex::default(),
            order: Mutex::default(),
        }
    }
}

impl ActivityStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// The same store bounding each run's file at `bound` bytes.
    pub fn with_file_bound(bound: usize) -> Self {
        ActivityStore {
            file_bound: bound,
            ..ActivityStore::default()
        }
    }

    /// Append one record and return the run's state afterwards.
    ///
    /// The record's `seq` is assigned here rather than by the caller: a caller
    /// that numbered its own could restart a run and reuse a sequence a consumer
    /// still holds, which is the one thing a cursor cannot survive.
    ///
    /// `None` where the run has been thrown away (AGV-FR-12). Nothing is
    /// recorded, nothing is announced, and the run stays forgotten.
    pub fn append(
        &self,
        run_id: &str,
        event: AgentActivityEvent,
    ) -> Option<(ActivityRecord, ActivityState)> {
        let mut runs = lock(&self.runs);
        let run = runs.entry(run_id.to_string()).or_default();
        if run.forgotten {
            return None;
        }
        run.next_seq += 1;
        run.total += 1;
        let record = ActivityRecord {
            seq: run.next_seq,
            at: event.at,
            channel: event.channel.to_string(),
            kind: event.kind.to_string(),
            summary: event.summary,
            payload: event.payload,
            payload_truncated: event.payload_truncated,
        };
        run.bytes += record.weight();
        run.records.push_back(record.clone());
        // AGV-FR-05: oldest first, and counted — a reader who arrives late must
        // be able to tell a stream that started here from one that was trimmed.
        while run.records.len() > MEMORY_EVENTS_PER_RUN
            || (run.bytes > MEMORY_BYTES_PER_RUN && run.records.len() > 1)
        {
            if let Some(dropped) = run.records.pop_front() {
                run.bytes = run.bytes.saturating_sub(dropped.weight());
                run.dropped += 1;
            }
        }
        let state = ActivityState {
            run_id: run_id.to_string(),
            total: run.total,
            dropped: run.dropped,
            latest_seq: run.next_seq,
        };
        drop(runs);
        self.touch(run_id);
        Some((record, state))
    }

    /// How many runs this store is holding anything for, tombstones included.
    ///
    /// Exists for the bound of AGV-FR-06 to be assertable: a leak here is
    /// invisible from the read surface, which answers the same way for a run
    /// that was evicted and a run that never existed.
    pub fn tracked_runs(&self) -> usize {
        lock(&self.runs).len()
    }

    /// AGV-FR-10: the page a consumer asked for.
    ///
    /// With no cursor, the newest `limit` records — so a panel opening on a run
    /// that has been going for an hour starts at the end rather than paging to
    /// it. With one, everything after it, oldest first, which is the delta a
    /// consumer reads when it has been told the run grew.
    pub fn read(&self, run_id: &str, after: Option<u64>, limit: Option<usize>) -> ActivityPage {
        let limit = limit.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE);
        let runs = lock(&self.runs);
        let Some(run) = runs.get(run_id) else {
            return ActivityPage {
                run_id: run_id.to_string(),
                records: Vec::new(),
                total: 0,
                dropped: 0,
                latest_seq: 0,
            };
        };
        let records: Vec<ActivityRecord> = match after {
            Some(after) => run
                .records
                .iter()
                .filter(|record| record.seq > after)
                .take(limit)
                .cloned()
                .collect(),
            None => run
                .records
                .iter()
                .skip(run.records.len().saturating_sub(limit))
                .cloned()
                .collect(),
        };
        ActivityPage {
            run_id: run_id.to_string(),
            records,
            total: run.total,
            dropped: run.dropped,
            latest_seq: run.next_seq,
        }
    }

    /// AGV-FR-12: everything this run held, forgotten.
    ///
    /// Called when the work the activity belongs to is discarded, so a run the
    /// author threw away does not keep its narration around. What is left is a
    /// tombstone rather than nothing, so a line still in flight from the run's
    /// own container cannot bring it back — see [`RunActivity::forgotten`].
    ///
    /// The tombstone stays in the eviction order and is reclaimed by the run
    /// bound of AGV-FR-06 like any other entry, so forgetting runs costs a
    /// bounded amount of memory rather than a growing one.
    pub fn forget(&self, run_id: &str) {
        {
            let mut runs = lock(&self.runs);
            let run = runs.entry(run_id.to_string()).or_default();
            *run = RunActivity {
                forgotten: true,
                ..RunActivity::default()
            };
        }
        // At the *oldest* end of the eviction order, so a tombstone is the first
        // thing reclaimed rather than the last. Discarding runs would otherwise
        // accumulate one empty entry each for as long as the process runs, which
        // is a small leak but still a leak — and a run discarded before it
        // recorded anything was never in this order at all.
        //
        // The `runs` lock is released above rather than held across this: every
        // other path takes `order` before `runs`, and holding them the other way
        // round here is how the two would come to deadlock.
        let mut order = lock(&self.order);
        order.retain(|id| id != run_id);
        order.push_front(run_id.to_string());
        self.evict_beyond_bounds(&mut order);
    }

    /// Move a run to the newest end of the eviction order, dropping the memory
    /// of whatever fell off the other end.
    fn touch(&self, run_id: &str) {
        let mut order = lock(&self.order);
        order.retain(|id| id != run_id);
        order.push_back(run_id.to_string());
        self.evict_beyond_bounds(&mut order);
    }

    /// AGV-FR-06 and AGV-FR-09's two bounds, taken from the oldest end.
    ///
    /// The first reclaims records; the second forgets the run outright. They are
    /// separate because what makes a run expensive to remember is its records,
    /// and what makes it dangerous to forget is its sequence.
    fn evict_beyond_bounds(&self, order: &mut VecDeque<String>) {
        if order.len() > RUNS_IN_MEMORY {
            let stale: Vec<String> = order
                .iter()
                .take(order.len() - RUNS_IN_MEMORY)
                .cloned()
                .collect();
            let mut runs = lock(&self.runs);
            for id in stale {
                if let Some(run) = runs.get_mut(&id) {
                    // The file stays, and so does the count of what this run has
                    // said. What is dropped is the fast copy of a run nobody has
                    // looked at for longer than fifteen others.
                    run.records.clear();
                    run.bytes = 0;
                }
            }
        }
        while order.len() > RUNS_TRACKED {
            if let Some(forgotten) = order.pop_front() {
                lock(&self.runs).remove(&forgotten);
            }
        }
    }

    /// The file bound of AGV-FR-07, tracked without asking the filesystem.
    ///
    /// Returns whether this record may still be written, and whether this is the
    /// call that filled the file.
    fn admit_to_file(&self, run_id: &str, bytes: usize) -> FileAdmission {
        let mut runs = lock(&self.runs);
        // Never creates an entry: this is only ever called for a record `append`
        // just accepted, so the entry is there. Creating one here would put a
        // run in the map that the eviction order knows nothing about, which is a
        // run that can never be reclaimed and whose file accounting has silently
        // started again from zero.
        let Some(run) = runs.get_mut(run_id) else {
            return FileAdmission::Refused;
        };
        // A discarded run stops being written to for the same reason it stops
        // being recorded: the author threw it away while a line was still in
        // flight from its container.
        if run.file_full || run.forgotten {
            return FileAdmission::Refused;
        }
        if run.file_bytes + bytes > self.file_bound {
            run.file_full = true;
            return FileAdmission::Filled;
        }
        run.file_bytes += bytes;
        FileAdmission::Allowed
    }
}

enum FileAdmission {
    Allowed,
    /// This is the write that reached the bound.
    Filled,
    Refused,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    // A poisoned lock means some other caller panicked while holding it. The
    // records are plain data with no invariant a panic could have broken, and
    // refusing to record anything ever again because one call panicked is worse
    // than carrying on.
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

// ---------------------------------------------------------------------------
// The sink the executor writes into
// ---------------------------------------------------------------------------

/// AGV-FR-03: one run's sink — the store, the file, and the event, in that
/// order.
///
/// Bound to a run id at construction, which is what makes it impossible for a
/// caller to attribute one run's activity to another: the executor is handed a
/// sink and never a run identifier.
pub struct RunActivitySink<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    run_id: String,
    store: std::sync::Arc<ActivityStore>,
    /// Where the file goes. `None` where `app_data_dir()` could not be resolved,
    /// which costs the file and nothing else — a run still streams and is still
    /// read.
    file: Option<PathBuf>,
    fs: Option<std::sync::Arc<FsAccess>>,
}

impl<R: tauri::Runtime> RunActivitySink<R> {
    pub fn new(
        app: &tauri::AppHandle<R>,
        run_id: &str,
        store: std::sync::Arc<ActivityStore>,
        fs: Option<std::sync::Arc<FsAccess>>,
    ) -> Self {
        let root = crate::fs::app_data_dir().ok().map(|dir| dir.join(ACTIVITY_DIRNAME));
        Self::rooted_at(app, run_id, store, fs, root)
    }

    /// The same sink writing its file under `root`.
    ///
    /// The application always resolves that under `app_data_dir()`. A test names
    /// its own directory instead, so a suite never writes into the author's real
    /// application data — the same arrangement the executor's session state has.
    pub fn rooted_at(
        app: &tauri::AppHandle<R>,
        run_id: &str,
        store: std::sync::Arc<ActivityStore>,
        fs: Option<std::sync::Arc<FsAccess>>,
        root: Option<PathBuf>,
    ) -> Self {
        let file = root.and_then(|root| activity_file(run_id, &root, fs.as_deref()));
        RunActivitySink {
            app: app.clone(),
            run_id: run_id.to_string(),
            store,
            file,
            fs,
        }
    }
}

impl<R: tauri::Runtime> AgentActivitySink for RunActivitySink<R> {
    fn activity(&self, event: AgentActivityEvent) {
        let Some((record, state)) = self.store.append(&self.run_id, event) else {
            // AGV-FR-12: the run was thrown away while this line was in flight.
            return;
        };
        self.write_to_file(&record);
        // AGV-FR-11: state, never content. A consumer re-reads under whatever
        // cursor it holds, so nothing here has to know what anybody is showing.
        let _ = self.app.emit(AGENT_ACTIVITY_APPENDED, &state);
    }
}

impl<R: tauri::Runtime> RunActivitySink<R> {
    /// AGV-FR-07: one JSON document per line, appended.
    ///
    /// A failure to write is not reported to the caller and never interrupts the
    /// run: the file is the durable copy of something already held in memory and
    /// already emitted, and a full disk is not a reason to stop an agent.
    fn write_to_file(&self, record: &ActivityRecord) {
        let (Some(path), Some(fs)) = (self.file.as_ref(), self.fs.as_ref()) else {
            return;
        };
        let Ok(line) = serde_json::to_string(record) else {
            return;
        };
        match self.store.admit_to_file(&self.run_id, line.len() + 1) {
            FileAdmission::Allowed => {
                let _ = fs.append_lines(path, &[line]);
            }
            FileAdmission::Filled => {
                // Said once, in the file, so a reader of the file alone knows
                // it stops short rather than that the run stopped.
                let notice = serde_json::json!({
                    "seq": record.seq,
                    "at": record.at,
                    "channel": "executor",
                    "kind": "error",
                    "summary": "this activity file reached its byte bound; later \
                                records are in memory and in the log only",
                    "payload": "",
                    "payloadTruncated": false,
                });
                let _ = fs.append_lines(path, &[notice.to_string()]);
            }
            FileAdmission::Refused => {}
        }
    }
}

/// Where one run's activity file lives, and `None` where there is nowhere to put
/// it.
fn activity_file(run_id: &str, dir: &std::path::Path, fs: Option<&FsAccess>) -> Option<PathBuf> {
    let fs = fs?;
    match fs.create_dir(dir) {
        Ok(()) => {}
        Err(crate::fs::FsError::AlreadyExists { .. }) => {}
        Err(_) => return None,
    }
    // The run id is generated by this application and is already safe as a file
    // name; the filter is what keeps that from being an assumption.
    let safe: String = run_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if safe.is_empty() {
        return None;
    }
    Some(dir.join(format!("{safe}.jsonl")))
}

// ---------------------------------------------------------------------------
// The command surface (AGV contract surface)
// ---------------------------------------------------------------------------

/// `"read agent activity (run id, after, limit)"`.
#[tauri::command]
pub fn read_agent_activity(
    run_id: String,
    after: Option<u64>,
    limit: Option<usize>,
    store: tauri::State<'_, std::sync::Arc<ActivityStore>>,
) -> ActivityPage {
    store.read(&run_id, after, limit)
}

#[cfg(test)]
mod tests;
