/**
 * What a graduation run is, as the frontend reads it
 * (`../../specifications/core/GRD-graduation.md`).
 *
 * Every shape here mirrors what the backend serialises. Nothing is derived from
 * a state name or an elapsed time: a surface renders what the record says.
 */

import type { GraduationObservability } from "./graduationObservability";
import type { GraduationLogStream } from "./graduationLogs";

/** GRD-FR-QJHM: the nine states a run stands in. */
export type GraduationRunState =
  /** Waiting for its stream. */
  | "queued"
  /** A work turn holds the stream. */
  | "working"
  /** A review turn is judging what the work turn wrote. */
  | "reviewing"
  /** An escalation, or a review that did not settle, waits on the author. */
  | "awaiting_author"
  /** A condition the author clears. */
  | "blocked"
  /** Stopped, or resting on a retryable failure. */
  | "interrupted"
  /** Terminal: the work is committed on the stream. */
  | "completed"
  /** Terminal. */
  | "discarded"
  /** Terminal. */
  | "failed";

/** GRD-FR-QJHM: the three states a run never leaves. */
export const TERMINAL_RUN_STATES: readonly GraduationRunState[] = [
  "completed",
  "discarded",
  "failed",
];

/** GRD-FR-BNTC: the states in which a run is holding its stream. */
export const STREAM_HOLDING_STATES: readonly GraduationRunState[] = [
  "working",
  "reviewing",
  "blocked",
];

/** GRD-FR-FTBQ: the prompt a run answers, as it stood at capture. */
export interface CapturedGraduationInput {
  draftId: string;
  /** The draft's name at capture. */
  draftName: string;
  /** The whole text of the draft's one prompt at capture. */
  prompt: string;
  promptChecksum: string;
  capturedAt: string;
}

/** One response an agent proposed for a question it asked. */
export interface GraduationProposedResponse {
  answer: string;
  summary: string;
  description: string;
}

/** GXD-FR-HGSU: one recorded question, at its stable position. */
export interface GraduationEscalationQuestion {
  /** 1-based, stable, assigned in the order the questions arrived. */
  position: number;
  question: string;
  /** 0 to 3; empty where no fixed response suits. */
  options: GraduationProposedResponse[];
}

/** GXD-FR-XPUR: which turn an answered escalation is delivered into. */
export type GraduationEscalationOrigin =
  | "work"
  | "review"
  | "semantic_merge";

/** One answer the author gave, at the position of the question it answers. */
export interface EscalationAnswerInput {
  position: number;
  answer: string;
  summary: string;
}

/** GXD-FR-HGSU: what a run in `awaiting_author` is waiting for. */
export interface GraduationEscalation {
  reason: string;
  questions: GraduationEscalationQuestion[];
  origin: GraduationEscalationOrigin;
  raisedAt: string;
}

/** GRL-FR-VIAT: how serious a finding is. */
export type ReviewSeverity = "critical" | "major" | "minor";

/** GRL-FR-VIAT: one problem, stated once. */
export interface ReviewFinding {
  severity: ReviewSeverity;
  description: string;
  /** Project-relative; may name a path that does not exist yet. */
  affectedFiles: string[];
  correction: string;
}

/** GRL-FR-VIAT: the two verdicts. */
export type ReviewOutcome = "ready" | "revise";

/**
 * GRD-FR-PUXO: why a run stopped. `retryable_failure` is a stop whose cause was
 * not recorded.
 */
export type GraduationInterruptionReason =
  | "author_pause"
  | "application_shutdown"
  | "project_changed"
  | "retryable_failure"
  | "log_persistence_failed"
  | "execution_abandoned"
  | "execution_timeout"
  | "agent_exited"
  | "agent_terminated"
  | "unreadable_answer"
  | "launch_failed";

