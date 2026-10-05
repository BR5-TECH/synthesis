//! Search — the engine behind the universal search bar
//! (`specifications/core/SCC-search.md`).
//!
//! A search answers a query by matching the text of the project's files rather
//! than by consulting an index. There is no index at all: one asynchronous
//! producer enumerates the candidate files and a fixed pool of asynchronous
//! consumers matches them, streaming each hit to the UI the moment it is found
//! (SCC-FR-02 / SCC-FR-19).
//!
//! The candidates are not walked here. They are the `file` nodes the Library's
//! scan already maintains (`scanning::CandidateStore`, ASC-FR-17), so a search
//! costs no second walk of the tree and honours exactly one set of ignore rules
//! (SCC-FR-03 / SCC-FR-04).
//!
//! Two seams keep the whole pipeline testable without a Tauri runtime:
//!
//! - [`SearchSink`] is the emitter. `AppHandle` implements it in production; the
//!   tests use a collecting stub, so streaming, capping, cancellation and
//!   supersession are all exercised without a window.
//! - [`run_search`] is the pipeline itself, pure over (sink, root, candidates,
//!   matcher, scope, stop). The `#[tauri::command]` halves only resolve state and
//!   spawn it.

use std::sync::atomic::{AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, RecvTimeoutError, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, State};

use crate::fs as fsa;
use crate::progress::{self, OperationState, ProgressRegistry};
use crate::project::ProjectState;
use crate::scanning::{ArtifactType, Candidate, CandidateStore};

// ---------------------------------------------------------------------------
// Event names (SCC contract surface)
// ---------------------------------------------------------------------------

/// SCC-FR-01 / SCC-FR-19: hits found since the previous emission.
///
/// Kebab-case rather than the spec's abstract `"search results"`, for the reason
/// every other channel in this crate is: Tauri validates event names and accepts
/// only alphanumerics, `-`, `/`, `:` and `_`, so a name with spaces is rejected
/// on both sides — `emit` returns an error the emitters discard and `listen`
/// rejects symmetrically, leaving the channel silently dead. Matches the
/// constant in `src/events.ts` byte-for-byte; pinned by
/// `every_event_name_is_one_tauri_will_actually_deliver` in `lib.rs`.
pub const SEARCH_RESULTS: &str = "search-results";
/// SCC-FR-13: emitted exactly once per search, whatever the outcome.
pub const SEARCH_ENDED: &str = "search-ended";

// ---------------------------------------------------------------------------
// Tuning (SCC non-functional requirements)
// ---------------------------------------------------------------------------

/// SCC-FR-11: the total hit count a `capped` search stops at — a single total
/// across all groups, not a per-group allowance. Of the order of a few tens:
/// enough to fill an overlay that shows a prefix per group, small enough that a
/// common query stops almost immediately.
pub const CAPPED_HIT_LIMIT: usize = 40;

/// SCC-FR-15: a candidate larger than this is never read line by line. Its path
/// can still match, so a large asset stays findable by name.
pub const MAX_CONTENT_BYTES: u64 = 2 * 1024 * 1024;

/// SCC-FR-07: maximum length of a snippet, in characters — a window around the
/// match, not a prefix of the line. Sized for the overlay, which renders a
/// snippet on a single clipped line: a budget much wider than that is spent on
/// text the user never sees.
pub const SNIPPET_MAX_CHARS: usize = 120;

/// How much of the line may precede the match inside a snippet.
///
/// Enough to read the match in context — the start of the statement it sits in —
/// and little enough that the match itself stays visible however narrow the
/// surface rendering it. Without this cap a centred window spends half its
/// budget on lead-in, and on a match late in a long line that lead-in is what
/// survives the clipping while the match does not.
pub const SNIPPET_LEAD_CHARS: usize = 28;

/// How long the collector waits for more hits before flushing what it has. The
/// contract is only that hits are not withheld until the end (SCC-FR-19) and
/// that the event bus is not flooded per hit; the interval is an implementation
/// choice.
const BATCH_INTERVAL: Duration = Duration::from_millis(25);

