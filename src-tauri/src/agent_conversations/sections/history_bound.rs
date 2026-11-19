//! The bound of `discussion_history` (`AGC-agent-conversations.md`
//! AGC-FR-HOND, AGC-FR-XOZQ, AGC-FR-YQVB, AGC-FR-MATQ).
//!
//! The section has a bound of its own, relative to the context window. Within
//! it, the state (decomposition, action requests, split, open proposal,
//! question set) fills at most half, and the comments fill the rest. The state
//! goes into its own `discussion_state` section after `current_comment`
//! (AGC-FR-06). Comments
//! that do not fit are dropped whole, oldest first, so the agent always reads
//! the newest part of the conversation.

use super::*;

/// The pieces of the tail, in the order they end the section.
struct TailPieces {
    decomposition: Option<DecompositionText>,
    action_requests: Option<String>,
    split: Option<String>,
    proposal: Option<String>,
    questions: Option<String>,
}

impl TailPieces {
    fn chars(&self) -> usize {
        self.decomposition.as_ref().map_or(0, |d| d.text.chars().count())
            + [&self.action_requests, &self.split, &self.proposal, &self.questions]
                .into_iter()
                .flatten()
                .map(|t| t.chars().count())
                .sum::<usize>()
    }

    fn text(self) -> String {
        [
            self.decomposition.map(|d| d.text),
            self.action_requests,
            self.split,
            self.proposal,
            self.questions,
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

/// AGC-FR-YQVB, AGC-FR-MATQ: the tail of `discussion_history` within half of
/// `bound`, and whether a part of it was cut. Above half, the pending new-draft
/// body is cut first, then the hunks of the open proposal. The question set is
/// never cut, so a tail can stay above half.
pub(in crate::agent_conversations) fn history_tail(
    roots: Roots<'_>,
    target: &DiscussionTarget,
    questions: Option<String>,
    bound: usize,
) -> (String, bool) {
    let half = bound / 2;
    let mut pieces = TailPieces {
        decomposition: decomposition_text_within(roots, target, usize::MAX),
        action_requests: action_requests_text(roots, target),
        split: split_text(roots, target),
        proposal: open_proposal_text(roots, target),
        questions,
    };
    let mut cut = false;
    if pieces.chars() > half {
        if let Some(body_chars) = pieces.decomposition.as_ref().map(|d| d.body_chars) {
            if body_chars > 0 {
                // The room that is left for the body once everything else of
                // the tail and the partial line stand.
                let others = pieces.chars() - body_chars;
                let marker = DRAFT_BODY_PARTIAL_LINE.chars().count() + 1;
                let room = half.saturating_sub(others + marker);
                if room < body_chars {
                    pieces.decomposition = decomposition_text_within(roots, target, room);
                    cut = true;
                }
            }
        }
    }
    if pieces.chars() > half {
        if let Some(proposal) = &pieces.proposal {
            let others = pieces.chars() - proposal.chars().count();
            let room = half.saturating_sub(others);
            let bounded = open_proposal_text_within(roots, target, room);
            if bounded.as_ref() != Some(proposal) {
                pieces.proposal = bounded;
                cut = true;
            }
        }
    }
    (pieces.text(), cut)
}

/// AGC-FR-XOZQ: the line that follows the anchor where older comments were
/// dropped.
pub(in crate::agent_conversations) fn omitted_comments_line(count: usize) -> String {
    format!("[Earlier comments omitted: {count} comments.]\n")
}

/// The characters of text a comment's parts hold. Images are not counted, as
/// the bound of AGC-FR-11 does not count them.
pub(in crate::agent_conversations) fn text_chars(parts: &[InputPart]) -> usize {
    parts
        .iter()
        .map(|part| match part {
            InputPart::Text(text) => text.chars().count(),
            InputPart::Image(_) => 0,
        })
        .sum()
}

/// The line that ends an anchor that was cut (AGC-FR-11).
const ANCHOR_PARTIAL_LINE: &str = "\n\n[This material is partial: it was truncated here.]\n\n";

/// AGC-FR-11: the anchor in at most `room` characters, and whether it was cut.
fn bounded_anchor(anchor: String, room: usize) -> (String, bool) {
    if anchor.chars().count() <= room {
        return (anchor, false);
    }
    let keep = room.saturating_sub(ANCHOR_PARTIAL_LINE.chars().count());
    let kept: String = anchor.chars().take(keep).collect();
    (format!("{kept}{ANCHOR_PARTIAL_LINE}"), true)
}

/// AGC-FR-11, AGC-FR-XOZQ: the line that opens a comment whose start was left
/// out, so the section says it holds less than the whole.
pub(in crate::agent_conversations) const PARTIAL_COMMENT_LINE: &str =
    "[This comment is partial: its start was left out.]\n";

/// AGC-FR-XOZQ: keep the end of one comment's text in `room` characters,
/// after the line that says it is partial.
fn end_of_text(parts: Vec<InputPart>, room: usize) -> Vec<InputPart> {
    let room = room.saturating_sub(PARTIAL_COMMENT_LINE.chars().count());
    let text: String = parts
        .into_iter()
        .filter_map(|part| match part {
            InputPart::Text(text) => Some(text),
            InputPart::Image(_) => None,
        })
        .collect();
    let total = text.chars().count();
    let kept: String = text.chars().skip(total.saturating_sub(room)).collect();
    vec![InputPart::Text(format!("{PARTIAL_COMMENT_LINE}{kept}"))]
}

/// AGC-FR-06: a `discussion_history` section of the anchor, the omitted line
/// where `omitted` comments were dropped, and `comments`. The same order of
/// parts as [`bounded_history`] gives, so the two give the same bytes for the
/// same comments.
pub(in crate::agent_conversations) fn compose_history(
    anchor: &str,
    omitted: usize,
    comments: impl IntoIterator<Item = Vec<InputPart>>,
    truncated: bool,
) -> InputSection {
    let mut parts: Vec<InputPart> = Vec::new();
    if !anchor.is_empty() {
        parts.push(InputPart::Text(anchor.to_string()));
    }
    if omitted > 0 {
        parts.push(InputPart::Text(omitted_comments_line(omitted)));
    }
    parts.extend(comments.into_iter().flatten());
    let mut section =
        section_of_parts_within(TAG_DISCUSSION_HISTORY, Vec::new(), parts, usize::MAX);
    section.truncated = truncated;
    section
}

/// ERP-FR-IICX: the index of the first comment that a history of comments of
/// `sizes` characters keeps in `room` characters beside an anchor of
/// `anchor_chars`. 0 where every comment fits. `hint`, the first comment an
/// earlier turn kept, is kept while the rest from it fits. Otherwise comments
/// are dropped, oldest first, until the rest fits three quarters of the room.
/// `None` where not even the newest comment fits the room whole. Pure.
pub(in crate::agent_conversations) fn first_kept_comment(
    anchor_chars: usize,
    sizes: &[usize],
    room: usize,
    hint: Option<usize>,
) -> Option<usize> {
    let total = anchor_chars + sizes.iter().sum::<usize>();
    if total <= room {
        return Some(0);
    }
    let line = omitted_comments_line(sizes.len()).chars().count();
    let from = |start: usize| anchor_chars + line + sizes[start..].iter().sum::<usize>();
    if let Some(hint) = hint.filter(|h| *h > 0 && *h < sizes.len()) {
        if from(hint) <= room {
            return Some(hint);
        }
    }
    for target in [room * 3 / 4, room] {
        let mut used = anchor_chars + line;
        let mut first = sizes.len();
        while first > 0 && used + sizes[first - 1] <= target {
            first -= 1;
            used += sizes[first];
        }
        if first < sizes.len() {
            return Some(first);
        }
    }
    None
}

/// AGC-FR-HOND, AGC-FR-XOZQ, AGC-FR-YQVB: the `discussion_history` section of
/// these comments (oldest first, each with its own parts) in `bound`
/// characters less the tail, and the `discussion_state` section of the tail
/// where it is not empty. Comments that
/// do not fit are dropped whole, oldest first; the anchor stays first, and one
/// line says how many were dropped. Where the newest comment alone does not
/// fit, the section keeps the end of its text.
pub(in crate::agent_conversations) fn bounded_history(
    anchor: String,
    comments: Vec<Vec<InputPart>>,
    tail: (String, bool),
    bound: usize,
) -> (InputSection, Option<InputSection>) {
    let (tail, tail_cut) = tail;
    let room = bound.saturating_sub(tail.chars().count());
    // AGC-FR-11: an anchor above the room keeps its start and says it is
    // partial, as any bounded material does.
    let (anchor, anchor_cut) = bounded_anchor(anchor, room);
    let sizes: Vec<usize> = comments.iter().map(|c| text_chars(c)).collect();
    let anchor_chars = anchor.chars().count();
    let total = anchor_chars + sizes.iter().sum::<usize>();

    let mut parts: Vec<InputPart> = Vec::new();
    if !anchor.is_empty() {
        parts.push(InputPart::Text(anchor));
    }
    let mut truncated = anchor_cut;
    if total <= room {
        parts.extend(comments.into_iter().flatten());
    } else {
        truncated = true;
        // Keep comments from the newest back while they fit beside the
        // anchor and the line that counts the dropped ones.
        let count = comments.len();
        let line_chars = omitted_comments_line(count).chars().count();
        let mut used = anchor_chars + line_chars;
        let mut first_kept = count;
        while first_kept > 0 && used + sizes[first_kept - 1] <= room {
            first_kept -= 1;
            used += sizes[first_kept];
        }
        let mut comments = comments;
        let left = room.saturating_sub(anchor_chars + line_chars);
        let kept: Vec<Vec<InputPart>> = if first_kept == count && count > 0 && left > 0 {
            // The newest comment alone does not fit: the end of its text.
            let newest = comments.pop().unwrap_or_default();
            first_kept = count - 1;
            vec![end_of_text(newest, left)]
        } else if first_kept == count {
            // No room is left at all: every comment is omitted.
            Vec::new()
        } else {
            comments.split_off(first_kept)
        };
        if first_kept > 0 {
            parts.push(InputPart::Text(omitted_comments_line(first_kept)));
        }
        parts.extend(kept.into_iter().flatten());
    }
    let mut section =
        section_of_parts_within(TAG_DISCUSSION_HISTORY, Vec::new(), parts, usize::MAX);
    section.truncated = truncated;
    let state = (!tail.is_empty()).then(|| state_section(tail, tail_cut));
    (section, state)
}

#[cfg(test)]
mod tests;
