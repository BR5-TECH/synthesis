//! Adding a comment, and the lock, resolution and re-anchor writes.

use super::*;

/// CMS-FR-16 / CMS-FR-17: add a comment to a thread, refusing a locked one and a
/// quote that names a comment outside it.
pub fn add_comment_in(
    root: &crate::fs::RootFs,
    artifact_id: &str,
    thread_id: &str,
    body: String,
    quotes: Vec<CommentQuote>,
    attachments: Vec<AttachmentInput>,
    by: &Participant,
    at: &str,
) -> Result<Discussion, String> {
    add_comment_to(
        root,
        ThreadRef::artifact(artifact_id),
        thread_id,
        body,
        quotes,
        attachments,
        by,
        at,
    )
}

/// [`add_comment_to`] under a comment id the caller chose, carrying no quotes
/// and no attachments.
///
/// The only caller is `crate::draft_proposals`' decision path (DCP-FR-15), which
/// needs the id it is about to write so it can name that comment as the trigger
/// of the fresh turn the decision dispatches (DCR-FR-15) — without re-folding the
/// thread afterwards to work out which of its comments it had just appended, a
/// read that is both wasteful and ambiguous under a concurrent append.
pub fn add_comment_with_id(
    root: &crate::fs::RootFs,
    target: ThreadRef<'_>,
    thread_id: &str,
    comment_id: String,
    body: String,
    by: &Participant,
    at: &str,
) -> Result<Discussion, String> {
    let thread = target.find(root, thread_id)?;
    // CMS-FR-17: the one thing a lock prevents.
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
                attachments: Vec::new(),
            },
        )],
    )?;
    target.find(root, thread_id)
}

/// [`add_comment_with_id`] under an `event_id` the caller chose as well, and
/// without the lock check.
///
/// The only caller is the acceptance transaction of `crate::draft_history`
/// (DHS-FR-20), and both departures are what make that transaction recoverable:
///
/// - **The `event_id` is the caller's.** A committed acceptance owes the
///   conversation one line, and a crash between the commit point and the append
///   means the next reconciliation appends it again. Under an id minted here
///   that would fold as a second comment; under the one the journal minted
///   before the transaction began it folds as the same one, the fold applying
///   the first event it sees for an id and ignoring every later one
///   (CMS-FR-06). That is what lets an append-only log (CMS-FR-04) carry a
///   recoverable transaction without any line of it ever being taken back.
/// - **The lock is not consulted.** `DCP-FR-15` refuses a decision against a
///   locked conversation *before* the transaction begins. A lock applied in the
///   window between that check and this append is a fact about what the
///   conversation will take next, not licence to leave a decision the author
///   already made unrecorded — and there is no other place to record it.
///
/// Nothing else may use this: an ordinary comment has no id to preserve and no
/// reason to pass a lock.
pub fn append_decision_comment_event(
    root: &crate::fs::RootFs,
    target: ThreadRef<'_>,
    thread_id: &str,
    event_id: String,
    comment_id: String,
    body: String,
    by: &Participant,
    at: &str,
) -> Result<Discussion, String> {
    // The thread still has to exist: an append into a log for a thread that was
    // never opened would fold to nothing and quietly lose the decision.
    target.find(root, thread_id)?;
    let event = Event {
        v: SCHEMA_VERSION,
        event_id,
        thread_id: thread_id.to_string(),
        at: at.to_string(),
        by: by.clone(),
        body: EventBody::CommentAdded {
            comment_id,
            body,
            quotes: Vec::new(),
            attachments: Vec::new(),
        },
    };
    let line = serde_json::to_string(&event).map_err(|e| format!("could not encode event: {e}"))?;
    let path = target.scope.log_path(root, target.file_rel)?;
    // CMS-FR-VJRP: nothing is staged, committed, or armed for a commit — the log
    // stands outside every worktree.
    root.append_lines(&path, &[line]).map_err(|e| e.to_string())?;
    target.find(root, thread_id)
}

/// [`add_comment_in`] naming the conversation as a [`ThreadRef`] (CMS-FR-54).
pub fn add_comment_to(
    root: &crate::fs::RootFs,
    target: ThreadRef<'_>,
    thread_id: &str,
    body: String,
    quotes: Vec<CommentQuote>,
    attachments: Vec<AttachmentInput>,
    by: &Participant,
    at: &str,
) -> Result<Discussion, String> {
    let thread = target.find(root, thread_id)?;
    // CMS-FR-17: the one thing a lock prevents.
    if thread.locked {
        return Err(ERR_DISCUSSION_LOCKED.to_string());
    }
    for quote in &quotes {
        if !thread.comments.iter().any(|c| c.id == quote.comment_id) {
            return Err(ERR_QUOTED_COMMENT_NOT_IN_DISCUSSION.to_string());
        }
    }
    // Last of the checks and first of the writes: every other refusal above has
    // already had its say, so nothing reaches the attachments folder for an
    // append that was going to be refused anyway (CMS-FR-47).
    let stored = store_attachments(root, target.scope, attachments)?;
    append_as_scoped(
        root,
        target.scope,
        target.file_rel,
        by,
        at,
        vec![(
            thread_id.to_string(),
            EventBody::CommentAdded {
                comment_id: new_note_id(),
                body,
                quotes,
                attachments: stored,
            },
        )],
    )?;
    target.find(root, thread_id)
}

/// CMS-FR-18 / CMS-FR-19 / CMS-FR-20: set the lock, on a thread in any state of
/// its resolution. Setting the state it already holds appends nothing, so a
/// repeated lock does not accumulate lines or move `updated_at`.
pub fn set_lock_in(
    root: &crate::fs::RootFs,
    artifact_id: &str,
    thread_id: &str,
    locked: bool,
    by: &Participant,
    at: &str,
) -> Result<Discussion, String> {
    set_lock_reporting(root, artifact_id, thread_id, locked, by, at).map(|(thread, _)| thread)
}

