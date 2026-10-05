/**
 * Notes, comments, search, and the session log.
 *
 * One part of the typed wrappers over the backend's `#[tauri::command]` set;
 * `./index.ts` carries the whole rule these files are written under.
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  PendingQuestionSet,
  QuestionAnswerInput,
  QuestionAnswersSubmitted,
  DiscussionTarget,
  BrowseResult,
  AttachmentContent,
  AttachmentInput,
  CommentQuote,
  Discussion,
  DiscussionListItem,
  FragmentTarget,
  LogCursor,
  LogFilter,
  LogInput,
  LogPage,
  Note,
  NoteFields,
  NoteListItem,
  NoteScope,
  Participant,
  SearchMode,
  SearchScope,
} from "../types";

// --- Notes (notes.rs / NTC-notes-storage.md) ------------------------------

/** NTC-FR-09: the entity's notes, most-recently-edited first (NTS-FR-02). */
export const listNotesForEntity = (entityId: string) =>
  invoke<NoteListItem[]>("list_notes_for_entity", { entityId });

/** NTC-FR-09: only the project-scoped notes (NTS-FR-03). */
export const listProjectNotes = () =>
  invoke<NoteListItem[]>("list_project_notes");

/** NTC-FR-09: every note in the project, both kinds in one list (NTS-FR-10). */
export const listAllNotes = () => invoke<NoteListItem[]>("list_all_notes");

/** NTC-FR-04: create a note under `scope`, optionally carrying a reminder. */
export const createNote = (args: {
  scope: NoteScope;
  body: string;
  reminder?: string;
  revision?: string;
}) =>
  invoke<Note>("create_note", {
    scope: args.scope,
    body: args.body,
    reminder: args.reminder ?? null,
    revision: args.revision ?? null,
  });

/**
 * NTC-FR-05: a partial update. Only the fields present in `fields` are written;
 * `reminder: null` clears the reminder, and an absent `reminder` leaves it as
 * it was.
 */
export const updateNote = (id: string, fields: NoteFields) =>
  invoke<Note>("update_note", { id, fields });

/**
 * NTC-FR-08 / NTC-FR-21: remove the note **and whatever discussion it carries**,
 * in one backend-owned transaction.
 *
 * Refuses with the typed `discussion_cleanup_failed` when the conversation could
 * not be removed, having removed nothing — so a caller that sees this error must
 * not report the note as deleted (NTS-FR-30).
 */
export const deleteNote = (id: string) =>
  invoke<void>("delete_note", { id });

// --- Discussions (comments.rs) --------------------------------------------

/**
 * Every discussion of one owner: fragment discussions first, in source order,
 * then whole-target discussions in the order they were opened.
 */
export const listDiscussions = (target: DiscussionTarget) => {
  const key = JSON.stringify(target);
  const pending = listsInFlight.get(key);
  if (pending) return pending;
  const read = invoke<Discussion[]>("list_discussions", { target }).finally(
    () => {
      listsInFlight.delete(key);
    },
  );
  listsInFlight.set(key, read);
  return read;
};

/**
 * Reads of one target that run in the same moment share one call, so a tab with
 * a rail and an action control reads its discussions once (ACT-FR-22).
 */
const listsInFlight = new Map<string, Promise<Discussion[]>>();

/**
 * Every discussion the project holds, whatever its owner kind, most recently
 * active first. Each item says whether its owner still resolves. What the
 * Comments panel groups and renders.
 */
export const listAllDiscussions = () =>
  invoke<DiscussionListItem[]>("list_all_discussions");

/**
 * The one discussion an id names, whatever its owner. The only read keyed by a
 * discussion rather than by its owner. It serves a surface whose owner is not
 * mounted, for example the conversation tab of an owner that has closed.
 */
export const readDiscussion = (discussionId: string) =>
  invoke<Discussion>("read_discussion", { discussionId });

/**
 * The question set this discussion holds, or `null` where it holds none. A
 * surface reads this when it mounts and follows
 * `"discussion question set changed"` after that.
 */
export const readDiscussionQuestionSet = (discussionId: string) =>
  invoke<PendingQuestionSet | null>("read_discussion_question_set", { discussionId });

/**
 * Submit the whole ordered set of answers in one call. The append and the
 * deletion of the set are one committed step, so a refusal leaves the set as it
 * stood.
 */
export const submitDiscussionQuestionAnswers = (args: {
  discussionId: string;
  setId: string;
  answers: QuestionAnswerInput[];
}) => invoke<QuestionAnswersSubmitted>("submit_discussion_question_answers", args);

/**
 * Who this machine writes as. Refuses with the typed distinction the composer
 * routes on rather than returning a placeholder.
 */
export const resolveCommentAuthorIdentity = () =>
  invoke<Participant>("resolve_comment_author_identity");

/**
 * Open a discussion on the target and post its opening message, both in one
 * append. `fragmentTarget` is optional: absent means a whole-target
 * discussion. A `note` target is refused with `not_supported`; notes use
 * `getOrCreateNoteDiscussion`.
 */
export const openDiscussion = (args: {
  target: DiscussionTarget;
  fragmentTarget?: FragmentTarget | null;
  body: string;
  attachments: AttachmentInput[];
}) => invoke<Discussion>("open_discussion", args);

