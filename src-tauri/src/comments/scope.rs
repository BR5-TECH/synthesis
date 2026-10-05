//! Which log a write is destined for, and the one thread it names.

use super::*;

/// Which log a write is destined for. The `artifact` and `draft` scopes of
/// CMS-FR-36 differ in exactly three things, and this type carries the first of
/// them: where the log is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogScope<'a> {
    Artifact,
    Draft { draft_id: &'a str },
    /// CMS-FR-55: the draft's reserved discussion log. Its *attachments* sit in
    /// the same folder [`LogScope::Draft`]'s do — only the log's filename differs,
    /// because a discussion is draft-scoped like any other draft thread.
    DraftDiscussion { draft_id: &'a str },
    /// CMS-FR-55: an artifact's reserved discussion log, beside its anchored one
    /// in the store's `comments/` folder. Its attachments share the same folder — only the
    /// log's filename differs, because a discussion about a file of the project
    /// is artifact-scoped like any other thread on it.
    ArtifactDiscussion { artifact_id: &'a str },
    /// CMS-FR-37 / CMS-FR-55: a note's reserved discussion log, inside that
    /// note's own folder under the store's comments root. Its attachments sit
    /// in that same folder, which is what lets CMS-FR-64 remove the whole
    /// conversation in one recursive delete.
    NoteDiscussion { note_id: &'a str },
}

impl LogScope<'_> {
    pub(super) fn log_path(&self, root: &crate::fs::RootFs, file_rel: &str) -> Result<PathBuf, String> {
        match self {
            LogScope::Artifact => log_path(root, file_rel),
            LogScope::NoteDiscussion { note_id } => note_discussion_log_path(root, note_id),
            LogScope::Draft { draft_id } => fsa::resolve_under(
                draft_comments_dir(root, draft_id)?,
                format!("{}.jsonl", log_id(file_rel)),
            )
            .map_err(|e| e.to_string()),
            LogScope::DraftDiscussion { draft_id } => discussion_log_path(root, draft_id),
            LogScope::ArtifactDiscussion { artifact_id } => {
                fsa::resolve_under(root, artifact_discussion_log_rel(artifact_id))
                    .map_err(|e| e.to_string())
            }
        }
    }

    /// CMS-FR-49: attachments follow their thread's scope exactly as its log
    /// does. The two scopes never share a folder, so identical bytes attached in
    /// a draft and in the project are stored once in each — and deleting the
    /// draft takes only its copy, the project's being somewhere else entirely.
    ///
    /// The digest is checked before it becomes a path, and that check is
    /// load-bearing rather than belt-and-braces. The path is confined to the
    /// scope's **own comments folder** — `attachments/<digest>` resolved beneath
    /// it — so one `..` segment is all it takes to climb out of `attachments/`
    /// and name any file the scope's folder holds, the logs included, while
    /// satisfying the escape gate. A digest is not caller-chosen data in the
    /// write path — this module computes every one it stores — but it *is* on
    /// the read path, and it is read back out of a JSONL log that is editable by
    /// hand. A line crafted with
    /// `"digest": "../<log-id>.jsonl"` would otherwise serve that file's bytes
    /// to anyone who opens the artifact.
    pub(super) fn attachment_path(&self, root: &crate::fs::RootFs, digest: &str) -> Result<PathBuf, String> {
        if !is_storage_digest(digest) {
            return Err(ERR_ATTACHMENT_NOT_FOUND.to_string());
        }
        // A draft or a note that is gone holds no attachment, its folder having
        // been removed with it (CMS-FR-39, CMS-FR-64), which is what
        // `attachment_not_found` says (CMS-FR-48).
        let dir = self
            .comments_folder(root)
            .map_err(|_| ERR_ATTACHMENT_NOT_FOUND.to_string())?;
        fsa::resolve_under(dir, format!("{ATTACHMENTS_SUBDIR}/{digest}"))
            .map_err(|e| e.to_string())
    }

    /// The scope's own comments folder — the directory its log, its attachments,
    /// and its question sets all sit under (CMS-FR-49, CMS-FR-XWDA).
    ///
    /// One match rather than one per thing stored there. That is what makes
    /// CMS-FR-EKUP true by construction: a set lives inside the very folder a
    /// deleted draft or a deleted note already removes whole, so nothing
    /// collects a set separately.
    pub(super) fn comments_folder(&self, root: &crate::fs::RootFs) -> Result<PathBuf, String> {
        Ok(match self {
            // CMS-FR-49: an artifact's discussion sits in the very folder its
            // anchored threads use.
            LogScope::Artifact | LogScope::ArtifactDiscussion { .. } => root.join(COMMENTS_REL),
            LogScope::Draft { draft_id } | LogScope::DraftDiscussion { draft_id } => {
                draft_comments_dir(root, draft_id)?
            }
            // CMS-FR-49: a note's folder is its own rather than one shared with
            // the notes beside it, so deleting one note's conversation takes
            // nothing from another's.
            LogScope::NoteDiscussion { note_id } => note_comments_dir(root, note_id)?,
        })
    }

    /// CMS-FR-XWDA: where this discussion's pending question set is written.
    ///
    /// `<scope-comments-folder>/question-sets/<thread-id>.json`. The thread id is
    /// checked before it becomes a path for the reason a digest is
    /// (see [`LogScope::attachment_path`]): the id reaches this from a tool
    /// argument and from a Tauri command, and one `..` segment would otherwise
    /// name a log beside the folder.
    pub(super) fn question_set_path(
        &self,
        root: &crate::fs::RootFs,
        thread_id: &str,
    ) -> Result<PathBuf, String> {
        if !is_path_safe_id(thread_id) {
            return Err(ERR_DISCUSSION_NOT_FOUND.to_string());
        }
        let dir = self.comments_folder(root)?;
        fsa::resolve_under(dir, format!("{QUESTION_SETS_SUBDIR}/{thread_id}.json"))
            .map_err(|e| e.to_string())
    }
}

/// CMS-FR-53: what a discussion is about.
///
/// The whole thing rather than a passage of one — a file of the project taken
/// entire, or a draft taken entire, files included. Which of the two it is
/// decides where the log lives and how long it lives (CMS-FR-56), and nothing
/// else about the conversation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DiscussionTarget {
    #[serde(rename_all = "camelCase")]
    Draft { draft_id: String },
    #[serde(rename_all = "camelCase")]
    Artifact { artifact_id: String },
    /// CMS-FR-53: one note of the project, itself — rather than whatever entity
    /// the note happens to be filed against.
    #[serde(rename_all = "camelCase")]
    Note { note_id: String },
}

/// Which conversation a command is operating on, in the one shape every write
/// path here takes.
///
/// CMS-FR-36 / CMS-FR-54: a thread is opened, commented on, locked, resolved, and
/// re-anchored identically whichever scope and kind it is, so the write helpers
/// take this rather than a path — and a discussion travels the same code an
/// anchored thread does, differing only in what a thread without a passage cannot
/// have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThreadRef<'a> {
    pub scope: LogScope<'a>,
    /// The file the log belongs to, and `""` for the reserved discussion log,
    /// which belongs to no file.
    pub file_rel: &'a str,
}

