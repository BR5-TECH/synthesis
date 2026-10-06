/**
 * Work streams, as the frontend reads them
 * (`../../specifications/core/WKS-work-streams.md`).
 *
 * A work stream is a named branch and working copy the application owns and
 * that lives longer than one run. Runs execute in it and commit onto it.
 */

import type { GraduationEscalation, GraduationRunState } from "./graduation";

/** WKS-FR-QMTV: what one stream is. */
export interface WorkStream {
  id: string;
  projectKey: string;
  /** The author's own name for the stream. */
  name: string;
  /** `synthesis/stream/<slug>`. */
  branch: string;
  /** Absolute, under the application's short data root. */
  worktreePath: string;
  /** The branch it was created from and merges back into. */
  baseBranch: string;
  baseRevision: string;
  createdAt: string;
  /** WKS-FR-JQJA: the run holding it, or null. Durable. */
  busyRunId?: string | null;
  /** WKS-FR-AXRD: Git lists it but its directory has gone. */
  isMissing: boolean;
}

/** WKS-FR-SGCM: one stream as a listing reports it. */
export interface WorkStreamSummary {
  stream: WorkStream;
  /** WKS-FR-YBST: read from the run order index, never from run records. */
  queuedRunCount: number;
  /** Commits the base branch does not hold. */
  aheadOfBase: number;
  /**
   * WKS-FR-RJPD: commits the recorded base branch holds and this stream does
   * not. Zero means the stream is up to date with its base, which is what
   * disables Update on the row (WSS-FR-HZVQ).
   */
  behindBase: number;
  /**
   * WKS-FR-KFVJ: the base branch's tip when this listing was read. It is the
   * revision the stream update window displays and pins on confirmation.
   */
  baseTipRevision: string;
  /** WKS-FR-UBGX: the first of the commits counted by `behindBase`. */
  missingCommits: StreamUpdateCommit[];
  /**
   * WKS-FR-KHJS: the stream's newest merge run that is not discarded and not
   * archived, read from the run order index alone. Absent where it has none.
   *
   * A merge run is the one durable record of a merge Git could not settle, so
   * a row renders its merge status from this and from nothing the surface
   * attempted (WSS-FR-OFCU).
   */
  mergeRun?: StreamMergeRunLink | null;
  /** WKS-FR-SGCM: what this stream's last update did, or nothing. */
  update?: StreamUpdateRecord | null;
}

/** WKS-FR-KHJS: where a stream's merge run stands, as a listing carries it. */
export interface StreamMergeRunLink {
  runId: string;
  /** `Merge <stream name>`. */
  name: string;
  /** The run's own state. */
  state: GraduationRunState;
}

/** WKS-FR-GKPX: how a merge lands on the base branch. */
export type StreamMergePublication =
  | { kind: "uncommitted" }
  | { kind: "commit"; message: string };

/**
 * WKS-FR-QNHF: what `"merge work stream (id, publication)"` returns.
 *
 * The three kinds are different facts, so they are different shapes. A clean
 * merge and a stream that held nothing make no run. A conflict makes a merge
 * run, and the call names it.
 */
export type StreamMergeResult =
  | { kind: "nothing_to_merge" }
  | {
      kind: "merged";
      /** Project-relative paths the merge wrote. */
      mergedPaths: string[];
      /** The merge commit, where the publication committed. */
      commit?: string | null;
    }
  | {
      kind: "conflicted";
      /** The merge run the conflict was handed to. */
      runId: string;
      /** What Git could not settle. Nothing was written. */
      conflictedPaths: string[];
    };

/**
 * WSS-FR-XRHT: whether the stream's merge run stands in a state that keeps
 * another reconciliation from starting.
 *
 * Only `completed` and `failed` let go: every other state is a run that still
 * holds, or still asks for, the stream.
 */
export function mergeRunBlocksStream(
  link: StreamMergeRunLink | null | undefined,
): boolean {
  return Boolean(link) && link!.state !== "completed" && link!.state !== "failed";
}

/**
 * GRB-FR-SRVN: one path a reconciliation could not settle, as the record keeps
 * it.
 *
 * No content and no container path: what survives is the path and what either
 * side did to it.
 */
