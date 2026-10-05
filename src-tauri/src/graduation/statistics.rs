//! What a run's agent work cost, recorded against its source draft
//! (`../../../specifications/core/GRD-graduation.md` GRD-FR-NHRY).
//!
//! Every agent or model operation a run performs appends one `agent_operation`
//! line to the source draft's statistics log, carrying the operation's own
//! stable id, the run's id, the bucket the work it supports belongs to, and the
//! instants its execution began and ended — together with one `token_usage`
//! line of scope `graduation` for each reliable provider-reported usage record
//! the operation produced.
//!
//! Three rules decide what a line says, and all three are the specification's
//! rather than this module's:
//!
//! * **The interval is the operation's actual execution** — from the moment the
//!   execution turn or the model call begins to the moment it returns. Time a
//!   run spends queued, waiting for the author, interrupted, or otherwise idle
//!   is not agent time and is recorded in no line.
//! * **The whole interval is recorded**, this module applying no telemetry
//!   boundary of its own: the clip is the fold's (per
//!   `DSS-draft-statistics-storage.md` DSS-FR-KYTB), so no producer has to know
//!   where a boundary stands and two of them can never disagree about one.
//! * **Nothing is estimated.** A token value is never derived from a task, an
//!   instruction, a change set, a character count, a model limit, or a price,
//!   and a direction the provider did not report is absent rather than zero.
//!
//! The append is asynchronous and never blocking (DSS-FR-TUMX): it is no part of
//! any transition, it is never journaled or checkpointed, and no run is delayed,
//! refused, interrupted, or reported differently because a line could not be
//! written.

use crate::agent_conversations::ModelReply;
use crate::logging::LogSink;
use crate::notes::{new_note_id, now_rfc3339_millis};
use crate::statistics::{
    self, Bucket, EventBody, Pending, ReportedTokens, Representation, UsageRecord, UsageScope,
};

use super::GraduationRun;

/// GRD-FR-NHRY / DSS-FR-QLEH: the bucket an execution turn of `part` belongs
/// to, decided by the phase or the work the turn supports.
///
/// Specification authoring is **Authoring**, implementation execution is
/// **Implementation**, a Review turn is **Reviews**, and a semantic rebase is
/// **Reconciliation**. A part this build does not know is read as Authoring,
/// which is the phase every run begins in.
pub fn bucket_for_part(part: &str) -> Bucket {
    match part {
        crate::graduation::driver::PART_REVIEW => Bucket::Reviews,
        crate::graduation::driver::PART_SEMANTIC_MERGE => Bucket::Reconciliation,
        // A work turn is Authoring. The Implementation and
        // Validation / hand-off / publication buckets stay readable, because a
        // draft's committed statistics log holds events written under them, but
        // no operation of this build carries either (DSS-FR-QLEH).
        _ => Bucket::Authoring,
    }
}

/// One agent or model operation, measured from where it begins to where it
/// returns.
///
/// Held on the stack of the phase that performs the operation, which is what
/// keeps the interval honest: it cannot accidentally span a queue wait or an
/// author wait, because neither happens inside the call this straddles.
pub struct OperationRecorder {
    operation_id: String,
    bucket: Bucket,
    started_at: String,
    usage: Vec<UsageRecord>,
}

impl OperationRecorder {
    /// Begin measuring an operation of `bucket`.
    pub fn begin(bucket: Bucket) -> OperationRecorder {
        OperationRecorder {
            operation_id: new_note_id(),
            bucket,
            started_at: now_rfc3339_millis(),
            usage: Vec::new(),
        }
    }

    /// GLG-FR-XQTM: one answered model call, with whatever reliable usage the
    /// provider reported for it.
    ///
    /// `round` is the logical model-call round and `attempt` the physical
    /// attempt within it, as the provider reported them; a physical attempt that
    /// was never answered reports no record and never reaches here.
    pub fn observed(&mut self, round: u32, attempt: u32, reply: &ModelReply) {
        let tokens = ReportedTokens {
            input_tokens: reply.prompt_tokens,
            cached_input_tokens: reply.input_tokens.map(|split| split.cached),
            uncached_input_tokens: reply.input_tokens.map(|split| split.uncached),
            output_tokens: reply.output_tokens,
        };
        if tokens.input().is_none() && tokens.output().is_none() {
            // The provider reported nothing, so there is no reliable record to
            // surface. Recording an empty one would make the bucket read
            // `incomplete` on the strength of a line that says nothing.
            return;
        }
        self.usage.push(UsageRecord {
            // The providers this loop reaches name no usage identity of their
            // own, so one is derived deterministically from the operation, the
            // round, and the attempt (per `../../../specifications/ai/CVL-conversation-loop.md`
            // CVL-FR-WQZD).
            usage_id: format!("{}:{round}:{attempt}", self.operation_id),
            round,
            attempt,
            representation: Representation::PerCall,
            tokens,
        });
    }

    /// Close the interval and append the operation's lines to the run's source
    /// draft (GRD-FR-NHRY).
    pub fn finish<R: tauri::Runtime>(self, app: &tauri::AppHandle<R>, run: &GraduationRun)
    where
        tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
    {
        // GRD-FR-VCTH: a merge run has no source draft, so no line is written
        // for what its turns cost.
        if run.input.draft_id.is_empty() {
            return;
        }
        let ended_at = now_rfc3339_millis();
        let mut pending = vec![Pending::now(EventBody::AgentOperation {
            operation_id: self.operation_id.clone(),
            bucket: self.bucket,
            run_id: run.id.clone(),
            started_at: self.started_at.clone(),
            ended_at,
        })];
        pending.extend(statistics::usage_events(
            &self.started_at,
            UsageScope::Graduation,
            self.bucket,
            &self.operation_id,
            &self.usage,
        ));
        statistics::record_statistics_events(app, &run.input.draft_id, pending);
    }
}
