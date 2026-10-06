//! Names, limits and relative paths this module writes to.

/// CMS-FR-30: a `discussion_id` that names no discussion in the open project.
pub const ERR_DISCUSSION_NOT_FOUND: &str = "discussion_not_found";
/// CMS-FR-57: a `draft_id` that names no draft in the open project.
pub const ERR_DRAFT_NOT_FOUND: &str = "draft_not_found";
/// CMS-FR-57: a discussion target naming a path the project does not hold.
pub const ERR_ARTIFACT_NOT_FOUND: &str = "artifact_not_found";

/// CMS-FR-62: a `note_id` naming no note in the open project.
pub const ERR_NOTE_NOT_FOUND: &str = "note_not_found";

/// CMS-FR-57: an operation that cannot serve the target it was handed —
/// `open_discussion` on a note, which would be a route to a second
/// discussion on a target that carries at most one.
pub const ERR_NOT_SUPPORTED: &str = "not_supported";

/// CMS-FR-14 / CMS-FR-62: an opening carrying nothing to say.
///
/// A note's conversation is the one opening this module refuses on the body
/// alone, and deliberately: it is the only one whose surface can be reached
/// before the conversation exists (CVP-FR-60), so an empty submission is a
/// state the user can actually get to rather than one the composer's own
/// enablement has already ruled out.
pub const ERR_EMPTY_BODY: &str = "empty_body";
/// CMS-FR-59: a fragment move asked of a discussion that has no fragment.
pub const ERR_NOT_FRAGMENT_TARGETED: &str = "not_fragment_targeted";
/// CMS-FR-17: the one thing a lock prevents.
pub const ERR_DISCUSSION_LOCKED: &str = "discussion_locked";
/// CMS-FR-16: a quote naming a comment outside the discussion it is posted into.
pub const ERR_QUOTED_COMMENT_NOT_IN_DISCUSSION: &str = "quoted_comment_not_in_discussion";
/// CMS-FR-21: a fragment whose `end` is not past its `start`, or whose owner is
/// not an artifact or a draft.
pub const ERR_INVALID_FRAGMENT: &str = "invalid_fragment";
/// CMS-FR-47: an attachment whose decoded length exceeds [`MAX_ATTACHMENT_BYTES`].
pub const ERR_ATTACHMENT_TOO_LARGE: &str = "attachment_too_large";
/// CMS-FR-47: an attachment whose media type is outside [`media_type_accepted`].
pub const ERR_UNSUPPORTED_MEDIA_TYPE: &str = "unsupported_media_type";
/// CMS-FR-47: an attachment input that does not parse at all.
pub const ERR_MALFORMED_ATTACHMENT: &str = "malformed_attachment";
/// CMS-FR-48: a digest no attachment in the thread's scope carries.
pub const ERR_ATTACHMENT_NOT_FOUND: &str = "attachment_not_found";

/// CMS-FR-51: the folded discussion, emitted after every append this module performs.
///
/// Kebab-case rather than the spec's abstract wording for the reason every other
/// channel in this application is: Tauri validates event names and rejects one
/// containing spaces on both the emit and the listen side, so a prose name is a
/// silently dead channel. Matches `src/events.ts`'s constant byte-for-byte.
pub const DISCUSSION_CHANGED: &str = "discussion-changed";

/// CMS-FR-HTOA: the display name of the local participant.
pub const LOCAL_PARTICIPANT_NAME: &str = "Me";

/// CMS-FR-BQEN: the event a reserved or deleted question set announces itself
/// on.
///
/// A second event rather than a field of the thread one, because a set is not a
/// line of the log and no fold produces it (CMS-FR-XWDA): a surface that reads
/// the thread learns nothing about a set standing over it.
pub const DISCUSSION_QUESTION_SET_CHANGED: &str = "discussion-question-set-changed";

/// CMS-FR-QLDW: a discussion that already holds a set takes no second one.
pub const ERR_QUESTION_SET_ALREADY_PENDING: &str = "question_set_already_pending";

/// CMS-FR-PJBV: the submission names a set the discussion does not hold.
pub const ERR_QUESTION_SET_NOT_FOUND: &str = "question_set_not_found";

/// CMS-FR-PJBV: the answers do not match the questions that were recorded.
pub const ERR_QUESTION_ANSWERS_INCOMPLETE: &str = "question_answers_incomplete";

/// CMS-FR-XWDA: the subdirectory of a scope's comments folder that holds its
/// pending question sets, one JSON file per discussion.
///
/// A directory of its own rather than a file beside the logs, so the walk that
/// picks `.jsonl` files out of the comments folder steps over it and a set
/// reaches no thread listing.
pub(super) const QUESTION_SETS_SUBDIR: &str = "question-sets";

/// Where a scope's stored attachments live, relative to its comments folder
/// (CMS-FR-44). A subfolder rather than the comments folder itself so the fold's
/// `*.jsonl` enumeration never has to distinguish a log from a payload.
pub(super) const ATTACHMENTS_SUBDIR: &str = "attachments";

/// CMS-FR-47: the bound past which an attachment is refused.
///
/// An `artifact`-scoped attachment is committed to the project's repository, so
/// this is a limit on what a review can push into everyone's clone, not just on
/// what this process will hold in memory.
pub const MAX_ATTACHMENT_BYTES: usize = 10 * 1024 * 1024;

