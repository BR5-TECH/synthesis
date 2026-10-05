//! Session logging (`specifications/core/LGC-logging.md`).
//!
//! The application-wide diagnostic channel every other module emits through.
//! A record is structured rather than a formatted string — a level, one or more
//! domains, a message, and a flat bag of fields — so `../ui/LOG-logs.md` can
//! filter and search a session's records on terms the emitter never had to
//! anticipate (LGC-FR-03 / LGC-FR-04).
//!
//! The buffer lives in memory for the duration of a session and is discarded
//! rather than archived (LGC-FR-02). Nothing here writes a log file of its own;
//! the only record that reaches disk is one the user explicitly exports
//! (LGC-FR-18).
//!
//! Two seams make the whole thing testable without a Tauri runtime, mirroring
//! `progress.rs`:
//!
//! - [`LogBuffer`] holds the ring and decides *what* to emit, including the
//!   coalescing of LGC-FR-17. Its methods return the state due rather than
//!   emitting it, and every one that depends on the clock takes `now`.
//! - [`LogSink`] is the emitter. `AppHandle` implements it in production; the
//!   tests use a `Vec`-collecting one.
//!
//! **This module redacts nothing** (LGC-FR-16). It stores what it is given
//! verbatim — it inspects no message, no key, and no value. Keeping a secret out
//! of a record is the obligation of the emitter, which every module holding
//! credential material already carries (`github_tokens.rs` GTS-FR-01,
//! `ai_api.rs` AAP-FR-07, `agentic.rs` AIC-FR-20, `agent_conversations.rs`
//! AGC-FR-26).

use std::collections::{BTreeMap, VecDeque};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::Emitter;

use crate::fs;

/// The event every append and every clear is published on (LGC contract
/// surface).
///
/// Kebab-case rather than the spec's abstract `"log records appended"`: Tauri
/// validates event names and accepts only alphanumerics, `-`, `/`, `:` and `_`,
/// so a name with spaces is rejected by `emit` and the channel is silently dead.
/// Matches the constant in `src/events.ts` byte-for-byte, and pinned by
/// `every_event_name_is_one_tauri_will_actually_deliver` in `lib.rs`.
pub const LOG_RECORDS_APPENDED: &str = "log-records-appended";

/// LGC-FR-07: the ring holds at most this many records; appending to a full
/// buffer evicts the oldest first.
pub const BUFFER_CAPACITY: usize = 20_000;

/// LGC-FR-08: a record whose serialised size exceeds this is appended with its
/// `fields` replaced by a marker rather than being rejected, so one oversized
/// record costs one slot rather than the buffer.
pub const MAX_RECORD_BYTES: usize = 16 * 1024;

/// The field name an oversized record's `fields` collapse to (LGC-FR-08).
pub const OMITTED_FIELD: &str = "fieldsOmittedBytes";

/// LGC-FR-17: appends within this window collapse to one event carrying the
/// latest state. An implementation choice, not a contract.
pub const COALESCE_WINDOW: Duration = Duration::from_millis(50);

/// LGC-FR-11: the typed error a query whose regex does not compile returns.
pub const INVALID_QUERY: &str = "invalid query";

/// LGC-FR-19: the typed error an export that cannot be written returns.
pub const EXPORT_FAILED: &str = "export failed";

// ---------------------------------------------------------------------------
// Wire shapes (LGC "Payload shapes")
// ---------------------------------------------------------------------------

/// LGC-FR-09: `min_level` is a floor, ordered `Debug < Info < Warn < Error`.
/// The derived `Ord` is what implements that ordering, so the variant order
/// below is load-bearing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl Default for LogLevel {
    /// The panel's default floor hides nothing (`../ui/LOG-logs.md` LOG-FR-07).
    fn default() -> Self {
        LogLevel::Debug
    }
}