/**
 * The one discussion a note carries: returned if it has one, created with this
 * opening message if it does not. Atomic and idempotent. A second call against a
 * note that already has a discussion returns it and appends nothing.
 */
export const getOrCreateNoteDiscussion = (args: {
  noteId: string;
  body: string;
  attachments: AttachmentInput[];
}) => invoke<Discussion>("get_or_create_note_discussion", args);

/**
 * An optional hint for which log holds the discussion. The backend finds the
 * discussion by id alone. A hint only narrows where it looks.
 */
export type DiscussionLocator = {
  artifactId?: string;
  draftId?: string;
  noteId?: string;
};

/** Add a comment, optionally quoting earlier ones in the discussion. */
export const addComment = (
  args: DiscussionLocator & {
    discussionId: string;
    body: string;
    quotes: CommentQuote[];
    attachments: AttachmentInput[];
  },
) => invoke<Discussion>("add_comment", args);

/**
 * The stored bytes of one attachment, base64-encoded. Only a `blob` is served.
 * A link is loaded from its own address by whatever renders it.
 */
export const readCommentAttachment = (args: {
  discussionId: string;
  digest: string;
  /** The draft the discussion belongs to, if any. It only narrows the lookup. */
  draftId?: string;
}) => invoke<AttachmentContent>("read_comment_attachment", args);

/** Set the lock, independently of the resolution. */
export const setDiscussionLock = (
  args: DiscussionLocator & { discussionId: string; locked: boolean },
) => invoke<Discussion>("set_discussion_lock", args);

/** Set the resolution, independently of the lock. */
export const setDiscussionResolution = (
  args: DiscussionLocator & { discussionId: string; resolved: boolean },
) => invoke<Discussion>("set_discussion_resolution", args);

/**
 * Move a fragment discussion to where its passage now is. Refused with
 * `not_fragment_targeted` on a whole-target discussion.
 */
export const reanchorDiscussionFragment = (
  args: DiscussionLocator & { discussionId: string; fragmentTarget: FragmentTarget },
) => invoke<Discussion>("reanchor_discussion_fragment", args);

// --- Universal search (search.rs) -----------------------------------------

/**
 * SCC-FR-01 / SCC-FR-02: start a search and resolve with its id, before any file
 * has been matched. Hits arrive on the `"search results"` event and the search
 * announces its end on `"search ended"` (see `./events`).
 *
 * SCC-FR-12: starting supersedes any search still running, so a caller never has
 * to cancel before starting. SCC-FR-06: a `regex` query that does not compile
 * rejects with the typed `"invalid query"` error and starts nothing.
 */
export const startSearch = (query: string, mode: SearchMode, scope: SearchScope) =>
  invoke<string>("start_search", { query, mode, scope });

/**
 * SCC-FR-14: stop a running search. An id that has already ended is a no-op
 * rather than an error, so a caller need not track whether its search is still
 * in flight.
 */
export const cancelSearch = (searchId: string) =>
  invoke<void>("cancel_search", { searchId });

// --- Session logging (logging.rs) ------------------------------------------

/**
 * LGC-FR-07 / LGC-FR-20: append a batch of records emitted by the frontend, in
 * the order given.
 *
 * Never rejects — a failure to record a diagnostic is not worth failing the
 * caller over (LOG-FR-19) — and answers normally with no project open. Call it
 * through `../logging`'s emitters rather than directly, so emission stays
 * batched and off the render path.
 */
export const appendLogRecords = (records: LogInput[]) =>
  invoke<void>("append_log_records", { records });

/**
 * LGC-FR-12 / LGC-FR-22: the records matching `filter`, positioned by `cursor`
 * and bounded by `limit`.
 *
 * Every act of narrowing happens here rather than in the panel (LOG-FR-06), so
 * what comes back is exactly what is rendered. An absent `cursor` returns the
 * *newest* `limit` matching records. LGC-FR-11: a `queryIsRegex` filter whose
 * pattern does not compile rejects with the typed `"invalid query"` error and
 * reads nothing.
 */
export const queryLogs = (
  filter: LogFilter,
  cursor: LogCursor | null,
  limit: number,
) => invoke<LogPage>("query_logs", { filter, cursor, limit });

/**
 * LGC-FR-18: write every record matching `filter` to `destinationPath` as
 * JSONL and resolve with how many were written — the whole match set, bounded
 * by no limit and unrelated to what the panel currently has loaded.
 *
 * LGC-FR-19: a destination that cannot be written rejects with the typed
 * `"export failed"` error, leaving no partial file behind.
 */
export const exportLogs = (filter: LogFilter, destinationPath: string) =>
  invoke<number>("export_logs", { filter, destinationPath });

/**
 * FSA-FR-16: open the OS-native save-file dialog with `defaultName` pre-filled
 * and resolve with the chosen path or the `cancelled` sentinel. It resolves a
 * destination and writes nothing, so a caller that never follows up leaves the
 * filesystem untouched.
 */
export const browseForSavePath = (defaultName: string) =>
  invoke<BrowseResult>("browse_for_save_path", { defaultName });
