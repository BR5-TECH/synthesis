//! A discussion's one pending question set (CMS-FR-XWDA through CMS-FR-EKUP).
//!
//! An agent working in a discussion may put several questions to the author at
//! once (`../../specifications/tools/ADQ-ask-discussion-questions-tool.md`).
//! What that call leaves behind is **not a comment**: it is one small JSON
//! record beside the discussion's log, holding the questions and their proposed
//! options, which the author answers in a block above the composer
//! (`../../specifications/ui/DQA-discussion-question-answering.md`).
//!
//! This module stores it, serves it, and deletes it, and takes no view of what
//! it says (CMS-FR-XWDA).
//!
//! ## Why the record is not a log line
//!
//! A log is append-only and every line of it folds into the conversation
//! (CMS-FR-01, CMS-FR-09). A pending set is neither: it is deleted when it is
//! answered, and until then no reader of the conversation sees it
//! (DQA-FR-TVMH). Writing it as a line would put a question in the history
//! twice — once as the pending record and again as the comment the submission
//! appends — so it stands beside the log rather than in it.
//!
//! ## Why the slot is claimed twice
//!
//! CMS-FR-QLDW asks for reservation and creation to be one step that cannot
//! expose two sets, and `create_file(.., Exclusive)` is what delivers it:
//! `O_EXCL` is the check and the creation in one operation (FSA-FR-24), atomic
//! against every other caller including one in another application process
//! sharing this store (CMS-FR-XQBM). `write_bytes_atomic` cannot stand in for
//! it — it replaces what it finds.
//!
//! The **keyed mutex** beside it does not exist for that race, which the
//! exclusive create already settles on its own. It exists because the slot is a
//! *discussion's*, and one other operation has to read it and act on what it
//! read: `ask_user_comment` must decide no set stands and post its comment
//! without a reservation landing between the two (AUC-FR-QSVN). That check-then-
//! act is not one filesystem operation and cannot be made into one, so both
//! sides take this lock and the pair is serialized.

use serde::{Deserialize, Serialize};
use tauri::State;

use super::*;

// ---------------------------------------------------------------------------
// The record (the Contract surface of CMS-comments-storage.md)
// ---------------------------------------------------------------------------

/// CMS-FR-QLDW: the durable record one successful tool call creates.
///
/// It carries no agent-turn id, no model exchange, no selected option, no note,
/// and no presentation state (ADQ-FR-PZWD). A restart restores the questions and
/// none of the answers, because the answers were never here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingQuestionSet {
    pub set_id: String,
    /// Sets an older build wrote name the discussion `threadId`; both read the same.
    #[serde(alias = "threadId")]
    pub discussion_id: String,
    /// CMS-FR-41: the asking agent's participant snapshot, taken when the set was
    /// recorded. It is what the generated question comments are stamped with,
    /// after a restart as before one (ADQ-FR-PZWD).
    pub asked_by: Participant,
    pub asked_at: String,
    pub questions: Vec<PendingQuestion>,
}

/// One question of a set, at the position the application assigned it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingQuestion {
    /// ADQ-FR-TWNS: 1-based, stable, ascending, and assigned here rather than by
    /// the model. Two questions may read alike, so an answer matched by text
    /// would attach to whichever matched first.
    pub position: usize,
    pub text: String,
    pub options: Vec<QuestionOption>,
}

/// One proposed answer, at the position it was written in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionOption {
    pub position: usize,
    pub value: String,
}

