//! Reading one page of one stream of one run
//! (`GRS-graduation-run-log-storage.md` GRS-FR-RZXA, GRS-FR-GSUY,
//! GRS-FR-OWDT).
//!
//! Three rules shape everything here:
//!
//! * **A read answers for one run, one phase, one scope, and one stream**
//!   (GRS-FR-RZXA). It crosses none of the four.
//! * **A record's scope is its persisted `pass` and nothing else**
//!   (GRS-FR-JXRV). A record whose `pass` is a number belongs to that pass; a
//!   record whose `pass` is null belongs to the run-level scope. Nothing here
//!   assigns a run-level record to a pass.
//! * **A read writes nothing** (GRS-FR-HVUJ). The record is returned as the
//!   file holds it, and the `presentation` beside it is computed for this read.
//!
//! The index segments (GRS-FR-WFWD) bound a scope by sequence, so a read holds
//! the scope rather than the file and stops at the scope's last record instead
//! of walking to the end. They carry no byte offset, so reaching the scope's
//! first record still costs a walk of the lines before it.

use serde::{Deserialize, Serialize};

use crate::fs::FsAccess;

use super::index::{
    GraduationLogFailure, GraduationLogIndexes, GraduationStreamIndex, CODE_STREAM_CORRUPT,
    STATUS_FAILED,
};
use super::records::GraduationLogStream;
use super::writer::LogPaths;

/// GLW-FR-DDXJ: what a record with no pass is shown and searched as.
pub const RUN_LEVEL_MARKER: &str = "run-level";

/// GRS-FR-EYNU: the read-failure code a file that could not be read reports.
pub const CODE_STREAM_UNREADABLE: &str = "log_stream_unreadable";

/// One page holds at most this many records where the caller names no limit.
pub const DEFAULT_LIMIT: u32 = 200;
/// `../../../../specifications/ui/GLW-graduation-log-window.md` GLW-FR-ZPUH is
/// the surface's rule; this is the bound the backend keeps whatever a caller
/// asks for.
pub const MAX_LIMIT: u32 = 1000;

/// GRS contract surface: what a read's pass scope names.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GraduationLogPassScope {
    /// One pass's own records.
    Pass { pass: u32 },
    /// The records whose persisted `pass` is null.
    RunLevel,
    /// Every record the selected phase holds.
    Phase,
}

/// GRS-FR-UEIL: a cursor is valid only for the run and the stream it names.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraduationLogCursor {
    pub run_id: String,
    pub stream: GraduationLogStream,
    /// `after` or `before`.
    pub direction: String,
    pub sequence: u64,
}

pub const DIRECTION_AFTER: &str = "after";
pub const DIRECTION_BEFORE: &str = "before";

/// GRS-FR-HVUJ: computed for one read, and written to neither file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraduationLogPresentation {
    /// True exactly where the persisted `pass` is null.
    pub run_level: bool,
}

/// One record of a page, verbatim, beside what this read computed about it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraduationLogPageEntry {
    /// The persisted record, exactly as the file holds it (GRS-FR-HVUJ).
    pub record: serde_json::Value,
    pub presentation: GraduationLogPresentation,
}

/// GRS-FR-CTQI: exactly one of four.
pub const STATUS_AVAILABLE: &str = "available";
pub const STATUS_EMPTY: &str = "empty";
pub const STATUS_UNAVAILABLE: &str = "unavailable";
pub const STATUS_PERSISTENCE_FAILED: &str = "persistence_failed";

/// GRS contract surface: what a query did.
pub const SEARCH_NOT_REQUESTED: &str = "not_requested";
pub const SEARCH_MATCHED: &str = "matched";
pub const SEARCH_NO_MATCH: &str = "search_no_match";

