/**
 * Work streams and the graduation runs that work in them.
 *
 * One part of the typed wrappers over the backend's `#[tauri::command]` set;
 * `./index.ts` carries the whole rule these files are written under.
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  AgentActivityPage,
  WorkStream,
  WorkStreamSummary,
  StreamMergePublication,
  StreamMergeResult,
  StreamUpdateRecord,
  StreamUpdateStrategy,
  EscalationAnswerInput,
  DirectGraduationPreflight,
  GraduationCapacity,
  GraduationQueue,
  GraduationRun,
  StandingWork,
  GraduationLogCursor,
  GraduationLogPage,
  GraduationLogPassScope,
  GraduationLogStream,
} from "../types";

// --- Work streams (streams.rs / WKS-work-streams.md) -----------------------
//
// A work stream is a named branch and working copy the application owns.
// Graduation runs execute in one and commit onto it, so consecutive runs stack
// with no reconciliation between them.

/** WKS-FR-KDXF: create a stream, or create nothing. */
export const createWorkStream = (name: string, baseBranch?: string) =>
  invoke<WorkStream>("create_work_stream", { name, baseBranch });

/** WKS-FR-SGCM: every live stream of the project, with what each holds. */
export const listWorkStreams = () =>
  invoke<WorkStreamSummary[]>("list_work_streams");

/** WKS-FR-SGCM: one stream, or the typed `unknown_stream`. */
export const getWorkStream = (streamId: string) =>
  invoke<WorkStream>("get_work_stream", { streamId });

/**
 * WKS-FR-GKPX / WKS-FR-QNHF: merge a stream into the branch it was created
 * from.
 *
 * Git merges first. The call answers once that check settles, with one of
 * three kinds: `nothing_to_merge`, `merged` (applied at once, no run) or
 * `conflicted` (handed to a merge run, named by `runId`). It does not wait for
 * the turns of a merge run. What a merge run is doing is read from the run and
 * from the stream's `mergeRun` in the listing, never from this call.
 */
export const mergeWorkStream = (
  streamId: string,
  publication: StreamMergePublication,
) => invoke<StreamMergeResult>("merge_work_stream", { streamId, publication });

// --- Work stream update (base branch → stream) -----------------------------
//
// The opposite direction to a merge. Every one of these answers from the
// stream's own update record.

/**
 * WKS-FR-NRQT / WKS-FR-MJEB: bring a stream up to a pinned revision of the
 * branch it was created from.
 *
 * `baseRevision` is the revision the stream update window displayed. The
 * backend refuses with `stale_base_revision` where the recorded base branch has
 * left it, and changes nothing (WKS-FR-KFVJ).
 *
 * The update runs on its own and this answers at once with its record, in
 * `running`. What it settled is read from that record and never from this call.
 */
export const updateWorkStream = (
  streamId: string,
  strategy: StreamUpdateStrategy,
  baseRevision: string,
) =>
  invoke<StreamUpdateRecord>("update_work_stream", {
    streamId,
    strategy,
    baseRevision,
  });

/** WKS-FR-GYHF: what one stream's last update did, or nothing. */
export const getWorkStreamUpdate = (streamId: string) =>
  invoke<StreamUpdateRecord | null>("get_work_stream_update", { streamId });

/** WKS-FR-CBXW: the author's answers to an update that stopped to ask. */
export const answerWorkStreamUpdateEscalation = (
  streamId: string,
  answers: EscalationAnswerInput[],
) =>
  invoke<StreamUpdateRecord>("answer_work_stream_update_escalation", {
    streamId,
    answers,
  });

/**
 * WKS-FR-DPNM: run the update again, with three fresh semantic turns.
 *
 * The strategy and the pinned revision the record retains are what it runs on,
 * so a base branch that has moved since is refused rather than silently used.
 */
export const retryWorkStreamUpdate = (streamId: string) =>
  invoke<StreamUpdateRecord>("retry_work_stream_update", { streamId });

/** WKS-FR-LRAV: forget what an update settled. Neither branch is touched. */
export const clearWorkStreamUpdate = (streamId: string) =>
  invoke<void>("clear_work_stream_update", { streamId });

/**
 * WKS-FR-WEJK: stop the update of one stream.
 *
 * A stream with no update running is answered without an error. The update
 * itself answers `update_cancelled` and leaves both branches as they were.
 */
export const cancelWorkStreamUpdate = (streamId: string) =>
  invoke<void>("cancel_work_stream_update", { streamId });

/**
 * WKS-FR-EIBC: remove a stream, its branch and its working copy.
 *
 * `force` answers the unmerged refusal alone, and `discardUncommitted` answers
 * the `stream_dirty` refusal alone (WKS-FR-DBXN). Neither reaches the busy, the
 * has-runs, or the active-worktree refusal (WKS-FR-OVLQ).
 */
export const deleteWorkStream = (
  streamId: string,
  force: boolean,
  discardUncommitted = false,
) =>
  invoke<void>("delete_work_stream", { streamId, force, discardUncommitted });

/** WKS-FR-GQSU: the complete uncommitted path set of the stream's working copy. */
export const getWorkStreamUncommittedPaths = (streamId: string) =>
  invoke<string[]>("get_work_stream_uncommitted_paths", { streamId });

// --- Graduation (graduation.rs / GRD-graduation.md) ------------------------
//
// `dispatchGraduationTurn` and the run's commit are deliberately absent: each
// is an internal seam that launches an agent or writes into a working copy the
// author is not looking at (GXD-FR-DRFF).

/**
 * GSU-FR-ELZO: capture the draft's prompt and enqueue a run on a work stream.
 *
 * `standingWork` answers GSU-FR-MLEJ: what the run does with work standing
 * uncommitted in that stream when its turn comes. It is recorded on the run and
 * applied at the dispatch, so the start reads no working copy.
 *
 * `standingWorkMessage` is what a commit that choice makes says (GRD-FR-RJFC).
 * `null` takes the run's own name.
 */
