//! The refusals, the gate, and the log records that the plan tools share
//! (`PIR-plan-issue-read-tools.md`, `PPT-propose-plan-changes-tool.md`).
//!
//! The plan tools name a number, a row, or a label in some refusals, so their
//! messages are not all fixed text. `PlanRefusal` holds the message as an owned
//! string. A message holds an integer that the model sent, or a name that the
//! plan tools check against the plan, and never any other text of the model.

use rig::tool::{ToolErrorKind, ToolExecutionError};

use super::{require_open_project, ToolRefusal};
use crate::delivery::DeliveryError;
use crate::logging::{self, Domain, Fields, LogBuffer, LogSink};

/// TLC-FR-14: the domains of every record of a tool call.
const DOMAINS: [Domain; 2] = [Domain::Ai, Domain::Backend];

/// A refusal of a plan tool, in the vocabulary of `TLC-tool-conventions.md`
/// (TLC-FR-09, TLC-FR-10, TLC-FR-11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanRefusal {
    pub kind: ToolErrorKind,
    pub message: String,
    /// The stable reason of the log record.
    pub code: &'static str,
    pub retryable: bool,
}

impl PlanRefusal {
    pub fn new(
        kind: ToolErrorKind,
        code: &'static str,
        retryable: bool,
        message: impl Into<String>,
    ) -> Self {
        PlanRefusal { kind, message: message.into(), code, retryable }
    }

    pub fn invalid(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(ToolErrorKind::InvalidArgs, code, true, message)
    }

    /// TLC-FR-10, TLC-FR-11: the refusal as `rig` delivers it to the model.
    pub fn to_execution_error(&self) -> ToolExecutionError {
        ToolExecutionError::new(self.kind, self.message.clone()).with_retryable(self.retryable)
    }
}

impl From<ToolRefusal> for PlanRefusal {
    fn from(refusal: ToolRefusal) -> Self {
        PlanRefusal {
            kind: refusal.kind(),
            message: refusal.to_string(),
            code: refusal.reason(),
            retryable: refusal.retryable(),
        }
    }
}

impl std::fmt::Display for PlanRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for PlanRefusal {}

/// The refusal of the gate, in the words of one tool group.
pub struct GateWords {
    pub plan_unavailable: &'static str,
}

/// PIR-FR-HRXN, PPT-FR-DDSX: the conditions of a call of a plan tool. A project
/// is open, and the delivery source is a GitHub Project with a valid
/// configuration. The turn can be of any discussion (CVL-FR-RWQB). It reads
/// memory and the settings of the project and makes no request.
pub fn gate<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    words: &GateWords,
) -> Result<(), PlanRefusal> {
    require_open_project(app)?;
    if !crate::agent_conversations::tool_wiring::map_plan_tools_enabled(app) {
        return Err(PlanRefusal::new(
            ToolErrorKind::NotFound,
            "plan_unavailable",
            false,
            words.plan_unavailable,
        ));
    }
    Ok(())
}

/// Whether the plan itself is not available: no GitHub source, no selected
/// Project, or a configuration that is not valid.
pub fn plan_is_unavailable(error: &DeliveryError) -> bool {
    use crate::github_polling::records::{
        is_configuration_error, ERR_CONFIGURATION_INVALID, ERR_UNCONFIGURED,
    };
    error.code == crate::delivery::source::ERR_NOT_GITHUB_SOURCE
        || error.code == ERR_UNCONFIGURED
        || error.code == ERR_CONFIGURATION_INVALID
        || is_configuration_error(&error.code)
}

/// TLC-FR-14: one `WARN` record at a refusal. It names the tool and the reason,
/// and `extra` holds counts and kinds, never a title, a body, a query, a label,
/// or a rationale.
pub fn log_refused<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    tool: &'static str,
    refusal: &PlanRefusal,
    extra: Fields,
) {
    let mut fields = crate::log_fields! {
        "tool" => tool,
        "reason" => refusal.code,
        "retryable" => refusal.retryable,
    };
    for (key, value) in extra {
        fields.entry(key).or_insert(value);
    }
    logging::log_warn(sink, buffer, &DOMAINS, "tool call refused", fields);
}

/// PIR-FR-VNGK: one `DEBUG` record at the end of a read.
pub fn log_read_done<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    tool: &'static str,
    count: usize,
) {
    logging::log_debug(
        sink,
        buffer,
        &DOMAINS,
        "tool call completed",
        crate::log_fields! { "tool" => tool, "count" => count },
    );
}

/// PPT-FR-DGAA: one `INFO` record at a recording.
pub fn log_recorded<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    tool: &'static str,
    extra: Fields,
) {
    let mut fields = crate::log_fields! { "tool" => tool };
    for (key, value) in extra {
        fields.entry(key).or_insert(value);
    }
    logging::log_info(sink, buffer, &DOMAINS, "tool call completed", fields);
}