/// GRS-FR-DYPS: the whole of what one read answers with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraduationLogPage {
    pub run_id: String,
    pub stream: GraduationLogStream,
    pub phase_id: String,
    pub scope: GraduationLogPassScope,
    /// Ascending by `sequence`, whichever cursor produced them.
    pub entries: Vec<GraduationLogPageEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<GraduationLogCursor>,
    /// GRS-FR-OWDT: null exactly where `oldest_reached` is true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub older_cursor: Option<GraduationLogCursor>,
    pub matched_total: u64,
    pub oldest_reached: bool,
    pub latest_sequence: u64,
    pub status: String,
    pub search: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<GraduationLogFailure>,
}

impl GraduationLogPage {
    fn bare(
        run_id: &str,
        stream: GraduationLogStream,
        phase_id: &str,
        scope: &GraduationLogPassScope,
        latest_sequence: u64,
        status: &str,
    ) -> Self {
        Self {
            run_id: run_id.to_string(),
            stream,
            phase_id: phase_id.to_string(),
            scope: scope.clone(),
            entries: Vec::new(),
            next_cursor: None,
            older_cursor: None,
            matched_total: 0,
            oldest_reached: true,
            latest_sequence,
            status: status.to_string(),
            search: SEARCH_NOT_REQUESTED.to_string(),
            failure: None,
        }
    }
}

/// The typed errors of `read_graduation_logs` (GRS contract surface).
pub const ERR_UNKNOWN_PHASE: &str = "unknown_phase";
pub const ERR_UNKNOWN_SCOPE: &str = "unknown_scope";
pub const ERR_CURSOR_NOT_FOR_THIS_SCOPE: &str = "cursor_not_for_this_scope";

/// One decoded line of a stream, with what a read decides from it.
struct Line {
    sequence: u64,
    phase_id: String,
    pass: Option<u32>,
    value: serde_json::Value,
}

/// GRS-FR-KYWE: a line inside the file that does not decode is corruption.
struct Corruption {
    stopped_sequence: u64,
    byte_offset: u64,
}

/// GRS-FR-WFWD: the sequences one scope stands between, from the index alone.
///
/// `None` where the index names no segment of the scope, which is where a read
/// walks the file rather than trust a bound it does not have.
fn scope_bounds(
    index: &GraduationStreamIndex,
    phase_id: &str,
    scope: &GraduationLogPassScope,
) -> Option<(u64, u64)> {
    let mut bounds: Option<(u64, u64)> = None;
    for segment in index.segments.iter().filter(|segment| {
        segment.phase_id == phase_id
            && match scope {
                GraduationLogPassScope::Pass { pass } => segment.pass == Some(*pass),
                GraduationLogPassScope::RunLevel => segment.pass.is_none(),
                GraduationLogPassScope::Phase => true,
            }
    }) {
        bounds = Some(match bounds {
            None => (segment.first_sequence, segment.last_sequence),
            Some((first, last)) => (
                first.min(segment.first_sequence),
                last.max(segment.last_sequence),
            ),
        });
    }
    bounds
}

