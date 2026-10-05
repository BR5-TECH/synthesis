//! Draft statistics storage
//! (`../../specifications/core/DSS-draft-statistics-storage.md`).
//!
//! The durable per-machine record of what a draft cost to make. One append-only
//! event log per draft, one JSON object per line, at
//! `statistics/<draft-id>.jsonl` inside the repository machine store
//! (DSS-FR-KQVN, `RMS-repository-machine-storage.md` RMS-FR-ZXHM). The store
//! stands outside every worktree, so a log modifies no checkout and reaches Git
//! by no route (DSS-FR-MVTK).
//!
//! Duplicate-safe by construction (DSS-FR-PNUE): every event carries a stable
//! identity, and the fold applies the first event it sees for an identity and
//! ignores every later one. A relaunch, a replay, a second application process
//! (RMS-FR-PFOB), and the import pass of an earlier build's committed log
//! (DSS-FR-RBQH) therefore all fold to the totals one write folds to.
//!
//! Statistics describe **captured** activity alone (DSS-FR-VCTQ, DSS-FR-GAWO): a
//! draft's telemetry begins at the moment its first event is recorded, that
//! instant is persisted as the draft's activation boundary, and nothing before
//! it is read, reconstructed, or inferred from the comment logs, the draft
//! history, the graduation records, the run logs, or any session state. This
//! module reads its own log and nothing else.
//!
//! Recording is **asynchronous and never blocking** (DSS-FR-TUMX). A caller
//! hands an event over and returns; the append happens on this module's own
//! writer thread, and an append that cannot be performed is logged at `WARN` and
//! dropped rather than handed back to the operation that produced it. No user
//! action and no agent run is ever delayed, refused, or reported differently
//! because a statistic could not be written.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, OnceLock};

use serde::Serialize;
use tauri::{Emitter, Manager, State};

use crate::fs as fsa;
use crate::log_fields;
use crate::logging::{self, Domain, LogSink, BUFFER};
use crate::notes::{new_note_id, now_rfc3339_millis};
use crate::project::ProjectState;

pub mod fold;
pub mod model;
pub mod time;

pub use model::*;

#[cfg(test)]
mod tests;

/// DSS-FR-KQVN: the folder the per-draft logs live in, relative to the
/// repository machine store.
///
/// The store is keyed by the repository rather than by the worktree
/// (`RMS-repository-machine-storage.md` RMS-FR-QJVT), so a draft's account reads
/// the same from every worktree of one repository (DSS-FR-PWXG).
pub const STATISTICS_REL: &str = crate::repository_store::STATISTICS_DIRNAME;

/// DSS-FR-TWNA: events for a draft have been appended.
///
/// Kebab-case for the reason every event in this application is: Tauri rejects
/// the rest of the character set at `emit`, leaving a channel that is silently
/// dead in production. Matches `src/events.ts`'s constant byte-for-byte.
pub const DRAFT_STATISTICS_CHANGED: &str = "draft-statistics-changed";

/// DSS-FR-QLXV: no project is open.
pub const ERR_NO_PROJECT_OPEN: &str = "no_project_open";
/// DSS-FR-QLXV: a `draft_id` naming no draft in the active worktree.
pub const ERR_DRAFT_NOT_FOUND: &str = "draft_not_found";
/// DSS-FR-RIDW: an interval whose `ended_at` precedes its `started_at`.
pub const ERR_INVALID_INTERVAL: &str = "invalid_interval";
/// DSS-FR-QLXV: the statistics folder itself could not be read, which is
/// distinct from a log that is absent.
pub const ERR_READ_FAILED: &str = "read_failed";

/// DSS-FR-TWNA: what the change event carries — the draft's id, and no totals.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftStatisticsChanged {
    pub draft_id: String,
}

// ---------------------------------------------------------------------------
// Paths (DSS-FR-KQVN, DSS-FR-WGUP)
// ---------------------------------------------------------------------------

/// DSS-FR-KQVN: the `statistics/` folder inside the repository machine store.
pub fn statistics_dir(root: &Path) -> PathBuf {
    root.join(STATISTICS_REL)
}

