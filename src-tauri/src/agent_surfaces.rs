//! Agent surfaces (`AUI-agent-ui-surfaces.md`).
//!
//! An agent in a conversation shows the author a small interactive panel,
//! composed from the discussion catalog in the declarative shape of A2UI v1.0.
//! This module holds the catalog, validates each surface against it, stores it
//! as a record beside its conversation, and turns the author's one action into
//! a comment of the author. The tool `crate::tools::show_surface` records
//! surfaces through [`store::record_with`].
//!
//! On-disk layout, under the store root of the conversation (AUI-FR-DGUP):
//!
//! ```text
//! agent-surfaces/<surface-id>.json
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::comments::{DiscussionTarget, Participant};

pub mod actions;
pub mod catalog;
pub mod commands;
pub mod log;
pub mod media;
pub mod store;
pub mod validate;

#[cfg(test)]
mod tests;

pub use catalog::{catalog, CATALOG_ID};
pub use commands::{get_agent_surface, submit_surface_action};
pub use validate::{validate_surface, SurfaceViolation};

/// AUI-FR-DGUP: the folder of the records under the store root.
pub const SURFACES_DIR: &str = "agent-surfaces";

/// AUI-FR-WDZG: the event that each change of a record emits.
pub const AGENT_SURFACE_CHANGED: &str = "agent-surface-changed";

// AUI contract surface, "Typed errors".
pub const ERR_NO_PROJECT_OPEN: &str = "no_project_open";
pub const ERR_SURFACE_NOT_FOUND: &str = "surface_not_found";
pub const ERR_ALREADY_SUBMITTED: &str = "already_submitted";
pub const ERR_UNKNOWN_ACTION: &str = "unknown_action";
pub const ERR_INVALID_CONTEXT: &str = "invalid_context";
pub const ERR_DISCUSSION_RESOLVED: &str = crate::comments::ERR_DISCUSSION_RESOLVED;
pub const ERR_COMMENT_FAILED: &str = "comment_failed";
pub const ERR_WRITE_FAILED: &str = "write_failed";

/// AUI-FR-IAAP: the state of a surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceState {
    Open,
    Submitted,
}

/// AUI-FR-AIRH: the author's one action on a surface.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceSubmission {
    pub name: String,
    pub source_component_id: String,
    pub context: Value,
    pub data_model: Value,
    pub by: Participant,
    pub at: String,
    pub comment_id: String,
}

/// AUI-FR-IAAP: one agent surface, as stored and as served. A stored record
/// of the first shape reads through [`log::StoredSurface`] (AUI-FR-JYKT).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", from = "log::StoredSurface")]
pub struct AgentSurface {
    pub id: String,
    pub discussion_id: String,
    pub target: DiscussionTarget,
    /// The agent comment that carries the surface.
    pub comment_id: String,
    pub catalog_id: String,
    /// AUI-FR-IAAP / AUI-FR-TYBX: the A2UI v1.0 messages, in order.
    pub messages: Vec<Value>,
    pub authored_by: Participant,
    pub created_at: String,
    pub state: SurfaceState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submission: Option<SurfaceSubmission>,
}

/// AUI-FR-KBVY: the action that the frontend sends.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceAction {
    pub name: String,
    pub source_component_id: String,
    #[serde(default = "empty_object")]
    pub context: Value,
    #[serde(default = "empty_object")]
    pub data_model: Value,
}

fn empty_object() -> Value {
    Value::Object(serde_json::Map::new())
}

/// AUI-FR-MAXD: what an accepted action returns.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceSubmitResult {
    pub surface: AgentSurface,
    pub comment_id: String,
}