export interface StreamMergeConflict {
  /** Project-relative. */
  path: string;
  /** `created` | `updated` | `deleted` | `unchanged`, or empty where unknown. */
  baseChange: string;
  streamChange: string;
}

/** GRB-FR-KMXT: one decision the author has already made about an update. */
export interface StreamMergeDecision {
  /** The recorded position of the question this answers. */
  position: number;
  question: string;
  answer: string;
  summary: string;
}

// --- Update (base branch → stream) ----------------------------------------
//
// The opposite direction to a merge: an update brings the stream branch up to a
// pinned revision of the branch it was created from. It is a durable operation
// of its own, with a record of its own beside the stream's merge run link.

/** WKS-FR-NRQT: which direction Git is asked to take the stream up to its base. */
export type StreamUpdateStrategy = "merge_source" | "rebase_source";

/** WKS-FR-VQRD: where one stream's last update stands. */
export type StreamUpdateState =
  | "running"
  | "updated"
  | "nothing_to_update"
  | "conflicted"
  | "escalated"
  | "cancelled"
  | "failed";

/** WKS-FR-UBGX: one commit the stream is missing from its base branch. */
export interface StreamUpdateCommit {
  /** The full object id. */
  revision: string;
  /** The commit's first line. */
  summary: string;
  author: string;
  committedAt: string;
}

/**
 * WKS-FR-ZKUP: what one stream's most recent update did.
 *
 * An update outlives the window that started it, so this — and not the call
 * that started it — is what every surface reads an update from.
 */
export interface StreamUpdateRecord {
  streamId: string;
  projectKey: string;
  state: StreamUpdateState;
  /** WKS-FR-AMWE: the strategy the author chose. */
  strategy: StreamUpdateStrategy;
  /** The attempt the state belongs to. Empty before the first attempt. */
  attemptId: string;
  /** WKS-FR-XDBM: the stream's recorded base branch, which is the source. */
  baseBranch: string;
  /** WKS-FR-AMWE: the revision the author's confirmation pinned. */
  baseRevision: string;
  semanticTurns: number;
  /** The turn's own words, where the state is `escalated`. */
  reason: string;
  /** The typed refusal, where the state is `failed`. */
  failure: string;
  requestedAt: string;
  updatedAt: string;
  /** The commits the stream was missing at the request. */
  missingCommits: StreamUpdateCommit[];
  escalation?: GraduationEscalation | null;
  /** Set where the state is `conflicted`. */
  conflicts: StreamMergeConflict[];
  /** Set where the state is `updated`. */
  updatedPaths: string[];
  decisions: StreamMergeDecision[];
}

/** WKS-FR-VQRD: whether an update of this stream is working now. */
export function isUpdateRunning(
  record: StreamUpdateRecord | null | undefined,
): boolean {
  return record?.state === "running";
}

/** WKS-FR-VQRD: whether the update rests on something only the author answers. */
export function updateWaitsOnAuthor(
  record: StreamUpdateRecord | null | undefined,
): boolean {
  return record?.state === "escalated";
}

/** WKS-FR-DPNM: whether a new update of the stream may start from this one. */
export function isUpdateRetryable(
  record: StreamUpdateRecord | null | undefined,
): boolean {
  return (
    record?.state === "conflicted" ||
    record?.state === "cancelled" ||
    record?.state === "failed"
  );
}

/** WSS-FR-GTQL: whether the row must offer the author something to do. */
export function updateNeedsAuthor(
  record: StreamUpdateRecord | null | undefined,
): boolean {
  return updateWaitsOnAuthor(record) || isUpdateRetryable(record);
}

/**
 * WSS-FR-HZVQ: whether **Update…** is enabled for this stream.
 *
 * Only where the stream stands behind its base branch and holds no queued and
 * no active run. A stream that is up to date has nothing to bring in, and one a
 * run holds is being written by an agent.
 */
export function canUpdateStream(summary: WorkStreamSummary): boolean {
  return (
    summary.behindBase > 0 &&
    summary.queuedRunCount === 0 &&
    !summary.stream.busyRunId &&
    !summary.stream.isMissing &&
    summary.baseTipRevision.length > 0
  );
}

/**
 * WSS-FR-XRHT: whether either reconciliation of this stream is in force.
 *
 * **Merge…** and **Update…** are both disabled while one is, because one
 * reconciliation of a stream is startable at a time.
 */