/// The log path for a draft, behind the FSA-FR-10 escape gate.
///
/// The id is checked against `DRS-draft-storage.md`'s own vocabulary before it
/// reaches a path at all, so a `draft_id` crafted to resolve outside the folder
/// is refused having written and read nothing (DSS-FR-WGUP). The gate stays in
/// the way regardless, because a primitive that is only safe by virtue of its
/// caller is one refactor from not being.
pub fn log_path(root: &fsa::RootFs, draft_id: &str) -> Result<PathBuf, String> {
    if !crate::drafts::is_valid_draft_id(draft_id) {
        return Err(fsa::FsError::PathEscape {
            root: root.path().to_path_buf(),
            rel: PathBuf::from(draft_id),
        }
        .to_string());
    }
    fsa::resolve_under(root, format!("{STATISTICS_REL}/{draft_id}.jsonl"))
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Appending (DSS-FR-WRPD, DSS-FR-VCTQ, DSS-FR-EKZI)
// ---------------------------------------------------------------------------

/// One event on its way to a log: the fact, and the instant it was captured.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pending {
    pub at: String,
    pub body: EventBody,
}

impl Pending {
    /// DSS-FR-MHJC: an event whose capture instant is its own — an interval's
    /// `started_at` — takes it, and every other event is captured now.
    pub fn now(body: EventBody) -> Pending {
        let at = body
            .capture_instant()
            .map(str::to_string)
            .unwrap_or_else(now_rfc3339_millis);
        Pending { at, body }
    }

    /// [`Pending::now`] with the capture instant supplied, for a producer that
    /// knows when the fact happened rather than when it got round to recording
    /// it (DSS-FR-MHJC).
    pub fn at(at: impl Into<String>, body: EventBody) -> Pending {
        let at = body
            .capture_instant()
            .map(str::to_string)
            .unwrap_or_else(|| at.into());
        Pending { at, body }
    }
}

/// DRS-FR-ZLBK: whether the draft an event names is still in the worktree.
///
/// The one thing that stands between an asynchronous append and a log for a
/// draft nobody kept. A draft is found by the walk rather than by composing its
/// path, because a draft's folder is where it is filed rather than where its id
/// would put it (per `DRS-draft-storage.md` DRS-FR-36).
fn draft_still_exists(worktree: &fsa::RootFs, draft_id: &str) -> bool {
    crate::drafts::draft_dir(worktree, draft_id).is_ok()
}

/// [`append_events`] behind the deleted-draft guard the writer applies.
///
/// The seam a test drives a late event through: production reaches it from the
/// writer's own task, and nothing else may append without the guard, because an
/// append that recreates a deleted draft's log is one nothing cleans up.
pub fn append_for_existing_draft(
    root: &fsa::RootFs,
    worktree: &fsa::RootFs,
    draft_id: &str,
    pending: &[Pending],
) {
    if !draft_still_exists(worktree, draft_id) {
        return;
    }
    let _ = append_events(root, draft_id, pending);
}

/// DSS-FR-EKZI: the synchronous half of the one recording path.
///
/// Stamps `event_id`, `v`, and `draft_id` on each event, creates the draft's
/// activation boundary where the log does not exist yet, and appends the whole
/// batch as a single `append_lines` call — which is what makes DSS-FR-VCTQ's
/// "before or in the same append as that event" true by construction rather than
/// by ordering two writes.
pub fn append_events(
    root: &fsa::RootFs,
    draft_id: &str,
    pending: &[Pending],
) -> Result<(), String> {
    if pending.is_empty() {
        return Ok(());
    }
    let path = log_path(root, draft_id)?;

    let mut lines: Vec<String> = Vec::with_capacity(pending.len() + 1);
    // DSS-FR-VCTQ: the boundary is created when the **first** event for the
    // draft is about to be recorded, and by no other path. A log that is already
    // there already has one; a race between two first attempts writes two, and
    // the fold converges them on the earlier (DSS-FR-JRSY).
    if root.file_info(&path).is_err() {
        let boundary_at = pending
            .iter()
            .map(|event| event.at.as_str())
            .min()
            .unwrap_or_default()
            .to_string();
        lines.push(line_of(draft_id, &boundary_at, &EventBody::TelemetryActivated)?);
    }
    for event in pending {
        lines.push(line_of(draft_id, &event.at, &event.body)?);
    }
    root.append_lines(&path, &lines).map_err(|e| e.to_string())
}

