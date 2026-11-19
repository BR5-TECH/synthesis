//! The material of a Map turn (AGC-FR-UOPJ, MPR-FR-PMLX, MPR-FR-NXDX).
//!
//! The builder of the `map` kind emits one `map_context` section. It holds
//! the focus of the turn and the rows of the plan proposals of the discussion.
//! It is pure over its roots, as the other builders are. The dispatch path
//! then adds the short summary of the delivery plan, because the summary needs
//! the application handle (see [`extend_map_material`]). The summary is read
//! from the read cache of the GitHub plan and makes no request.

use super::*;
use crate::comments::{Attachment, MapNodeRef};
use crate::delivery::github::plan::BuiltPlan;
use crate::flow_validation::model::{Hierarchy, Kind};

/// The most milestones the plan summary names. A plan with more is cut and says so.
const SUMMARY_MILESTONES_MAX: usize = 30;

/// MPR-FR-PMLX: what a turn of a Map thread is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Focus<'a> {
    /// One node of the Map.
    Node(&'a MapNodeRef),
    /// No node: the whole Map.
    WholeMap,
}

/// MPR-FR-PMLX: the focus of the turn that `trigger_comment_id` starts.
///
/// It is the node reference of the latest **human** comment that carries one,
/// else the node reference of the thread, else the whole Map. Only the comments
/// up to the trigger count, as in the history. A trigger that the thread does
/// not hold counts every comment.
pub(crate) fn focus_of<'a>(discussion: &'a Discussion, trigger_comment_id: &str) -> Focus<'a> {
    let upto = discussion
        .comments
        .iter()
        .position(|c| c.id == trigger_comment_id)
        .map_or(discussion.comments.len(), |at| at + 1);
    let from_comment = discussion.comments[..upto]
        .iter()
        .rev()
        .filter(|c| matches!(c.author, Participant::Human { .. }))
        .find_map(|c| {
            c.attachments.iter().find_map(|a| match a {
                Attachment::MapNode { node } => Some(node),
                _ => None,
            })
        });
    match from_comment.or(discussion.map_node.as_ref()) {
        Some(node) => Focus::Node(node),
        None => Focus::WholeMap,
    }
}

/// A node reference as one line of text, for the focus and for the history.
pub(crate) fn node_line(node: &MapNodeRef) -> String {
    format!(
        "[focus: {} {} \"{}\" (id {})]",
        node.lens.as_str(),
        node.kind.as_str(),
        node.label,
        node.id
    )
}

/// MPR-FR-NXDX: what a row is about, in a few words. The text is the agent's
/// own, from the row it recorded.
fn row_subject(fields: &crate::map_plan::RowFields) -> String {
    use crate::map_plan::RowFields;
    match fields {
        RowFields::CreateIssue(f) => format!("new issue \"{}\"", f.title),
        RowFields::ModifyIssue(f) => format!("change to issue {} \"{}\"", f.number, f.title),
        RowFields::NewDraft(f) => format!("new draft \"{}\"", f.name),
    }
}

/// MPR-FR-NXDX: one line for each row of the plan proposals of the discussion,
/// with the state of the row, so the agent knows what the author decided.
///
/// The lines come from the Map folder of `roots.store`, oldest proposal first.
/// Each line holds the row key, what the row is, its state, the issue number
/// or the draft of an accepted row, the typed error of a failed row, and the
/// reason that a waiting row is blocked.
pub(crate) fn proposal_rows_summary(roots: Roots<'_>, discussion_id: &str) -> Vec<String> {
    use crate::map_plan::RowState;
    let state_word = |state: RowState| match state {
        RowState::Waiting => "waiting",
        RowState::Accepted => "accepted",
        RowState::Rejected => "rejected",
        RowState::Failed => "failed",
    };
    let mut lines = Vec::new();
    for (index, proposal) in
        crate::map_plan::proposals_in(roots.store, discussion_id).iter().enumerate()
    {
        for row in &proposal.rows {
            let mut line = format!(
                "proposal {} row \"{}\": {}: {}",
                index + 1,
                row.key,
                row_subject(&row.fields),
                state_word(row.state)
            );
            // MPR-FR-CPGZ: an accepted row names what it made, so a later
            // turn can name the new issue as a parent.
            if row.state == RowState::Accepted {
                let result = row.result.as_ref();
                match (&row.fields, result.and_then(|r| r.issue_number)) {
                    (crate::map_plan::RowFields::NewDraft(_), _) => {
                        line.push_str(" (draft created)")
                    }
                    (_, Some(number)) => line.push_str(&format!(" (issue #{number})")),
                    _ => {}
                }
            }
            if let Some(error) = &row.error {
                line.push_str(&format!(" (error {})", error.code));
            }
            if let Some(block) = &row.blocked_by {
                line.push_str(&format!(
                    " (blocked: row \"{}\" is {})",
                    block.row_key,
                    state_word(block.reason)
                ));
            }
            lines.push(line);
        }
    }
    lines
}

