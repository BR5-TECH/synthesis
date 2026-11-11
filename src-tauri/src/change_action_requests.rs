//! Change action requests (`CAR-change-action-requests.md`).
//!
//! An agent in an OpenSpec change discussion never starts a run: it asks the
//! author to **apply** the change or to **rework** it
//! (`crate::tools::request_change_action`). A request is one record and one
//! agent comment with a card. Only the author decides it, on the card or by an
//! explicit instruction to `decide_change_edits`, and a confirmation starts the
//! run through `crate::graduation` (`OCR-openspec-change-runs.md`).
//!
//! On-disk layout, in the repository machine store (CAR-FR-VQJC):
//!
//! ```text
//! changes/<stream-id>/<change>/action-requests/
//!   <request-id>.toml    — the record
//! ```
//!
//! Nothing here reaches the network or dispatches an agent turn
//! (CAR-FR-AQHX). The start of a confirmed run is the one call that can, and
//! the command makes it on a blocking thread.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::change_proposals::SiteResolver;
use crate::comments::{self, Participant};
use crate::fs;
use crate::log_fields;
use crate::logging::{self, Domain, LogSink, BUFFER};
use crate::notes::{new_note_id, now_rfc3339};

/// The `#[tauri::command]` surface, the event, and the logs.
mod commands;
/// Confirm and decline.
mod deciding;
/// The records, the errors, the store, and the read of a turn.
mod model;
/// Recording a request.
mod recording;

pub use commands::*;
pub use deciding::*;
pub use model::*;
pub use recording::*;

#[cfg(test)]
mod tests;