/** GRD-FR-XVUD: why an interrupted run stopped and what it waits for. */
export interface GraduationInterruption {
  reason: GraduationInterruptionReason;
  at: string;
  /** A short, author-facing explanation. Never a credential, never content. */
  detail: string;
  /** GRD-FR-MDQZ: the run gave its stream back. True for an author pause. */
  streamReleased: boolean;
  resumeRequestedAt?: string | null;
}

/** GRD-FR-QJHM: what blocks a run, and the act that clears it. */
export interface GraduationBlocker {
  code: string;
  message: string;
  /** What the author does about it, in their own terms. */
  clearsBy: string;
  /**
   * GRU-FR-LBPR: which consecutive attempt this code blocked the run on,
   * counted from one. A record written before this field existed reads as zero,
   * which renders as no attempt at all.
   */
  attempt: number;
}

/** A run that ended on a failure. */
export interface GraduationFailure {
  code: string;
  message: string;
  retryable: boolean;
}

/** GXD-FR-GMDI: what a stopped run is continued from. */
/**
 * GRL-FR-ARPX: how many passes a run takes where the project configures no
 * pass budget of its own (PSS-FR-WPKS).
 *
 * A run that has been dispatched carries its own bound in the checkpoint
 * (GXD-FR-PWYD), so the surfaces read `passBoundOf` rather than this constant.
 * This is what they render before the first dispatch has set one.
 */
export const DEFAULT_PASS_BUDGET = 2;

/**
 * GXD-FR-PWYD / GRU-FR-FZCN: the highest pass this run may take.
 *
 * The bound the run's own budget window holds, which the loop recomputes from
 * the project's settings at every dispatch. A run that has not been dispatched
 * yet holds no window, and reads as the default budget.
 */
export function passBoundOf(run: GraduationRun): number {
  const limit = run.checkpoint?.passLimit ?? 0;
  return limit > 0 ? limit : DEFAULT_PASS_BUDGET;
}

export interface GraduationCheckpoint {
  /** The pass the run stands at, counted from one. */
  pass: number;
  /**
   * GXD-FR-GMDI: the pass this run's current budget window starts from.
   *
   * Continue after an exhausted budget moves it to the next pass, so the run
   * repeats no completed pass (GRL-FR-XBUE).
   */
  passFloor?: number;
  /**
   * GXD-FR-PWYD: the highest pass the current window allows. Zero, or absent,
   * before the run's first dispatch, which is no bound yet.
   */
  passLimit?: number;
  /** What the working copy holds against the base commit. */
  changedPaths: string[];
  /** What the repository's ignore rules kept out of that change set. */
  hiddenPaths: string[];
  /** How many more of those there were than the record carries. */
  hiddenPathsOmitted: number;
  /** GRL-FR-GQAB: what the next work turn is to act on. */
  loopInstruction?: string | null;
  pendingEscalationAnswers: EscalationAnswerInput[];
  verdictRefusals: number;
  /** GXD-FR-TJRV: the phase the run resumes at. */
  resumePart?: "work" | "review" | null;
  /** GRL-FR-QZFB: the code of the blocker last raised. */
  blockedCode?: string | null;
  /** GRL-FR-QZFB: how many times in a row that code was raised. */
  blockAttempts: number;
}

/** GRS-FR-WFWD: how far one of a run's two log streams stands. */
export interface GraduationStreamIndex {
  stream: GraduationLogStream;
  latestSequence: number;
  recordCount: number;
  durableThroughSequence: number;
  byteLength: number;
  segments: Array<{
    phaseId: string;
    pass: number | null;
    firstSequence: number;
    lastSequence: number;
    recordCount: number;
  }>;
}

/** GRS-FR-EYNU: what failed, and the act that clears it. */
export interface GraduationLogFailure {
  kind: "write" | "read";
  code: string;
  stream: GraduationLogStream;
  message: string;
  at: string;
  stoppedSequence?: number | null;
  byteOffset?: number | null;
  pendingRecordIds: string[];
}