/// LGC-FR-03: what a record is *about*, rather than which process emitted it.
/// A record carries a non-empty set of these.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Domain {
    Frontend,
    Ai,
    Backend,
    Remote,
}

/// A flat map of JSON values (LGC-FR-04). `BTreeMap` rather than a `HashMap` so
/// a record serialises with its fields in a stable order — an exported JSONL
/// file diffs cleanly, and a test can assert on the whole line.
pub type Fields = BTreeMap<String, serde_json::Value>;

/// One diagnostic record as it sits in the buffer and travels on the wire.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogRecord {
    /// LGC-FR-05: assigned here, unique for the life of the running
    /// application, never reused, and never reset by a clear.
    pub sequence: u64,
    /// ISO-8601 UTC, millisecond precision. Supplied by the caller (LGC-FR-06).
    pub ts: String,
    pub level: LogLevel,
    /// Non-empty (LGC-FR-03).
    pub domains: Vec<Domain>,
    pub message: String,
    pub fields: Fields,
}

/// What a caller supplies. `sequence` is stamped by this module (LGC-FR-05).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogInput {
    pub ts: String,
    pub level: LogLevel,
    pub domains: Vec<Domain>,
    pub message: String,
    #[serde(default)]
    pub fields: Fields,
}

/// LGC-FR-09 / LGC-FR-10. Supplied per call and retained nowhere (LGC-FR-13).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFilter {
    /// Records below this level do not match.
    #[serde(default)]
    pub min_level: LogLevel,
    /// A record matches when it carries at least one; empty matches every
    /// record.
    #[serde(default)]
    pub domains: Vec<Domain>,
    /// Absent or empty matches every record.
    #[serde(default)]
    pub query: Option<String>,
    /// `false`: case-insensitive substring. `true`: regular expression.
    #[serde(default)]
    pub query_is_regex: bool,
}

/// LGC-FR-22: where a page sits within the match set. Absent means the newest
/// `limit` matching records.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Cursor {
    /// Matching records newer than this sequence, oldest-first.
    After(u64),
    /// The `limit` matching records nearest below this sequence.
    Before(u64),
}

/// One page of the match set, plus what the panel needs to describe the buffer
/// it came from.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogPage {
    /// Always ascending by `sequence`, whichever cursor produced it
    /// (LGC-FR-22).
    pub records: Vec<LogRecord>,
    pub generation: u64,
    pub matched_total: usize,
    pub buffer_total: usize,
    pub dropped_total: u64,
    pub highest_sequence: Option<u64>,
}

/// What `"log records appended"` carries. No record content, so no consumer can
/// evaluate a filter from it (LGC-FR-12).
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BufferState {
    pub generation: u64,
    pub buffer_total: usize,
    pub dropped_total: u64,
    pub highest_sequence: Option<u64>,
}

// ---------------------------------------------------------------------------
// The emitter seam
// ---------------------------------------------------------------------------

/// Where a log event goes. Implemented by `AppHandle` in production and by a
/// collecting stub in the tests.
pub trait LogSink {
    fn publish(&self, state: &BufferState);
}

impl<R: tauri::Runtime> LogSink for tauri::AppHandle<R> {
    fn publish(&self, state: &BufferState) {
        // LGC-FR-17: no consumer's absence or failure delays an emit, and a
        // failure here must never propagate into the work that logged. Not
        // `eprintln!`-reported the way a dropped progress event is: a logging
        // channel that reports its own failure through a second channel is the
        // shape that turns one dropped event into a loop.
        let _ = self.emit(LOG_RECORDS_APPENDED, state);
    }
}

