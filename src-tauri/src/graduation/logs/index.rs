//! The indexes and the persistence status a run record holds about its two log
//! streams (`GRS-graduation-run-log-storage.md` GRS-FR-CGSP, GRS-FR-WFWD).
//!
//! **No log payload lives here**: no chunk, no decoded text, no structured
//! field value, and no excerpt of any of them. What the run record carries is
//! how far each stream is durable, what scopes it holds records for, and
//! whether the last write succeeded.

use serde::{Deserialize, Serialize};

use super::records::GraduationLogStream;

/// GRS-FR-WFWD: one scope a stream holds records for.
///
/// A read resolves its scope from these rather than by scanning the whole file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraduationLogSegment {
    pub phase_id: String,
    pub pass: Option<u32>,
    pub first_sequence: u64,
    pub last_sequence: u64,
    pub record_count: u64,
}

/// GRS-FR-WFWD: how far one stream stands, and what it holds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraduationStreamIndex {
    pub stream: GraduationLogStream,
    /// 0 for a stream with no record.
    pub latest_sequence: u64,
    pub record_count: u64,
    /// The newest sequence the file holds durably.
    pub durable_through_sequence: u64,
    /// The durable length of the file, in bytes.
    pub byte_length: u64,
    /// Append-only, in first-sequence order.
    #[serde(default)]
    pub segments: Vec<GraduationLogSegment>,
}

impl GraduationStreamIndex {
    pub fn new(stream: GraduationLogStream) -> Self {
        Self {
            stream,
            latest_sequence: 0,
            record_count: 0,
            durable_through_sequence: 0,
            byte_length: 0,
            segments: Vec::new(),
        }
    }

    /// Record that one line of `phase_id` and `pass` now stands at `sequence`.
    ///
    /// A record of the scope the newest segment already names extends it. Any
    /// other record opens a segment of its own, so the list stays append-only
    /// and in first-sequence order.
    pub fn note(&mut self, phase_id: &str, pass: Option<u32>, sequence: u64) {
        match self.segments.last_mut() {
            Some(open) if open.phase_id == phase_id && open.pass == pass => {
                open.last_sequence = sequence;
                open.record_count += 1;
            }
            _ => self.segments.push(GraduationLogSegment {
                phase_id: phase_id.to_string(),
                pass,
                first_sequence: sequence,
                last_sequence: sequence,
                record_count: 1,
            }),
        }
        self.latest_sequence = sequence;
        self.record_count += 1;
    }
}

/// GRS-FR-EYNU: what failed, and the act that clears it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraduationLogFailure {
    /// `write` or `read`: which side of the stream failed.
    pub kind: String,
    pub code: String,
    pub stream: GraduationLogStream,
    /// One sentence naming the act that clears it.
    pub message: String,
    /// RFC 3339 UTC.
    pub at: String,
    /// Read failures: the newest sequence read whole before the failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stopped_sequence: Option<u64>,
    /// Read failures: where in the file decoding stopped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_offset: Option<u64>,
    /// GRS-FR-DDSB: write failures — the records still to be written, in append
    /// order, named by their ids alone.
    #[serde(default)]
    pub pending_record_ids: Vec<String>,
}

/// The write-failure codes of GRS-FR-EYNU.
pub const CODE_APPEND_FAILED: &str = "log_append_failed";
pub const CODE_STREAM_CREATE_FAILED: &str = "log_stream_create_failed";
pub const CODE_STORAGE_UNAVAILABLE: &str = "log_storage_unavailable";
/// The read-failure code a line that does not decode reports (GRS-FR-KYWE).
pub const CODE_STREAM_CORRUPT: &str = "log_stream_corrupt";

impl GraduationLogFailure {
    /// A write failure, carrying what is still to be written.
    pub fn write(
        code: &str,
        stream: GraduationLogStream,
        message: impl Into<String>,
        pending_record_ids: Vec<String>,
    ) -> Self {
        Self {
            kind: "write".to_string(),
            code: code.to_string(),
            stream,
            message: message.into(),
            at: crate::notes::now_rfc3339(),
            stopped_sequence: None,
            byte_offset: None,
            pending_record_ids,
        }
    }
}

/// GRS-FR-CGSP: whether the streams are being written.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraduationLogPersistence {
    /// `healthy` or `failed`.
    pub status: String,
    /// `null` while the status is `healthy`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<GraduationLogFailure>,
    /// Records accepted from a producer and not yet durable.
    pub pending_count: u32,
    /// RFC 3339 UTC.
    pub updated_at: String,
}

pub const STATUS_HEALTHY: &str = "healthy";
pub const STATUS_FAILED: &str = "failed";

impl Default for GraduationLogPersistence {
    fn default() -> Self {
        Self {
            status: STATUS_HEALTHY.to_string(),
            failure: None,
            pending_count: 0,
            updated_at: crate::notes::now_rfc3339(),
        }
    }
}

impl GraduationLogPersistence {
    pub fn is_healthy(&self) -> bool {
        self.status == STATUS_HEALTHY
    }
}

/// GRS-FR-CGSP: the whole of what a run record holds about its logs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraduationLogIndexes {
    /// A reader that does not recognise a version renders none of it.
    pub log_storage_version: u32,
    pub source: GraduationStreamIndex,
    pub structured: GraduationStreamIndex,
    pub persistence: GraduationLogPersistence,
    /// GRS-FR-EYNU: retained from the most recent read that met corruption. It
    /// is a record of a damaged file and never a persistence status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_read_failure: Option<GraduationLogFailure>,
}

impl Default for GraduationLogIndexes {
    fn default() -> Self {
        Self {
            log_storage_version: 1,
            source: GraduationStreamIndex::new(GraduationLogStream::Source),
            structured: GraduationStreamIndex::new(GraduationLogStream::Structured),
            persistence: GraduationLogPersistence::default(),
            last_read_failure: None,
        }
    }
}

impl GraduationLogIndexes {
    pub fn stream(&self, stream: GraduationLogStream) -> &GraduationStreamIndex {
        match stream {
            GraduationLogStream::Source => &self.source,
            GraduationLogStream::Structured => &self.structured,
        }
    }

    pub fn stream_mut(&mut self, stream: GraduationLogStream) -> &mut GraduationStreamIndex {
        match stream {
            GraduationLogStream::Source => &mut self.source,
            GraduationLogStream::Structured => &mut self.structured,
        }
    }

    /// The write succeeded, so the streams are healthy again.
    pub fn note_healthy(&mut self, pending_count: u32) {
        self.persistence = GraduationLogPersistence {
            status: STATUS_HEALTHY.to_string(),
            failure: None,
            pending_count,
            updated_at: crate::notes::now_rfc3339(),
        };
    }

    /// GRS-FR-DDSB: the write failed, and this is what it left to retry.
    pub fn note_failed(&mut self, failure: GraduationLogFailure, pending_count: u32) {
        self.persistence = GraduationLogPersistence {
            status: STATUS_FAILED.to_string(),
            failure: Some(failure),
            pending_count,
            updated_at: crate::notes::now_rfc3339(),
        };
    }
}
