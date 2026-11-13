//! New change proposals: the split of an OpenSpec change
//! (`NCP-new-change-proposals.md`).
//!
//! An agent proposes, one at a time, the new changes that take over parts of a
//! change that grew too big (`crate::tools::propose_new_change`). Nothing is
//! created until the author accepts a proposal. An acceptance writes the new
//! change folder in the stream's working copy and commits it on the stream
//! branch. Only the author ends the split, and a confirmation never edits,
//! deletes, or archives the original change (NCP-FR-MHBS).
//!
//! On-disk layout, in the repository machine store (NCP-FR-AZON):
//!
//! ```text
//! changes/<stream-id>/<change>/new-changes/
//!   <proposal-id>.toml    — the record
//!   <proposal-id>.files   — the proposed files
//! changes/<stream-id>/<change>/split.toml   — written by a confirmation
//! ```
//!
//! This module shares no state and no operation with `crate::change_proposals`
//! or `crate::draft_proposals` (NCP-FR-BFCK). It reads the stream's working
//! copy through the same site resolver the change edit proposals use.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::change_proposals::{ChangeSite, SiteResolver};
use crate::comments::{self, Participant};
use crate::fs;
use crate::log_fields;
use crate::logging::{self, Domain, LogSink, BUFFER};
use crate::notes::{new_note_id, now_rfc3339};

/// The `#[tauri::command]` surface, the event, and the logs.
mod commands;
/// Accept, reject, complete, and confirm.
mod deciding;
/// The records, the errors, and the store.
mod model;
/// Recording and updating a proposal.
mod recording;

pub use commands::*;
pub use deciding::*;
pub use model::*;
pub use recording::*;

#[cfg(test)]
mod tests;