/// DSS-FR-EKZI: the module stamps the envelope itself, so no caller can forge an
/// identity or attribute an event to another draft.
fn line_of(draft_id: &str, at: &str, body: &EventBody) -> Result<String, String> {
    let event = StatisticsEvent {
        v: SCHEMA_VERSION,
        event_id: new_note_id(),
        draft_id: draft_id.to_string(),
        at: at.to_string(),
        body: body.clone(),
    };
    serde_json::to_string(&event).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Reading (DSS-FR-YOVS, DSS-FR-GAWO)
// ---------------------------------------------------------------------------

/// DSS-FR-YOVS: fold a draft's log into its totals.
///
/// A draft with no log answers with `boundary_at` null and every statistic
/// unavailable, which is an answer rather than an error — and reading writes
/// nothing anywhere and creates no boundary (DSS-FR-JRSY).
pub fn read_statistics_impl(root: &fsa::RootFs, draft_id: &str) -> Result<DraftStatistics, String> {
    let path = log_path(root, draft_id)?;
    let text = match root.read_text(&path) {
        Ok(text) => text,
        Err(fsa::FsError::NotFound { .. }) => String::new(),
        // DSS-FR-QLXV: a folder that cannot be read at all is `read_failed`,
        // which is a different thing from a log that is simply absent.
        Err(_) => return Err(ERR_READ_FAILED.to_string()),
    };
    Ok(fold::fold_log(draft_id, &text))
}

// ---------------------------------------------------------------------------
// Deletion (DSS-FR-VPBS)
// ---------------------------------------------------------------------------

/// DSS-FR-VPBS: remove `statistics/<draft-id>.jsonl` in the repository machine
/// store, and nothing else.
///
/// Scoped to that one stable draft id, idempotent against a log that is already
/// absent, and retryable — a failure leaves the log exactly as it was and
/// returns the typed error the caller repeats.
pub fn delete_draft_statistics(root: &fsa::RootFs, draft_id: &str) -> Result<(), String> {
    // Resolved first, so a crafted id is refused as the path escape it is
    // having deleted nothing (DSS-FR-WGUP).
    log_path(root, draft_id)?;
    match root.delete_under(statistics_dir(root.path()), format!("{draft_id}.jsonl"), false) {
        Ok(()) => Ok(()),
        // DSS-FR-VPBS: a log that is already absent is an already completed
        // deletion rather than an error.
        Err(fsa::FsError::NotFound { .. }) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

// ---------------------------------------------------------------------------
// The writer (DSS-FR-TUMX)
// ---------------------------------------------------------------------------

/// The bound past which the writer drops its oldest pending work.
///
/// A statistic is worth less than the operation it describes, so the queue never
/// delays a caller and never grows without limit: at the bound the oldest batch
/// is dropped with a `WARN` rather than the newest being refused, because the
/// events still coming are the ones a reader is waiting on.
const QUEUE_BOUND: usize = 1024;

type Task = Box<dyn FnOnce() + Send + 'static>;

/// This module's own writer: one thread, one bounded queue.
///
/// Process-global rather than Tauri-managed state because it holds nothing of a
/// project — every task carries its own root and its own handle — and because a
/// recording path that could fail for want of managed state would be one more
/// way for a statistic to take an operation down with it.
struct Writer {
    queue: Mutex<WriterQueue>,
    idle: Condvar,
}

#[derive(Default)]
struct WriterQueue {
    tasks: VecDeque<Task>,
    /// Whether a task is being run right now, so a waiter can tell a queue that
    /// is empty from one that is merely between tasks.
    running: bool,
    /// How many batches the bound dropped, for the `WARN` that names them.
    dropped: u64,
}

static WRITER: OnceLock<Writer> = OnceLock::new();

fn writer() -> &'static Writer {
    WRITER.get_or_init(|| Writer {
        queue: Mutex::new(WriterQueue::default()),
        idle: Condvar::new(),
    })
}

/// Start the writer thread once, the first time anything is queued.
fn ensure_thread() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        std::thread::Builder::new()
            .name("draft-statistics-writer".to_string())
            .spawn(|| loop {
                let writer = writer();
                let task = {
                    let mut queue = writer.queue.lock().unwrap_or_else(|e| e.into_inner());
                    loop {
                        if let Some(task) = queue.tasks.pop_front() {
                            queue.running = true;
                            break Some(task);
                        }
                        queue.running = false;
                        writer.idle.notify_all();
                        queue = writer
                            .idle
                            .wait(queue)
                            .unwrap_or_else(|e| e.into_inner());
                    }
                };
                if let Some(task) = task {
                    // A panic in one batch must not take the writer with it:
                    // every later event would then be dropped silently for the
                    // rest of the session.
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(task));
                }
            })
            .ok();
    });
}

