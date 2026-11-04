//! The decision functions behind the hunk commands, called in-process by
//! `crate::tools::decide_prompt_changes` (`PCP-prompt-change-proposals.md`
//! PCP-FR-MXSB).
//!
//! Each one calls the same function the matching command calls, with the same
//! arguments, so a decision by tool passes the same checks and transaction
//! (PCP-FR-14), emits the same event, and appends the same decision comment
//! (PCP-FR-VCBX).

use super::*;

/// One hunk decision by `decide_prompt_changes`: the hunk, the author it is
/// recorded as, and the instruction line of PCP-FR-QKCT.
pub(crate) struct HunkByTool<'a> {
    pub(crate) proposal_id: &'a str,
    pub(crate) hunk_id: &'a str,
    pub(crate) by: &'a Participant,
    pub(crate) instruction_line: Option<&'a str>,
}

/// PCP-FR-12 / PCP-FR-MXSB: accept one hunk, with no feedback and with the
/// instruction line of PCP-FR-QKCT.
pub(crate) fn accept_hunk_in_process<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    repo_store: &fs::RootFs,
    by_tool: HunkByTool<'_>,
    tracker: &ContentTracker,
) -> Result<PromptDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let request = DecisionRequest {
        proposal_id: by_tool.proposal_id,
        decision: Decision::Accept,
        hunk: Some(by_tool.hunk_id),
        // PCP-FR-QKCT: no feedback. The instruction line of a decision by tool
        // takes the place feedback takes, so the decision comment, and the
        // body the journal holds for it, end with that line.
        feedback: by_tool.instruction_line,
        by: by_tool.by,
    };
    decide_in_process(app, "accept_hunk", root, repo_store, request, tracker)
}

/// PCP-FR-RJHN / PCP-FR-MXSB: reject one hunk, with no feedback.
pub(crate) fn reject_hunk_in_process<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    repo_store: &fs::RootFs,
    by_tool: HunkByTool<'_>,
    tracker: &ContentTracker,
) -> Result<PromptDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let request = DecisionRequest {
        proposal_id: by_tool.proposal_id,
        decision: Decision::Decline,
        hunk: Some(by_tool.hunk_id),
        // PCP-FR-QKCT: no feedback. The instruction line of a decision by tool
        // takes the place feedback takes, so the decision comment, and the
        // body the journal holds for it, end with that line.
        feedback: by_tool.instruction_line,
        by: by_tool.by,
    };
    decide_in_process(app, "reject_hunk", root, repo_store, request, tracker)
}

/// PCP-FR-LDHK (as DCP-FR-BMLX): whether one hunk resolves nowhere in the
/// file as it stands. A hunk that the document does not hold reads as not
/// lost, so the caller refuses it for the reason that applies to it.
pub(crate) fn hunk_is_lost(root: &fs::RootFs, proposal_id: &str, hunk_id: &str) -> bool {
    let Ok(read) = load_hunks_impl(root, proposal_id) else {
        return false;
    };
    read.hunks
        .iter()
        .position(|hunk| hunk.id == hunk_id)
        .and_then(|at| read.resolutions.get(at))
        .is_some_and(|placement| matches!(placement, HunkPlacement::Lost))
}