/// Flush early once a batch reaches this many hits, so a query matching
/// everything does not sit on a growing buffer for a whole interval.
const BATCH_MAX_HITS: usize = 32;

/// Capacity of the producer→consumer queue. Bounded so the producer cannot run
/// arbitrarily far ahead of the pool and buffer the whole tree.
const QUEUE_CAPACITY: usize = 64;

/// The consumer pool's size, chosen from the machine's available parallelism and
/// bounded so a search never starves the rest of the application (SCC NFR). It
/// does not grow with the size of the tree.
fn consumer_pool_size() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get().saturating_sub(1).clamp(2, 8))
        .unwrap_or(4)
}

// ---------------------------------------------------------------------------
// Wire shapes (SCC "Payload shapes")
// ---------------------------------------------------------------------------

/// SCC-FR-05: what the query text means. No mode assigns meaning to any
/// character the others treat literally.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    /// Literal substring, ignoring case.
    LiteralInsensitive,
    /// Literal substring, ignoring case while the query is all-lowercase and
    /// respecting case as soon as it contains an uppercase character.
    SmartCase,
    /// The query compiled as a regular expression.
    Regex,
}

/// SCC-FR-11: how far the sweep runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchScope {
    /// Stop at [`CAPPED_HIT_LIMIT`] total hits (the overlay, SCH-FR-18).
    Capped,
    /// Run the enumeration to exhaustion (the Search results tab).
    Full,
}

/// SCC-FR-07: whether the file matched on its content or only on its path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchKind {
    Content,
    Name,
}

/// SCC-FR-08 / SCC-FR-09: which group a hit renders under. `Run` and `History`
/// are part of the shape and are never populated by this engine, which matches
/// only files under the active content root.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchGroup {
    Artifact,
    Playbook,
    Workstream,
    Role,
    Run,
    History,
    File,
}

/// Where an artifact is edited, which is what `../ui/SCH-search.md` SCH-FR-09
/// routes a click on it by (Editor vs Flow).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EditContext {
    Standalone,
    Flow,
}

/// One matching file (SCC contract surface). Serialised camelCase for the
/// frontend; optional fields are omitted when absent so the wire shape matches
/// the `SearchHit` interface in `src/types.ts`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    /// The ASC node id of the file (ASC-FR-13) — the identity
    /// `../ui/TAB-tabs.md` TAB-FR-04 compares.
    pub id: String,
    pub name: String,
    pub path: String,
    /// The file's position in the enumeration; the ordering key of SCC-FR-10.
    pub ordinal: u64,
    pub group: SearchGroup,
    pub match_kind: MatchKind,
    /// Artifact group only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtype: Option<ArtifactType>,
    /// Artifact group only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edit_context: Option<EditContext>,
    /// Content matches only: 1-based line number of the first match.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// Content matches only: the text of that line (SCC-FR-07).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

/// Payload of `"search results"` (SCC-FR-19).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResultsPayload {
    pub search_id: String,
    pub hits: Vec<SearchHit>,
}

/// Why a search ended (SCC contract surface).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EndReason {
    Completed,
    Capped,
    Cancelled,
    Superseded,
    Failed,
}

impl EndReason {
    /// How this outcome terminates the search's PRG operation (SCC-FR-18).
    fn operation_state(self) -> OperationState {
        match self {
            EndReason::Cancelled | EndReason::Superseded => OperationState::Cancelled,
            EndReason::Failed => OperationState::Failed,
            EndReason::Completed | EndReason::Capped => OperationState::Finished,
        }
    }
}

/// Payload of `"search ended"` (SCC-FR-13).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchEndedPayload {
    pub search_id: String,
    pub reason: EndReason,
}

/// SCC-FR-06: the only typed error `start_search` returns.
pub const INVALID_QUERY: &str = "invalid query";

// ---------------------------------------------------------------------------
// The emitter seam
// ---------------------------------------------------------------------------

/// Where a search's events go. Implemented by `AppHandle` in production and by a
/// collecting stub in the tests, so the pipeline's streaming, capping and
/// termination invariants are exercised without a Tauri runtime.
pub trait SearchSink: Send + Sync {
    fn results(&self, payload: &SearchResultsPayload);
    fn ended(&self, payload: &SearchEndedPayload);
}