/// Hand a batch to the writer and return. Reports how many batches the bound has
/// dropped so far, so the caller can name them in a `WARN`.
fn enqueue(task: Task) -> u64 {
    ensure_thread();
    let writer = writer();
    let mut queue = writer.queue.lock().unwrap_or_else(|e| e.into_inner());
    while queue.tasks.len() >= QUEUE_BOUND {
        queue.tasks.pop_front();
        queue.dropped += 1;
    }
    queue.tasks.push_back(task);
    let dropped = queue.dropped;
    writer.idle.notify_all();
    dropped
}

/// Wait until the writer has nothing left to do.
///
/// For a test that queued work through the asynchronous path and then wants to
/// read what it produced. Production never waits for the writer — that is the
/// whole of DSS-FR-TUMX.
pub fn wait_for_writer() {
    ensure_thread();
    let writer = writer();
    let mut queue = writer.queue.lock().unwrap_or_else(|e| e.into_inner());
    while !queue.tasks.is_empty() || queue.running {
        queue = writer.idle.wait(queue).unwrap_or_else(|e| e.into_inner());
    }
}

/// DSS-FR-EKZI / DSS-FR-TUMX: the one path by which an event is recorded.
///
/// Accepts the events, hands them to this module's writer, and returns. The
/// operation that produced them neither waits for the append nor fails with it:
/// an append that cannot be performed is recorded at `WARN` and dropped, and no
/// caller anywhere is given a statistics failure to handle.
pub fn record_statistics_events<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    draft_id: &str,
    pending: Vec<Pending>,
) where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    if pending.is_empty() {
        return;
    }
    // DSS-FR-KQVN: the log is in the repository machine store; the draft whose
    // existence the writer checks is in the active worktree.
    let roots = app.try_state::<ProjectState>().and_then(|state| {
        state
            .require_store()
            .inspect_err(|reason| {
                // DSS-FR-TUMX / DSS-FR-ZMHT: an append that cannot be performed
                // — the store unresolvable among them — is named at `WARN` and
                // dropped, and handed to nobody. The reason and the shape of
                // what was lost; no event field and no draft content.
                logging::log_warn(
                    app,
                    &BUFFER,
                    &[Domain::Backend],
                    "draft statistics append dropped: no store to write into",
                    log_fields! { "events" => pending.len(), "reason" => reason.clone() },
                );
            })
            .ok()
            .zip(state.require_root().ok())
    });
    let Some((root, worktree)) = roots else {
        // No project open is not an error a caller handles: there is simply
        // nowhere for the event to go (DSS-FR-PWXG).
        return;
    };
    let sink = app.clone();
    let draft_id = draft_id.to_string();
    let queued_for = draft_id.clone();
    let dropped = enqueue(Box::new(move || {
        let types = pending
            .iter()
            .map(|event| event.body.type_name())
            .collect::<Vec<_>>()
            .join(",");
        // DRS-FR-ZLBK: a draft that has been deleted takes its log with it, and
        // an event queued before that deletion must not bring the log back.
        // Checked **here**, on the writer, rather than where the event was
        // handed over: the whole point of DSS-FR-TUMX is that the two are far
        // apart in time, and a graduation operation or a conversational turn
        // that settles while its draft is being deleted is exactly the case
        // that would otherwise recreate the file — with a fresh boundary, for a
        // draft nothing will ever read again.
        if !draft_still_exists(&worktree, &draft_id) {
            logging::log_debug(
                &sink,
                &BUFFER,
                &[Domain::Backend],
                "draft statistics events dropped for a draft that is gone",
                log_fields! {
                    "draftId" => &draft_id,
                    "events" => pending.len(),
                    "types" => &types,
                },
            );
            return;
        }
        match append_events(&root, &draft_id, &pending) {
            Ok(()) => {
                // DSS-FR-ZMHT: a `DEBUG` record when events are appended, naming
                // the draft and the event types and counts — and never a value
                // DSS-FR-XRDM keeps out of an event.
                logging::log_debug(
                    &sink,
                    &BUFFER,
                    &[Domain::Backend],
                    "draft statistics events appended",
                    log_fields! {
                        "draftId" => &draft_id,
                        "events" => pending.len(),
                        "types" => &types,
                    },
                );
                // DSS-FR-TWNA: emitted after the append has landed and never
                // before, carrying the draft's id and no totals.
                let _ = sink.emit(
                    DRAFT_STATISTICS_CHANGED,
                    DraftStatisticsChanged {
                        draft_id: draft_id.clone(),
                    },
                );
            }
            Err(reason) => {
                // DSS-FR-TUMX / DSS-FR-ZMHT: dropped, named at `WARN`, and
                // handed to nobody. Nothing is emitted for an append that
                // landed nowhere.
                logging::log_warn(
                    &sink,
                    &BUFFER,
                    &[Domain::Backend],
                    "draft statistics append dropped",
                    log_fields! {
                        "draftId" => &draft_id,
                        "events" => pending.len(),
                        "types" => &types,
                        "reason" => reason,
                    },
                );
            }
        }
    }));
    if dropped > 0 {
        logging::log_warn(
            app,
            &BUFFER,
            &[Domain::Backend],
            "draft statistics writer queue is at its bound",
            log_fields! { "draftId" => &queued_for, "droppedBatches" => dropped },
        );
    }
}

