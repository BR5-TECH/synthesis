//! Change edit proposals (`CEP-change-edit-proposals.md`).
//!
//! A **change edit proposal** is a set of anchored hunks across the files of
//! one OpenSpec change on one work stream, offered by an agent in that change's
//! discussion (`crate::tools::propose_change_edits`) and held until the author
//! decides each hunk. An OpenSpec change is several files that must agree with
//! one another, so one proposal can touch the proposal, a delta, and the tasks
//! together.
//!
//! On-disk layout, in the repository machine store (CEP-FR-IISM):
//!
//! ```text
//! changes/<stream-id>/<change>/proposals/
//!   <proposal-id>.toml      — the record, with the per-hunk ledger
//!   <proposal-id>.hunks     — the hunk set: one hunk document per file
//!   <proposal-id>.journal   — the standing acceptance, when one stands
//! ```
//!
//! An accepted hunk writes its file in the stream's working copy at once
//! (CEP-FR-OPGW). The acceptance has one commit point, the file write, on the
//! terms of PCP-FR-14: the journal and the record move first, and the file write
//! is the last fallible step. When the proposal closes with at least one
//! accepted hunk, one commit on the stream branch holds the files those hunks
//! wrote (CEP-FR-FOJW).
//!
//! This module shares no state and no operation with `crate::prompt_proposals`
//! and `crate::draft_proposals` (CEP-FR-FQMZ). It uses the pure text module,
//! `crate::text_hunks`, alone, and it copies the ledger shapes of PCP so a
//! surface reads all three alike.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::comments::{self, Participant};
use crate::fs;
use crate::log_fields;
use crate::logging::{self, Domain, LogSink, BUFFER};
use crate::notes::{new_note_id, now_rfc3339};

/// The `#[tauri::command]` surface, the event, and the logs.
mod commands;
/// The commit of a closed proposal, and a commit that is owed.
mod committing;
/// Deciding hunks: accept, reject, decline, hold, edit.
mod deciding;
/// CEP-FR-KLWZ: the decision functions that the tool calls in-process.
mod in_process;
/// The ledger, its derived state, and the decision body.
mod ledger;
/// The typed errors and the record shapes.
mod model;
/// The reads of a review and of a turn.
mod reads;
/// Settling an interrupted acceptance.
mod reconcile;
/// The one path that records a proposal.
mod recording;
/// Where a change lives: its stream, its working copy, and its store folder.
mod site;
/// Reading records and hunk sets.
mod store;
/// The one path that updates the open proposal.
mod updating;

pub use commands::*;
pub use committing::commit_message;
pub use deciding::{edit_hunk_impl, hold_hunk_impl};
pub(crate) use in_process::*;
pub use ledger::{derive_state, ChangeHunkRow};
pub use model::*;
pub use reads::*;
pub use recording::*;
pub use site::*;
pub use store::{is_valid_proposal_id, list_proposals_impl};
pub use updating::*;

use committing::*;
use deciding::*;
use ledger::*;
use reconcile::*;
use store::*;

#[cfg(test)]
mod tests;
