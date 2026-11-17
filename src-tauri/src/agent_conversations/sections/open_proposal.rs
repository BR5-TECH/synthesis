//! The open proposal at the end of `discussion_history`
//! (`AGC-agent-conversations.md` AGC-FR-EPNB, AGC-FR-TCCV).
//!
//! The author names a change by its place ("the second change"), and the
//! agent maps that place to a hunk id for `decide_draft_changes` or
//! `decide_prompt_changes`. So the history ends with every hunk of the open
//! proposal, in proposal order, each with its id, its place, its stored state,
//! and a bounded excerpt of its text.

use super::*;

/// AGC-FR-EPNB: the bound on each excerpt, in characters.
pub(in crate::agent_conversations) const OPEN_PROPOSAL_EXCERPT_MAX_CHARS: usize = 200;

/// One hunk of the open proposal, as the rendering reads it.
struct OpenHunk {
    id: String,
    state: &'static str,
    withdrawn: bool,
    before: String,
    after: String,
}

/// AGC-FR-TCCV: the open proposal of the target, read through
/// `read_proposal_changes` (DCP-FR-HNWD) on a draft target and through
/// `read_prompt_proposal_changes` (PCP-FR-LQLT) on an artifact target. Both
/// reads run no reconciliation and write nothing (CVL-FR-09).
fn open_hunks(roots: Roots<'_>, target: &DiscussionTarget) -> Option<(String, Vec<OpenHunk>)> {
    match target {
        DiscussionTarget::Draft { draft_id } => {
            let open = crate::draft_proposals::pending_for(roots.worktree, draft_id)?;
            let proposal = crate::draft_proposals::read_proposal_changes(roots.worktree, draft_id)
                .ok()?
                .into_iter()
                .find(|proposal| proposal.id == open.id)?;
            let hunks = proposal.changes.into_iter().map(open_hunk).collect();
            Some((proposal.path, hunks))
        }
        DiscussionTarget::Artifact { artifact_id } => {
            let open = crate::prompt_proposals::pending_for(roots.worktree, artifact_id)?;
            let proposal =
                crate::prompt_proposals::read_prompt_proposal_changes(roots.worktree, &open.id)?;
            let hunks =
                proposal.changes.into_iter().map(|c| open_hunk(prompt_change_view(c))).collect();
            Some((proposal.path, hunks))
        }
        DiscussionTarget::Note { .. } | DiscussionTarget::Map => None,
        // CEP-FR-XXMW: the change's pending proposal, each hunk with its path.
        DiscussionTarget::Change { stream_id, change } => {
            let open =
                crate::change_proposals::pending_change_proposal(roots.store, stream_id, change)?;
            let proposal =
                crate::change_proposals::read_change_proposal_changes(roots.store, &open.id)?;
            let hunks =
                proposal.changes.into_iter().map(|c| open_hunk(change_hunk_view(c))).collect();
            Some((change_label(change), hunks))
        }
    }
}

fn open_hunk(change: crate::draft_proposals::ProposalChange) -> OpenHunk {
    use crate::draft_proposals::hunks::HunkState;
    // AGC-FR-EPNB: the **stored** state, so a held hunk reads as discussing.
    let state = match change.state {
        HunkState::Pending => "pending",
        HunkState::Accepted => "accepted",
        HunkState::Rejected => "rejected",
        HunkState::Discussing => "discussing",
        HunkState::Withdrawn => "withdrawn",
    };
    OpenHunk {
        id: change.id,
        state,
        withdrawn: change.state == HunkState::Withdrawn,
        before: change.before,
        after: change.after,
    }
}

/// AGC-FR-EPNB: at most 200 characters of one text, on one line.
fn bounded_excerpt(text: &str) -> String {
    let folded = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if folded.chars().count() <= OPEN_PROPOSAL_EXCERPT_MAX_CHARS {
        return format!("\"{folded}\"");
    }
    let kept: String = folded.chars().take(OPEN_PROPOSAL_EXCERPT_MAX_CHARS - 1).collect();
    format!("\"{kept}…\"")
}

/// AGC-FR-EPNB: the text that ends `discussion_history`, or `None` where the
/// target holds no open proposal.
///
/// The place is counted from 1 over the hunks that are not withdrawn, as the
/// review counter counts (DCR-FR-11, DDC-FR-MDCX). A withdrawn hunk is listed
/// with no place.
pub(in crate::agent_conversations) fn open_proposal_text(
    roots: Roots<'_>,
    target: &DiscussionTarget,
) -> Option<String> {
    open_proposal_text_within(roots, target, usize::MAX)
}

/// AGC-FR-MATQ: [`open_proposal_text`] in at most `max_chars` characters. The
/// hunks stand in proposal order while they fit, and a line that counts the
/// rest ends the text. The opening line always stands.
pub(in crate::agent_conversations) fn open_proposal_text_within(
    roots: Roots<'_>,
    target: &DiscussionTarget,
    max_chars: usize,
) -> Option<String> {
    let (path, hunks) = open_hunks(roots, target)?;
    let mut text = format!(
        "\nThe open proposal to {path}, every change in proposal order. When the author names a change by its place, it is the change with that place:\n"
    );
    let total = hunks.len();
    let mut place = 0;
    for (index, hunk) in hunks.into_iter().enumerate() {
        let mut line = if hunk.withdrawn {
            format!("  no place — id {} — {}", hunk.id, hunk.state)
        } else {
            format!("  place {} — id {} — {}", place + 1, hunk.id, hunk.state)
        };
        if !hunk.before.is_empty() {
            line.push_str(&format!(" — changes: {}", bounded_excerpt(&hunk.before)));
        }
        if !hunk.after.is_empty() {
            line.push_str(&format!(" — offers: {}", bounded_excerpt(&hunk.after)));
        }
        line.push('\n');
        // The room of the closing count stays free while a hunk is still left
        // after this one.
        let rest = total - index - 1;
        let reserve = if rest > 0 { more_changes_line(rest).chars().count() } else { 0 };
        let used = text.chars().count();
        if used.saturating_add(line.chars().count()).saturating_add(reserve) > max_chars {
            text.push_str(&more_changes_line(total - index));
            return Some(text);
        }
        if !hunk.withdrawn {
            place += 1;
        }
        text.push_str(&line);
    }
    Some(text)
}

/// AGC-FR-MATQ: the line that counts the hunks a bounded proposal left out.
fn more_changes_line(count: usize) -> String {
    format!("  …and {count} more changes.\n")
}

/// AGC-FR-TCCV: put the open proposal at the end of a section that was
/// bounded with room for it, so a truncation cuts comments and keeps the
/// proposal whole.
pub(in crate::agent_conversations) fn append_open_proposal(
    section: &mut InputSection,
    text: String,
) {
    section.body.push_str(&text);
    // A section that holds an image carries its parts; the proposal goes after
    // the last of them, as it goes after the last line of the text form.
    if !section.parts.is_empty() {
        section.parts.push(InputPart::Text(text));
    }
}

#[cfg(test)]
mod tests;