/// Whether a string is the shape this module's own content addressing produces:
/// exactly the 64 lowercase hex characters of a SHA-256, and nothing else.
///
/// A separator, a `.`, or an upper-case letter is not a digest this module ever
/// wrote (`fsa::sha256_bytes` emits lowercase hex), so refusing them costs
/// nothing and is what keeps a filename a filename. See `attachment_path` for
/// why a name that is merely *inside the root* is not good enough.
pub fn is_storage_digest(candidate: &str) -> bool {
    candidate.len() == 64 && candidate.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// CMS-FR-47: what a comment may carry — what a reviewer points at, rather than
/// arbitrary binaries in a file everyone who clones the project has to fetch.
///
/// Matched on the media type's base, so a parameterised `text/plain; charset=utf-8`
/// is the same type as a bare `text/plain`.
pub fn media_type_accepted(media_type: &str) -> bool {
    let base = media_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    base.starts_with("image/") || base == "application/pdf" || base == "text/plain"
}
/// CMS-FR-01: the conversation folder, relative to the repository machine store.
///
/// The store stands under `app_data_dir()` and outside every worktree
/// (`RMS-repository-machine-storage.md` RMS-FR-ZXHM), so this is a folder of the
/// application's own data rather than a folder of the project.
pub(super) const COMMENTS_REL: &str = crate::repository_store::COMMENTS_DIRNAME;

/// CMS-FR-37: the folder each draft's own conversation storage sits under,
/// relative to the store.
///
/// A draft's review is addressed by the draft's **stable id** alone
/// (`DRS-draft-storage.md` DRS-FR-02) and by nothing about where the draft is
/// filed, so renaming a draft, filing it under a folder, and moving it between
/// folders each leave its whole review where it is.
pub(super) const DRAFT_COMMENTS_ROOT: &str = crate::repository_store::DRAFTS_DIRNAME;

/// The folder inside `drafts/<draft-id>/` the logs and attachments sit in.
pub(super) const DRAFT_COMMENTS_SUBDIR: &str = "comments";

/// The project's drafts root, project-relative — for asserting that this module
/// writes nothing at all beneath it, and for nothing else.
///
/// Deliberately `#[cfg(test)]`. A draft's files stay in the active worktree
/// (`DRS-draft-storage.md` DRS-FR-01) while its review lives in the repository
/// machine store (CMS-FR-37), so a production path composed from this constant
/// would put a conversation back into the project's Git repository.
#[cfg(test)]
pub(super) const DRAFTS_REL: &str = ".synthesis/drafts";

/// CMS-FR-55: the one **reserved** log every discussion of a draft lives in.
///
/// Reserved rather than derived, and no derivation of CMS-FR-02 produces it —
/// `log_id` emits 32 hex characters and nothing else — so a draft file can never
/// be given the discussion log's filename however the author names it.
pub const DISCUSSION_LOG_NAME: &str = "discussion.jsonl";

/// CMS-FR-55: the reserved marker that tells an artifact's **discussion** log
/// apart from the anchored log beside it.
///
/// A file's two logs are found by one derivation — `<log-id>.jsonl` holds the
/// threads pinned to its passages and `<log-id>.discussion.jsonl` the
/// conversations about the whole of it — and no derivation of CMS-FR-02 produces
/// a name ending in the marker, `log_id` yielding bare hex with no dot in it.
pub const DISCUSSION_LOG_MARKER: &str = ".discussion";

/// CMS-FR-37: the subdirectory of the store's comments folder under which each
/// note's conversation folder sits.
///
/// A directory rather than a log, which is what keeps it out of
/// [`list_all_threads_in`] for free: that walk picks `.jsonl` files out of the
/// comments folder's top level and steps into nothing, so a note's discussion
/// reaches no project-wide list (CMS-FR-40) without a scope check of its own.
pub(super) const NOTES_SUBDIR: &str = "notes";

/// CMS-FR-64: the name a note's conversation folder is moved aside to before it
/// is deleted.
///
/// A leading dot so a leftover — the folder a delete-after-rename failed to
/// remove — is not a note id any lookup will ever produce, and so contributes no
/// entry to the discussion index (CMS-FR-63): `note_discussion_of` is asked only
/// about ids the notes store serves, and `is_valid_id` admits no leading dot.
pub(super) const DISCARDED_PREFIX: &str = ".discarded-";

/// CMS-FR-07: the schema version every line this build writes carries. A line
/// with any other `v` is skipped by the fold rather than failing it, so a log
/// written by a newer build still serves the events this one understands.
pub(super) const SCHEMA_VERSION: u32 = 1;

/// CMS-FR-YQND: the typed refusal of a command whose store could not be
/// resolved (`RMS-repository-machine-storage.md` RMS-FR-WGQS).
///
/// Spelled out rather than re-exported: `comment-error-parity.test.ts` reads
/// this module's `ERR_*` literals to check the frontend's list against them, and
/// an alias carries no string for it to read. The test below is what keeps the
/// two spellings one value.
pub const ERR_STORE_UNAVAILABLE: &str = "store_unavailable";

#[cfg(test)]
mod store_error_tests {
    /// CMS-FR-YQND: one spelling, whichever module a caller reaches it through.
    #[test]
    fn the_store_refusal_is_the_one_the_store_module_defines() {
        assert_eq!(
            super::ERR_STORE_UNAVAILABLE,
            crate::repository_store::ERR_STORE_UNAVAILABLE,
        );
    }
}