/// CMS-FR-TXRB: one entry of a submission.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionAnswer {
    pub question_position: usize,
    /// CMS-FR-GNTB: the recorded position of the option chosen, absent where the
    /// author answered in their own words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub option_position: Option<usize>,
    /// Sent back as well as the position, and checked against the record
    /// (CMS-FR-PJBV). A surface answering a set it read before the set changed
    /// is refused rather than silently recording the option that now stands at
    /// that position.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub option_value: Option<String>,
    /// DQA-FR-FCZL: what the author wrote instead of choosing a recorded option.
    ///
    /// CMS-FR-GNTB: an entry names this or an option, never both and never
    /// neither. A recorded option is what the agent proposed; this is what the
    /// author said when none of them was right.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub own_answer: Option<String>,
    /// DQA-FR-JJON: the addition to a **chosen option** and to nothing else. An
    /// entry carrying own words and a note is refused (CMS-FR-GNTB).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// CMS-FR-TXRB: what an accepted submission returns.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionAnswersSubmitted {
    pub discussion: Discussion,
    /// DQA-FR-CIRK: the comment a fresh turn names as its trigger. The **final**
    /// answer of the set, so one submission dispatches one turn per active agent
    /// rather than one per question (DQA-FR-QMEA).
    pub final_answer_comment_id: String,
}

// ---------------------------------------------------------------------------
// The exclusion (CMS-FR-QLDW)
// ---------------------------------------------------------------------------

/// One in-process mutex per discussion, held across the whole reserve.
///
/// Public because it guards a **discussion** rather than this module: the
/// reservation and `ask_user_comment`'s check-then-act are the two operations
/// that must not interleave, and only one of them lives here (see the module
/// docs for why the exclusive create does not cover it).
///
/// The map is never pruned: a mutex is two words, a session touches a handful of
/// discussions, and reference-counting the entries would reintroduce the race
/// this exists to close.
pub fn question_set_lock(thread_id: &str) -> std::sync::MutexGuard<'static, ()> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static LOCKS: OnceLock<Mutex<HashMap<String, &'static Mutex<()>>>> = OnceLock::new();
    let locks = LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mutex = {
        let mut guard = locks.lock().unwrap_or_else(|e| e.into_inner());
        *guard
            .entry(thread_id.to_string())
            .or_insert_with(|| Box::leak(Box::new(Mutex::new(()))))
    };
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

// ---------------------------------------------------------------------------
// Resolving a discussion from a thread id (CMS-FR-JWVH)
// ---------------------------------------------------------------------------

/// The scope and log a thread id names, owned so the caller can re-borrow it.
///
/// [`LogScope`] borrows its ids, so a helper returning one could not outlive the
/// thread it read them from. This carries the strings instead and hands back a
/// borrowed scope on demand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiscussionRef {
    Draft { draft_id: String },
    Artifact { artifact_id: String },
    Note { note_id: String },
}

impl DiscussionRef {
    /// The thread this discussion's log is reached by (CMS-FR-55).
    pub fn thread_ref(&self) -> ThreadRef<'_> {
        match self {
            DiscussionRef::Draft { draft_id } => ThreadRef::discussion(draft_id),
            DiscussionRef::Artifact { artifact_id } => ThreadRef::artifact_discussion(artifact_id),
            DiscussionRef::Note { note_id } => ThreadRef::note_discussion(note_id),
        }
    }
}

/// Which whole-target discussion a folded discussion is, or `None` where it has a
/// fragment.
///
/// Read off the thread the ordinary lookup already produced rather than by a
/// second walk of the store, which is what keeps CMS-FR-JWVH's "exactly as
/// `read_comment_thread` resolves a log" honest without a second search order to
/// keep correct.
pub fn discussion_ref_of(discussion: &Discussion) -> Option<DiscussionRef> {
    if discussion.is_fragment_targeted() {
        return None;
    }
    Some(match &discussion.target {
        DiscussionTarget::Note { note_id } => DiscussionRef::Note {
            note_id: note_id.clone(),
        },
        DiscussionTarget::Draft { draft_id } => DiscussionRef::Draft {
            draft_id: draft_id.clone(),
        },
        DiscussionTarget::Artifact { artifact_id } => DiscussionRef::Artifact {
            artifact_id: artifact_id.clone(),
        },
    })
}

