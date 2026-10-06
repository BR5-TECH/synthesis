//! The Tauri commands of the comments contract surface.

use super::*;

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// CMS-FR-11 / CMS-FR-12: who this machine writes as.
///
/// Resolved on every write rather than taken from the caller, so no frontend call
/// can attribute a comment to someone else, and stamped into the event so it is
/// never re-resolved afterwards — a later change of token does not rewrite the
/// authorship of an existing comment. A project that stores no token writes as
/// the local participant (CMS-FR-HTOA); every other refusal stays a refusal.
pub(crate) fn acting_participant(
    store: &GlobalSettingsStore,
    project: &ProjectState,
) -> Result<Participant, String> {
    participant_for_slot(store, &project.slot_key())
}

/// [`acting_participant`] for a project slot key, so the resolution is testable
/// without a running project.
pub(crate) fn participant_for_slot(
    store: &GlobalSettingsStore,
    slot_key: &str,
) -> Result<Participant, String> {
    Ok(resolve_github_identity_if_stored(store, slot_key)?
        .map_or_else(Participant::local_human, Participant::from))
}

/// The discussion a write command names, read from the store.
///
/// A discussion id alone names the conversation (CMS-FR-30): the owner, the
/// fragment and therefore the log are all read off the folded record, so no
/// command takes a locator beside the id.
fn discussion_for_write(
    root: &crate::fs::RootFs,
    worktree: &crate::fs::RootFs,
    discussion_id: &str,
) -> Result<Discussion, String> {
    read_discussion_by_id(root, worktree, discussion_id)
        .ok_or_else(|| ERR_DISCUSSION_NOT_FOUND.to_string())
}

/// CMS-FR-58: every discussion of one owner, fragment and whole-target.
///
/// Fragment discussions come first, ordered by `fragment_target.start` and then
/// `created_at`; whole-target ones follow, ordered by `created_at`. A note holds at
/// most one.
#[tauri::command]
pub fn list_discussions(
    target: DiscussionTarget,
    project: State<'_, ProjectState>,
) -> Result<Vec<Discussion>, String> {
    let root = project.require_store()?;
    Ok(list_discussions_in(&root, &target))
}

/// CMS-FR-62: the one discussion a note carries — returned if it has one,
/// created with the author's opening message if it does not.
///
/// Atomic and idempotent under concurrent and repeated calls, which is why it
/// exists rather than the UI checking and then opening: those two steps race,
/// and the thing they race over is whether the note ends up with two
/// conversations. The `"discussion changed"` event is emitted on the creating call
/// alone (CMS-FR-52) — a call that found an existing discussion appended nothing
/// and has nothing to announce.
#[tauri::command]
pub fn get_or_create_note_discussion<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    note_id: String,
    body: String,
    attachments: Vec<AttachmentInput>,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<Discussion, String> {
    let root = project.require_store()?;
    let worktree = project.require_root()?;
    let by = acting_participant(&store, &project)?;
    let (discussion, created) = get_or_create_note_discussion_in(
        &root,
        &worktree,
        &note_id,
        body,
        attachments,
        &by,
        &now_rfc3339(),
    )
    .inspect_err(|err| {
        crate::logging::log_warn(
            &app,
            &crate::logging::BUFFER,
            &[crate::logging::Domain::Backend],
            "note discussion could not be opened",
            crate::log_fields! { "noteId" => note_id.clone(), "error" => err.clone() },
        );
    })?;
    if created {
        crate::logging::log_info(
            &app,
            &crate::logging::BUFFER,
            &[crate::logging::Domain::Backend],
            "note discussion opened",
            crate::log_fields! { "noteId" => note_id.clone(), "discussionId" => discussion.id.clone() },
        );
        emit_discussion_changed(&app, &discussion);
    }
    Ok(discussion)
}

/// CMS-FR-32: every discussion the project holds, of every owner, for
/// `CMP-comments-panel.md`.
#[tauri::command]
pub fn list_all_discussions(
    project: State<'_, ProjectState>,
) -> Result<Vec<DiscussionListItem>, String> {
    let root = project.require_store()?;
    let worktree = project.require_root()?;
    Ok(list_all_discussions_in(&root, &worktree))
}

