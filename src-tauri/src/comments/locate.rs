//! The agent write path, and finding a thread by its id alone.

use super::*;


/// CMS-FR-41: the agent write path — the *only* way a comment is attributed to
/// an agent, and reachable from no Tauri command.
///
/// It goes through [`append_as`]'s machinery rather than beside it, so an agent
/// is subject to every rule an author is: a locked thread refuses it with the
/// same `thread_locked` error and gains no line (CMS-FR-17), and the appended
/// line is the same `comment_added` event with the same fields. A thread's log
/// therefore carries no evidence of who or what appended it beyond the
/// participant already stamped in `by`.
///
pub fn append_agent_comment(
    root: &crate::fs::RootFs,
    scope: LogScope<'_>,
    file_rel: &str,
    thread_id: &str,
    body: String,
    by: &Participant,
    at: &str,
) -> Result<Discussion, String> {
    append_agent_comment_to(
        root,
        ThreadRef { scope, file_rel },
        thread_id,
        body,
        by,
        at,
    )
}

/// [`append_agent_comment`] naming the conversation as a [`ThreadRef`], which is
/// what lets an agent answer a discussion on exactly the terms it answers an
/// anchored thread (CMS-FR-54).
pub fn append_agent_comment_to(
    root: &crate::fs::RootFs,
    target: ThreadRef<'_>,
    thread_id: &str,
    body: String,
    by: &Participant,
    at: &str,
) -> Result<Discussion, String> {
    // AGC-FR-17: an agent contributes prose and never a file, so every `url` and
    // every `blob` a thread holds was put there by the participant who attached
    // it. The one exception is a proposal reference, which goes through the
    // sibling below rather than through here.
    append_agent_comment_with(
        root,
        target,
        thread_id,
        new_note_id(),
        body,
        Vec::new(),
        by,
        at,
    )
}

/// [`append_agent_comment_to`] carrying attachments, under a comment id the
/// caller chose.
///
/// The only caller is `crate::draft_proposals` (DCP-FR-05), and it needs both
/// halves. The `proposal` attachment is what a surface renders its accept control
/// from (CMS-FR-60); and the **id is supplied rather than returned** because
/// DCP-FR-06 requires the proposal's record to reach disk *before* this comment
/// is appended, and a record has to name the comment it will be announced by. An
/// id minted here would force the record to be written after the append, which is
/// the one ordering that can leave a conversation announcing a proposal that was
/// never recorded.
///
/// Deliberately not a Tauri command and not reachable from the frontend — see
/// [`AttachmentInput`], which has no variant that could produce a proposal
/// reference.
///
/// Attachments here are already-stored [`Attachment`] values rather than inputs,
/// because a `proposal` has no content to store: `store_attachment` is the path
/// bytes take to disk, and this reference has none.
pub fn append_agent_comment_with(
    root: &crate::fs::RootFs,
    target: ThreadRef<'_>,
    thread_id: &str,
    comment_id: String,
    body: String,
    attachments: Vec<Attachment>,
    by: &Participant,
    at: &str,
) -> Result<Discussion, String> {
    let thread = target.find(root, thread_id)?;
    // CMS-FR-17: a lock stops an agent exactly as it stops a person.
    if thread.locked {
        return Err(ERR_DISCUSSION_LOCKED.to_string());
    }
    append_as_scoped(
        root,
        target.scope,
        target.file_rel,
        by,
        at,
        vec![(
            thread_id.to_string(),
            EventBody::CommentAdded {
                comment_id,
                body,
                quotes: Vec::new(),
                attachments,
            },
        )],
    )?;
    target.find(root, thread_id)
}

/// One discussion of a known owner, by id.
///
/// The fast form of [`read_discussion_by_id`] for a caller that holds the owner as
/// well as the id, so only that owner's logs are read.
pub fn locate_discussion_of(
    root: &crate::fs::RootFs,
    target: &DiscussionTarget,
    discussion_id: &str,
) -> Option<Discussion> {
    list_discussions_in(root, target)
        .into_iter()
        .find(|d| d.id == discussion_id)
}

/// One note discussion by id alone, for a caller that holds a conversation's
/// identity and not the note it belongs to — `read_discussion` serving a surface
/// that no longer holds the owner (CMS-FR-61).
///
/// A walk of the notes comments root rather than a derivation, because the id
/// this is given is the *discussion's* and the folders are keyed by the note's.
pub fn locate_note_discussion(
    root: &crate::fs::RootFs,
    discussion_id: &str,
) -> Option<Discussion> {
    let dir = fsa::resolve_under(root, format!("{COMMENTS_REL}/{NOTES_SUBDIR}")).ok()?;
    let entries = root.list_dir(&dir).ok()?;
    entries
        .into_iter()
        .filter(|entry| !entry.name.starts_with(DISCARDED_PREFIX))
        .filter_map(|entry| note_discussion_of(root, &entry.name))
        .find(|d| d.id == discussion_id)
}

/// CMS-FR-61: the one discussion an id names, whatever its owner and whether or
/// not it has a fragment.
///
/// The only read keyed by a **discussion** rather than by the owner it belongs
/// to. The artifact scope goes first, being one directory listing where almost
/// every discussion lives; then the notes, which cost one directory listing; then
/// each draft's own storage, which costs a drafts scan.
pub fn read_discussion_by_id(
    root: &crate::fs::RootFs,
    worktree: &crate::fs::RootFs,
    discussion_id: &str,
) -> Option<Discussion> {
    if let Some(item) = list_all_artifact_discussions_in(root, worktree)
        .into_iter()
        .find(|item| item.discussion.id == discussion_id)
    {
        return Some(item.discussion);
    }
    if let Some(discussion) = locate_note_discussion(root, discussion_id) {
        return Some(discussion);
    }
    for draft in crate::drafts::list_drafts_impl(worktree).drafts {
        let target = DiscussionTarget::Draft { draft_id: draft.id };
        if let Some(discussion) = locate_discussion_of(root, &target, discussion_id) {
            return Some(discussion);
        }
    }
    None
}

/// The note a discussion id belongs to, for a caller that has to reach the note
/// behind a conversation — the builder assembling a turn's `note_context`
/// (AGC-FR-07).
pub fn note_of_discussion(root: &crate::fs::RootFs, discussion_id: &str) -> Option<String> {
    locate_note_discussion(root, discussion_id).and_then(|d| d.note_id().map(str::to_string))
}