impl<R: tauri::Runtime> SearchSink for tauri::AppHandle<R> {
    fn results(&self, payload: &SearchResultsPayload) {
        // A dropped event must never take down the search it reports on; the
        // terminal event is what a consumer keys its teardown on, and it is
        // emitted on every path out of `run_search`.
        let _ = self.emit(SEARCH_RESULTS, payload);
    }

    fn ended(&self, payload: &SearchEndedPayload) {
        let _ = self.emit(SEARCH_ENDED, payload);
    }
}

// ---------------------------------------------------------------------------
// Stop signalling (SCC-FR-12 / SCC-FR-14)
// ---------------------------------------------------------------------------

const STOP_RUNNING: u8 = 0;
const STOP_CANCELLED: u8 = 1;
const STOP_SUPERSEDED: u8 = 2;

/// The one signal that stops a running search, carrying *why* — so the search
/// itself decides between `cancelled` and `superseded` rather than the caller
/// having to emit the terminal event on its behalf (SCC-FR-13: exactly one).
#[derive(Clone, Debug, Default)]
pub struct Stop(Arc<AtomicU8>);

impl Stop {
    pub fn new() -> Stop {
        Stop(Arc::new(AtomicU8::new(STOP_RUNNING)))
    }

    /// SCC-FR-14: stop at the next file boundary and end with `cancelled`.
    pub fn cancel(&self) {
        // Only the first stop wins: a cancel arriving after a supersede must not
        // relabel the reason the search already committed to.
        let _ = self.0.compare_exchange(
            STOP_RUNNING,
            STOP_CANCELLED,
            Ordering::SeqCst,
            Ordering::SeqCst,
        );
    }

    /// SCC-FR-12: stop because a newer search has taken this one's place.
    pub fn supersede(&self) {
        let _ = self.0.compare_exchange(
            STOP_RUNNING,
            STOP_SUPERSEDED,
            Ordering::SeqCst,
            Ordering::SeqCst,
        );
    }

    /// The reason to end with, or `None` while the search is still running.
    fn reason(&self) -> Option<EndReason> {
        match self.0.load(Ordering::SeqCst) {
            STOP_CANCELLED => Some(EndReason::Cancelled),
            STOP_SUPERSEDED => Some(EndReason::Superseded),
            _ => None,
        }
    }

    fn is_stopped(&self) -> bool {
        self.0.load(Ordering::SeqCst) != STOP_RUNNING
    }
}

// ---------------------------------------------------------------------------
// Matching (SCC-FR-05 / SCC-FR-06) — pure
// ---------------------------------------------------------------------------

/// A compiled query. Compilation is where SCC-FR-06's typed error is decided, so
/// a regular expression that does not compile never reaches the pipeline.
#[derive(Debug)]
pub enum Matcher {
    /// Literal substring, case-insensitive. The needle is pre-lowered.
    Insensitive(String),
    /// Literal substring, case-sensitive.
    Sensitive(String),
    Regex(regex::Regex),
}

impl Matcher {
    /// SCC-FR-05 / SCC-FR-06: compile `query` under `mode`, or return the typed
    /// `"invalid query"` error.
    pub fn compile(query: &str, mode: SearchMode) -> Result<Matcher, String> {
        match mode {
            SearchMode::LiteralInsensitive => Ok(Matcher::Insensitive(query.to_lowercase())),
            // SCC-FR-05: case is respected as soon as the query contains an
            // uppercase character, and ignored while it does not.
            SearchMode::SmartCase => {
                if query.chars().any(|c| c.is_uppercase()) {
                    Ok(Matcher::Sensitive(query.to_string()))
                } else {
                    Ok(Matcher::Insensitive(query.to_lowercase()))
                }
            }
            SearchMode::Regex => regex::Regex::new(query)
                .map(Matcher::Regex)
                .map_err(|_| INVALID_QUERY.to_string()),
        }
    }