export function isReconciling(summary: WorkStreamSummary): boolean {
  return mergeRunBlocksStream(summary.mergeRun) || isUpdateRunning(summary.update);
}

/**
 * WKS-FR-FQLS: where a running update has reached.
 *
 * `turn` is zero once the update has settled, so a surface reading this alone
 * tells an update that is working from one that has stopped.
 */
export interface WorkStreamUpdateProgress {
  projectKey: string;
  streamId: string;
  /** What a surface reads that turn's agent activity by. */
  attemptId: string;
  strategy: StreamUpdateStrategy;
  /** Which semantic turn has begun, or 0 once the update has settled. */
  turn: number;
  turnsMax: number;
  /** Project-relative paths this turn is reconciling. */
  reconcilingPaths: string[];
}

/** The typed refusals the stream operations answer with. */
export const STREAM_ERRORS = {
  notAGitRepository: "not_a_git_repository",
  baseBranchRequired: "base_branch_required",
  nameTaken: "stream_name_taken",
  nameInvalid: "stream_name_invalid",
  creationFailed: "stream_creation_failed",
  cleanupFailed: "stream_cleanup_failed",
  unknownStream: "unknown_stream",
  busy: "stream_busy",
  dirty: "stream_dirty",
  baseDirty: "base_dirty",
  missing: "stream_missing",
  baseNotCheckedOut: "base_not_checked_out",
  unmerged: "stream_unmerged",
  hasRuns: "stream_has_runs",
  /** WKS-FR-OVLQ: the stream's working copy is the project's active worktree. */
  active: "stream_active",
  /** GRB-FR-JIRD: one merge or update of one repository runs at a time. */
  mergeInProgress: "merge_in_progress",
  /** GRD-FR-XHSE: a branch moved after the merge was handed off. */
  mergeBranchMoved: "merge_branch_moved",
  /** GRD-FR-JSBE: a working copy holds uncommitted work, so nothing was applied. */
  mergeDirtySide: "merge_dirty_side",
  /** GRD-FR-JSBE: another merge or update holds the repository. */
  mergeGuardHeld: "merge_guard_held",
  /** GRD-FR-JSBE: the application could not write the result. */
  mergeApplyFailed: "merge_apply_failed",
  /** WKS-FR-BPGM: the image preflight refusals a conflicting merge answers with. */
  vendorImageUnconfigured: "vendor_image_unconfigured",
  vendorImageInvalid: "vendor_image_invalid",
  vendorExecutionUnsupported: "vendor_execution_unsupported",
  dockerBackendUnverified: "docker_backend_unverified",
  /** WKS-FR-GKPX: a conflict a text turn cannot reconcile; nothing was written. */
  unsupportedConflict: "unsupported_conflict",
  /** GRB-FR-YPEX: the material an update turn reads could not be written. */
  artifactGenerationFailed: "artifact_generation_failed",
  /** WKS-FR-KFVJ: the base branch moved since the window read its revision. */
  staleBaseRevision: "stale_base_revision",
  /** GRB-FR-CLRO: three semantic turns of one update settled nothing. */
  updateAttemptsExhausted: "update_attempts_exhausted",
  /** WKS-FR-TSOA: an update of this repository already holds the update guard. */
  updateInProgress: "update_in_progress",
  /** GRB-FR-TXVL: the author stopped the update. Neither branch was changed. */
  updateCancelled: "update_cancelled",
  /** WKS-FR-QFTH: the application stopped while the update was running. */
  updateInterrupted: "update_interrupted",
  /** WKS-FR-DPNM / WKS-FR-CBXW: the update does not rest where the request needs it. */
  updateStateNotPermitted: "update_state_not_permitted",
} as const;

/** WKS-FR-MFDW: whether a branch is a work stream's. */
export function isStreamBranch(branch: string): boolean {
  return branch.startsWith("synthesis/stream/");
}

/** WKS-FR-CYAG: whether a run is holding this stream. */
export function isStreamBusy(stream: WorkStream): boolean {
  return Boolean(stream.busyRunId);
}

/** WKS-FR-WULF: what the work-streams-changed event carries. */
export interface WorkStreamsChangedPayload {
  projectKey: string;
}
