/**
 * What a run tells the author about itself
 * (`../../specifications/core/GOB-graduation-observability.md`).
 *
 * A description of the run rather than a part of it: nothing here gates,
 * delays or decides a transition.
 */

import type { ReviewFinding, ReviewOutcome } from "./graduation";

/** GOB-FR-JAJU: the four stages, in order. */
export type GraduationVisualStage = "queued" | "working" | "review" | "done";

/** GOB-FR-ZMCA: the six conditions a stage stands in. */
export type GraduationStageCondition =
  | "active"
  | "waiting"
  | "paused"
  | "blocked"
  | "stopped"
  | "complete";

/** GOB-FR-HDZI: why a run moved from one stage to another. */
export type StageReason =
  | "enqueued"
  | "work_started"
  | "review_started"
  /** GOB-FR-BTXN: a backward move. */
  | "review_revision"
  /** GOB-FR-BTXN: the other backward move — the author continued a run that
   * had come to rest, so it goes back to the queue. */
  | "blocked_retry"
  | "finished"
  | "ended";

/** GOB-FR-VVNI: one append-only entry of the stage history. */
export interface StageTransition {
  from: GraduationVisualStage;
  to: GraduationVisualStage;
  /** The pass the move belongs to. */
  pass: number;
  at: string;
  reason: StageReason;
}

/** GOB-FR-ABRE: what a pass is doing, or what it decided. */
export type PassStatus = "working" | "passed" | "failed";

/** GOB-FR-XYCY: one pass, whole. */
export interface PassRecord {
  pass: number;
  status: PassStatus;
  /** The whole instruction the work turn was given. */
  task: string;
  verdict?: ReviewOutcome | null;
  /** The review's own words. */
  rationale?: string | null;
  findings: ReviewFinding[];
  /** What the following turn was told, whole. */
  nextInstruction?: string | null;
  startedAt: string;
  endedAt?: string | null;
}

/** GOB-FR-XMDU: the whole record, versioned. */
export interface GraduationObservability {
  /**
   * GOB-FR-XMDU: the integer 1. A reader that does not recognise a version
   * renders none of it rather than guessing at its shape.
   */
  observabilityVersion: number;
  currentStage: GraduationVisualStage;
  stageCondition: GraduationStageCondition;
  stageHistory: StageTransition[];
  passes: PassRecord[];
}

/** The one version this build renders. */
export const OBSERVABILITY_VERSION = 1;

/** GOB-FR-XMDU: whether this record is one this build can render. */
export function hasReadableProgress(
  observability: GraduationObservability | null | undefined,
): boolean {
  return observability?.observabilityVersion === OBSERVABILITY_VERSION;
}
