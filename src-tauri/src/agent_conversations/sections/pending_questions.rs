//! The pending question set at the end of `discussion_history`
//! (`AGC-agent-conversations.md` AGC-FR-QSWD).
//!
//! A set that waits for the author is not a comment, so no comment of the
//! history holds it. The author can ask about it ("explain option 2") while it
//! waits, and the agent can explain an option only when it reads the set. So
//! the history ends with the set: each question in recorded order, and each
//! option with its position, its label, and its description.

use super::*;

/// AGC-FR-QSWD: the text that ends `discussion_history`, `Ok(None)` where the
/// discussion holds no pending question set, and the kind of the failure
/// where its record cannot be read or parsed. Such a record adds nothing, and
/// the caller logs the kind, which holds no content.
pub(in crate::agent_conversations) fn pending_questions_text(
    roots: Roots<'_>,
    discussion_id: &str,
) -> Result<Option<String>, &'static str> {
    comments::read_question_set_checked(roots.store, roots.worktree, discussion_id)
        .map(|set| set.as_ref().map(render_pending_questions))
}

/// AGC-FR-QSWD: the rendering of one set. It reads no file.
pub(in crate::agent_conversations) fn render_pending_questions(
    set: &comments::PendingQuestionSet,
) -> String {
    let mut text = format!(
        "\nA question set from {} waits for the author's answers. The author has not answered it yet, and it is not a comment of this discussion:\n",
        handle_of(&set.asked_by)
    );
    for question in &set.questions {
        text.push_str(&format!("  question {} — {}\n", question.position, question.text.trim()));
        for option in &question.options {
            text.push_str(&format!("    option {} — {}", option.position, option.value.trim()));
            if let Some(description) =
                option.description.as_deref().map(str::trim).filter(|d| !d.is_empty())
            {
                text.push_str(&format!(" — {description}"));
            }
            text.push('\n');
        }
    }
    text
}

#[cfg(test)]
pub(in crate::agent_conversations) mod tests;