// ---------------------------------------------------------------------------
// The buffer
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Inner {
    /// The ring. `VecDeque` so eviction is O(1) at the front and append is O(1)
    /// at the back (LGC-FR-07).
    records: VecDeque<LogRecord>,
    /// LGC-FR-05: monotonic across the whole run, never reset by a clear, so a
    /// cursor held from before one can never match a record appended after it.
    next_sequence: u64,
    /// LGC-FR-14: increases by one on each clear.
    generation: u64,
    /// LGC-FR-07: evictions since the buffer was last cleared.
    dropped_total: u64,
    /// LGC-FR-17: when a state was last published, for the coalescing window.
    last_emit: Option<Instant>,
    /// Set when an append was coalesced away. A trailing flush is owed.
    pending_emit: bool,
}

/// The session's diagnostic records. Held in memory only (LGC-FR-02).
#[derive(Default)]
pub struct LogBuffer {
    inner: Mutex<Inner>,
    /// Whether a trailing flush is already scheduled, so a burst of a thousand
    /// appends schedules one timer rather than a thousand.
    flush_scheduled: AtomicBool,
}

/// What an append produced: the state now, and whether it is due for immediate
/// publication or was coalesced (LGC-FR-17).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AppendOutcome {
    pub state: BufferState,
    pub emit_now: bool,
}

impl LogBuffer {
    /// LGC-FR-05 / LGC-FR-06 / LGC-FR-07: append `inputs` in the order given,
    /// stamping each with the next sequence, evicting oldest-first past the
    /// cap.
    ///
    /// A record supplying an empty domain set is rejected and not appended
    /// (LGC-FR-03) — silently, because the alternative is a logging call that
    /// returns an error its caller has nowhere to report.
    pub fn append_at(&self, inputs: Vec<LogInput>, now: Instant) -> AppendOutcome {
        let mut inner = self.lock();
        for input in inputs {
            // LGC-FR-03: a record must be attributable to something.
            if input.domains.is_empty() {
                continue;
            }
            let sequence = inner.next_sequence;
            inner.next_sequence += 1;
            let record = bound_record(LogRecord {
                sequence,
                ts: input.ts,
                level: input.level,
                domains: input.domains,
                message: input.message,
                fields: input.fields,
            });
            inner.records.push_back(record);
            // LGC-FR-07: oldest-first eviction, counted so the panel can say the
            // record at the head of the list is not the oldest of the session
            // (`../ui/LOG-logs.md` LOG-FR-14).
            while inner.records.len() > BUFFER_CAPACITY {
                inner.records.pop_front();
                inner.dropped_total += 1;
            }
        }
        let state = inner.state();
        // LGC-FR-17: leading-edge emit, then coalesce for the window. The
        // trailing flush is owed by `take_pending_flush` — without it a burst
        // that ends inside the window would leave the panel stale forever.
        let elapsed = inner
            .last_emit
            .map(|last| now.duration_since(last) >= COALESCE_WINDOW)
            .unwrap_or(true);
        if elapsed {
            inner.last_emit = Some(now);
            inner.pending_emit = false;
            AppendOutcome {
                state,
                emit_now: true,
            }
        } else {
            inner.pending_emit = true;
            AppendOutcome {
                state,
                emit_now: false,
            }
        }
    }

    /// The trailing half of LGC-FR-17: the state owed by appends that were
    /// coalesced away, once the window has elapsed. `None` when nothing is owed.
    pub fn take_pending_flush(&self, now: Instant) -> Option<BufferState> {
        let mut inner = self.lock();
        if !inner.pending_emit {
            return None;
        }
        let elapsed = inner
            .last_emit
            .map(|last| now.duration_since(last) >= COALESCE_WINDOW)
            .unwrap_or(true);
        if !elapsed {
            return None;
        }
        inner.pending_emit = false;
        inner.last_emit = Some(now);
        Some(inner.state())
    }

    /// Whether a trailing flush is still owed.
    ///
    /// The scheduling loop's exit condition. A flush thread that finds the
    /// window has not yet elapsed must not simply leave: the debt it was woken
    /// for is still outstanding, and if it released its slot and returned, an
    /// append that had already lost the scheduling race would be stranded with
    /// no timer anywhere in the process (see `append_and_publish`).
    pub fn is_flush_pending(&self) -> bool {
        self.lock().pending_emit
    }