// ---------------------------------------------------------------------------
// Reading (CMS-FR-JWVH)
// ---------------------------------------------------------------------------

/// CMS-FR-JWVH: the set a discussion holds, or `None` where it holds none.
///
/// A record that cannot be read is **no set**, and it still holds the slot. That
/// pair is deliberate: a corrupt file must never fabricate questions for the
/// author, and it must never be silently overwritten by a second agent either —
/// which is what reserving over it would do (CMS-FR-QLDW).
///
/// Writes nothing and emits nothing (CMS-FR-52).
pub fn read_question_set(
    root: &crate::fs::RootFs,
    worktree: &crate::fs::RootFs,
    thread_id: &str,
) -> Option<PendingQuestionSet> {
    let thread = read_discussion_by_id(root, worktree, thread_id)?;
    let discussion = discussion_ref_of(&thread)?;
    let path = discussion
        .thread_ref()
        .scope
        .question_set_path(root, thread_id)
        .ok()?;
    let bytes = root.read_bytes(&path).ok()?;
    serde_json::from_slice::<PendingQuestionSet>(&bytes).ok()
}

/// CMS-FR-JWVH: `"read discussion question set"`.
#[tauri::command]
pub fn read_discussion_question_set<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    discussion_id: String,
    project: State<'_, ProjectState>,
) -> Result<Option<PendingQuestionSet>, String> {
    let thread_id = discussion_id;
    let root = project.require_store()?;
    let worktree = project.require_root()?;
    let thread = read_discussion_by_id(&root, &worktree, &thread_id)
        .ok_or_else(|| ERR_DISCUSSION_NOT_FOUND.to_string())?;
    let Some(discussion) = discussion_ref_of(&thread) else {
        // An anchored thread carries no set and never will (ADQ-FR-LFDX). Null
        // rather than a refusal: a surface asking about every thread it renders
        // is asking an ordinary question.
        return Ok(None);
    };
    let path = discussion
        .thread_ref()
        .scope
        .question_set_path(&root, &thread_id)?;
    let Ok(bytes) = root.read_bytes(&path) else {
        return Ok(None);
    };
    match serde_json::from_slice::<PendingQuestionSet>(&bytes) {
        Ok(set) => Ok(Some(set)),
        Err(_) => {
            // The slot is held by something this build cannot read. Worth a
            // record: the author sees no questions and the agent is refused,
            // and nothing else would explain either.
            crate::logging::log_warn(
                &app,
                &crate::logging::BUFFER,
                &[crate::logging::Domain::Backend],
                "discussion question set could not be decoded",
                crate::log_fields! { "threadId" => &thread_id, "bytes" => bytes.len() as u64 },
            );
            Ok(None)
        }
    }
}

// ---------------------------------------------------------------------------
// Reserving (CMS-FR-QLDW)
// ---------------------------------------------------------------------------

/// CMS-FR-QLDW: claim the discussion's one pending-set slot and write the record.
///
/// Registered as **no Tauri command** (CMS-FR-EKUP): a set is created by an agent
/// tool and by nothing a frontend can call.
///
/// The order of the refusals is the spec's: the thread is resolved first (which
/// also yields the scope), an anchored thread is `not_supported`, a locked
/// discussion is `thread_locked`, and only then is the slot claimed.
pub fn reserve_discussion_question_set<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &crate::fs::RootFs,
    worktree: &crate::fs::RootFs,
    thread_id: &str,
    set: &PendingQuestionSet,
) -> Result<(), String> {
    reserve_question_set_record(root, worktree, thread_id, set)?;
    emit_question_set_changed(app, thread_id, Some(set));
    Ok(())
}