/// [`record_statistics_events`] for one event.
pub fn record_statistics_event<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    draft_id: &str,
    body: EventBody,
) where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    record_statistics_events(app, draft_id, vec![Pending::now(body)]);
}

/// The `token_usage` events one operation's reliable provider-reported usage
/// records become (DSS-FR-SVJU).
///
/// Composed here rather than at each producer so the scope, the bucket, and the
/// owner are stamped in one place and two producers cannot disagree about what a
/// usage record of a bucket looks like.
pub fn usage_events(
    at: &str,
    scope: UsageScope,
    bucket: Bucket,
    owner_id: &str,
    records: &[UsageRecord],
) -> Vec<Pending> {
    records
        .iter()
        // A record the provider reported nothing in is not a reliable usage
        // record: appending it would make a family `incomplete` on the strength
        // of a line that says nothing (DSS-FR-SVJU).
        .filter(|record| !record.tokens.is_empty())
        .map(|record| {
            Pending::at(
                at,
                EventBody::TokenUsage {
                    usage_id: record.usage_id.clone(),
                    scope,
                    bucket,
                    owner_id: owner_id.to_string(),
                    representation: record.representation,
                    round: record.round,
                    attempt: record.attempt,
                    tokens: record.tokens,
                },
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// DSS-FR-QLXV: the open project's root, or the typed refusal.
fn require_root(project: &State<'_, ProjectState>) -> Result<fsa::RootFs, String> {
    project
        .require_root()
        .map_err(|_| ERR_NO_PROJECT_OPEN.to_string())
}

/// DSS-FR-QLXV: a draft the active worktree does not hold is `draft_not_found`.
fn require_draft(root: &fsa::RootFs, draft_id: &str) -> Result<(), String> {
    if !crate::drafts::is_valid_draft_id(draft_id) {
        // A crafted id is refused as the path escape it is, having read nothing
        // (DSS-FR-WGUP).
        log_path(root, draft_id)?;
    }
    crate::drafts::draft_dir(root, draft_id)
        .map(|_| ())
        .map_err(|_| ERR_DRAFT_NOT_FOUND.to_string())
}

/// DSS-FR-YOVS: the draft's captured lifetime aggregate totals.
#[tauri::command]
pub fn read_draft_statistics(
    draft_id: String,
    project: State<'_, ProjectState>,
) -> Result<DraftStatistics, String> {
    let root = require_root(&project)?;
    require_draft(&root, &draft_id)?;
    // DSS-FR-PWXG: the totals are folded from the store, and the draft they
    // belong to is checked against the active worktree.
    let store = project.require_store()?;
    read_statistics_impl(&store, &draft_id)
}

/// DSS-FR-RIDW: record one settled foreground editing interval.
///
/// The one recording command the frontend reaches, and it composes the event
/// itself — no frontend call can forge an event, attribute one to another draft,
/// or write a line of a log directly (DSS-FR-EKZI).
#[tauri::command]
pub fn record_draft_editing_interval(
    draft_id: String,
    started_at: String,
    ended_at: String,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<(), String> {
    let root = require_root(&project)?;
    require_draft(&root, &draft_id)?;
    let (Some(started), Some(ended)) = (
        time::parse_instant_ms(&started_at),
        time::parse_instant_ms(&ended_at),
    ) else {
        return Err(ERR_INVALID_INTERVAL.to_string());
    };
    if ended < started {
        return Err(ERR_INVALID_INTERVAL.to_string());
    }
    record_statistics_event(
        &app,
        &draft_id,
        EventBody::EditingInterval {
            interval_id: new_note_id(),
            started_at,
            ended_at,
        },
    );
    Ok(())
}