    /// Whether `haystack` matches, without locating the match.
    pub fn is_match(&self, haystack: &str) -> bool {
        match self {
            Matcher::Insensitive(needle) => haystack.to_lowercase().contains(needle),
            Matcher::Sensitive(needle) => haystack.contains(needle),
            Matcher::Regex(re) => re.is_match(haystack),
        }
    }

    /// The byte range of the first match in `haystack`, for snippet windowing.
    fn find(&self, haystack: &str) -> Option<(usize, usize)> {
        match self {
            // The lowered copy is byte-indexed independently of the original, so
            // the offset is mapped back by lowering successive prefixes rather
            // than assumed equal — `İ` lowers to two chars, and a naive reuse of
            // the offset would slice mid-character and panic.
            Matcher::Insensitive(needle) => {
                let lowered = haystack.to_lowercase();
                let at = lowered.find(needle.as_str())?;
                Some((map_lowered_offset(haystack, at), needle.chars().count()))
            }
            Matcher::Sensitive(needle) => haystack
                .find(needle.as_str())
                .map(|at| (at, needle.chars().count())),
            Matcher::Regex(re) => re
                .find(haystack)
                .map(|m| (m.start(), haystack[m.start()..m.end()].chars().count())),
        }
    }
}

/// Translate a byte offset in `haystack.to_lowercase()` back to a char index in
/// `haystack`. Lowercasing is not length-preserving in general, so the mapping is
/// found by growing a prefix until its lowered form reaches the offset.
fn map_lowered_offset(haystack: &str, lowered_offset: usize) -> usize {
    let mut lowered_len = 0usize;
    for (byte_index, ch) in haystack.char_indices() {
        if lowered_len >= lowered_offset {
            return byte_index;
        }
        lowered_len += ch.to_lowercase().map(|c| c.len_utf8()).sum::<usize>();
    }
    haystack.len()
}

/// SCC-FR-07: the snippet for a matching line — a window **around the match**,
/// bounded to [`SNIPPET_MAX_CHARS`] and always retaining the matched text.
///
/// The window is anchored on the match rather than on the start of the line.
/// Anchoring it on the line would put the match wherever it happened to fall,
/// and the overlay renders a snippet on one clipped line — so a match late in a
/// long line would be cut off the right-hand edge and the user would be shown a
/// result whose reason for matching is invisible.
///
/// At most [`SNIPPET_LEAD_CHARS`] of the line precede the match, so the match
/// sits near the head of the snippet whatever the container's width, with the
/// remaining budget spent on what follows it. An ellipsis marks whichever side
/// was cut. A line that fits entirely is returned verbatim.
pub fn snippet_for(line: &str, match_start_chars: usize, match_len_chars: usize) -> String {
    let chars: Vec<char> = line.chars().collect();
    // Centre the window on the match, then cap the lead-in. Centring alone
    // spends up to half the budget before the match, which on a narrow overlay
    // is exactly the text that gets clipped away again.
    let half = SNIPPET_MAX_CHARS.saturating_sub(match_len_chars.min(SNIPPET_MAX_CHARS)) / 2;
    let lead = half.min(SNIPPET_LEAD_CHARS);
    let start = match_start_chars.saturating_sub(lead);
    // Deliberately NOT pulled back to fill the budget when the match sits near
    // the end of the line: a full window would push the match to the far right,
    // which is the very thing this anchoring exists to prevent. A short snippet
    // with the match visible beats a long one with the match off-screen.
    let end = (start + SNIPPET_MAX_CHARS).min(chars.len());
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.extend(&chars[start..end]);
    if end < chars.len() {
        out.push('…');
    }
    out
}

/// SCC-FR-08: which group a candidate's hit renders under, highest precedence
/// first — the `.synthesis/` entity directories, then artifact classification,
/// then everything else.
///
/// The entity directories deliberately outrank classification, so a playbook
/// that also classifies as an artifact is grouped as a playbook.
pub fn group_for(path: &str, artifact_type: Option<ArtifactType>) -> SearchGroup {
    if path.starts_with(".synthesis/playbooks/") {
        return SearchGroup::Playbook;
    }
    if path.starts_with(".synthesis/workstreams/") {
        return SearchGroup::Workstream;
    }
    if path.starts_with(".synthesis/roles/") {
        return SearchGroup::Role;
    }
    match artifact_type {
        Some(_) => SearchGroup::Artifact,
        None => SearchGroup::File,
    }
}

