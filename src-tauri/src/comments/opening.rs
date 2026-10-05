//! Appending a line, and opening a thread or a discussion.

use super::*;

// ---------------------------------------------------------------------------
// Writing (CMS-FR-11 … CMS-FR-21, CMS-FR-26, CMS-FR-27)
// ---------------------------------------------------------------------------

/// CMS-FR-21: a fragment is well formed, and belongs to the owner it is opened on.
///
/// Refused as `invalid_fragment`: an `end` that is not past its `start`, an owner
/// that is not the discussion's own target, an owner that is a note (a note has no
/// source to point into), and a draft fragment naming no file.
///
/// Returns the fragment as it is stored. The path of an artifact fragment is the
/// artifact's own path, so a caller cannot store a path other than its owner's.
pub(super) fn validate_fragment(
    target: &DiscussionTarget,
    fragment: &FragmentTarget,
) -> Result<FragmentTarget, String> {
    if fragment.end <= fragment.start || fragment.owner != *target {
        return Err(ERR_INVALID_FRAGMENT.to_string());
    }
    let path = match target {
        DiscussionTarget::Artifact { artifact_id } => artifact_id.clone(),
        DiscussionTarget::Draft { .. } => {
            if fragment.path.trim().is_empty() {
                return Err(ERR_INVALID_FRAGMENT.to_string());
            }
            fragment.path.clone()
        }
        DiscussionTarget::Note { .. } => return Err(ERR_INVALID_FRAGMENT.to_string()),
    };
    Ok(FragmentTarget {
        path,
        ..fragment.clone()
    })
}

/// CMS-FR-26 / CMS-FR-27: the single write path.
///
/// Everything that reaches the log goes through here — the Tauri commands below,
/// and (when one exists) an agent posting through the internal API. It takes a
/// `Participant` rather than resolving one, which is what lets an agent be
/// attributed on exactly the same terms as a human without the commands' identity
/// resolution standing in the way.
///
/// The events of one call are appended as a single write (`append_lines`), so the
/// pair CMS-FR-14 writes cannot be split by a concurrent appender.
pub fn append_as(
    root: &crate::fs::RootFs,
    artifact_id: &str,
    by: &Participant,
    at: &str,
    bodies: Vec<(String, EventBody)>,
) -> Result<(), String> {
    append_as_scoped(root, LogScope::Artifact, artifact_id, by, at, bodies)
}

/// [`append_as`] with the scope of CMS-FR-36 made explicit.
///
/// One writer rather than two: an agent's comment (CMS-FR-41) and an author's
/// travel exactly the same path, so a rule added here reaches both and a
/// `draft`-scoped log is written with the same event shape an `artifact`-scoped
/// one is.
pub fn append_as_scoped(
    root: &fsa::RootFs,
    scope: LogScope<'_>,
    file_rel: &str,
    by: &Participant,
    at: &str,
    bodies: Vec<(String, EventBody)>,
) -> Result<(), String> {
    if bodies.is_empty() {
        return Ok(());
    }
    let path = scope.log_path(root, file_rel)?;
    let lines: Result<Vec<String>, String> = bodies
        .into_iter()
        .map(|(thread_id, body)| {
            let event = Event {
                v: SCHEMA_VERSION,
                // CMS-FR-06: opaque and unique, so a duplicated line is
                // recognisable as the same event rather than a second one.
                event_id: new_note_id(),
                thread_id,
                at: at.to_string(),
                by: by.clone(),
                body,
            };
            serde_json::to_string(&event).map_err(|e| format!("could not encode event: {e}"))
        })
        .collect();
    // CMS-FR-VJRP: nothing is staged, committed, or armed for a commit. The log
    // stands in the repository machine store, outside every worktree
    // (`RMS-repository-machine-storage.md` RMS-FR-HDNZ), so an append modifies
    // no checkout and there is nothing for Git to carry.
    root.append_lines(&path, &lines?).map_err(|e| e.to_string())?;
    Ok(())
}