/// The reservation itself, without the event.
///
/// Split out because the race it closes is the whole of what is worth testing
/// here and an `AppHandle` needs the Tauri runtime to exist. The wrapper above
/// adds the emit and nothing else.
pub fn reserve_question_set_record(
    root: &crate::fs::RootFs,
    worktree: &crate::fs::RootFs,
    thread_id: &str,
    set: &PendingQuestionSet,
) -> Result<(), String> {
    let thread = read_discussion_by_id(root, worktree, thread_id)
        .ok_or_else(|| ERR_DISCUSSION_NOT_FOUND.to_string())?;
    // ADQ-FR-LFDX: a passage under review is one remark rather than a set of
    // open points, so no anchored thread holds one.
    let discussion = discussion_ref_of(&thread).ok_or_else(|| ERR_NOT_SUPPORTED.to_string())?;
    // CMS-FR-QLDW / ADQ-FR-VDGT: a conversation that takes no further
    // contribution takes no further question either.
    if thread.locked {
        return Err(ERR_DISCUSSION_LOCKED.to_string());
    }
    let target = discussion.thread_ref();
    let path = target.scope.question_set_path(root, thread_id)?;

    let _guard = question_set_lock(thread_id);
    // FSA-FR-24: the check and the creation in one operation, so two processes
    // racing on one discussion cannot both claim it.
    match root.create_file(&path, crate::fs::CreateMode::Exclusive) {
        Ok(()) => {}
        Err(crate::fs::FsError::AlreadyExists { .. }) => {
            return Err(ERR_QUESTION_SET_ALREADY_PENDING.to_string())
        }
        Err(e) => return Err(e.to_string()),
    }
    let encoded = match serde_json::to_vec_pretty(set) {
        Ok(encoded) => encoded,
        Err(e) => {
            release_claimed_slot(root, &path);
            return Err(format!("could not encode question set: {e}"));
        }
    };
    // CMS-FR-XWDA: written whole through the atomic primitive, so a reader never
    // sees half a record.
    if let Err(e) = root.write_bytes_atomic(&path, &encoded) {
        // ADQ-FR-HBLN says nothing was recorded, so the claim must go with the
        // failure. A zero-length file left here would hold the slot forever and
        // no route would clear it (CMS-FR-EKUP).
        release_claimed_slot(root, &path);
        return Err(e.to_string());
    }
    Ok(())
}

/// Give back a slot this call claimed and could not fill.
///
/// A failure to remove it is logged rather than propagated: the caller is
/// already returning a refusal, and replacing that refusal with this one would
/// tell the agent the wrong thing about what happened.
fn release_claimed_slot(root: &crate::fs::RootFs, path: &std::path::Path) {
    let _ = root.delete_under(root.path(), path, false);
}

// ---------------------------------------------------------------------------
// Deleting (CMS-FR-OKMU)
// ---------------------------------------------------------------------------

/// Remove a discussion's set. Reports success where it holds none.
pub(super) fn delete_question_set(
    root: &crate::fs::RootFs,
    target: ThreadRef<'_>,
    thread_id: &str,
) -> Result<(), String> {
    let path = target.scope.question_set_path(root, thread_id)?;
    match root.delete_under(root.path(), &path, false) {
        Ok(()) => Ok(()),
        Err(crate::fs::FsError::NotFound { .. }) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

// ---------------------------------------------------------------------------
// The event (CMS-FR-BQEN)
// ---------------------------------------------------------------------------

/// CMS-FR-BQEN: announce the set a discussion now holds, or that it holds none.
///
/// Emitted when a set is reserved and when one is deleted, and never for a
/// refused reservation or a submission that did not commit — every silent write
/// stays silent (CMS-FR-52).
///
/// The emit result is discarded for the reason [`emit_discussion_changed`] discards
/// it: an event must never take down the operation it reports on.
pub fn emit_question_set_changed<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    thread_id: &str,
    set: Option<&PendingQuestionSet>,
) {
    use tauri::Emitter as _;
    let _ = app.emit(
        DISCUSSION_QUESTION_SET_CHANGED,
        serde_json::json!({ "discussionId": thread_id, "set": set }),
    );
}