/** GRS-FR-CGSP: whether the run's streams are being written. */
export interface GraduationLogPersistence {
  status: "healthy" | "failed";
  failure?: GraduationLogFailure | null;
  pendingCount: number;
  updatedAt: string;
}

/** GRS-FR-CGSP: what the run record holds about its logs. It holds no payload. */
export interface GraduationLogIndexes {
  logStorageVersion: number;
  activity: GraduationStreamIndex;
  structured: GraduationStreamIndex;
  persistence: GraduationLogPersistence;
  lastReadFailure?: GraduationLogFailure | null;
}

/**
 * GRD-FR-HQPD: what a run does with work standing uncommitted in its stream
 * when its turn comes.
 *
 * The author chooses it when the run is enqueued, so a run that waits behind
 * another needs no decision of theirs at the moment it starts.
 */
export type StandingWork = "keep" | "commit" | "commit_and_push";

/** GRD-FR-KDWA: what the standing-work step of one run did. */
export interface StandingWorkOutcome {
  /** The revision the standing work was committed as, where it committed one. */
  commit?: string | null;
  /** Whether the remote took the stream branch. Absent where none was asked. */
  pushed?: boolean | null;
  /** GRD-FR-PXVJ: why the remote did not take it. The run is not stopped by it. */
  pushFailure?: { code: string } | null;
}

/** GRD-FR-BSNI: where a direct run works, pinned when the author confirmed. */
export interface DirectTarget {
  /** Absolute path of the pinned worktree. */
  worktreePath: string;
  worktreeName: string;
  /** The branch the worktree held at confirmation. */
  branch: string;
}

/** GRD-FR-XRDY: why a queued direct run is not dispatched. */
export interface TargetHold {
  code: "target_branch_changed" | string;
  /** The branch the run pinned. */
  expectedBranch: string;
  /** The branch the worktree holds now; absent when detached or gone. */
  actualBranch?: string | null;
}

/** GSU-FR-MZGD: what the active worktree says about a direct start. */
export interface DirectGraduationPreflight {
  worktreePath: string;
  worktreeName: string;
  /** Absent when the worktree is detached. */
  branch?: string | null;
  isDetached: boolean;
  /** The work stream the worktree is the working copy of, if it is one. */
  stream?: { streamId: string; streamName: string; busyRunId?: string | null } | null;
  /** Every uncommitted path, complete. */
  dirtyPaths: string[];
}

/** GRD-FR-KZPT: how a merge run's result reaches the base branch. */
export type GraduationMergePublication =
  | { kind: "uncommitted" }
  | { kind: "commit"; message: string };

/** GRD-FR-KZPT: what either side did to one path Git could not merge. */
export interface GraduationMergeConflict {
  /** Project-relative. */
  path: string;
  /** `created` | `updated` | `deleted` | `unchanged`. */
  baseChange: string;
  streamChange: string;
}

/** GRD-FR-AQNW: what an applied merge put on the base branch. */
export interface GraduationMergeResult {
  /** Whether the result was left uncommitted or committed. */
  published: "uncommitted" | "commit";
  /** The merge commit, where the publication committed. */
  commit?: string | null;
  /** Project-relative. */
  mergedPaths: string[];
}

/**
 * GRD-FR-MRNQ: what makes a run a merge run.
 *
 * A merge run is the only durable record of a stream merge Git could not
 * settle. It has no draft, so `name` is its title wherever a draft run shows
 * its draft's name (GRD-FR-VCTH).
 */
export interface GraduationMergeData {
  /** `Merge <stream name>`. */
  name: string;
  streamBranch: string;
  baseBranch: string;
  baseTip: string;
  streamTip: string;
  mergeBase: string;
  snapshotCommit: string;
  publication: GraduationMergePublication;
  /** Every path the Git merge changes against the base tip. */
  changedPaths: string[];
  /** The paths Git could not merge. */
  unresolvedPaths: string[];
  conflicts: GraduationMergeConflict[];
  /** Written when the merge is applied (GRD-FR-AQNW). */
  result?: GraduationMergeResult | null;
}