    /// LGC-FR-14 / LGC-FR-15: discard every record, increase the generation, and
    /// reset the eviction count. Returns the post-clear state, which is always
    /// published — a clear is never coalesced away, because a consumer that
    /// misses it renders a buffer that no longer exists.
    pub fn clear(&self) -> BufferState {
        let mut inner = self.lock();
        inner.records.clear();
        inner.generation += 1;
        inner.dropped_total = 0;
        inner.pending_emit = false;
        // The clear itself is the emit, so the window restarts from here. Left
        // at its previous value, the first record appended *after* a clear —
        // which is the one naming what the fresh buffer is fresh for, appended
        // microseconds later by the same teardown — would fall inside the old
        // window and be coalesced away.
        inner.last_emit = None;
        // Deliberately NOT resetting `next_sequence` (LGC-FR-05).
        inner.state()
    }

    /// The state right now, without appending anything.
    pub fn state(&self) -> BufferState {
        self.lock().state()
    }

    /// LGC-FR-08 / LGC-FR-09 / LGC-FR-10 / LGC-FR-22: the page of the match set
    /// `cursor` positions and `limit` bounds, always ascending by `sequence`.
    ///
    /// Returns the typed `"invalid query"` error when a regex query does not
    /// compile (LGC-FR-11) — the buffer is not read at all in that case.
    pub fn query(
        &self,
        filter: &LogFilter,
        cursor: Option<Cursor>,
        limit: usize,
    ) -> Result<LogPage, String> {
        let matcher = Matcher::compile(filter)?;
        let inner = self.lock();
        let matched: Vec<&LogRecord> = inner
            .records
            .iter()
            .filter(|r| matcher.matches(r))
            .collect();
        let matched_total = matched.len();

        // LGC-FR-22. The buffer is ascending by construction, so each arm is a
        // slice of `matched` rather than a re-sort.
        let page: Vec<LogRecord> = match cursor {
            // The newest `limit`: the tail.
            None => matched
                .iter()
                .skip(matched_total.saturating_sub(limit))
                .map(|r| (*r).clone())
                .collect(),
            // Newer than `s`, oldest-first: the head of what remains.
            Some(Cursor::After(s)) => matched
                .iter()
                .filter(|r| r.sequence > s)
                .take(limit)
                .map(|r| (*r).clone())
                .collect(),
            // Nearest below `s`: the tail of what precedes it.
            Some(Cursor::Before(s)) => {
                let below: Vec<&&LogRecord> =
                    matched.iter().filter(|r| r.sequence < s).collect();
                below
                    .iter()
                    .skip(below.len().saturating_sub(limit))
                    .map(|r| (**r).clone())
                    .collect()
            }
        };

        Ok(LogPage {
            records: page,
            generation: inner.generation,
            matched_total,
            buffer_total: inner.records.len(),
            dropped_total: inner.dropped_total,
            highest_sequence: inner.records.back().map(|r| r.sequence),
        })
    }

    /// LGC-FR-18: every record matching `filter` as JSONL, bounded by no limit.
    /// Returns the serialised text and the count, leaving the write to the
    /// caller so the formatting is testable without touching a filesystem.
    pub fn export_text(&self, filter: &LogFilter) -> Result<(String, usize), String> {
        let matcher = Matcher::compile(filter)?;
        let inner = self.lock();
        let mut out = String::new();
        let mut count = 0usize;
        for record in inner.records.iter().filter(|r| matcher.matches(r)) {
            // A record that cannot serialise is skipped rather than failing the
            // whole export; `fields` is `serde_json::Value` throughout, so this
            // is unreachable in practice.
            if let Ok(line) = serde_json::to_string(record) {
                out.push_str(&line);
                out.push('\n');
                count += 1;
            }
        }
        Ok((out, count))
    }

