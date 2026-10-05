//! Comments storage (`specifications/core/CMS-comments-storage.md`).
//!
//! A comment thread's whole life is an append-only sequence of immutable events,
//! one JSON object per line, in a JSONL log the **repository machine store**
//! holds (CMS-FR-01, CMS-FR-04, `RMS-repository-machine-storage.md`
//! RMS-FR-ZXHM). Nothing is ever rewritten or removed: a thread is opened,
//! comments are added, it is locked, resolved, reopened and re-anchored, and the
//! state the UI renders is a fold over those lines (CMS-FR-09).
//!
//! **Two roots, and only one of them holds a log.** Every path this module
//! resolves goes against the store, which stands under `app_data_dir()` and
//! outside every worktree, so no conversation modifies a checkout and none
//! reaches Git (CMS-FR-VJRP). The four functions that also take a `worktree` do
//! so to read the *project's* own paths — the artifact set behind CMS-FR-33, and
//! the note or draft a target names — and they write nothing there.
//!
//! The store is keyed by the **repository** rather than by the worktree
//! (CMS-FR-XQBM), so every worktree of one repository reads and writes one set
//! of logs and a switch of active worktree changes nothing (CMS-FR-28). One log
//! per artifact, named deterministically from the artifact's path (CMS-FR-02),
//! means two checkouts write into the *same* file; the fold dedupes by
//! `event_id` (CMS-FR-06), so a line two application processes both appended
//! folds once (RMS-FR-PFOB).
//!
//! Every event names the participant who produced it, and a participant is either
//! a human resolved from the project's GitHub credential or an agent (CMS-FR-09),
//! so a conversation with an automated collaborator is the same log shape as one
//! between two authors. [`append_as`] is the single writer and takes either kind
//! — the Tauri commands are the human-facing wrapper over it (CMS-FR-26).
//!
//! Every write goes through the FSA primitives (CMS-FR-27): `append_lines` for a
//! log line, `write_bytes_atomic` for an attachment's bytes, `rename_path` when
//! a rename is followed, all behind the FSA-FR-10 path-escape gate.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::State;

use crate::fs as fsa;
use crate::github_tokens::{resolve_github_identity_if_stored, GithubIdentity};
use crate::global_settings::GlobalSettingsStore;
use crate::notes::{new_note_id, now_rfc3339};
use crate::project::ProjectState;
use crate::scanning;

mod constants;
mod paths;
mod fold;
mod listing;
mod scope;
mod attachment_store;
mod locate;
mod opening;
mod mutating;
mod question_sets;
mod question_submit;
mod rename;
mod model;
mod commands;

pub use constants::*;
pub use paths::*;
pub use fold::*;
pub use listing::*;
pub use scope::*;
pub use attachment_store::*;
pub use locate::*;
pub use opening::*;
pub use mutating::*;
pub use question_sets::*;
pub use question_submit::*;
pub use rename::*;
pub use model::*;
pub use commands::*;


#[cfg(test)]
mod tests;
