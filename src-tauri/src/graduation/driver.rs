//! The graduation loop (`../ai/GRL-graduation-loop.md`).
//!
//! Two phases and no more. A **work** turn drives an execution agent against the
//! run's work stream until the agent reports the task finished, and a **review**
//! turn judges what it wrote. A review that answers `ready` ends the run; one
//! that asks for revision starts one further pass, and a second review that
//! still asks for revision stops the run for the author.
//!
//! The loop enforces no shape on the work. It tells the agent what was asked for
//! and where to find the project's own instructions; what a change must contain
//! is the project's rule rather than this module's (GRL-FR-DAIB).

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::log_fields;
use crate::logging::{self, Domain};
use crate::tools::agent_exec::protocol::{
    AgentTaskRequest, Cancellation, ExecutionControls, ResultContract, SessionRef,
    PROTOCOL_VERSION,
};
use crate::tools::agent_exec::runtime::CancellationToken;
use crate::tools::agent_exec::{
    AgentCliExecutor, AgentExecution, AgentExecutionError, AgentExecutionRequest, DirectoryProblem,
    LaunchContext, ProcessOutcome,
};

use super::{
    observability, GraduationQueue, transitions, GraduationEscalationAnswer, GraduationEscalationOrigin,
    GraduationEscalationQuestion,
    GraduationInterruptionReason, GraduationProposedResponse, GraduationResumeRef, GraduationRun,
    GraduationRunState, GraduationState, ReviewOutcome, ReviewSeverity, ReviewVerdict,
};

pub mod drive;
pub mod merge_drive;
pub mod merge_workspace;
pub mod phases;
pub mod prompts;
pub mod review_checkout;
pub mod seams;
pub mod settings;
pub mod stops;
pub mod task_input;
pub mod turns;

pub use drive::{spawn, spawn_with};
#[cfg(test)]
pub(crate) use drive::drive_for_test;
pub use phases::*;
pub use prompts::*;
pub use seams::{AgentCliDispatch, DispatchFuture, GraduationDispatch};
pub use settings::LoopSettings;
pub use task_input::*;
