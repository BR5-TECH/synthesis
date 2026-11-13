//! The decision functions that `crate::tools::decide_change_edits` calls
//! in-process (CEP-FR-KLWZ). Each calls the function the matching command
//! calls, so a decision by tool passes the same checks, writes the same file,
//! emits the same event, and appends the same decision comment.

use super::*;

/// One hunk decision by `decide_change_edits`.
pub(crate) struct ChangeHunkByTool<'a> {
    pub(crate) proposal_id: &'a str,
    pub(crate) hunk_id: &'a str,
    pub(crate) by: &'a Participant,
    /// PCP-FR-QKCT: the instruction line in the place of feedback.
    pub(crate) instruction_line: Option<&'a str>,
}

/// CEP-FR-KLWZ: accept one hunk.
pub(crate) fn accept_change_hunk_in_process<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    sites: &dyn SiteResolver,
    by_tool: ChangeHunkByTool<'_>,
) -> Result<ChangeDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let request = DecisionRequest {
        proposal_id: by_tool.proposal_id,
        decision: Decision::Accept,
        hunk: Some(by_tool.hunk_id),
        feedback: by_tool.instruction_line,
        by: by_tool.by,
    };
    decide_command(app, "accept_hunk", store, sites, request)
}

/// CEP-FR-KLWZ: reject one hunk.
pub(crate) fn reject_change_hunk_in_process<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    sites: &dyn SiteResolver,
    by_tool: ChangeHunkByTool<'_>,
) -> Result<ChangeDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let request = DecisionRequest {
        proposal_id: by_tool.proposal_id,
        decision: Decision::Decline,
        hunk: Some(by_tool.hunk_id),
        feedback: by_tool.instruction_line,
        by: by_tool.by,
    };
    decide_command(app, "reject_hunk", store, sites, request)
}

/// Whether one hunk resolves nowhere in its file as the file stands. A hunk the
/// proposal does not hold reads as not lost.
pub(crate) fn change_hunk_is_lost(
    store: &fs::RootFs,
    sites: &dyn SiteResolver,
    proposal_id: &str,
    hunk_id: &str,
) -> bool {
    let Ok(read) = load_hunks_impl(store, sites, proposal_id) else {
        return false;
    };
    read.hunks
        .iter()
        .position(|hunk| hunk.hunk.id == hunk_id)
        .and_then(|at| read.resolutions.get(at))
        .is_some_and(|placement| matches!(placement, HunkPlacement::Lost))
}
