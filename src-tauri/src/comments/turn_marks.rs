//! The group marks of a running turn, for the comments its tools append.
//!
//! A Meta turn that ends on a question or a proposal appends that comment from a
//! tool, and the tool knows nothing about the group of its turn. The turn
//! registers its marks here, by conversation and agent, and the agent write path
//! adds them to every comment that agent appends into that conversation until the
//! turn clears them (AGC-FR-VDOO).

use std::sync::Mutex;

use super::*;

static MARKS: Mutex<Vec<(String, String, Vec<Attachment>)>> = Mutex::new(Vec::new());

fn key_of(by: &Participant) -> Option<&str> {
    match by {
        Participant::Agent { agent_id, .. } => Some(agent_id),
        Participant::Human { .. } => None,
    }
}

fn lock() -> std::sync::MutexGuard<'static, Vec<(String, String, Vec<Attachment>)>> {
    MARKS.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// AGC-FR-VDOO: the marks that the comments of `author` carry in `thread_id`,
/// until [`clear_turn_marks`]. A second call replaces the first.
pub fn set_turn_marks(thread_id: &str, author: &Participant, marks: Vec<Attachment>) {
    let Some(agent) = key_of(author) else { return };
    let mut all = lock();
    all.retain(|(thread, owner, _)| !(thread == thread_id && owner == agent));
    if !marks.is_empty() {
        all.push((thread_id.to_string(), agent.to_string(), marks));
    }
}

/// AGC-FR-VDOO: the end of the marks that [`set_turn_marks`] set.
pub fn clear_turn_marks(thread_id: &str, author: &Participant) {
    set_turn_marks(thread_id, author, Vec::new());
}

/// The attachments of a comment, and the marks of its author's turn that the
/// attachments do not already hold.
pub(super) fn with_turn_marks(
    thread_id: &str,
    by: &Participant,
    mut attachments: Vec<Attachment>,
) -> Vec<Attachment> {
    let Some(agent) = key_of(by) else { return attachments };
    let all = lock();
    let Some((_, _, marks)) =
        all.iter().find(|(thread, owner, _)| thread == thread_id && owner == agent)
    else {
        return attachments;
    };
    for mark in marks {
        let held =
            attachments.iter().any(|a| std::mem::discriminant(a) == std::mem::discriminant(mark));
        if !held {
            attachments.push(mark.clone());
        }
    }
    attachments
}
