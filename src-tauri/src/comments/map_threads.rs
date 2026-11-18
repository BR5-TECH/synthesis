//! The Map owner: the Map thread and the node threads (CMS-FR-RHXL, CMS-FR-SJTY,
//! CMS-FR-ACLX, CMS-FR-QUMW; `MPR-map-plan-proposals.md` MPR-FR-NRNZ, MPR-FR-MYLA,
//! MPR-FR-MAXH).
//!
//! All Map threads share the one log `comments/map/discussion.jsonl`. A thread
//! with no node reference is a whole-map thread, and a thread with a node
//! reference is a node thread. The Map holds any number of each, and several
//! threads can carry the same node (CMS-FR-SJTY). The opening step runs under a
//! lock, so two openings that arrive together append in turn.

use super::*;

/// MPR-FR-RVRN: the subfolder of the plan proposals in the Map folder.
const MAP_PROPOSALS_SUBDIR: &str = "proposals";

/// MPR-FR-RVTT: no project is open.
const ERR_NO_PROJECT_OPEN: &str = "no_project_open";

/// MPR-FR-MAXH: serializes the opening step, so two openings that arrive
/// together each append one whole thread.
static MAP_DISCUSSION_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// What [`open_map_discussion_in`] did.
pub struct MapWrite {
    /// The new thread, as it stands after the write.
    pub discussion: Discussion,
    /// The id of the opening comment.
    pub comment_id: String,
}

/// MPR-FR-MYLA: the open thread of a node reference with the latest
/// `updated_at`, or the open whole-map thread with the latest `updated_at`
/// where the reference is absent. A resolved thread is never returned. A node
/// reference matches on `lens`, `kind`, and `id`. Creates nothing.
pub fn find_map_discussion_in(
    root: &crate::fs::RootFs,
    map_node: Option<&MapNodeRef>,
) -> Option<Discussion> {
    fold_map_discussions(root)
        .into_iter()
        .filter(|d| !d.resolved && same_thread(d.map_node.as_ref(), map_node))
        // The id breaks a tie, so the answer does not depend on the fold order.
        .max_by(|a, b| a.updated_at.cmp(&b.updated_at).then_with(|| a.id.cmp(&b.id)))
}

fn same_thread(thread: Option<&MapNodeRef>, wanted: Option<&MapNodeRef>) -> bool {
    match (thread, wanted) {
        (None, None) => true,
        (Some(thread), Some(wanted)) => thread.names_same_node(wanted),
        _ => false,
    }
}

/// MPR-FR-NRNZ: every Map thread, resolved ones included, in the order they
/// were opened. The id breaks a tie.
pub fn list_map_discussions_in(root: &crate::fs::RootFs) -> Vec<Discussion> {
    let mut threads = fold_map_discussions(root);
    threads.sort_by(|a, b| a.created_at.cmp(&b.created_at).then_with(|| a.id.cmp(&b.id)));
    threads
}

/// MPR-FR-MAXH: hold the Map-thread lock across a caller's own sequence of steps.
fn lock_map_discussions() -> std::sync::MutexGuard<'static, ()> {
    // The lock guards no in-memory state: the log on disk is the only state, so
    // a lock that a panic poisoned is safe to take again.
    MAP_DISCUSSION_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// MPR-FR-MAXH: a new thread with the node reference, or with none, and its
/// first comment, in one append (CMS-FR-14). It never appends to an existing
/// thread, so a resolved thread of the same node blocks nothing.
///
/// An opening with an empty body is refused with `empty_body` before anything is
/// written.
pub fn open_map_discussion_in(
    root: &crate::fs::RootFs,
    map_node: Option<MapNodeRef>,
    opening: Opening,
    stamp: Stamp<'_>,
) -> Result<MapWrite, String> {
    let Stamp { by, at } = stamp;
    let Opening { body, attachments, spoken_original } = opening;
    let _guard = lock_map_discussions();

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
    Ok(MapWrite { discussion, comment_id })
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
    update: Option<super::ProposalUpdateCounts>,
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
        vec![Attachment::MapPlanProposal { proposal_id: proposal_id.to_string(), update }],
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

/// MPR-FR-MYLA, `"find map discussion (map node)"`: the latest open thread of
/// the node reference, or the latest open whole-map thread where the reference
/// is absent, or nothing. It creates nothing and emits nothing.
#[project_scope_macros::project_scoped]
#[tauri::command]
pub fn find_map_discussion<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    map_node: Option<MapNodeRef>,
    project: crate::project_scope::Scoped<'_, ProjectState>,
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

/// MPR-FR-MAXH, `"open map discussion (map node, body, attachments)"`.
///
/// A human write command (CMS-FR-VRZA): the participant is resolved on this
/// machine, and the comment is given to the author-comment bookkeeping of the
/// agent conversations (AGC-FR-TFEN). The `"discussion changed"` event follows
/// each write.
#[project_scope_macros::project_scoped]
#[tauri::command]
pub fn open_map_discussion<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    map_node: Option<MapNodeRef>,
    body: String,
    attachments: Vec<AttachmentInput>,
    spoken_original: Option<String>,
    store: State<'_, GlobalSettingsStore>,
    project: crate::project_scope::Scoped<'_, ProjectState>,
) -> Result<Discussion, String> {
    let root = open_store(&project).inspect_err(|err| log_map_refusal(&app, "write", err))?;
    let by = acting_participant(&store, &project)
        .inspect_err(|err| log_map_refusal(&app, "write", err))?;
    let has_node = map_node.is_some();
    let write = open_map_discussion_in(
        &root,
        map_node,
        Opening { body, attachments, spoken_original },
        Stamp { by: &by, at: &now_rfc3339() },
    )
    .inspect_err(|err| log_map_refusal(&app, "write", err))?;
    // CMS-FR-VRZA: the written comment is an author comment (AGC-FR-TFEN).
    crate::agent_conversations::note_author_comment(&app, &write.comment_id);
    crate::logging::log_info(
        &app,
        &crate::logging::BUFFER,
        &[crate::logging::Domain::Backend],
        "map discussion opened",
        crate::log_fields! {
            "discussionId" => write.discussion.id.clone(),
            "node" => has_node,
        },
    );
    emit_discussion_changed(&app, &write.discussion);
    Ok(write.discussion)
}

/// MPR-FR-NRNZ, `"list map discussions"`: every Map thread in the order they
/// were opened.
#[project_scope_macros::project_scoped]
#[tauri::command]
pub fn list_map_discussions<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    project: crate::project_scope::Scoped<'_, ProjectState>,
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
