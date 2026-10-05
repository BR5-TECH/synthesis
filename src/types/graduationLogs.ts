/**
 * One page of one of a run's two log streams
 * (`../../specifications/core/GRS-graduation-run-log-storage.md`).
 *
 * The window reads these and holds no source of truth of its own
 * (`../../specifications/ui/GLW-graduation-log-window.md` GLW-FR-THAX). A
 * record is returned exactly as the file holds it, so its own fields are the
 * persisted ones and stay in the shape the backend wrote (GRS-FR-HVUJ).
 */

import type { GraduationLogFailure } from "./graduation";

export type GraduationLogStream = "source" | "structured";

/** GRS contract surface: what a read's pass scope names. */
export type GraduationLogPassScope =
  | { kind: "pass"; pass: number }
  | { kind: "run_level" }
  | { kind: "phase" };

/** GRS-FR-UEIL: valid only for the run and the stream it names. */
export interface GraduationLogCursor {
  runId: string;
  stream: GraduationLogStream;
  direction: "after" | "before";
  sequence: number;
}

/** GRS-FR-HVUJ: computed for one read, and written to neither file. */
export interface GraduationLogPresentation {
  /** True exactly where the persisted `pass` is null. */
  runLevel: boolean;
}

/** GRS-FR-DYPS: the persisted record beside what the read computed about it. */
export interface GraduationLogPageEntry {
  record: Record<string, unknown>;
  presentation: GraduationLogPresentation;
}

/** GRS-FR-CTQI: exactly one of four. */
export type GraduationLogReadStatus =
  | "available"
  | "empty"
  | "unavailable"
  | "persistence_failed";

/** GRS-FR-NSGX: what a query did, beside the status the scope settles. */
export type GraduationLogSearchResult =
  | "not_requested"
  | "matched"
  | "search_no_match";

/** GRS-FR-DYPS: the whole of what one read answers with. */
export interface GraduationLogPage {
  runId: string;
  stream: GraduationLogStream;
  phaseId: string;
  scope: GraduationLogPassScope;
  /** Ascending by the record's own `sequence`, whichever cursor produced them. */
  entries: GraduationLogPageEntry[];
  nextCursor?: GraduationLogCursor | null;
  /** GRS-FR-OWDT: null exactly where `oldestReached` is true. */
  olderCursor?: GraduationLogCursor | null;
  matchedTotal: number;
  oldestReached: boolean;
  latestSequence: number;
  status: GraduationLogReadStatus;
  search: GraduationLogSearchResult;
  failure?: GraduationLogFailure | null;
}

/** GRS-FR-UCZL: records became durable. It carries no record content. */
export interface GraduationLogAppendedPayload {
  runId: string;
  stream: GraduationLogStream;
  latestSequence: number;
}