/// [`set_lock_in`], additionally reporting whether a line was actually appended.
///
/// The pair exists because CMS-FR-19 makes "set it to what it already is" a
/// legitimate no-op and CMS-FR-52 makes that no-op *unobservable*: no event may
/// be emitted for a write that did not happen. Every caller that only wants the
/// resulting thread uses the plain form; the Tauri command uses this one, because
/// it is the one that has to decide whether to announce anything.
pub fn set_lock_reporting(
    root: &crate::fs::RootFs,
    artifact_id: &str,
    thread_id: &str,
    locked: bool,
    by: &Participant,
    at: &str,
) -> Result<(Discussion, bool), String> {
    set_lock_to(
        root,
        ThreadRef::artifact(artifact_id),
        thread_id,
        locked,
        by,
        at,
    )
}

/// [`set_lock_reporting`] naming the conversation as a [`ThreadRef`] (CMS-FR-54).
pub fn set_lock_to(
    root: &crate::fs::RootFs,
    target: ThreadRef<'_>,
    thread_id: &str,
    locked: bool,
    by: &Participant,
    at: &str,
) -> Result<(Discussion, bool), String> {
    let thread = target.find(root, thread_id)?;
    if thread.locked == locked {
        return Ok((thread, false));
    }
    let body = if locked {
        EventBody::ThreadLocked
    } else {
        EventBody::ThreadUnlocked
    };
    append_as_scoped(
        root,
        target.scope,
        target.file_rel,
        by,
        at,
        vec![(thread_id.to_string(), body)],
    )?;
    target.find(root, thread_id).map(|thread| (thread, true))
}

/// CMS-FR-18 / CMS-FR-19 / CMS-FR-20: set the resolution, independently of the
/// lock and on the same no-op terms.
pub fn set_resolution_in(
    root: &crate::fs::RootFs,
    artifact_id: &str,
    thread_id: &str,
    resolved: bool,
    by: &Participant,
    at: &str,
) -> Result<Discussion, String> {
    set_resolution_reporting(root, artifact_id, thread_id, resolved, by, at)
        .map(|(thread, _)| thread)
}

/// [`set_resolution_in`], reporting whether a line was appended. The counterpart
/// of [`set_lock_reporting`], and there for the same reason.
pub fn set_resolution_reporting(
    root: &crate::fs::RootFs,
    artifact_id: &str,
    thread_id: &str,
    resolved: bool,
    by: &Participant,
    at: &str,
) -> Result<(Discussion, bool), String> {
    set_resolution_to(
        root,
        ThreadRef::artifact(artifact_id),
        thread_id,
        resolved,
        by,
        at,
    )
}

/// [`set_resolution_reporting`] naming the conversation as a [`ThreadRef`]
/// (CMS-FR-54).
pub fn set_resolution_to(
    root: &crate::fs::RootFs,
    target: ThreadRef<'_>,
    thread_id: &str,
    resolved: bool,
    by: &Participant,
    at: &str,
) -> Result<(Discussion, bool), String> {
    let thread = target.find(root, thread_id)?;
    if thread.resolved == resolved {
        return Ok((thread, false));
    }
    let body = if resolved {
        EventBody::ThreadResolved
    } else {
        EventBody::ThreadReopened
    };
    append_as_scoped(
        root,
        target.scope,
        target.file_rel,
        by,
        at,
        vec![(thread_id.to_string(), body)],
    )?;
    target.find(root, thread_id).map(|thread| (thread, true))
}

/// CMS-FR-21 / CMS-FR-59: move a discussion's fragment to where its text now is.
///
/// Accepted on a locked or resolved discussion, because following the text a
/// discussion is attached to is not a contribution to the conversation: refusing
/// it would strand every settled discussion the moment the source above it grew.
///
/// A whole-target discussion has no fragment to follow, so it is a typed
/// `not_fragment_targeted` and nothing is appended. Moving a fragment to the
/// position it already holds appends nothing, and CMS-FR-52 makes that no-op
/// unobservable, so the second value reports whether a line was appended.
pub fn move_fragment_to(
    root: &crate::fs::RootFs,
    target: ThreadRef<'_>,
    discussion_id: &str,
    fragment_target: FragmentTarget,
    by: &Participant,
    at: &str,
) -> Result<(Discussion, bool), String> {
    if fragment_target.end <= fragment_target.start {
        return Err(ERR_INVALID_FRAGMENT.to_string());
    }
    let discussion = target.find(root, discussion_id)?;
    let Some(current) = discussion.fragment_target.as_ref() else {
        return Err(ERR_NOT_FRAGMENT_TARGETED.to_string());
    };
    if fragment_target.owner != discussion.target {
        return Err(ERR_INVALID_FRAGMENT.to_string());
    }
    if current.start == fragment_target.start
        && current.end == fragment_target.end
        && current.quote == fragment_target.quote
    {
        return Ok((discussion, false));
    }
    let moved = FragmentTarget {
        owner: current.owner.clone(),
        path: current.path.clone(),
        start: fragment_target.start,
        end: fragment_target.end,
        quote: fragment_target.quote,
    };
    append_as_scoped(
        root,
        target.scope,
        target.file_rel,
        by,
        at,
        vec![(
            discussion_id.to_string(),
            EventBody::FragmentMoved {
                fragment_target: moved,
            },
        )],
    )?;
    target.find(root, discussion_id).map(|d| (d, true))
}
