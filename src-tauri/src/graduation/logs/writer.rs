//! Appending to a run's two log streams, durably
//! (`GRS-graduation-run-log-storage.md` GRS-FR-RGPN, GRS-FR-CYAP,
//! GRS-FR-KQHY, GRS-FR-MQEQ).
//!
//! Three rules shape everything here:
//!
//! * **A producer told a record was written may rely on it** (GRS-FR-RGPN). The
//!   append flushes and syncs before it answers, and the index is written in the
//!   same durable act as the records it names (GRS-FR-MHJM).
//! * **Nothing is dropped, truncated, evicted, or silently acknowledged**
//!   (GRS-FR-CYAP). A write that fails answers a failure and keeps the records
//!   in a durable pending queue of its own.
//! * **A retry appends no record twice** (GRS-FR-MXDJ). The replay reads back
//!   which `record_id`s the file already holds and appends only the rest.

use std::path::{Path, PathBuf};

use crate::fs::{self as fsa, FsAccess};

use super::index::{
    GraduationLogFailure, GraduationLogIndexes, GraduationStreamIndex, CODE_APPEND_FAILED,
    CODE_STREAM_CREATE_FAILED,
};
use super::records::{record_id_of_line, GraduationLogRecord, GraduationLogStream};

/// Where the pending records of a failed write are kept.
///
/// GRS-FR-CYAP allows a durable pending-write queue, and this is it. The run
/// record names those records by id alone (GRS-FR-DDSB); their content lives
/// here, because `run.toml` holds no log payload (GRS-FR-CGSP).
const PENDING_FILE: &str = "pending.jsonl";

/// The two streams and the pending queue of one run.
#[derive(Clone, Debug)]
pub struct LogPaths {
    pub directory: PathBuf,
}

impl LogPaths {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    pub fn stream(&self, stream: GraduationLogStream) -> PathBuf {
        self.directory.join(stream.file_name())
    }

    pub fn pending(&self) -> PathBuf {
        self.directory.join(PENDING_FILE)
    }
}

/// GRS-FR-KDOY: create both streams, so a run that has emitted nothing still
/// has two readable ones.
pub fn initialize(fs: &FsAccess, paths: &LogPaths) -> Result<(), GraduationLogFailure> {
    if !paths.directory.is_dir() {
        fs.create_dir(&paths.directory).map_err(|error| {
            GraduationLogFailure::write(
                CODE_STREAM_CREATE_FAILED,
                GraduationLogStream::Structured,
                format!("the run's log directory could not be created: {error}"),
                Vec::new(),
            )
        })?;
    }
    for stream in [GraduationLogStream::Source, GraduationLogStream::Structured] {
        let path = paths.stream(stream);
        if path.is_file() {
            continue;
        }
        fs.create_file(&path, fsa::CreateMode::Exclusive)
            .map_err(|error| {
                GraduationLogFailure::write(
                    CODE_STREAM_CREATE_FAILED,
                    stream,
                    format!("the run's {} stream could not be created: {error}", stream.as_str()),
                    Vec::new(),
                )
            })?;
    }
    Ok(())
}