export interface GraduationRun {
  id: string;
  /**
   * GRD-FR-PZAK: the stream this run runs in. It outlives the stream. Empty for
   * a direct run on an ordinary worktree.
   */
  streamId: string;
  streamName: string;
  /** GRD-FR-BSNI: where a direct run works. Absent on a stream run. */
  directTarget?: DirectTarget | null;
  /** GRD-FR-XRDY: why dispatch is withheld from this queued direct run. */
  targetHold?: TargetHold | null;
  projectKey: string;
  state: GraduationRunState;
  /** GRD-FR-HQPD: chosen when the run was enqueued, applied at its dispatch. */
  standingWork: StandingWork;
  /** GRD-FR-RJFC: the message the commit that choice makes takes. */
  standingWorkMessage?: string | null;
  /** GRD-FR-KDWA: what that choice did. Written at the first dispatch. */
  standingWorkOutcome?: StandingWorkOutcome | null;
  input: CapturedGraduationInput;
  /** GRD-FR-YBUM: the revision this run is measured from. */
  baseCommit?: string | null;
  /** The revisions this run created, in order. */
  commits: string[];
  autoStart: boolean;
  /** GRD-FR-JOFE: where the author filed it. Independent of `state`. */
  archived: boolean;
  archivedAt?: string | null;
  /** GRD-FR-MRNQ: present on a merge run alone. */
  merge?: GraduationMergeData | null;
  workTurns: number;
  reviewTurns: number;
  checkpoint: GraduationCheckpoint;
  logs: GraduationLogIndexes;
  observability: GraduationObservability;
  escalation?: GraduationEscalation | null;
  blocker?: GraduationBlocker | null;
  interruption?: GraduationInterruption | null;
  restartedFromRunId?: string | null;
  failure?: GraduationFailure | null;
  createdAt: string;
  updatedAt: string;
}

/** GRD-FR-LGDV: the project's whole run order. */
export interface GraduationQueue {
  projectKey: string;
  /** Every run the project has made, in the project's run order. */
  runs: GraduationRun[];
}

/**
 * PSS-FR-JRWC: the project-wide limit of graduation runs. A positive integer,
 * or the named value `unlimited`, which is never an integer sentinel
 * (PSS-FR-FHQU).
 */
export type GraduationConcurrencyLimit = number | "unlimited";

/** GRD-FR-GRHC: what the project's slots hold now. */
export interface GraduationCapacity {
  limit: GraduationConcurrencyLimit;
  /** GRD-FR-KKKN: how many project slots are held. */
  inUse: number;
  /** GRD-FR-GRHC: the queued runs that wait for a project slot alone. */
  waitingForSlot: string[];
}

/** DRS-FR-18: what a draft's row is told about its run. */
export interface DraftGraduation {
  runId: string;
  state: GraduationRunState;
  /** DRS-FR-19: a non-terminal run holds the draft. */
  locked: boolean;
  /** DRS-FR-KQTW: a run of this draft committed and is not discarded. */
  graduated: boolean;
}

/** GRD-FR-EFAU: what the queue-changed event carries. */
export interface GraduationQueueChangedPayload {
  projectKey: string;
  /** The stream whose queue changed; empty for an ordinary worktree's queue. */
  streamId: string;
  /** The worktree a direct run's queue belongs to; empty for a stream run. */
  worktreePath: string;
}

/** GRD-FR-EFAU: what the run-changed event carries. */
export interface GraduationRunChangedPayload {
  runId: string;
  state: GraduationRunState;
}

/** GRD-FR-QJHM: whether this state is one a run never leaves. */
export function isTerminalRun(state: GraduationRunState): boolean {
  return TERMINAL_RUN_STATES.includes(state);
}

/** GRD-FR-BNTC: whether a run in this state is holding its stream. */
export function holdsStream(state: GraduationRunState): boolean {
  return STREAM_HOLDING_STATES.includes(state);
}