/// CMS-FR-TXRB: append an ordered batch whose lines carry **their own**
/// participant and **their own** event identity.
///
/// [`append_as_scoped`] stamps one `by` across a whole batch and mints an
/// `event_id` per line, which is right for every write a single participant
/// makes. A question-set submission is neither: it alternates an agent-authored
/// question with a human-authored answer (CMS-FR-41, CMS-FR-11), and every line
/// carries an identity derived from the set id and the question's position
/// (ADQ-FR-YQTB) so a retry appends lines the fold already ignores (CMS-FR-06).
///
/// One `append_lines` call for the whole batch, exactly as CMS-FR-14 relies on:
/// a submission that reached disk reached it entire, so no reader ever sees a
/// question whose answer is still to come.
///
/// Deliberately not a Tauri command. The only caller is the submission, which
/// composes every body itself.
pub fn append_events_with_identities(
    root: &fsa::RootFs,
    scope: LogScope<'_>,
    file_rel: &str,
    at: &str,
    events: Vec<(String, String, Participant, EventBody)>,
) -> Result<(), String> {
    if events.is_empty() {
        return Ok(());
    }
    let path = scope.log_path(root, file_rel)?;
    let lines: Result<Vec<String>, String> = events
        .into_iter()
        .map(|(thread_id, event_id, by, body)| {
            let event = Event {
                v: SCHEMA_VERSION,
                event_id,
                thread_id,
                at: at.to_string(),
                by,
                body,
            };
            serde_json::to_string(&event).map_err(|e| format!("could not encode event: {e}"))
        })
        .collect();
    root.append_lines(&path, &lines?).map_err(|e| e.to_string())?;
    Ok(())
}

/// A fragment discussion of an artifact, opened without the checks of the open
/// project.
///
/// `position` supplies the range and the quote. The owner and the path are the
/// artifact's own, so a caller cannot store a path other than its owner's.
pub fn open_artifact_fragment_in(
    root: &crate::fs::RootFs,
    artifact_id: &str,
    position: FragmentTarget,
    body: String,
    attachments: Vec<AttachmentInput>,
    by: &Participant,
    at: &str,
) -> Result<Discussion, String> {
    let target = DiscussionTarget::Artifact {
        artifact_id: artifact_id.to_string(),
    };
    let fragment = validate_fragment(
        &target,
        &FragmentTarget::in_artifact(artifact_id, position.start, position.end, &position.quote),
    )?;
    open_discussion_unchecked(root, &target, Some(fragment), body, attachments, by, at)
}

/// CMS-FR-14 / CMS-FR-57: open a discussion and post its opening comment in one
/// append, into the log its target and fragment select.
///
/// The two events are written together so a discussion is never created empty
/// (CMS-FR-15 refuses to serve one this module never produces). A `note` target is
/// `not_supported`: a note's discussion is opened through
/// [`get_or_create_note_discussion_in`] alone, so there is no route by which a note
/// ends up with two. A fragment on any other owner is checked by
/// [`validate_fragment`]. A `draft_id` naming no draft, or an artifact whole-target
/// discussion naming no file, appends nothing.
pub fn open_discussion_in(
    root: &crate::fs::RootFs,
    worktree: &crate::fs::RootFs,
    target: &DiscussionTarget,
    fragment_target: Option<FragmentTarget>,
    body: String,
    attachments: Vec<AttachmentInput>,
    by: &Participant,
    at: &str,
) -> Result<Discussion, String> {
    if matches!(target, DiscussionTarget::Note { .. }) {
        return Err(ERR_NOT_SUPPORTED.to_string());
    }
    let fragment = fragment_target
        .as_ref()
        .map(|f| validate_fragment(target, f))
        .transpose()?;
    // CMS-FR-57: checked before anything is stored, so a target naming nothing in
    // the open project leaves neither a log nor an orphaned attachment behind.
    match target {
        DiscussionTarget::Draft { draft_id } => {
            if crate::drafts::read_draft_record(worktree, draft_id).is_err() {
                return Err(ERR_DRAFT_NOT_FOUND.to_string());
            }
        }
        DiscussionTarget::Artifact { artifact_id } if fragment.is_none() => {
            let exists = fsa::resolve_under(worktree.path(), artifact_id)
                .ok()
                .and_then(|p| worktree.file_info(p).ok())
                .is_some_and(|i| i.kind == fsa::EntryKind::File);
            if !exists {
                return Err(ERR_ARTIFACT_NOT_FOUND.to_string());
            }
        }
        _ => {}
    }
    open_discussion_unchecked(root, target, fragment, body, attachments, by, at)
}