/// CMS-FR-61: the one discussion an id names, whatever its owner and whether or
/// not it has a fragment.
///
/// The only read keyed by a **discussion** rather than by the owner it belongs
/// to, and it exists for a caller that holds a conversation's identity and no
/// longer holds that owner: a conversation tab whose owner surface has closed, or
/// whose artifact has since been deleted. Writes nothing and emits nothing
/// (CMS-FR-52).
#[tauri::command]
pub fn read_discussion(
    discussion_id: String,
    project: State<'_, ProjectState>,
) -> Result<Discussion, String> {
    let root = project.require_store()?;
    let worktree = project.require_root()?;
    read_discussion_by_id(&root, &worktree, &discussion_id)
        .ok_or_else(|| ERR_DISCUSSION_NOT_FOUND.to_string())
}

#[tauri::command]
pub fn resolve_comment_author_identity(
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<Participant, String> {
    acting_participant(&store, &project)
}

/// CMS-FR-51 / CMS-FR-52: announce a discussion this module has just written into.
///
/// Called on the *success* path alone, which is what makes CMS-FR-52 true without
/// a second rule: a refused write returns early and never reaches here, and a
/// no-op lock or resolution returns the discussion unchanged from a path that
/// appended nothing — so a consumer never redraws for a write that did not happen.
///
/// The emit result is discarded deliberately: an event must never take down the
/// operation it reports on. A dropped event costs a redraw, and the surface's
/// next read corrects it; a propagated error would cost the author their comment.
pub fn emit_discussion_changed<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    discussion: &Discussion,
) {
    use tauri::Emitter as _;
    let _ = app.emit(DISCUSSION_CHANGED, discussion);
}

/// CMS-FR-57: open a discussion over a target — a whole one, or about a fragment
/// of an artifact or a draft prompt — and post its opening comment, both in one
/// append.
///
/// A `note` target is `not_supported`: a note's discussion is created by
/// `get_or_create_note_discussion`, so a note never holds two.
#[tauri::command]
pub fn open_discussion<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    target: DiscussionTarget,
    fragment_target: Option<FragmentTarget>,
    body: String,
    attachments: Vec<AttachmentInput>,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<Discussion, String> {
    let root = project.require_store()?;
    let worktree = project.require_root()?;
    let by = acting_participant(&store, &project)?;
    let discussion = open_discussion_in(
        &root,
        &worktree,
        &target,
        fragment_target,
        body,
        attachments,
        &by,
        &now_rfc3339(),
    )
    .inspect_err(|err| {
        crate::logging::log_warn(
            &app,
            &crate::logging::BUFFER,
            &[crate::logging::Domain::Backend],
            "discussion could not be opened",
            crate::log_fields! { "error" => err.clone() },
        );
    })?;
    crate::logging::log_info(
        &app,
        &crate::logging::BUFFER,
        &[crate::logging::Domain::Backend],
        "discussion opened",
        crate::log_fields! {
            "discussionId" => discussion.id.clone(),
            "fragment" => discussion.is_fragment_targeted(),
        },
    );
    emit_discussion_changed(&app, &discussion);
    Ok(discussion)
}

#[tauri::command]
pub fn add_comment<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    discussion_id: String,
    body: String,
    quotes: Vec<CommentQuote>,
    attachments: Vec<AttachmentInput>,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<Discussion, String> {
    let root = project.require_store()?;
    let worktree = project.require_root()?;
    let by = acting_participant(&store, &project)?;
    let current = discussion_for_write(&root, &worktree, &discussion_id)?;
    let discussion = add_comment_to(
        &root,
        current.log_ref(),
        &discussion_id,
        body,
        quotes,
        attachments,
        &by,
        &now_rfc3339(),
    )?;
    // AGC-FR-31 / CTA-FR-RUVS: a later **human** comment retires this
    // conversation's offer to retry a failed turn. Here rather than in the agent
    // write path because that is exactly the distinction the rule turns on: an
    // agent answering elsewhere in the discussion does not mean the author has
    // stopped wanting the answer that failed.
    crate::agent_conversations::retire_recoverable_for_human_comment(&app, &discussion_id);
    emit_discussion_changed(&app, &discussion);
    Ok(discussion)
}