/// SCC-FR-07 / SCC-FR-15 (pure): decide a candidate's hit, given its content or
/// `None` when the file was not read for content — because it is too large, not
/// valid UTF-8, or no longer there.
///
/// Content outranks path: a file whose lines match carries `match_kind =
/// "content"` with the **first** matching line, and a file matching only by path
/// carries `match_kind = "name"` with neither `line` nor `snippet`. A file
/// produces at most one hit either way.
pub fn hit_for(
    candidate: &Candidate,
    matcher: &Matcher,
    content: Option<&str>,
) -> Option<SearchHit> {
    let group = group_for(&candidate.path, candidate.artifact_type);
    let (subtype, edit_context) = match group {
        SearchGroup::Artifact => (
            candidate.artifact_type,
            Some(match candidate.artifact_type {
                // DSH-FR-06 / SCH-FR-09: a Flow is edited on the Flow canvas;
                // every other artifact type in the Editor.
                Some(ArtifactType::Flow) => EditContext::Flow,
                _ => EditContext::Standalone,
            }),
        ),
        _ => (None, None),
    };
    let base = |match_kind, line, snippet| SearchHit {
        id: candidate.id.clone(),
        name: candidate.name.clone(),
        path: candidate.path.clone(),
        ordinal: candidate.ordinal,
        group,
        match_kind,
        subtype,
        edit_context,
        line,
        snippet,
    };

    if let Some(text) = content {
        for (index, line) in text.lines().enumerate() {
            if let Some((start, len)) = matcher.find(line) {
                let start_chars = line[..start].chars().count();
                return Some(base(
                    MatchKind::Content,
                    Some(index as u32 + 1),
                    Some(snippet_for(line, start_chars, len)),
                ));
            }
        }
    }
    if matcher.is_match(&candidate.path) {
        return Some(base(MatchKind::Name, None, None));
    }
    None
}

/// SCC-FR-15: read a candidate's text for content matching, or `None` when it
/// must not be read line by line — an oversized file, one whose bytes are not
/// valid UTF-8, or one that has been removed since it was enumerated (SCC-FR-04,
/// which skips it rather than failing).
fn readable_content(root: &crate::fs::RootFs, candidate: &Candidate) -> Option<String> {
    let path = fsa::resolve_under(root, &candidate.path).ok()?;
    // SCC non-functional requirement: a search reads nothing outside the active
    // content root. `resolve_under` enforces that *syntactically* — it rejects a
    // path that climbs out with `..` — but a symlink sitting inside the root can
    // still point anywhere on the machine, and `metadata`/`read` follow it. A
    // hit's snippet would then carry bytes from another checkout, or from
    // `/etc`, attributed to a path under this project.
    //
    // So the link itself is inspected rather than its target, and a symlink is
    // never read. An ancestor cannot smuggle one in either: the scan walks with
    // `follow_links(false)` (ASC-FR-09), so it never descends *through* a
    // symlinked directory, and no candidate has a symlinked ancestor.
    //
    // Such a file stays findable by its path (SCC-FR-07), exactly as an
    // oversized or non-UTF-8 one does — only its content is off limits.
    let link = root.file_info(&path).ok()?;
    if link.kind != crate::fs::EntryKind::File || link.size > MAX_CONTENT_BYTES {
        return None;
    }
    // Through the helper: the size and symlink checks above are this module's
    // own policy, and the read itself still goes through the gate (FSA-FR-19).
    let bytes = root.read_bytes(&path).ok()?;
    String::from_utf8(bytes).ok()
}

// ---------------------------------------------------------------------------
// The pipeline (SCC-FR-02) — producer + consumer pool + collector
// ---------------------------------------------------------------------------