/// The append of [`open_discussion_in`] without the checks of the open project.
///
/// Public so the writers that already hold a validated owner, and the tests that
/// build logs without a worktree, reach the one opening path rather than a second
/// one. The fragment is taken as given.
pub fn open_discussion_unchecked(
    root: &crate::fs::RootFs,
    target: &DiscussionTarget,
    fragment_target: Option<FragmentTarget>,
    body: String,
    attachments: Vec<AttachmentInput>,
    by: &Participant,
    at: &str,
) -> Result<Discussion, String> {
    let log = log_ref_of(target, fragment_target.as_ref());
    // CMS-FR-45 / CMS-FR-47: every attachment is stored before a line is written,
    // so a refusal of one leaves no discussion behind and no folded comment can
    // name a blob that is absent.
    let stored = store_attachments(root, log.scope, attachments)?;
    let discussion_id = new_note_id();
    append_as_scoped(
        root,
        log.scope,
        log.file_rel,
        by,
        at,
        vec![
            (
                discussion_id.clone(),
                EventBody::DiscussionOpened {
                    target: Some(target.clone()),
                    draft_id: None,
                    fragment_target: fragment_target.clone(),
                },
            ),
            (
                discussion_id.clone(),
                EventBody::CommentAdded {
                    comment_id: new_note_id(),
                    body,
                    quotes: Vec::new(),
                    attachments: stored,
                },
            ),
        ],
    )?;
    log.find(root, &discussion_id)
}

/// The log a target and an optional fragment select.
pub(super) fn log_ref_of<'a>(
    target: &'a DiscussionTarget,
    fragment: Option<&'a FragmentTarget>,
) -> ThreadRef<'a> {
    match (target, fragment) {
        (DiscussionTarget::Artifact { artifact_id }, Some(_)) => ThreadRef::artifact(artifact_id),
        (DiscussionTarget::Artifact { artifact_id }, None) => {
            ThreadRef::artifact_discussion(artifact_id)
        }
        (DiscussionTarget::Draft { draft_id }, Some(f)) => ThreadRef::draft_file(draft_id, &f.path),
        (DiscussionTarget::Draft { draft_id }, None) => ThreadRef::discussion(draft_id),
        (DiscussionTarget::Note { note_id }, _) => ThreadRef::note_discussion(note_id),
    }
}