export const startGraduation = (
  draftId: string,
  streamId: string,
  standingWork: StandingWork,
  standingWorkMessage: string | null,
) =>
  invoke<GraduationRun>("start_graduation", {
    draftId,
    streamId,
    standingWork,
    standingWorkMessage,
  });

/**
 * GSU-FR-PVFP: capture the draft's prompt and enqueue a direct run on the
 * worktree and branch the author confirmed.
 *
 * The backend compares both with the active worktree and refuses, creating
 * nothing, where either has changed (GSU-FR-SZTZ).
 */
export const startDirectGraduation = (
  draftId: string,
  expectedWorktree: string,
  expectedBranch: string,
) =>
  invoke<GraduationRun>("start_direct_graduation", {
    draftId,
    expectedWorktree,
    expectedBranch,
  });

/** GSU-FR-MZGD: what the active worktree says about a direct start. */
export const preflightDirectGraduation = () =>
  invoke<DirectGraduationPreflight>("preflight_direct_graduation");

/** GRD-FR-LGDV: every run the project has made, in the project's run order. */
export const listGraduationQueue = () =>
  invoke<GraduationQueue>("list_graduation_queue");

/**
 * GRD-FR-GRHC: the project's limit, the slots held, and the queued runs that
 * wait for a slot alone.
 */
export const getGraduationCapacity = () =>
  invoke<GraduationCapacity>("get_graduation_capacity");

/** GRD-FR-LGDV: one run's whole record. */
export const getGraduationRun = (runId: string) =>
  invoke<GraduationRun>("get_graduation_run", { runId });

/**
 * GRS-FR-RZXA: one page of one stream for one run, phase, and pass scope.
 *
 * The only operation the graduation log window invokes (GLW-FR-LUQI). It is
 * read-only and bounded to one page (GLW-FR-ZPUH).
 */
export const readGraduationLogs = (request: {
  runId: string;
  phaseId: string;
  pass: GraduationLogPassScope;
  stream: GraduationLogStream;
  cursor?: GraduationLogCursor | null;
  limit?: number | null;
  query?: string | null;
}) =>
  invoke<GraduationLogPage>("read_graduation_logs", {
    runId: request.runId,
    phaseId: request.phaseId,
    pass: request.pass,
    stream: request.stream,
    cursor: request.cursor ?? null,
    limit: request.limit ?? null,
    query: request.query ?? null,
  });

/** DRS-FR-18: the run a draft's row names, if it has one. */
export const getDraftGraduation = (draftId: string) =>
  invoke<GraduationRun | null>("get_draft_graduation", { draftId });

/** GRD-FR-CYIB: the one operation behind both Continue and Resume. */
export const continueGraduationRun = (runId: string) =>
  invoke<GraduationRun>("continue_graduation_run", { runId });

/** GRD-FR-MDQZ: the author's own stop, while the run is doing agent work. */
export const pauseGraduationRun = (runId: string) =>
  invoke<GraduationRun>("pause_graduation_run", { runId });

/** GRD-FR-TKUR: whether a stream's queue may start this run. */
export const setGraduationAutoStart = (runId: string, enabled: boolean) =>
  invoke<GraduationRun>("set_graduation_auto_start", { runId, enabled });

/**
 * GRD-FR-RHNP: move one run within its own stream's queue.
 *
 * `expectedPosition` is the index the caller read. One that is stale is
 * refused rather than obeyed, so a listing read before another change landed
 * is told rather than acted on.
 */
export const reorderGraduationRun = (
  runId: string,
  expectedPosition: number,
  newPosition: number,
) =>
  invoke<GraduationQueue>("reorder_graduation_run", {
    runId,
    expectedPosition,
    newPosition,
  });

/** GXD-FR-BJYT: the author's answers, covering every recorded question. */
export const answerGraduationEscalation = (
  runId: string,
  answers: EscalationAnswerInput[],
) => invoke<GraduationRun>("answer_graduation_escalation", { runId, answers });

/** GRD-FR-BLCR: revert the commits one completed run made, as new commits. */
export const revertGraduationRun = (runId: string) =>
  invoke<GraduationRun>("revert_graduation_run", { runId });

/** GRD-FR-EWTN: end a run without reverting anything it committed. */
export const discardGraduationRun = (runId: string) =>
  invoke<GraduationRun>("discard_graduation_run", { runId });

/**
 * GRD-FR-ZAMI: a new run over a discarded run's captured prompt.
 *
 * GSU-FR-IRAC: the restart takes its own standing-work choice and message,
 * against the stream it names, rather than the ones the discarded run carried.
 */
export const restartGraduationRun = (
  runId: string,
  streamId: string,
  standingWork: StandingWork,
  standingWorkMessage: string | null,
) =>
  invoke<GraduationRun>("restart_graduation_run", {
    runId,
    streamId,
    standingWork,
    standingWorkMessage,
  });

/** GRD-FR-JOFE: where the author filed a run. Independent of its lifecycle. */
export const archiveGraduationRun = (runId: string) =>
  invoke<GraduationRun>("archive_graduation_run", { runId });

export const unarchiveGraduationRun = (runId: string) =>
  invoke<GraduationRun>("unarchive_graduation_run", { runId });

/**
 * AGV-FR-10: one page of what an agent CLI did during a run.
 *
 * With no cursor the newest page, so a panel opened on a run that has been
 * working for an hour starts at the end rather than paging to it.
 */
export const readAgentActivity = (runId: string, after?: number) =>
  invoke<AgentActivityPage>("read_agent_activity", { runId, after });