/// AGC-FR-UOPJ: the `map_context` section of a Map thread, with the focus and
/// the plan proposal rows.
pub(super) fn map_context_section(
    roots: Roots<'_>,
    discussion: &Discussion,
    trigger_comment_id: &str,
) -> InputSection {
    let focus = focus_of(discussion, trigger_comment_id);
    let mut attributes = vec![("thread_id".to_string(), discussion.id.clone())];
    let mut body = String::new();
    match focus {
        Focus::Node(node) => {
            attributes.push(("focus".to_string(), "node".to_string()));
            attributes.push(("focus_lens".to_string(), node.lens.as_str().to_string()));
            attributes.push(("focus_kind".to_string(), node.kind.as_str().to_string()));
            attributes.push(("focus_id".to_string(), node.id.clone()));
            attributes.push(("focus_label".to_string(), node.label.clone()));
            body.push_str(&format!(
                "Focus: the {} {} \"{}\" (id {}).",
                node.lens.as_str(),
                node.kind.as_str(),
                node.label,
                node.id
            ));
        }
        Focus::WholeMap => {
            attributes.push(("focus".to_string(), "map".to_string()));
            body.push_str("Focus: the whole Map.");
        }
    }
    body.push_str("\n\nPlan proposals of this discussion:\n");
    let rows = proposal_rows_summary(roots, &discussion.id);
    if rows.is_empty() {
        body.push_str("none\n");
    } else {
        for row in rows {
            body.push_str(&format!("- {row}\n"));
        }
    }
    section(TAG_MAP_CONTEXT, attributes, body)
}

/// MPR-FR-NXDX: the short summary of a plan, as text. Pure.
///
/// It names each milestone with the count of open and closed plan issues under
/// it, the issues in no milestone, and the Types that map to no plan kind.
pub(super) fn plan_summary_text(plan: &BuiltPlan) -> String {
    let hierarchy = Hierarchy::new(&plan.document.items);
    let mut milestones: Vec<(&str, &str, usize, usize)> = plan
        .document
        .items
        .iter()
        .filter(|item| item.kind == Kind::Milestone)
        .map(|item| (item.id.as_str(), item.name.as_str(), 0, 0))
        .collect();
    let (mut loose_open, mut loose_closed) = (0usize, 0usize);
    for facts in plan.tracker.iter().filter(|f| f.reference.kind == "issue") {
        let closed = facts.state == "closed";
        let owner = hierarchy
            .ancestors(&facts.item_id)
            .into_iter()
            .find(|ancestor| hierarchy.kind(ancestor) == Some(Kind::Milestone));
        let slot = owner.and_then(|id| milestones.iter_mut().find(|m| m.0 == id));
        match (slot, closed) {
            (Some(m), true) => m.3 += 1,
            (Some(m), false) => m.2 += 1,
            (None, true) => loose_closed += 1,
            (None, false) => loose_open += 1,
        }
    }
    let mut out = String::from(
        "\n\nDelivery plan summary (from the last read of GitHub, which can be older than GitHub):\n",
    );
    if milestones.is_empty() {
        out.push_str("Milestones: none\n");
    } else {
        out.push_str(&format!("Milestones: {}\n", milestones.len()));
        for (id, name, open, closed) in milestones.iter().take(SUMMARY_MILESTONES_MAX) {
            out.push_str(&format!(
                "- {id} \"{name}\": {open} open issues, {closed} closed issues\n"
            ));
        }
        if milestones.len() > SUMMARY_MILESTONES_MAX {
            out.push_str(&format!(
                "- and {} more milestones\n",
                milestones.len() - SUMMARY_MILESTONES_MAX
            ));
        }
    }
    out.push_str(&format!(
        "Issues in no milestone: {loose_open} open issues, {loose_closed} closed issues\n"
    ));
    if !plan.notices.unmapped_types.is_empty() {
        let types: Vec<String> = plan
            .notices
            .unmapped_types
            .iter()
            .map(|t| {
                format!("{} ({} issues)", t.type_name.as_deref().unwrap_or("no Type"), t.count)
            })
            .collect();
        out.push_str(&format!("Types with no plan kind (not in the plan): {}\n", types.join(", ")));
    }
    out
}

/// MPR-FR-NXDX, AGC-FR-UOPJ: add the plan summary to the `map_context` section.
///
/// It reads the read cache of the GitHub plan (`GDP-github-delivery-plan.md`
/// GDP-FR-LKXA) and makes no request. Where the delivery source is not a GitHub
/// Project, or no read is cached, it adds nothing. The text counts against the
/// bound of the section, and a section that was cut gets none.
pub(super) fn extend_map_material<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    roots: Roots<'_>,
    budget: &DocumentBudget,
    sections: &mut [InputSection],
) {
    let Some(section) = sections.iter_mut().find(|s| s.tag == TAG_MAP_CONTEXT) else { return };
    let Some(plan) = crate::delivery::github::cached_delivery_plan(app, roots.worktree) else {
        logging::log_debug(
            app,
            &logging::BUFFER,
            &[Domain::Ai],
            "map plan summary not added",
            log_fields! { "reason" => "no cached github plan" },
        );
        return;
    };
    let extra = plan_summary_text(&plan);
    let added = super::flow_context::append_within(section, &extra, budget.bound_chars());
    logging::log_debug(
        app,
        &logging::BUFFER,
        &[Domain::Ai],
        "map plan summary for a turn",
        log_fields! { "items" => plan.document.items.len(), "added" => added },
    );
}