/// CMS-FR-62: serializes the decide-and-commit of a note's one discussion.
///
/// A single lock rather than one per note: the guarded section is two file
/// operations against a folder no other note touches, so contention between two
/// different notes is a few microseconds and the simpler invariant is worth more
/// than the parallelism. What it buys is the whole of CMS-FR-62's concurrency
/// clause — two calls arriving together resolve to one thread with one opening
/// comment, because the second reads the log the first has already written
/// rather than the emptiness both saw.
pub(super) static NOTE_DISCUSSION_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// CMS-FR-62: the one discussion a note carries — returned if it has one,
/// created with `body` as its single opening comment if it does not.
///
/// The association *is* the log's existence, so this is the whole of it: there
/// is no field on the note record to keep in step, nothing to infer from a
/// comment's text, and no second write that a crash could leave half-applied.
/// The opening pair is one `append_lines` call like every other opening
/// (CMS-FR-14), so a note is left either with the whole committed discussion or
/// with none — never with a log holding an opening event and no comment.
pub fn get_or_create_note_discussion_in(
    root: &crate::fs::RootFs,
    worktree: &crate::fs::RootFs,
    note_id: &str,
    body: String,
    attachments: Vec<AttachmentInput>,
    by: &Participant,
    at: &str,
) -> Result<(Discussion, bool), String> {
    let _guard = lock_note_discussions();

    // CMS-FR-62 / NTC-FR-22: checked **inside** the lock, which is what the lock
    // is for as much as the create-vs-create race is. Deletion takes this same
    // lock (CMS-FR-64), so a note that exists here cannot be removed out from
    // under the append below — where a check outside it would let a deletion
    // land between the check and the write and leave a conversation for a note
    // that is gone, reachable from nowhere and answerable by nobody.
    if !crate::notes::note_exists(worktree, note_id) {
        return Err(ERR_NOTE_NOT_FOUND.to_string());
    }

    // CMS-FR-62: an existing association is returned with nothing appended — not
    // a second discussion, not a second opening comment, and (by returning
    // early) no event.
    if let Some(existing) = note_discussion_of(root, note_id) {
        return Ok((existing, false));
    }

    // CMS-FR-14: a discussion is never created empty, so the body is refused
    // before a folder is created rather than after.
    if body.trim().is_empty() {
        return Err(ERR_EMPTY_BODY.to_string());
    }

    let thread_ref = ThreadRef::note_discussion(note_id);
    let stored = store_attachments(root, thread_ref.scope, attachments)?;
    let thread_id = new_note_id();
    append_as_scoped(
        root,
        thread_ref.scope,
        thread_ref.file_rel,
        by,
        at,
        vec![
            (
                thread_id.clone(),
                EventBody::DiscussionOpened {
                    target: Some(DiscussionTarget::Note {
                        note_id: note_id.to_string(),
                    }),
                    draft_id: None,
                    fragment_target: None,
                },
            ),
            (
                thread_id.clone(),
                EventBody::CommentAdded {
                    comment_id: new_note_id(),
                    body,
                    quotes: Vec::new(),
                    attachments: stored,
                },
            ),
        ],
    )?;
    thread_ref.find(root, &thread_id).map(|t| (t, true))
}

/// CMS-FR-63: which notes carry a discussion, in one pass over the notes root.
///
/// One directory listing plus one fold per note that has a folder, rather than
/// one read per note in the project — a panel listing two hundred notes learns
/// which of them have conversations without two hundred reads. A folder holding
/// no log, or one whose log folds to no thread, contributes no entry.
pub fn note_discussion_index(root: &crate::fs::RootFs) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Ok(dir) = fsa::resolve_under(root, format!("{COMMENTS_REL}/{NOTES_SUBDIR}")) else {
        return out;
    };
    let Ok(entries) = root.list_dir(&dir) else {
        // No notes comments folder at all is simply no discussions.
        return out;
    };
    for entry in entries {
        // A folder left behind by a delete whose second step failed (CMS-FR-64)
        // is not a note and contributes no entry: its name is not one
        // `is_valid_id` admits, so no note could ever match it, but skipping it
        // explicitly keeps the index a map of *notes* rather than of directories.
        if entry.name.starts_with(DISCARDED_PREFIX) {
            continue;
        }
        // The folder name *is* the note id (CMS-FR-37). A stray file among the
        // folders is skipped rather than read.
        if let Some(thread) = note_discussion_of(root, &entry.name) {
            out.insert(entry.name, thread.id);
        }
    }
    out
}

/// CMS-FR-64: remove a note's conversation — its log and its stored attachments
/// together — in one recursive delete.
///
/// One step rather than a sequence, which is what makes it recoverable: there is
/// no ordering inside it for a crash to fall between. **Idempotent**: a note
/// carrying no discussion is reported deleted having had nothing to delete, so a
/// retried deletion transaction never fails on work already done (NTC-FR-21).
///
/// Emits no `"discussion changed"` — the thread is gone rather than changed
/// — and touches no log of any other scope and no other note's folder.
pub fn delete_note_discussion_in(root: &crate::fs::RootFs, note_id: &str) -> Result<(), String> {
    let _guard = lock_note_discussions();
    delete_note_discussion_locked(root, note_id)
}