    /// A poisoned buffer must not take the application down with it: logging is
    /// peripheral to every operation that emits through it.
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The oldest record `predicate` accepts, cloned, or `None` where the buffer
    /// holds no such record.
    ///
    /// Exists for the one caller [`query`](Self::query) is wrong for: a test that
    /// **polls** for a record to arrive. `query` clones every record it answers
    /// with, so a poll loop reading a buffer of two thousand records through it
    /// holds this mutex for milliseconds at a time, ten times a second. Every
    /// thread that logs takes the same mutex, so such a loop starves the very
    /// work it is waiting on — a wait for a record delays the thread that emits
    /// it. This holds the lock for one pass of borrowed comparisons and clones
    /// the single record it answers with.
    #[cfg(test)]
    pub(crate) fn find(&self, predicate: impl Fn(&LogRecord) -> bool) -> Option<LogRecord> {
        self.lock().records.iter().find(|r| predicate(r)).cloned()
    }
}

impl Inner {
    fn state(&self) -> BufferState {
        BufferState {
            generation: self.generation,
            buffer_total: self.records.len(),
            dropped_total: self.dropped_total,
            highest_sequence: self.records.back().map(|r| r.sequence),
        }
    }
}

/// LGC-FR-04 / LGC-FR-08: make a record storable — flatten its `fields`, then
/// collapse them to a single marker naming the omitted size if the record
/// exceeds the ceiling, leaving `level`, `domains`, `message` and `ts` intact.
fn bound_record(mut record: LogRecord) -> LogRecord {
    // LGC-FR-04: `fields` is flat. A nested object or an array is rendered to
    // its compact JSON text rather than being dropped, so nothing a caller
    // passed is lost while a consumer can still render every field as one row
    // without recursing.
    for value in record.fields.values_mut() {
        if value.is_object() || value.is_array() {
            *value = serde_json::Value::String(value.to_string());
        }
    }
    // Deliberately no early return for empty `fields`: the ceiling has to be
    // measured against the whole record, not against the part of it that
    // happens to be collapsible.
    let size = serde_json::to_string(&record).map(|s| s.len()).unwrap_or(0);
    // `fields` is the only thing this requirement gives up. A record that is
    // oversized with no fields at all — a very long `message` — is stored whole
    // rather than truncated mid-value, because collapsing an empty map would
    // claim fields were omitted when there were none.
    if size <= MAX_RECORD_BYTES || record.fields.is_empty() {
        return record;
    }
    let omitted = serde_json::to_string(&record.fields)
        .map(|s| s.len())
        .unwrap_or(0);
    record.fields = Fields::new();
    record.fields.insert(
        OMITTED_FIELD.to_string(),
        serde_json::Value::from(omitted as u64),
    );
    record
}

// ---------------------------------------------------------------------------
// Filtering (LGC-FR-09 / LGC-FR-10)
// ---------------------------------------------------------------------------

/// The compiled form of a `LogFilter`'s text query. Shaped like
/// `search::Matcher`, but over a record's message and fields rather than a
/// file's lines.
enum Query {
    /// Every record matches — no query, or an empty one.
    Any,
    /// Case-insensitive substring; the needle is pre-lowered.
    Insensitive(String),
    Regex(regex::Regex),
}

struct Matcher<'a> {
    min_level: LogLevel,
    domains: &'a [Domain],
    query: Query,
}

