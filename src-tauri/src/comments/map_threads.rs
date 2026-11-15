//! The Map owner: the Map thread and the node threads (CMS-FR-RHXL, CMS-FR-SJTY,
//! CMS-FR-ACLX, CMS-FR-QUMW; `MPR-map-plan-proposals.md` MPR-FR-NRNZ, MPR-FR-MYLA,
//! MPR-FR-MAXH).
//!
//! All Map threads share the one log `comments/map/discussion.jsonl`. A thread
//! with no node reference is the Map thread, and there is at most one. A thread
//! with a node reference is a node thread, and there is at most one for each
//! node. The association is the log itself, as for a note (CMS-FR-62), so the
//! find-or-create step runs under a lock.

use super::*;

/// MPR-FR-RVRN: the subfolder of the plan proposals in the Map folder.
const MAP_PROPOSALS_SUBDIR: &str = "proposals";

/// MPR-FR-RVTT: no project is open.
const ERR_NO_PROJECT_OPEN: &str = "no_project_open";

/// CMS-FR-SJTY: serializes the find-and-create step, so two calls that arrive
/// together for one node resolve to one thread with one opening comment.
static MAP_DISCUSSION_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// What [`get_or_create_map_discussion_in`] did.
pub struct MapWrite {
    /// The thread, as it stands after the write.
    pub discussion: Discussion,
    /// The id of the comment that the call wrote.
    pub comment_id: String,
    /// True where the call created the thread.
    pub created: bool,
}

/// MPR-FR-MYLA: the thread of a node reference, or the Map thread where the
/// reference is absent. A node reference matches on `lens`, `kind`, and `id`.
/// Creates nothing.
pub fn find_map_discussion_in(
    root: &crate::fs::RootFs,
    map_node: Option<&MapNodeRef>,
) -> Option<Discussion> {
    fold_map_discussions(root).into_iter().find(|d| same_thread(d.map_node.as_ref(), map_node))
}

fn same_thread(thread: Option<&MapNodeRef>, wanted: Option<&MapNodeRef>) -> bool {
    match (thread, wanted) {
        (None, None) => true,
        (Some(thread), Some(wanted)) => thread.names_same_node(wanted),
        _ => false,
    }
}

/// MPR-FR-NRNZ: every Map thread, the Map thread first and then the node
/// threads by `updated_at` descending. The id breaks a tie.
pub fn list_map_discussions_in(root: &crate::fs::RootFs) -> Vec<Discussion> {
    let mut threads = fold_map_discussions(root);
    threads.sort_by(|a, b| {
        a.map_node
            .is_some()
            .cmp(&b.map_node.is_some())
            .then_with(|| b.updated_at.cmp(&a.updated_at))
            .then_with(|| a.id.cmp(&b.id))
    });
    threads
}

/// MPR-FR-MAXH: hold the Map-thread lock across a caller's own sequence of steps.
fn lock_map_discussions() -> std::sync::MutexGuard<'static, ()> {
    // The lock guards no in-memory state: the log on disk is the only state, so
    // a lock that a panic poisoned is safe to take again.
    MAP_DISCUSSION_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// MPR-FR-MAXH: the thread of a node reference. Where it exists, the comment
/// is appended to it. Where it does not, the thread and its first comment are
/// created in one append (CMS-FR-14).
///
/// An opening with an empty body is refused with `empty_body` before anything is
/// written. An existing thread that is resolved refuses the comment with
/// `discussion_resolved`, as every discussion does (CMS-FR-17).
pub fn get_or_create_map_discussion_in(
    root: &crate::fs::RootFs,
    map_node: Option<MapNodeRef>,
    opening: Opening,
    stamp: Stamp<'_>,
) -> Result<MapWrite, String> {
    let Stamp { by, at } = stamp;
    let Opening { body, attachments, spoken_original } = opening;
    let _guard = lock_map_discussions();

    if let Some(existing) = find_map_discussion_in(root, map_node.as_ref()) {
        let (discussion, comment_id) = add_comment_reporting_id(
            root,
            ThreadRef::map_discussion(),
            &existing.id,
            body,
            Vec::new(),
            attachments,
            spoken_original,
            Stamp { by, at },
        )?;
        return Ok(MapWrite { discussion, comment_id, created: false });
    }

    // CMS-FR-14: a discussion is never created empty.
    if body.trim().is_empty() {
        return Err(ERR_EMPTY_BODY.to_string());
    }
    let thread_ref = ThreadRef::map_discussion();
    let stored = store_attachments(root, thread_ref.scope, attachments)?;
    let thread_id = new_note_id();
    let comment_id = new_note_id();
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
                    target: Some(DiscussionTarget::Map),
                    draft_id: None,
                    fragment_target: None,
                    map_node,
                },
            ),
            (
                thread_id.clone(),
                EventBody::CommentAdded {
                    comment_id: comment_id.clone(),
                    body,
                    quotes: Vec::new(),
                    attachments: stored,
                    option_descriptions: Vec::new(),
                    spoken_original: spoken_original.filter(|t| !t.trim().is_empty()),
                    tool_calls: Vec::new(),
                },
            ),
        ],
    )?;
    let discussion = thread_ref.find(root, &thread_id)?;
    Ok(MapWrite { discussion, comment_id, created: true })
}

/// MPR-FR-RVRN: the folder of the plan proposals, `comments/map/proposals/`,
/// beside the log of the Map threads and behind the FSA-FR-10 escape gate.
pub fn map_proposals_dir(root: &crate::fs::RootFs) -> Result<PathBuf, String> {
    fsa::resolve_under(map_comments_dir(root)?, MAP_PROPOSALS_SUBDIR).map_err(|e| e.to_string())
}