fn decode(text: &str, bounds: Option<(u64, u64)>) -> (Vec<Line>, Option<Corruption>) {
    let mut lines = Vec::new();
    let mut offset: u64 = 0;
    let mut stopped: u64 = 0;
    for raw in text.split_inclusive('\n') {
        let byte_length = raw.len() as u64;
        let ends_the_line = raw.ends_with('\n');
        let trimmed = raw.trim_end_matches(['\n', '\r']);
        if trimmed.trim().is_empty() {
            offset += byte_length;
            continue;
        }
        match serde_json::from_str::<serde_json::Value>(trimmed) {
            Ok(value) => {
                let sequence = value.get("sequence").and_then(|v| v.as_u64()).unwrap_or(0);
                let phase_id = value
                    .get("phase_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let pass = value
                    .get("pass")
                    .and_then(|v| v.as_u64())
                    .map(|pass| pass as u32);
                stopped = sequence;
                match bounds {
                    // The scope's last record is already held, so the rest of
                    // the file is nothing this read answers for.
                    Some((_, last)) if sequence > last => return (lines, None),
                    Some((first, _)) if sequence < first => {}
                    _ => lines.push(Line {
                        sequence,
                        phase_id,
                        pass,
                        value,
                    }),
                }
                offset += byte_length;
            }
            // GRS-FR-MQEQ: a partial **final** line is the tail of a write the
            // process did not finish. It was never acknowledged, the next
            // append repairs it, and it is not a damaged file.
            Err(_) if !ends_the_line => return (lines, None),
            Err(_) => {
                return (
                    lines,
                    Some(Corruption {
                        stopped_sequence: stopped,
                        byte_offset: offset,
                    }),
                )
            }
        }
    }
    (lines, None)
}

/// GRS-FR-TQAO / GRS-FR-NKZP: the phase narrows every scope, and the scope is
/// the persisted `pass` and nothing else.
fn in_scope(line: &Line, phase_id: &str, scope: &GraduationLogPassScope) -> bool {
    if line.phase_id != phase_id {
        return false;
    }
    match scope {
        GraduationLogPassScope::Pass { pass } => line.pass == Some(*pass),
        GraduationLogPassScope::RunLevel => line.pass.is_none(),
        GraduationLogPassScope::Phase => true,
    }
}

/// GRS-FR-OYIC: what a search matches is exactly what the surface displays.
fn searchable(value: &serde_json::Value, stream: GraduationLogStream) -> String {
    let mut haystack = String::new();
    let mut push = |text: &str| {
        haystack.push_str(text);
        haystack.push('\n');
    };
    for key in ["run_id", "phase_id", "origin", "producer", "at"] {
        if let Some(text) = value.get(key).and_then(|v| v.as_str()) {
            push(text);
        }
    }
    if let Some(sequence) = value.get("sequence") {
        push(&sequence.to_string());
    }
    match value.get("pass").and_then(|v| v.as_u64()) {
        Some(pass) => push(&pass.to_string()),
        // GRS-FR-OYIC: what is searched and what is shown are one set, and a
        // record with no pass is shown with the run-level marker of
        // `../../../../specifications/ui/GLW-graduation-log-window.md`
        // GLW-FR-DDXJ rather than with a number.
        None => push(RUN_LEVEL_MARKER),
    }
    match stream {
        GraduationLogStream::Source => {
            for key in ["agent", "container", "source"] {
                if let Some(text) = value.get(key).and_then(|v| v.as_str()) {
                    push(text);
                }
            }
            // GRS-FR-QVTA: what is searched is what is decoded and shown.
            if let Some(encoded) = value.get("data_base64").and_then(|v| v.as_str()) {
                use base64::Engine as _;
                if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded) {
                    push(&String::from_utf8_lossy(&bytes));
                }
            }
        }
        GraduationLogStream::Structured => {
            for key in ["level", "event"] {
                if let Some(text) = value.get(key).and_then(|v| v.as_str()) {
                    push(text);
                }
            }
            if let Some(fields) = value.get("fields").and_then(|v| v.as_object()) {
                for (key, field) in fields {
                    push(key);
                    match field.as_str() {
                        Some(text) => push(text),
                        None => push(&field.to_string()),
                    }
                }
            }
        }
    }
    haystack.to_lowercase()
}