impl<'a> Matcher<'a> {
    /// LGC-FR-11: a regex that does not compile is the typed `"invalid query"`,
    /// returned before the buffer is read.
    fn compile(filter: &'a LogFilter) -> Result<Matcher<'a>, String> {
        let raw = filter.query.as_deref().unwrap_or("");
        let query = if raw.is_empty() {
            Query::Any
        } else if filter.query_is_regex {
            regex::Regex::new(raw)
                .map(Query::Regex)
                .map_err(|_| INVALID_QUERY.to_string())?
        } else {
            Query::Insensitive(raw.to_lowercase())
        };
        Ok(Matcher {
            min_level: filter.min_level,
            domains: &filter.domains,
            query,
        })
    }

    fn matches(&self, record: &LogRecord) -> bool {
        // LGC-FR-09: a floor, not an equality.
        if record.level < self.min_level {
            return false;
        }
        // LGC-FR-09: at least one listed domain; an empty list matches every
        // record, which is what makes "check none" and "check all" the same
        // thing in the panel (`../ui/LOG-logs.md` LOG-FR-07).
        if !self.domains.is_empty() && !record.domains.iter().any(|d| self.domains.contains(d)) {
            return false;
        }
        self.matches_text(record)
    }

    /// LGC-FR-10: the query matches against the message and against every key
    /// and value of `fields`.
    fn matches_text(&self, record: &LogRecord) -> bool {
        match &self.query {
            Query::Any => true,
            Query::Insensitive(needle) => {
                if record.message.to_lowercase().contains(needle.as_str()) {
                    return true;
                }
                record.fields.iter().any(|(k, v)| {
                    k.to_lowercase().contains(needle.as_str())
                        || field_text(v).to_lowercase().contains(needle.as_str())
                })
            }
            Query::Regex(re) => {
                if re.is_match(&record.message) {
                    return true;
                }
                record
                    .fields
                    .iter()
                    .any(|(k, v)| re.is_match(k) || re.is_match(&field_text(v)))
            }
        }
    }
}

/// A field value as the text a query matches against. A string matches on its
/// contents rather than on its quoted JSON form, so searching `vendor` finds
/// `"path": "vendor/"` — which is what a user typing into the panel's search box
/// means.
fn field_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

// ---------------------------------------------------------------------------
// The emit API every module logs through (LGC contract surface)
// ---------------------------------------------------------------------------

/// Append one record and publish what is due.
///
/// `sink` and `buffer` lead the argument list for the same reason they do in
/// `progress::attribute`: a module reaches both through the `AppHandle` and the
/// managed state it already holds, and threading them explicitly keeps this
/// module free of global state.
///
/// LGC-FR-17: never blocks the caller. The append itself is a lock and a copy;
/// the trailing flush of a coalesced burst is handed to a background thread so
/// no emit site ever waits on the coalescing window.
pub fn log<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    level: LogLevel,
    domains: &[Domain],
    message: &str,
    fields: Fields,
) {
    let input = LogInput {
        ts: crate::notes::now_rfc3339_millis(),
        level,
        domains: domains.to_vec(),
        message: message.to_string(),
        fields,
    };
    append_and_publish(sink, buffer, vec![input]);
}

/// Build a record's [`Fields`] from `"key" => value` pairs.
///
/// Exists because ergonomics decide whether a convention is followed. The
/// alternative at an emit site is
/// `[("k".to_string(), serde_json::Value::from(v))].into_iter().collect()`,
/// which is tedious enough that the tempting shortcut is to interpolate the
/// value into the message instead — losing the structure the Logs panel filters
/// and searches on, and making it that much easier to sweep something sensitive
/// into a string nobody reads closely.
///
/// ```ignore
/// log_warn(&app, &BUFFER, &[Domain::Remote], "push rejected",
///          log_fields! { "remote" => "origin", "status" => 403 });
/// ```
#[macro_export]
macro_rules! log_fields {
    ($($key:expr => $value:expr),* $(,)?) => {{
        #[allow(unused_mut)]
        let mut fields = $crate::logging::Fields::new();
        $( fields.insert($key.to_string(), serde_json::json!($value)); )*
        fields
    }};
}

/// `log` at `DEBUG`.
pub fn log_debug<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    domains: &[Domain],
    message: &str,
    fields: Fields,
) {
    log(sink, buffer, LogLevel::Debug, domains, message, fields);
}

