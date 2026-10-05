//! The graduation end-to-end suite
//! (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`).
//!
//! Every scenario here drives the **production** loop and the production state
//! machine. The one thing it substitutes is the dispatch seam of
//! `GXD-graduation-execution.md` GXD-FR-QLFA, so what a scenario proves is what
//! the application does rather than what a parallel arrangement does
//! (GTE-FR-QVHM).
//!
//! GTE-FR-ZPNC: every scenario composes what it sends and reads what it gets
//! back with the production types, and asserts no private field of the loop.
//! GTE-FR-OYTZ: the suite runs in isolation under
//! `cargo test graduation::tests::e2e` and as part of the backend lane, and it
//! needs nothing that lane does not already install.
//!
//! A scenario reads as one journey — arrange, script, act, assert — and the
//! language it is written in stands in [`scenario`]. `README.md` beside this
//! file documents the language, the harness boundary, how a journey is added,
//! and the coverage matrix.

mod acts;
mod binary_protocol;
mod blockers;
mod change_set;
mod dispatch_assert;
mod budget;
mod escalations;
mod hooks;
mod interruptions;
mod journeys;
mod lifecycle;
mod merge_acts;
mod merge_arrange;
mod merge_apply;
mod merge_assert;
mod merge_budget;
mod merge_clean;
mod merge_controls;
mod merge_discards;
mod merge_handoff;
mod merge_hooks;
mod merge_logs;
mod merge_queue;
mod merge_restart;
mod merge_revisions;
mod merge_tips;
mod queue;
mod reconcile;
mod reconciliation;
mod standing;
mod outcome;
mod repository_assert;
mod scenario;
mod script;
mod settings;
mod stop_assert;

mod matrix;