/// GRS-FR-RGPN / GRS-FR-MHJM: append `records` to their stream and update the
/// index, or answer what stopped it.
///
/// Every record must belong to one stream. The caller groups them, because a
/// single append is what makes one flush answer for all of them.
pub fn append(
    fs: &FsAccess,
    paths: &LogPaths,
    indexes: &mut GraduationLogIndexes,
    stream: GraduationLogStream,
    mut records: Vec<GraduationLogRecord>,
) -> Result<(), GraduationLogFailure> {
    if records.is_empty() {
        return Ok(());
    }
    let path = paths.stream(stream);
    // GRS-FR-MQEQ: the tail of a write the process did not finish is repaired
    // before anything further is appended. That record was never acknowledged,
    // so no producer believes it was stored.
    repair_partial_tail(fs, &path, indexes, stream)?;

    // GRS-FR-SXNY: the sequence ascends within one run and one stream, and is
    // assigned here rather than by the producer.
    let mut next = indexes.stream(stream).latest_sequence;
    let mut lines: Vec<String> = Vec::with_capacity(records.len());
    let mut scopes: Vec<(String, Option<u32>, u64)> = Vec::with_capacity(records.len());
    let mut refused: Option<String> = None;
    for record in records.iter_mut() {
        next += 1;
        record.attribution_mut().sequence = next;
        match record.to_line() {
            Ok(line) => {
                let attribution = record.attribution();
                scopes.push((attribution.phase_id.clone(), attribution.pass, next));
                lines.push(line);
            }
            Err(error) => {
                refused = Some(error);
                break;
            }
        }
    }
    if let Some(error) = refused {
        return Err(GraduationLogFailure::write(
            CODE_APPEND_FAILED,
            stream,
            format!("a record could not be serialized: {error}"),
            ids_of(&records),
        ));
    }

    let written: u64 = lines.iter().map(|line| line.len() as u64 + 1).sum();
    if let Err(error) = fs.append_lines(&path, &lines) {
        let ids = ids_of(&records);
        // The records are kept where a retry can find them, and the caller is
        // told the write did not happen.
        let _ = queue_pending(fs, paths, &records);
        return Err(GraduationLogFailure::write(
            CODE_APPEND_FAILED,
            stream,
            format!("the run's {} stream could not be appended to: {error}", stream.as_str()),
            ids,
        ));
    }

    // The append flushed and synced, so the index is now describing records the
    // file durably holds.
    let index = indexes.stream_mut(stream);
    for (phase_id, pass, sequence) in scopes {
        index.note(&phase_id, pass, sequence);
    }
    index.durable_through_sequence = index.latest_sequence;
    index.byte_length += written;
    Ok(())
}

