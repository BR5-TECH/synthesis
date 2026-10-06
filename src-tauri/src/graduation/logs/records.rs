//! What one line of a run's two log streams holds
//! (`GRS-graduation-run-log-storage.md` GRS-FR-MEPZ, GRS-FR-JAPO, GRS-FR-SQGZ,
//! GRS-FR-NPIB).
//!
//! A record is attributed by **what produced it** and by nothing else
//! (GRS-FR-KJVN): the producer settles the phase and the pass, so no reader has
//! to infer either from the text of a record or from its position.

use serde::{Deserialize, Serialize};

/// GRS-FR-MABD: the two streams a run owns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraduationLogStream {
    /// The safe normalized agent activity of the turns (GRS-FR-JAPO).
    Activity,
    /// Structured graduation observability records.
    Structured,
}

impl GraduationLogStream {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Activity => "activity",
            Self::Structured => "structured",
        }
    }

    /// The file this stream is written to, inside the run's own log directory.
    pub fn file_name(self) -> &'static str {
        match self {
            Self::Activity => "activity.jsonl",
            Self::Structured => "structured.jsonl",
        }
    }
}

/// GRS contract surface: what kind of thing wrote a record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraduationLogOrigin {
    Agent,
    Executor,
    Application,
}

/// GRS-FR-NPIB: what a reader should do about a structured record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraduationLogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// GRS-FR-KJVN: every producer this module accepts a record from.
///
/// A producer the attribution table does not name is one no record may be
/// written under, which is what makes the phase of every line answerable
/// without reading it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraduationLogProducer {
    QueueWait,
    WorkTurn,
    ClarificationJudgement,
    ReviewTurn,
    Commit,
    SemanticMergeTurn,
    StageTransition,
}

impl GraduationLogProducer {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::QueueWait => "queue_wait",
            Self::WorkTurn => "work_turn",
            Self::ClarificationJudgement => "clarification_judgement",
            Self::ReviewTurn => "review_turn",
            Self::Commit => "commit",
            Self::SemanticMergeTurn => "semantic_merge_turn",
            Self::StageTransition => "stage_transition",
        }
    }

    /// GRS-FR-KJVN / GRS-FR-XUOA: the phase a record of this producer carries.
    ///
    /// A stage transition carries the phase being **left** (GRS-FR-EPPM), which
    /// only the caller knows, so it supplies one instead.
    pub fn phase_id(
        self,
        leaving: Option<super::super::GraduationVisualStage>,
    ) -> super::super::GraduationVisualStage {
        use super::super::GraduationVisualStage as V;
        match self {
            Self::QueueWait => V::Queued,
            Self::WorkTurn | Self::ClarificationJudgement => V::Working,
            Self::ReviewTurn => V::Review,
            Self::Commit | Self::SemanticMergeTurn => V::Done,
            Self::StageTransition => leaving.unwrap_or(V::Queued),
        }
    }

    /// GRS-FR-URSZ / GRS-FR-ZQEM: whether a record of this producer belongs to
    /// one pass, or to the run as a whole.
    pub fn carries_pass(self) -> bool {
        matches!(
            self,
            Self::WorkTurn | Self::ClarificationJudgement | Self::ReviewTurn
        )
    }

    /// GRS-FR-NUXT: a run-level record names a non-agent source.
    pub fn origin(self) -> GraduationLogOrigin {
        match self {
            Self::WorkTurn | Self::ClarificationJudgement | Self::ReviewTurn => {
                GraduationLogOrigin::Agent
            }
            Self::SemanticMergeTurn => GraduationLogOrigin::Agent,
            _ => GraduationLogOrigin::Application,
        }
    }
}

impl GraduationLogProducer {
    /// GRS-FR-RDJP: whether a producer is an agent turn, which is the only
    /// kind of producer that writes an activity record.
    pub fn writes_activity(self) -> bool {
        matches!(
            self,
            Self::WorkTurn
                | Self::ClarificationJudgement
                | Self::ReviewTurn
                | Self::SemanticMergeTurn
        )
    }