/// GRS-FR-RZXA: one page of one stream for one run, phase, and scope.
///
/// `indexes` is the run record's own, which is what says how far the stream
/// stands and whether a write of it failed. Nothing here writes either file.
pub fn read_page(
    fs: &FsAccess,
    paths: &LogPaths,
    indexes: &GraduationLogIndexes,
    run_id: &str,
    phase_id: &str,
    scope: &GraduationLogPassScope,
    stream: GraduationLogStream,
    cursor: Option<GraduationLogCursor>,
    limit: Option<u32>,
    query: Option<String>,
) -> Result<GraduationLogPage, String> {
    // GRS-FR-UEIL: a cursor answers for the run and the stream it names.
    if let Some(cursor) = cursor.as_ref() {
        if cursor.run_id != run_id
            || cursor.stream != stream
            || (cursor.direction != DIRECTION_AFTER && cursor.direction != DIRECTION_BEFORE)
        {
            return Err(ERR_CURSOR_NOT_FOR_THIS_SCOPE.to_string());
        }
    }

    let latest_sequence = indexes.stream(stream).latest_sequence;

    // GRS-FR-MBED: a version this build does not recognise renders no log,
    // which is neither a read failure nor a reason to refuse the run.
    if indexes.log_storage_version != 1 {
        return Ok(GraduationLogPage::bare(
            run_id,
            stream,
            phase_id,
            scope,
            latest_sequence,
            STATUS_EMPTY,
        ));
    }

    // GRS-FR-THZA: a failed write of this stream is reported as such, so a
    // surface says that output stopped rather than that a run produced none.
    if indexes.persistence.status == STATUS_FAILED {
        if let Some(failure) = indexes
            .persistence
            .failure
            .as_ref()
            .filter(|failure| failure.stream == stream)
        {
            let mut page = GraduationLogPage::bare(
                run_id,
                stream,
                phase_id,
                scope,
                latest_sequence,
                STATUS_PERSISTENCE_FAILED,
            );
            page.failure = Some(failure.clone());
            return Ok(page);
        }
    }

    let path = paths.stream(stream);
    let text = match fs.read_text(&path) {
        Ok(text) => text,
        Err(error) => {
            // GRS-FR-TYQK: a run this module holds no storage for reads back
            // empty rather than as an error.
            if fs.file_info(&path).is_err() {
                return Ok(GraduationLogPage::bare(
                    run_id,
                    stream,
                    phase_id,
                    scope,
                    latest_sequence,
                    STATUS_EMPTY,
                ));
            }
            let mut page = GraduationLogPage::bare(
                run_id,
                stream,
                phase_id,
                scope,
                latest_sequence,
                STATUS_UNAVAILABLE,
            );
            page.failure = Some(read_failure(
                CODE_STREAM_UNREADABLE,
                stream,
                format!("the {} stream could not be read: {error}", stream.as_str()),
                0,
                0,
            ));
            return Ok(page);
        }
    };

    let bounds = scope_bounds(indexes.stream(stream), phase_id, scope);
    let (lines, corruption) = decode(&text, bounds);
    if let Some(corruption) = corruption {
        // GRS-FR-KYWE: the read reports the damage and the sequence it got as
        // far as, and this module rewrites nothing and removes nothing.
        let mut page = GraduationLogPage::bare(
            run_id,
            stream,
            phase_id,
            scope,
            latest_sequence,
            STATUS_UNAVAILABLE,
        );
        page.failure = Some(read_failure(
            CODE_STREAM_CORRUPT,
            stream,
            format!(
                "the {} stream holds a line that could not be decoded; the run itself is unaffected",
                stream.as_str()
            ),
            corruption.stopped_sequence,
            corruption.byte_offset,
        ));
        return Ok(page);
    }

    let scoped: Vec<&Line> = lines
        .iter()
        .filter(|line| in_scope(line, phase_id, scope))
        .collect();

    // GRS-FR-NSGX: the scope's own records settle the status, and the query
    // carries its own result beside it.
    let status = if scoped.is_empty() {
        STATUS_EMPTY
    } else {
        STATUS_AVAILABLE
    };

    let needle = query
        .map(|query| query.trim().to_lowercase())
        .filter(|query| !query.is_empty());
    let matched: Vec<&Line> = match needle.as_ref() {
        None => scoped,
        Some(needle) => scoped
            .into_iter()
            .filter(|line| searchable(&line.value, stream).contains(needle))
            .collect(),
    };

    let search = match needle {
        None => SEARCH_NOT_REQUESTED,
        Some(_) if matched.is_empty() => SEARCH_NO_MATCH,
        Some(_) => SEARCH_MATCHED,
    };

    let limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT) as usize;
    // GRS-FR-GSUY: an absent cursor returns the newest page, an `after` cursor
    // the records later than it, and a `before` cursor the page preceding it.
    // Every one of the three is returned in ascending sequence order.
    let window: Vec<&Line> = match cursor.as_ref() {
        None => {
            let mut newest: Vec<&Line> = matched.iter().rev().take(limit).copied().collect();
            newest.reverse();
            newest
        }
        Some(cursor) if cursor.direction == DIRECTION_AFTER => matched
            .iter()
            .filter(|line| line.sequence > cursor.sequence)
            .take(limit)
            .copied()
            .collect(),
        Some(cursor) => {
            let mut older: Vec<&Line> = matched
                .iter()
                .filter(|line| line.sequence < cursor.sequence)
                .rev()
                .take(limit)
                .copied()
                .collect();
            older.reverse();
            older
        }
    };

    let oldest_reached = match (matched.first(), window.first()) {
        (Some(oldest), Some(first)) => first.sequence <= oldest.sequence,
        // GRS-FR-OWDT: an empty page holds the scope's oldest record only where
        // the scope holds none either.
        (Some(_), None) => false,
        _ => true,
    };
    let newest_reached = match (matched.last(), window.last()) {
        (Some(newest), Some(last)) => last.sequence >= newest.sequence,
        (Some(_), None) => false,
        _ => true,
    };

    // GRS-FR-OWDT: `older_cursor` is null exactly where `oldest_reached` is
    // true, so a caller that opened on the newest page walks backwards without
    // guessing a sequence.
    let older_sequence = match window.first() {
        Some(first) => Some(first.sequence),
        // An empty `after` page stands past the newest record, so the page
        // before it begins one past the cursor the caller gave.
        None => cursor.as_ref().map(|cursor| cursor.sequence + 1),
    };
    let older_cursor = older_sequence
        .filter(|_| !oldest_reached)
        .map(|sequence| GraduationLogCursor {
            run_id: run_id.to_string(),
            stream,
            direction: DIRECTION_BEFORE.to_string(),
            sequence,
        });
    let asked_before = cursor
        .as_ref()
        .is_some_and(|cursor| cursor.direction == DIRECTION_BEFORE);
    let next_cursor = if asked_before {
        older_cursor.clone()
    } else {
        window
            .last()
            .filter(|_| !newest_reached)
            .map(|last| GraduationLogCursor {
                run_id: run_id.to_string(),
                stream,
                direction: DIRECTION_AFTER.to_string(),
                sequence: last.sequence,
            })
    };

    Ok(GraduationLogPage {
        run_id: run_id.to_string(),
        stream,
        phase_id: phase_id.to_string(),
        scope: scope.clone(),
        entries: window
            .iter()
            .map(|line| GraduationLogPageEntry {
                record: line.value.clone(),
                presentation: GraduationLogPresentation {
                    run_level: line.pass.is_none(),
                },
            })
            .collect(),
        next_cursor,
        older_cursor,
        matched_total: matched.len() as u64,
        oldest_reached,
        latest_sequence,
        status: status.to_string(),
        search: search.to_string(),
        failure: None,
    })
}

/// GRS-FR-EYNU: a read failure is typed, carries how far the read got, and is
/// never a persistence failure.
fn read_failure(
    code: &str,
    stream: GraduationLogStream,
    message: String,
    stopped_sequence: u64,
    byte_offset: u64,
) -> GraduationLogFailure {
    GraduationLogFailure {
        kind: "read".to_string(),
        code: code.to_string(),
        stream,
        message,
        at: crate::notes::now_rfc3339(),
        stopped_sequence: Some(stopped_sequence),
        byte_offset: Some(byte_offset),
        pending_record_ids: Vec::new(),
    }
}