/// GRS-FR-MHJM: bring an index up to what its file actually holds.
///
/// The run record is saved at turn boundaries, and a turn may run for hours. A
/// process killed inside one leaves the record naming fewer records than the
/// file durably holds — and a later append seeded from that record would
/// **reuse sequence numbers already written**, which GRS-FR-SXNY forbids. So
/// the file is the authority whenever the two disagree.
///
/// The check is one `byte_length` comparison, and the walk happens only when it
/// fails: an index that already matches its file costs nothing.
pub fn reconcile(fs: &FsAccess, paths: &LogPaths, indexes: &mut GraduationLogIndexes) {
    for stream in [GraduationLogStream::Structured, GraduationLogStream::Source] {
        let path = paths.stream(stream);
        let Ok(text) = fs.read_text(&path) else {
            continue;
        };
        if text.len() as u64 == indexes.stream(stream).byte_length {
            continue;
        }
        let mut rebuilt = GraduationStreamIndex::new(stream);
        let mut durable = 0u64;
        for line in text.lines() {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            let sequence = value.get("sequence").and_then(|v| v.as_u64()).unwrap_or(0);
            let phase = value
                .get("phase_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let pass = value
                .get("pass")
                .and_then(|v| v.as_u64())
                .map(|pass| pass as u32);
            rebuilt.note(&phase, pass, sequence.max(durable + 1));
            durable = rebuilt.latest_sequence;
        }
        // A trailing partial line is not a record; the byte length is the whole
        // file either way, and the next append cuts the partial tail.
        rebuilt.durable_through_sequence = rebuilt.latest_sequence;
        rebuilt.byte_length = text.len() as u64;
        *indexes.stream_mut(stream) = rebuilt;
    }
}

/// GRS-FR-KQHY / GRS-FR-MXDJ: replay whatever a failed write left, idempotently.
///
/// The file is read back for the `record_id`s it already holds; every pending
/// record among them is discarded, and the rest are appended in their original
/// order. Answers how many records were still pending afterwards.
pub fn replay_pending(
    fs: &FsAccess,
    paths: &LogPaths,
    indexes: &mut GraduationLogIndexes,
) -> Result<u32, GraduationLogFailure> {
    let pending = read_pending(fs, paths);
    if pending.is_empty() {
        clear_pending(fs, paths);
        return Ok(0);
    }
    for stream in [GraduationLogStream::Structured, GraduationLogStream::Source] {
        let held = record_ids_in(fs, &paths.stream(stream));
        let outstanding: Vec<GraduationLogRecord> = pending
            .iter()
            .filter(|record| record.stream() == stream)
            .filter(|record| !held.iter().any(|id| id == record.record_id()))
            .cloned()
            .collect();
        if outstanding.is_empty() {
            continue;
        }
        append(fs, paths, indexes, stream, outstanding)?;
    }
    clear_pending(fs, paths);
    Ok(0)
}

/// GRS-FR-MQEQ: cut a file back to its last complete record.
///
/// A partial final line is the tail of a write the process did not finish. It
/// was never acknowledged, so removing it loses nothing a producer was told was
/// stored, and the pending queue is what restores it.
fn repair_partial_tail(
    fs: &FsAccess,
    path: &Path,
    indexes: &mut GraduationLogIndexes,
    stream: GraduationLogStream,
) -> Result<(), GraduationLogFailure> {
    let Ok(bytes) = fs.read_bytes(path) else {
        // A stream that is not there yet is created by the append itself.
        return Ok(());
    };
    if bytes.is_empty() || bytes.last() == Some(&b'\n') {
        // The index's byte length is authoritative only for what this process
        // wrote; a file it opened afresh is measured here.
        indexes.stream_mut(stream).byte_length = bytes.len() as u64;
        return Ok(());
    }
    let cut = match bytes.iter().rposition(|byte| *byte == b'\n') {
        Some(last) => last + 1,
        None => 0,
    };
    fs.write_bytes_atomic(path, &bytes[..cut]).map_err(|error| {
        GraduationLogFailure::write(
            CODE_APPEND_FAILED,
            stream,
            format!("a partial record could not be cut from the {} stream: {error}", stream.as_str()),
            Vec::new(),
        )
    })?;
    indexes.stream_mut(stream).byte_length = cut as u64;
    Ok(())
}

/// Every `record_id` a stream's file already holds.
fn record_ids_in(fs: &FsAccess, path: &Path) -> Vec<String> {
    fs.read_text(path)
        .unwrap_or_default()
        .lines()
        .filter_map(record_id_of_line)
        .collect()
}

/// Keep records a write could not store, where a retry can find them.
fn queue_pending(
    fs: &FsAccess,
    paths: &LogPaths,
    records: &[GraduationLogRecord],
) -> Result<(), String> {
    let queued: Vec<String> = read_pending(fs, paths)
        .iter()
        .map(|record| record.record_id().to_string())
        .collect();
    let lines: Vec<String> = records
        .iter()
        .filter(|record| !queued.iter().any(|id| id == record.record_id()))
        .filter_map(|record| serde_json::to_string(record).ok())
        .collect();
    fs.append_lines(paths.pending(), &lines)
        .map_err(|error| error.to_string())
}

/// What a failed write left to be retried, in append order and once each.
///
/// A retry that fails queues its records again, so the same `record_id` can
/// stand in the queue more than once. GRS-FR-MXDJ is that a retry appends no
/// record twice, and the first occurrence is the one that keeps the order.
fn read_pending(fs: &FsAccess, paths: &LogPaths) -> Vec<GraduationLogRecord> {
    let mut seen: Vec<String> = Vec::new();
    let mut out: Vec<GraduationLogRecord> = Vec::new();
    for line in fs.read_text(paths.pending()).unwrap_or_default().lines() {
        let Ok(record) = serde_json::from_str::<GraduationLogRecord>(line) else {
            continue;
        };
        if seen.iter().any(|id| id == record.record_id()) {
            continue;
        }
        seen.push(record.record_id().to_string());
        out.push(record);
    }
    out
}

fn clear_pending(fs: &FsAccess, paths: &LogPaths) {
    let path = paths.pending();
    if path.exists() {
        let _ = fs.delete_path(&path, false);
    }
}

/// The ids a failure names, in append order (GRS-FR-DDSB).
fn ids_of(records: &[GraduationLogRecord]) -> Vec<String> {
    records
        .iter()
        .map(|record| record.record_id().to_string())
        .collect()
}