/// CMS-FR-39: remove one draft's whole conversation storage — its logs and its
/// stored attachments together — in one recursive delete.
///
/// Reached from `crate::drafts`' deletion alone (`DRS-draft-storage.md`
/// DRS-FR-WNTA) and registered as no Tauri command. **Idempotent**: a draft
/// carrying no conversation is reported deleted having had nothing to delete,
/// which is what makes a second `delete_draft` after a partial one succeed.
/// **Retryable**: a failure leaves the folder exactly as it stands and returns a
/// typed error the caller repeats.
pub fn delete_draft_comments(root: &crate::fs::RootFs, draft_id: &str) -> Result<(), String> {
    // Checked before a path is composed at all: this is a recursive removal, and
    // a `draft_id` carrying `..` would otherwise name a directory the escape
    // gate would still admit (see `is_storage_id`).
    let dir = draft_storage_dir(root, draft_id)?;
    match root.delete_under(root.path(), &dir, true) {
        Ok(()) => Ok(()),
        // Nothing to remove is the idempotent success, not a failure.
        Err(fsa::FsError::NotFound { .. }) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// CMS-FR-62 / NTC-FR-22: hold the note-discussion lock across a caller's own
/// sequence of steps.
///
/// `delete_note` is a **two-step** transaction — this module's cleanup, then the
/// note file's removal (NTC-FR-21) — and both steps have to be inside one
/// critical section rather than only the first. Releasing between them would
/// reopen exactly the race the lock closes: an opening could acquire the lock in
/// the gap, find the note still on disk, and append a conversation that the
/// second step then orphans.
///
/// The guard is opaque, so the only thing a caller can do with it is hold it.
/// The lock is **not** reentrant, so a holder must call
/// [`delete_note_discussion_locked`] rather than [`delete_note_discussion_in`].
pub fn lock_note_discussions() -> std::sync::MutexGuard<'static, ()> {
    // Poisoning would mean a previous holder panicked mid-decision. The lock
    // guards no in-memory invariant — the log on disk is the only state — so
    // recovering is right where propagating would wedge the feature for the
    // rest of the session.
    NOTE_DISCUSSION_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// [`delete_note_discussion_in`] for a caller already holding the lock
/// (NTC-FR-21).
pub fn delete_note_discussion_locked(
    root: &crate::fs::RootFs,
    note_id: &str,
) -> Result<(), String> {
    // Checked before a path is composed at all: this is a *recursive* removal,
    // and a `note_id` carrying `..` would otherwise name a directory outside the
    // notes folder that the escape gate would still admit (see `is_note_id`).
    if !is_note_id(note_id) {
        return Err(ERR_NOTE_NOT_FOUND.to_string());
    }
    let rel = note_comments_rel(note_id);
    // CMS-FR-64: the removal is a **rename aside, then a delete**, and the order
    // is what makes "a failure leaves the folder as it stands" true rather than
    // merely intended.
    //
    // A recursive delete walks bottom-up, so one that cannot finish leaves the
    // folder half-emptied — and half-emptied means the log is gone while the
    // caller is being told nothing was deleted, so the note reads as carrying no
    // conversation while its deletion reports a failure. The rename is a single
    // atomic step: it either moves the whole conversation out of the address
    // everything resolves it through, or it changes nothing at all. Only then is
    // anything deleted, and a failure of *that* leaves bytes nobody can reach
    // rather than a conversation that half exists.
    let aside = format!("{DISCARDED_PREFIX}{note_id}");
    match root.rename_under(root.path(), &rel, &aside) {
        Ok(()) => {}
        // Nothing to remove is the idempotent success, not a failure.
        Err(fsa::FsError::NotFound { .. }) => return Ok(()),
        Err(e) => return Err(e.to_string()),
    }
    // The conversation is already unreachable at this point, so a failure here
    // is not one the caller can act on and must not fail the transaction: the
    // note's discussion is gone by every route that resolves one.
    let discarded = format!("{COMMENTS_REL}/{NOTES_SUBDIR}/{aside}");
    let _ = root.delete_under(root.path(), &discarded, true);
    Ok(())
}