/// `log` at `INFO`.
pub fn log_info<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    domains: &[Domain],
    message: &str,
    fields: Fields,
) {
    log(sink, buffer, LogLevel::Info, domains, message, fields);
}

/// `log` at `WARN`.
pub fn log_warn<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    domains: &[Domain],
    message: &str,
    fields: Fields,
) {
    log(sink, buffer, LogLevel::Warn, domains, message, fields);
}

/// `log` at `ERROR`.
pub fn log_error<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    domains: &[Domain],
    message: &str,
    fields: Fields,
) {
    log(sink, buffer, LogLevel::Error, domains, message, fields);
}

/// Append `inputs` and publish the state that is due, scheduling a trailing
/// flush when the append was coalesced (LGC-FR-17).
///
/// Generic over the sink so the tests drive the whole path — including the
/// scheduling decision — without a Tauri runtime.
pub fn append_and_publish<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    inputs: Vec<LogInput>,
) {
    let outcome = buffer.append_at(inputs, Instant::now());
    if outcome.emit_now {
        sink.publish(&outcome.state);
        return;
    }
    // A burst schedules one timer, not one per record: the slot is held for as
    // long as a flush is owed, and an append that loses this race relies on the
    // thread already holding it to settle the debt before it exits.
    if buffer
        .flush_scheduled
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    let sink = sink.clone();
    std::thread::spawn(move || {
        // A loop rather than a single sleep, because the debt this thread was
        // woken for is not necessarily the debt it finds. An append landing
        // just outside the window resets `last_emit` and is delivered on the
        // leading edge; a further append immediately after it is coalesced and
        // loses the scheduling race to *this* thread, which then wakes to find
        // the window has not elapsed. Returning there would strand that append
        // with no timer anywhere — the panel would sit on a stale count until
        // some unrelated record happened to land outside a window, which is
        // precisely the staleness the trailing flush exists to prevent.
        loop {
            std::thread::sleep(COALESCE_WINDOW);
            if let Some(state) = buffer.take_pending_flush(Instant::now()) {
                sink.publish(&state);
            }
            buffer.flush_scheduled.store(false, Ordering::Release);
            if !buffer.is_flush_pending() {
                return;
            }
            // Something was appended while this thread was publishing or
            // sleeping. Re-take the slot to keep owning the debt; if another
            // emitter took it first, it owns the debt now and this thread is
            // free to exit.
            if buffer
                .flush_scheduled
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                return;
            }
        }
    });
}

/// LGC-FR-15: discard the whole buffer and publish the post-clear state.
///
/// Called as the *first* step of a project close or a worktree change, before
/// the operation that performs it runs, so the records that operation emits —
/// including those explaining a switch that fails — land in the fresh buffer
/// rather than being discarded by the clear that follows.
pub fn clear_and_publish(sink: &impl LogSink, buffer: &LogBuffer) {
    let state = buffer.clear();
    sink.publish(&state);
}

// ---------------------------------------------------------------------------
// Tauri commands (LGC contract surface)
// ---------------------------------------------------------------------------

/// The process-wide buffer.
///
/// A `static` rather than Tauri managed state, because the internal emit API of
/// LGC-FR-01 has to be reachable from modules that hold no `State` handle — a
/// helper deep inside `git.rs` logs without threading a registry down to it. The
/// lifetime is the process, which is exactly the buffer's own lifetime
/// (LGC-FR-02), so nothing is leaked by the choice.
pub static BUFFER: LogBuffer = LogBuffer::new();

impl LogBuffer {
    /// `const` so [`BUFFER`] can be a `static` rather than a lazy singleton.
    pub const fn new() -> Self {
        LogBuffer {
            inner: Mutex::new(Inner::new()),
            flush_scheduled: AtomicBool::new(false),
        }
    }
}