/// MPR-FR-RVRN, CMS-FR-OIUO: append the one agent comment that announces a plan
/// proposal to a Map thread. The comment carries one `map_plan_proposal`
/// attachment, and the caller chose its id, so the record of the proposal can
/// name it. A thread that does not exist is `discussion_not_found`, and a
/// thread that is resolved refuses the comment as every thread does (CMS-FR-17).
#[allow(clippy::too_many_arguments)]
// Reason: one comment has these parts, and the caller holds each of them already.
pub fn append_map_proposal_comment(
    root: &crate::fs::RootFs,
    thread_id: &str,
    comment_id: String,
    body: String,
    spoken_original: Option<String>,
    proposal_id: &str,
    stamp: Stamp<'_>,
) -> Result<Discussion, String> {
    let Stamp { by, at } = stamp;
    let thread_ref = ThreadRef::map_discussion();
    let thread = thread_ref.find(root, thread_id)?;
    if thread.resolved {
        return Err(ERR_DISCUSSION_RESOLVED.to_string());
    }
    let attachments = turn_marks::with_turn_marks(
        thread_id,
        by,
        vec![Attachment::MapPlanProposal { proposal_id: proposal_id.to_string() }],
    );
    append_as_scoped(
        root,
        thread_ref.scope,
        thread_ref.file_rel,
        by,
        at,
        vec![(
            thread_id.to_string(),
            EventBody::CommentAdded {
                comment_id,
                body,
                quotes: Vec::new(),
                attachments,
                option_descriptions: Vec::new(),
                spoken_original: spoken_original.filter(|t| !t.trim().is_empty()),
                tool_calls: Vec::new(),
            },
        )],
    )?;
    thread_ref.find(root, thread_id)
}

/// MPR-FR-RVTT: the store of the open project, or `no_project_open`.
fn open_store(project: &ProjectState) -> Result<crate::fs::RootFs, String> {
    project.require_root().map_err(|_| ERR_NO_PROJECT_OPEN.to_string())?;
    project.require_store()
}

/// MPR-FR-MYLA, `"find map discussion (map node)"`: the thread of the node
/// reference, or the Map thread where the reference is absent, or nothing. It
/// creates nothing and emits nothing.
#[tauri::command]
pub fn find_map_discussion<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    map_node: Option<MapNodeRef>,
    project: State<'_, ProjectState>,
) -> Result<Option<Discussion>, String> {
    let root = open_store(&project).inspect_err(|err| log_map_refusal(&app, "find", err))?;
    let found = find_map_discussion_in(&root, map_node.as_ref());
    crate::logging::log_debug(
        &app,
        &crate::logging::BUFFER,
        &[crate::logging::Domain::Backend],
        "map discussion looked up",
        crate::log_fields! { "node" => map_node.is_some(), "found" => found.is_some() },
    );
    Ok(found)
}

/// MPR-FR-MAXH, `"get or create map discussion (map node, body, attachments)"`.
///
/// A human write command (CMS-FR-VRZA): the participant is resolved on this
/// machine, and the comment is given to the author-comment bookkeeping of the
/// agent conversations (AGC-FR-TFEN). The `"discussion changed"` event follows
/// each write.
#[tauri::command]
pub fn get_or_create_map_discussion<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    map_node: Option<MapNodeRef>,
    body: String,
    attachments: Vec<AttachmentInput>,
    spoken_original: Option<String>,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<Discussion, String> {
    let root = open_store(&project).inspect_err(|err| log_map_refusal(&app, "write", err))?;
    let by = acting_participant(&store, &project)?;
    let has_node = map_node.is_some();
    let write = get_or_create_map_discussion_in(
        &root,
        map_node,
        Opening { body, attachments, spoken_original },
        Stamp { by: &by, at: &now_rfc3339() },
    )
    .inspect_err(|err| log_map_refusal(&app, "write", err))?;
    // CMS-FR-VRZA: the written comment is an author comment (AGC-FR-TFEN).
    crate::agent_conversations::note_author_comment(&app, &write.comment_id);
    if !write.created {
        // AGC-FR-31: a later human comment retires the offer to retry a failed turn.
        crate::agent_conversations::retire_recoverable_for_human_comment(
            &app,
            &write.discussion.id,
        );
    }
    crate::logging::log_info(
        &app,
        &crate::logging::BUFFER,
        &[crate::logging::Domain::Backend],
        if write.created { "map discussion opened" } else { "map discussion commented" },
        crate::log_fields! {
            "discussionId" => write.discussion.id.clone(),
            "node" => has_node,
        },
    );
    emit_discussion_changed(&app, &write.discussion);
    Ok(write.discussion)
}

/// MPR-FR-NRNZ, `"list map discussions"`: the Map thread first, then the node
/// threads by `updated_at` descending.
#[tauri::command]
pub fn list_map_discussions<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    project: State<'_, ProjectState>,
) -> Result<Vec<Discussion>, String> {
    let root = open_store(&project).inspect_err(|err| log_map_refusal(&app, "list", err))?;
    Ok(list_map_discussions_in(&root))
}

/// MPR-FR-KZRN: a refusal, with the operation and the typed error and nothing
/// of the node or the comment.
fn log_map_refusal<R: tauri::Runtime>(app: &tauri::AppHandle<R>, operation: &str, error: &str) {
    crate::logging::log_warn(
        app,
        &crate::logging::BUFFER,
        &[crate::logging::Domain::Backend],
        "map discussion operation refused",
        crate::log_fields! { "operation" => operation, "error" => error },
    );
}