/// CMS-FR-48: the stored bytes of an attachment, base64-encoded.
///
/// `discussion_id` rather than a bare digest because it is what resolves the scope
/// (CMS-FR-49) — the same digest can name a file in the project's folder and a
/// different draft's, and serving the wrong one would leak a draft's private
/// review into a project surface.
///
/// `draft_id` narrows *where the discussion is looked for first* and decides
/// nothing else: the scope served is still read off the folded discussion. It is
/// here because locating a discussion by its id alone means reading and folding
/// every log the scope holds, and this command is on the hot path — a card
/// carrying three screenshots calls it three times as it renders (CMT-FR-49).
#[tauri::command]
pub fn read_comment_attachment(
    discussion_id: String,
    digest: String,
    draft_id: Option<String>,
    project: State<'_, ProjectState>,
) -> Result<AttachmentContent, String> {
    let root = project.require_store()?;
    let worktree = project.require_root()?;
    let discussion =
        locate_discussion_for_attachment(&root, &worktree, &discussion_id, draft_id.as_deref())
            .ok_or(ERR_DISCUSSION_NOT_FOUND)?;
    read_attachment_in(&root, attachment_scope(&discussion), &discussion, &digest)
}

/// CMS-FR-49: which scope's attachment folder a discussion's blobs live in.
///
/// Read off the folded discussion rather than off the caller, so a caller that
/// named the wrong draft finds no discussion rather than being served another
/// scope's file.
pub(super) fn attachment_scope(discussion: &Discussion) -> LogScope<'_> {
    match &discussion.target {
        DiscussionTarget::Draft { draft_id } => LogScope::Draft { draft_id },
        // CMS-FR-49: a note's blobs live in that note's own folder.
        DiscussionTarget::Note { note_id } => LogScope::NoteDiscussion { note_id },
        DiscussionTarget::Artifact { .. } => LogScope::Artifact,
    }
}

/// The discussion an attachment read is against. A `draft_id` hint is tried first
/// and a miss falls through to the search over every owner.
pub(super) fn locate_discussion_for_attachment(
    root: &crate::fs::RootFs,
    worktree: &crate::fs::RootFs,
    discussion_id: &str,
    draft_id: Option<&str>,
) -> Option<Discussion> {
    if let Some(draft_id) = draft_id {
        let target = DiscussionTarget::Draft {
            draft_id: draft_id.to_string(),
        };
        if let Some(found) = locate_discussion_of(root, &target, discussion_id) {
            return Some(found);
        }
    }
    read_discussion_by_id(root, worktree, discussion_id)
}

#[tauri::command]
pub fn set_discussion_lock<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    discussion_id: String,
    locked: bool,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<Discussion, String> {
    let root = project.require_store()?;
    let worktree = project.require_root()?;
    let by = acting_participant(&store, &project)?;
    let current = discussion_for_write(&root, &worktree, &discussion_id)?;
    let (discussion, appended) = set_lock_to(
        &root,
        current.log_ref(),
        &discussion_id,
        locked,
        &by,
        &now_rfc3339(),
    )?;
    // CMS-FR-52: silent when the discussion already held the state asked for.
    if appended {
        emit_discussion_changed(&app, &discussion);
    }
    Ok(discussion)
}

#[tauri::command]
pub fn set_discussion_resolution<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    discussion_id: String,
    resolved: bool,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<Discussion, String> {
    let root = project.require_store()?;
    let worktree = project.require_root()?;
    let by = acting_participant(&store, &project)?;
    let current = discussion_for_write(&root, &worktree, &discussion_id)?;
    let (discussion, appended) = set_resolution_to(
        &root,
        current.log_ref(),
        &discussion_id,
        resolved,
        &by,
        &now_rfc3339(),
    )?;
    // CMS-FR-52: silent when the discussion already held the state asked for.
    if appended {
        emit_discussion_changed(&app, &discussion);
    }
    Ok(discussion)
}

/// CMS-FR-21: move a discussion's fragment to where its text now is.
#[tauri::command]
pub fn reanchor_discussion_fragment<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    discussion_id: String,
    fragment_target: FragmentTarget,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<Discussion, String> {
    let root = project.require_store()?;
    let worktree = project.require_root()?;
    let by = acting_participant(&store, &project)?;
    let current = discussion_for_write(&root, &worktree, &discussion_id)?;
    let (discussion, appended) = move_fragment_to(
        &root,
        current.log_ref(),
        &discussion_id,
        fragment_target,
        &by,
        &now_rfc3339(),
    )?;
    // CMS-FR-52: silent when the fragment already sat at this position.
    if appended {
        emit_discussion_changed(&app, &discussion);
    }
    Ok(discussion)
}
