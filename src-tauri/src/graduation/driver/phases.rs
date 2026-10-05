//! The phases the loop holds, and the bounds it counts
//! (`../../../specifications/ai/GRL-graduation-loop.md`).

/// GRL-FR-VYNX: the two phases of a pass.
pub const PART_WORK: &str = "work";
pub const PART_REVIEW: &str = "review";
/// The turn that stands outside a pass, at a stream update alone.
pub const PART_SEMANTIC_MERGE: &str = "semantic_merge";
/// GRL-FR-MWPQ: the two phases of one pass of a merge run.
pub const PART_MERGE_WORK: &str = "merge_work";
pub const PART_MERGE_REVIEW: &str = "merge_review";
/// GXD-FR-GMDI: the phase a merge run resumes at when its apply was blocked.
/// No turn is dispatched and no pass is spent at this phase.
pub const PART_APPLY: &str = "apply";

/// GRL-FR-ARPX: how many passes a run takes where the project configures no
/// budget of its own (`PSS-project-settings-storage.md` PSS-FR-WPKS).
///
/// A review that answers `revise` when the budget is spent rests the run for
/// the author rather than starting another pass: a loop that revises without a
/// bound spends the author's time and tokens re-deciding work it has already
/// been told about.
pub const DEFAULT_PASS_BUDGET: u32 = 2;

/// GRL-FR-ARPX: one review turn per pass.
pub const REVIEW_TURNS_PER_PASS: u32 = 1;

/// GRL-FR-TVXI: a verdict the application cannot read is asked again once. A
/// second consecutive unreadable verdict blocks the run.
pub const VERDICT_REFUSAL_BOUND: u32 = 2;

/// GRL-FR-QZFB: a blocker on the same condition rests the run for the author on
/// the second one.
///
/// Not configurable, on the terms GRL-FR-REPL sets for every other bound here.
pub const BLOCK_ATTEMPT_BOUND: u32 = 2;

/// GRL-FR-REPL: how long one agent turn may run where the project configures
/// no execution timeout of its own (`PSS-project-settings-storage.md`
/// PSS-FR-TQMV).
pub const DEFAULT_EXECUTION_TIMEOUT_MS: u64 = 2 * 60 * 60 * 1000;