impl Inner {
    const fn new() -> Self {
        Inner {
            records: VecDeque::new(),
            next_sequence: 0,
            generation: 0,
            dropped_total: 0,
            last_emit: None,
            pending_emit: false,
        }
    }
}

/// LGC-FR-07 / LGC-FR-20: append a batch of records emitted by the frontend, in
/// the order given. Never returns an error — a failure to record a diagnostic is
/// not worth failing the caller over (`../ui/LOG-logs.md` LOG-FR-19) — and
/// answers normally with no project open.
#[tauri::command]
pub fn append_log_records(app: tauri::AppHandle, records: Vec<LogInput>) {
    append_and_publish(&app, &BUFFER, records);
}

/// LGC-FR-09 – LGC-FR-12 / LGC-FR-22: the records matching `filter`, positioned
/// by `cursor` and bounded by `limit`.
#[tauri::command]
pub fn query_logs(
    filter: LogFilter,
    cursor: Option<Cursor>,
    limit: usize,
) -> Result<LogPage, String> {
    BUFFER.query(&filter, cursor, limit)
}

/// LGC-FR-18 / LGC-FR-19: write every record matching `filter` to
/// `destination_path` as JSONL and return how many were written.
///
/// The write goes through `FsAccess::write_text_at_user_choice` (FSA-FR-28),
/// which retains the atomicity of FSA-FR-04, so a failure leaves no partial file
/// at the destination. `destination_path` is the path the save dialog returned
/// (FSA-FR-16), and that write is the only kind that accepts it — which is what
/// lets an export land wherever the user pointed it, typically outside the
/// project and outside every directory the shared instance allowlists, without
/// any other operation in the application gaining that reach. A pre-existing
/// file there is replaced, since choosing an occupied path in a save dialog is
/// how a user asks for that.
#[tauri::command]
pub fn export_logs(
    app: tauri::AppHandle,
    fs_access: tauri::State<'_, fs::FsAccessState>,
    filter: LogFilter,
    destination_path: String,
) -> Result<usize, String> {
    let (text, count) = BUFFER.export_text(&filter)?;
    let access = fs_access.get().ok_or_else(|| {
        // Only reachable if `setup` never installed an instance, which panics —
        // but a command that silently returns the generic failure would leave
        // no trace of why, and this is the one channel a user reports through.
        log_export_failure(&app, &destination_path, "no filesystem instance");
        EXPORT_FAILED.to_string()
    })?;
    if let Err(e) = write_export(&access, Path::new(&destination_path), &text) {
        log_export_failure(&app, &destination_path, &e);
        return Err(EXPORT_FAILED.to_string());
    }
    Ok(count)
}

/// An export that failed is invisible unless it says so here: the caller gets
/// the flat `EXPORT_FAILED` string, and `fs::FsAccess` deliberately logs nothing
/// of its own, so this is the only place the reason survives. The destination is
/// a path the user picked and the reason is an `FsError`'s own `Display` — a
/// path and a failure kind, never file content and never a credential.
fn log_export_failure<R: tauri::Runtime>(app: &tauri::AppHandle<R>, destination: &str, reason: &str) {
    log(
        app,
        &BUFFER,
        LogLevel::Error,
        &[Domain::Backend],
        "log export failed",
        crate::log_fields! {
            "destination" => destination.to_string(),
            "reason" => reason.to_string(),
        },
    );
}

/// The write half of `export_logs`, split out so the error mapping is testable
/// against a real path without going through the command's argument shape.
/// Returns the reason on failure so the caller can log it.
fn write_export(access: &fs::FsAccess, path: &Path, text: &str) -> Result<(), String> {
    // FSA-FR-28: a destination that is not absolute did not come from a dialog,
    // whatever the frontend claims, so there is no user choice to honour.
    let chosen = fs::UserChosenPath::from_frontend_response(path)
        .ok_or_else(|| "destination is not an absolute path".to_string())?;
    access
        .write_text_at_user_choice(&chosen, text)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