/// Run one search to its end and return why it ended.
///
/// One producer thread enumerates `candidates` into a bounded queue; a fixed
/// pool of consumer threads takes files from that queue and matches them; this
/// thread collects their hits, batches them, and streams `"search results"`
/// (SCC-FR-02 / SCC-FR-19). Exactly one `"search ended"` is emitted, on every
/// path out (SCC-FR-13).
///
/// `on_progress` is handed the number of candidates consumed so far, so the
/// caller can attribute the sweep to `PRG-progress-reporting.md` without this
/// function knowing anything about progress reporting.
pub fn run_search<S: SearchSink + ?Sized>(
    sink: &S,
    root: &crate::fs::RootFs,
    candidates: Arc<Vec<Candidate>>,
    matcher: Arc<Matcher>,
    scope: SearchScope,
    stop: &Stop,
    search_id: &str,
    on_progress: &(dyn Fn(u64) + Send + Sync),
) -> EndReason {
    let (work_tx, work_rx) = sync_channel::<Candidate>(QUEUE_CAPACITY);
    let (hit_tx, hit_rx) = std::sync::mpsc::channel::<SearchHit>();
    let work_rx = Arc::new(Mutex::new(work_rx));
    // Consumed by the pool, read by the collector for progress. Reported rather
    // than derived from the hit count: the sweep's progress is how much of the
    // tree has been looked at, not how much of it matched.
    let consumed = Arc::new(AtomicU64::new(0));

    let producer = {
        let stop = stop.clone();
        let candidates = Arc::clone(&candidates);
        std::thread::spawn(move || {
            'enumerate: for candidate in candidates.iter() {
                // SCC-FR-11 / SCC-FR-14: the producer stops at the cap and at a
                // cancellation rather than finishing the tree.
                let mut pending = candidate.clone();
                loop {
                    if stop.is_stopped() {
                        break 'enumerate;
                    }
                    // `try_send` rather than `send`: a blocking send into the
                    // bounded queue parks the producer until a consumer takes
                    // one — and on a cancellation every consumer has already
                    // stopped taking, so a parked producer would never observe
                    // the stop and its join would hang forever.
                    match work_tx.try_send(pending) {
                        Ok(()) => break,
                        Err(TrySendError::Full(returned)) => {
                            pending = returned;
                            std::thread::sleep(Duration::from_millis(1));
                        }
                        // The collector tore the pipeline down.
                        Err(TrySendError::Disconnected(_)) => break 'enumerate,
                    }
                }
            }
            // Dropping the sender is what releases the consumers blocked on an
            // empty queue, and so what lets the pool finish.
        })
    };

    let consumers: Vec<_> = (0..consumer_pool_size())
        .map(|_| {
            let work_rx = Arc::clone(&work_rx);
            let hit_tx = hit_tx.clone();
            let matcher = Arc::clone(&matcher);
            let stop = stop.clone();
            let consumed = Arc::clone(&consumed);
            let root = root.clone();
            std::thread::spawn(move || loop {
                // SCC-FR-14: cancellation is observed at file boundaries, so a
                // file already being matched is finished rather than abandoned
                // half-read.
                if stop.is_stopped() {
                    break;
                }
                let next = {
                    let guard = match work_rx.lock() {
                        Ok(g) => g,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    guard.recv()
                };
                let Ok(candidate) = next else { break };
                let content = readable_content(&root, &candidate);
                consumed.fetch_add(1, Ordering::Relaxed);
                if let Some(hit) = hit_for(&candidate, &matcher, content.as_deref()) {
                    if hit_tx.send(hit).is_err() {
                        break;
                    }
                }
            })
        })
        .collect();
    // The collector must not hold a sender of its own, or the channel never
    // disconnects and the loop below never learns the pool is done.
    drop(hit_tx);

    let mut emitted = 0usize;
    let mut batch: Vec<SearchHit> = Vec::new();
    let mut capped = false;
    let mut last_progress = Instant::now();

    let flush = |batch: &mut Vec<SearchHit>| {
        if batch.is_empty() {
            return;
        }
        sink.results(&SearchResultsPayload {
            search_id: search_id.to_string(),
            hits: std::mem::take(batch),
        });
    };

    loop {
        match hit_rx.recv_timeout(BATCH_INTERVAL) {
            Ok(hit) => {
                batch.push(hit);
                emitted += 1;
                // SCC-FR-11: `capped` ends as soon as the TOTAL hit count across
                // all groups reaches the cap; `full` runs to exhaustion.
                if scope == SearchScope::Capped && emitted >= CAPPED_HIT_LIMIT {
                    capped = true;
                    stop.cancel();
                    break;
                }
                if batch.len() >= BATCH_MAX_HITS {
                    flush(&mut batch);
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                // SCC-FR-19: a hit found early is delivered while the rest of the
                // tree is still being matched, rather than withheld until the end.
                flush(&mut batch);
                if stop.is_stopped() {
                    break;
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                flush(&mut batch);
                break;
            }
        }
        if last_progress.elapsed() >= BATCH_INTERVAL {
            last_progress = Instant::now();
            on_progress(consumed.load(Ordering::Relaxed));
        }
    }
    flush(&mut batch);

    // Read the reason BEFORE the shutdown signal below, so this function's own
    // stop can never be mistaken for one the caller issued.
    let external_stop = stop.reason();

    // Signal shutdown so both joins return promptly: the producer stops
    // enumerating, and dropping its sender releases every consumer blocked on an
    // empty queue. Dropping the collector's receiver releases any consumer
    // parked on a hit send.
    stop.cancel();
    drop(hit_rx);
    let _ = producer.join();
    for consumer in consumers {
        let _ = consumer.join();
    }
    // The true count, not `total`: a capped or cancelled sweep stopped early,
    // and reporting the whole tree would claim it read files it never opened.
    // A sweep that ran to exhaustion reports `total` by arriving there.
    let read = consumed.load(Ordering::Relaxed);
    on_progress(read);

    // The cap names its own outcome; otherwise a stop that arrived from outside
    // names it, and an untouched stop means the enumeration was exhausted.
    let reason = if capped {
        EndReason::Capped
    } else {
        external_stop.unwrap_or(EndReason::Completed)
    };
    sink.ended(&SearchEndedPayload {
        search_id: search_id.to_string(),
        reason,
    });
    reason
}

// ---------------------------------------------------------------------------
// The registry (SCC-FR-12 / SCC-FR-14)
// ---------------------------------------------------------------------------

/// The at-most-one running search (SCC-FR-12), plus the id counter.
#[derive(Default)]
pub struct SearchRegistry {
    inner: Mutex<Option<(String, Stop)>>,
    next: AtomicUsize,
}

impl SearchRegistry {
    /// A fresh id, unique for the lifetime of the running application.
    fn next_id(&self) -> String {
        format!("search-{}", self.next.fetch_add(1, Ordering::SeqCst))
    }

    /// SCC-FR-12: install `id` as the running search, superseding whatever was
    /// running. The superseded search ends itself with `reason = "superseded"`,
    /// so a caller never has to cancel before starting.
    fn install(&self, id: &str) -> Stop {
        let stop = Stop::new();
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some((_, previous)) = guard.replace((id.to_string(), stop.clone())) {
            previous.supersede();
        }
        stop
    }

    /// SCC-FR-14: stop `id` if it is the running search. An id that has already
    /// ended is a no-op, not an error.
    fn cancel(&self, id: &str) {
        let guard = match self.inner.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some((running, stop)) = guard.as_ref() {
            if running == id {
                stop.cancel();
            }
        }
    }

    /// Drop `id` from the registry once it has ended — but only if it is still
    /// the running one, so a search that ended because it was superseded does
    /// not evict its successor on the way out.
    fn retire(&self, id: &str) {
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if guard.as_ref().is_some_and(|(running, _)| running == id) {
            *guard = None;
        }
    }

    /// Stop whatever is running, as part of a project close or worktree change.
    /// The search ends with `cancelled` and emits its one terminal event.
    pub fn cancel_running(&self) {
        let guard = match self.inner.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some((_, stop)) = guard.as_ref() {
            stop.cancel();
        }
    }
}

// ---------------------------------------------------------------------------
// Tauri commands (SCC contract surface)
// ---------------------------------------------------------------------------

/// SCC-FR-01 / SCC-FR-02: start a search and return its id immediately, before
/// any file has been matched. The producer, the consumer pool and the collector
/// all run off the thread serving the UI, so this command never blocks on
/// matching.
///
/// SCC-FR-06: a `regex` query that does not compile returns the typed
/// `"invalid query"` error — no id is issued, nothing starts, and neither event
/// is emitted for it.
///
/// SCC-FR-17: with no project open the search still issues an id, emits no hits,
/// and ends immediately with `completed`. That is not an error: v1 search is
/// scoped to the open project's content root.
#[tauri::command]
pub fn start_search<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    query: String,
    mode: SearchMode,
    scope: SearchScope,
    project: State<'_, ProjectState>,
    registry: State<'_, SearchRegistry>,
) -> Result<String, String> {
    // Compiled before anything is registered, so an invalid pattern leaves no
    // trace at all (SCC-FR-06).
    let matcher = Arc::new(Matcher::compile(&query, mode)?);
    let root = project.require_root().ok();
    let search_id = registry.next_id();
    let stop = registry.install(&search_id);

    let spawned_id = search_id.clone();
    std::thread::spawn(move || {
        let progress_registry = app.state::<ProgressRegistry>();
        // SCC-FR-18: every search attributes, capped and full alike. Registered
        // out here rather than inside the guarded body, because PRG-FR-09 says a
        // failure must never strand an operation as permanently running — and an
        // operation registered *inside* `catch_unwind` would be invisible to the
        // termination on the unwind path, leaving the status bar showing a
        // search that died for the rest of the session.
        let operation = progress::register_and_publish(
            &app,
            &progress_registry,
            "search",
            "Searching…",
            root.as_ref().map(|r| r.to_path_buf()),
            None,
        );
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            search_thread_body(&app, &spawned_id, root, matcher, scope, &stop, operation)
        }));
        let state = match outcome {
            Ok(reason) => reason.operation_state(),
            Err(_) => {
                // SCC-FR-13: exactly one terminal event, whatever the outcome. A
                // panic inside the pipeline must still end the search, or a
                // consumer keying by id waits on an event that never comes.
                app.ended(&SearchEndedPayload {
                    search_id: spawned_id.clone(),
                    reason: EndReason::Failed,
                });
                OperationState::Failed
            }
        };
        progress::terminate_and_publish(&app, &progress_registry, operation, state);
        app.state::<SearchRegistry>().retire(&spawned_id);
    });
    Ok(search_id)
}

