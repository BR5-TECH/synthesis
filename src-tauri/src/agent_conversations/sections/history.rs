//! The `discussion_history` and `current_comment` sections (AGC-FR-06), and
//! the `discussion_state` section beside them (AGC-FR-YQVB).

use super::*;

/// AGC-FR-06: the fragment first, then the discussion's comments oldest first.
///
/// The anchor belongs here because a *thread* is anchored rather than a comment
/// is, so an agent reading the history learns what the whole conversation is
/// about before it reads a word of it.
///
/// The history stops short of the comment that addressed the agent, which
/// `current_comment` carries on its own (AGC-FR-06): a message repeated in both
/// sections would be supporting context and the thing being answered at once,
/// which is the one distinction the two tags exist to draw. AGC-FR-13: a message
/// posted while a turn is in flight is therefore absent from both, belonging to
/// the next turn rather than changing the one already running.
/// AGC-FR-06: a discussion about a whole target (CMS-FR-53) opens this section
/// with its comments directly — an absent fragment being nothing to report rather
/// than something to describe.
///
/// The bound is the least one of AGC-FR-HOND, so a test reaches it with a
/// small fixture. A turn and an estimate pass their own bound to
/// [`discussion_history_with_status`].
#[cfg(test)]
pub(in crate::agent_conversations) fn discussion_history_section(
    roots: Roots<'_>,
    thread: &comments::Discussion,
    trigger_comment_id: &str,
) -> InputSection {
    discussion_history_with_status(roots, thread, trigger_comment_id, SECTION_MAX_CHARS).history
}

/// The text of the `discussion_state` section that
/// [`discussion_history_section`] builds beside the history, empty where there
/// is none (AGC-FR-06).
#[cfg(test)]
pub(in crate::agent_conversations) fn discussion_state_text(
    roots: Roots<'_>,
    thread: &comments::Discussion,
    trigger_comment_id: &str,
) -> String {
    discussion_history_with_status(roots, thread, trigger_comment_id, SECTION_MAX_CHARS)
        .state
        .map(|s| s.body)
        .unwrap_or_default()
}

/// AGC-FR-06: the anchor of a discussion and its comments before the trigger,
/// oldest first, each with its id and its rendered parts.
pub(in crate::agent_conversations) struct ThreadComments {
    pub anchor: String,
    pub comments: Vec<(String, Vec<InputPart>)>,
}

/// AGC-FR-06, AGC-FR-09: the anchor and the rendered comments before
/// `trigger_comment_id`. Every history section is composed from these, so a
/// section built for a replay opening and one built for an input are the same
/// bytes (ERP-FR-IVGL).
pub(in crate::agent_conversations) fn thread_comments(
    roots: Roots<'_>,
    thread: &comments::Discussion,
    trigger_comment_id: &str,
) -> ThreadComments {
    let anchor = match thread.fragment_target.as_ref() {
        Some(fragment) => format!(
            "This conversation is anchored to characters {}..{} of the file:\n\n{}\n\n",
            fragment.start, fragment.end, fragment.quote
        ),
        None => String::new(),
    };
    let handles = handles_of(thread);
    // AGC-FR-RVQP / AGC-FR-DKHT: read once for the whole section rather than
    // once per attachment.
    let reads = attachment_reads(roots, thread, true);
    // AGC-FR-09 / AGC-FR-35: a comment's attachments contribute into the section
    // holding it, in the order the comment carries them and in the position that
    // comment occupies.
    let mut comments = Vec::new();
    for comment in &thread.comments {
        if comment.id == trigger_comment_id {
            break;
        }
        let mut parts = comment_parts(roots, thread, comment, &handles, &reads);
        parts.push(InputPart::Text("\n".to_string()));
        comments.push((comment.id.clone(), parts));
    }
    ThreadComments { anchor, comments }
}

/// The history of a discussion as a builder assembles it (AGC-FR-06).
pub(in crate::agent_conversations) struct HistoryParts {
    /// The `discussion_history` section.
    pub history: InputSection,
    /// AGC-FR-YQVB: the `discussion_state` section, where the state is not
    /// empty.
    pub state: Option<InputSection>,
    /// AGC-FR-QSWD: the kind of a pending question set record that could not
    /// be read.
    pub unreadable: Option<&'static str>,
}

/// [`discussion_history_section`], its state, and the kind of a pending
/// question set record that could not be read (AGC-FR-QSWD).
pub(in crate::agent_conversations) fn discussion_history_with_status(
    roots: Roots<'_>,
    thread: &comments::Discussion,
    trigger_comment_id: &str,
    bound: usize,
) -> HistoryParts {
    let ThreadComments { anchor, comments } = thread_comments(roots, thread, trigger_comment_id);
    let comment_list: Vec<Vec<InputPart>> = comments.into_iter().map(|(_, parts)| parts).collect();
    // AGC-FR-KRMV / AGC-FR-JNOS / AGC-FR-EPNB / AGC-FR-TCCV / AGC-FR-QSWD: the
    // action requests, the split, the open proposal, then the pending question
    // set, are the state of the discussion (AGC-FR-YQVB). The bound of
    // AGC-FR-11 cuts the comments rather than them.
    let (questions, unreadable) = match pending_questions_text(roots, &thread.id) {
        Ok(text) => (text, None),
        Err(kind) => (None, Some(kind)),
    };
    // AGC-FR-YQVB / AGC-FR-MATQ: the state fills at most half of the bound.
    let tail = history_tail(roots, &thread.target, questions, bound);
    let (history, state) = bounded_history(anchor, comment_list, tail, bound);
    HistoryParts { history, state, unreadable }
}

/// AGC-FR-06: the body of the comment that addressed the agent.
///
/// A trigger the thread does not hold yields an empty section rather than none:
/// every builder produces the three sections, and a turn whose thread resolved
/// is not one AGC-FR-12 fails.
pub(in crate::agent_conversations) fn current_comment_section(
    roots: Roots<'_>,
    thread: &comments::Discussion,
    trigger_comment_id: &str,
) -> InputSection {
    let handles = handles_of(thread);
    let trigger = thread.comments.iter().find(|c| c.id == trigger_comment_id);
    // AGC-FR-DKHT: a `proposal_hunk` names its change here as well. Read only
    // where the comment carries one, so an ordinary trigger costs no read.
    let names_hunk = trigger.is_some_and(|comment| {
        comment.attachments.iter().any(|a| matches!(a, comments::Attachment::ProposalHunk { .. }))
    });
    let reads = if names_hunk {
        attachment_reads(roots, thread, false)
    } else {
        AttachmentReads::default()
    };
    let parts = trigger
        .map(|comment| comment_parts(roots, thread, comment, &handles, &reads))
        .unwrap_or_default();
    section_of_parts(TAG_CURRENT_COMMENT, Vec::new(), parts)
}