    /// GRS-FR-RDJP / GRS-FR-NUXT: the origin of one activity record. The
    /// executor's own `finished` activity and every run-level record are the
    /// executor's. The rest is the agent's.
    pub fn activity_origin(self, pass: Option<u32>, channel: &str) -> GraduationLogOrigin {
        if pass.is_none() || channel == "executor" {
            GraduationLogOrigin::Executor
        } else {
            GraduationLogOrigin::Agent
        }
    }
}

/// The attribution every record of either stream carries (GRS-FR-MEPZ).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GraduationLogAttribution {
    pub schema_version: u32,
    /// GRS-FR-HZRP: assigned once by the producer and unique within the run.
    pub record_id: String,
    /// GRS-FR-SXNY: assigned by this module, ascending within one run and one
    /// stream.
    pub sequence: u64,
    /// RFC 3339 UTC.
    pub at: String,
    pub run_id: String,
    pub phase_id: String,
    /// The pass, or `null` for a run-level record.
    pub pass: Option<u32>,
    pub origin: GraduationLogOrigin,
    pub producer: String,
}

/// GRS-FR-NPIB: one structured observability record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GraduationStructuredRecord {
    #[serde(flatten)]
    pub attribution: GraduationLogAttribution,
    pub level: GraduationLogLevel,
    /// A short stable name for what happened.
    pub event: String,
    /// A flat map of the values a surface renders. It may be empty.
    pub fields: serde_json::Map<String, serde_json::Value>,
}

/// GRS-FR-JAPO / GRS-FR-SQGZ: one safe activity of an agent turn.
///
/// It has no payload field, no task field, no reasoning field, and no raw
/// output field. A reader can show nothing of a turn but a kind and one masked
/// line, because nothing else was ever written.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GraduationActivityRecord {
    #[serde(flatten)]
    pub attribution: GraduationLogAttribution,
    /// `stdout`, `stderr`, or `executor`.
    pub channel: String,
    /// One of the safe kinds of GRS-FR-WJIA.
    pub kind: String,
    /// One masked line, bounded by the executor.
    pub summary: String,
}

/// One record of either stream, before a sequence is assigned to it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "stream", rename_all = "snake_case")]
pub enum GraduationLogRecord {
    Structured(GraduationStructuredRecord),
    Activity(GraduationActivityRecord),
}

impl GraduationLogRecord {
    pub fn stream(&self) -> GraduationLogStream {
        match self {
            Self::Structured(_) => GraduationLogStream::Structured,
            Self::Activity(_) => GraduationLogStream::Activity,
        }
    }

    pub fn attribution(&self) -> &GraduationLogAttribution {
        match self {
            Self::Structured(record) => &record.attribution,
            Self::Activity(record) => &record.attribution,
        }
    }

    pub(super) fn attribution_mut(&mut self) -> &mut GraduationLogAttribution {
        match self {
            Self::Structured(record) => &mut record.attribution,
            Self::Activity(record) => &mut record.attribution,
        }
    }

    pub fn record_id(&self) -> &str {
        &self.attribution().record_id
    }

    /// The line this record is written as. The stream tag is not part of it:
    /// the file the line stands in is what says which stream it belongs to.
    pub fn to_line(&self) -> Result<String, String> {
        let value = match self {
            Self::Structured(record) => serde_json::to_value(record),
            Self::Activity(record) => serde_json::to_value(record),
        };
        value.map_err(|e| e.to_string()).map(|v| v.to_string())
    }
}

/// The `record_id` a line holds, where it is a record this module wrote.
///
/// Used by the replay to decide what the file already carries (GRS-FR-KQHY),
/// which is why it reads one field rather than the whole record: a line the
/// running build cannot fully decode still names itself.
pub fn record_id_of_line(line: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(line)
        .ok()?
        .get("record_id")?
        .as_str()
        .map(str::to_string)
}