/// The spawned half of `start_search`, split out so the panic guard above wraps
/// exactly the work and nothing else. Returns why the search ended; the caller
/// owns registering and terminating the PRG operation, so an unwind out of here
/// still terminates it.
fn search_thread_body<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    search_id: &str,
    root: Option<crate::fs::RootFs>,
    matcher: Arc<Matcher>,
    scope: SearchScope,
    stop: &Stop,
    operation: progress::OperationId,
) -> EndReason {
    let progress_registry = app.state::<ProgressRegistry>();

    let Some(root) = root else {
        // SCC-FR-17: no project open — an id, no hits, and an immediate end.
        app.ended(&SearchEndedPayload {
            search_id: search_id.to_string(),
            reason: EndReason::Completed,
        });
        return EndReason::Completed;
    };

    // SCC-FR-03 / SCC-FR-04: the candidates are the scan's file nodes, served
    // from the mounted list. This module walks no tree of its own.
    let candidates = app
        .state::<CandidateStore>()
        .candidates(&root);
    // PRG-FR-05: the operation becomes determinate once the candidate count is
    // known (SCC-FR-18).
    progress::update_and_publish(
        app,
        &progress_registry,
        operation,
        Some(0),
        Some(candidates.len() as u64),
    );

    run_search(
        app,
        &root,
        candidates,
        matcher,
        scope,
        stop,
        search_id,
        &|consumed| {
            progress::update_and_publish(app, &progress_registry, operation, Some(consumed), None);
        },
    )
}

/// SCC-FR-14: stop a running search. A `search_id` that is already ended is a
/// no-op, not an error.
#[tauri::command]
pub fn cancel_search(search_id: String, registry: State<'_, SearchRegistry>) {
    registry.cancel(&search_id);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