impl<'a> ThreadRef<'a> {
    pub fn artifact(artifact_id: &'a str) -> Self {
        Self {
            scope: LogScope::Artifact,
            file_rel: artifact_id,
        }
    }

    pub fn draft_file(draft_id: &'a str, file_rel: &'a str) -> Self {
        Self {
            scope: LogScope::Draft { draft_id },
            file_rel,
        }
    }

    pub fn discussion(draft_id: &'a str) -> Self {
        Self {
            scope: LogScope::DraftDiscussion { draft_id },
            file_rel: "",
        }
    }

    /// CMS-FR-56: a discussion about one file of the project, taken entire.
    ///
    /// `file_rel` carries the artifact so the thread folds with its path — unlike
    /// a draft's discussion, which belongs to no one file and so carries none.
    pub fn artifact_discussion(artifact_id: &'a str) -> Self {
        Self {
            scope: LogScope::ArtifactDiscussion { artifact_id },
            file_rel: artifact_id,
        }
    }

    /// CMS-FR-62: the one discussion a note carries.
    ///
    /// `file_rel` is empty like a draft discussion's: a note's conversation is
    /// about the note and belongs to no file of the project, so nothing about
    /// the entity it is filed against reaches the thread.
    pub fn note_discussion(note_id: &'a str) -> Self {
        Self {
            scope: LogScope::NoteDiscussion { note_id },
            file_rel: "",
        }
    }

    /// Every thread the log this names currently holds.
    pub(super) fn list(&self, root: &fsa::RootFs) -> Vec<Discussion> {
        match self.scope {
            LogScope::Artifact => list_fragment_discussions_in(root, self.file_rel),
            LogScope::Draft { draft_id } => list_draft_fragment_discussions_in(root, draft_id, self.file_rel),
            LogScope::DraftDiscussion { draft_id } => fold_discussion(root, draft_id),
            LogScope::ArtifactDiscussion { artifact_id } => {
                fold_artifact_discussion(root, artifact_id)
            }
            LogScope::NoteDiscussion { note_id } => fold_note_discussion(root, note_id),
        }
    }

    pub(super) fn find(&self, root: &fsa::RootFs, thread_id: &str) -> Result<Discussion, String> {
        self.list(root)
            .into_iter()
            .find(|t| t.id == thread_id)
            .ok_or_else(|| ERR_DISCUSSION_NOT_FOUND.to_string())
    }
}

/// One thread of a conversation, by [`ThreadRef`] and id.
///
/// The typed "not found" of every operation that names a thread, exposed for a
/// caller outside this module that has to read a thread's state — its lock,
/// most of all — **before** it takes an action of its own (DCP-FR-15).
pub fn thread_at(
    root: &fsa::RootFs,
    target: ThreadRef<'_>,
    thread_id: &str,
) -> Result<Discussion, String> {
    target.find(root, thread_id)
}

/// A bare, filename-safe token, checked before an id is joined into a path.
///
/// `resolve_under` already refuses to escape the store root (FSA-FR-10), but a
/// dot segment would still name a file *inside* the scope's own folder — the
/// log beside `question-sets/`, for one — while satisfying that gate. A thread
/// id reaches [`LogScope::question_set_path`] from a tool argument and from a
/// Tauri command, so it is rejected outright rather than resolved.
pub(super) fn is_path_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}
