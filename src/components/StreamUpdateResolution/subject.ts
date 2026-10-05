/**
 * The shape the update resolution window renders
 * (`../../../specifications/ui/WSS-work-stream-selector.md` WSS-FR-ZMPC,
 * WSS-FR-PMYA).
 *
 * The window opens for an update alone. A merge Git could not settle is a merge
 * run, which stands in the Runs panel and is answered there (WSS-FR-NRCQ), so no
 * merge record reaches this window.
 */

import type {
  GraduationEscalation,
  StreamMergeConflict,
  StreamUpdateRecord,
} from "../../types";
import { refusalText } from "../WorkStreamSelector/refusals";

/** WSS-FR-ZMPC: what the window names, states and offers. */
export interface ResolutionSubject {
  streamId: string;
  /** The branch an update takes its source from. */
  baseBranch: string;
  /** WSS-FR-ZMPC: what the record rests on, in words. */
  sentence: string;
  /** The paths the update could not settle. */
  conflicts: StreamMergeConflict[];
  escalation: GraduationEscalation | null;
  running: boolean;
  /** WSS-FR-ZMPC: whether **Retry** is one of the acts the state permits. */
  retryable: boolean;
}

/** The window's view of a base-to-stream update record. */
export function updateSubject(record: StreamUpdateRecord): ResolutionSubject {
  return {
    streamId: record.streamId,
    baseBranch: record.baseBranch,
    sentence: updateStateSentence(record),
    conflicts: record.conflicts,
    escalation: record.escalation ?? null,
    running: record.state === "running",
    retryable:
      record.state === "conflicted" ||
      record.state === "cancelled" ||
      record.state === "failed",
  };
}

/**
 * WSS-FR-GTQL: what the update rests on, in words.
 *
 * Stated as what happened and what is left to do: an update that wrote nothing
 * says so, because the author's next act depends on whether either branch
 * moved.
 */
export function updateStateSentence(record: StreamUpdateRecord): string {
  const paths = record.conflicts.length;
  const pathWord = paths === 1 ? "path" : "paths";
  const strategy =
    record.strategy === "rebase_source" ? "rebase" : "merge of the source";
  switch (record.state) {
    case "running":
      return `The update is running as a ${strategy}.`;
    case "updated": {
      const wrote = record.updatedPaths.length;
      const turns = record.semanticTurns;
      return `Brought ${wrote} ${wrote === 1 ? "path" : "paths"} in from ${record.baseBranch}${
        turns > 0 ? ` after ${turns} agent ${turns === 1 ? "turn" : "turns"}` : ""
      }.`;
    }
    case "nothing_to_update":
      return `This stream already holds everything ${record.baseBranch} does.`;
    case "conflicted":
      return `Git and the agent turns could not settle ${paths} ${pathWord}. Nothing was written.`;
    case "escalated": {
      const asked = record.escalation?.questions.length ?? 0;
      return `The update turn could not choose, and asked you ${asked} ${
        asked === 1 ? "question" : "questions"
      }. Nothing was written.`;
    }
    case "cancelled":
      return "You cancelled this update. Neither branch was changed.";
    case "failed":
      return record.failure
        ? refusalText(record.failure)
        : "This update stopped. Nothing was written.";
  }
}

/**
 * What either side did to one path, as the record holds it.
 *
 * The pair is the whole reason the path is here, so it reads as a sentence
 * rather than as two field values the author has to combine themselves.
 */
export function fateSentence(conflict: StreamMergeConflict): string {
  const base = conflict.baseChange;
  const stream = conflict.streamChange;
  if (!base && !stream) return "";
  if (base === "deleted") return "the base branch deleted it; the stream changed it";
  if (stream === "deleted") return "the stream deleted it; the base branch changed it";
  if (base === "created" && stream === "created") return "both sides created it";
  return "both sides changed it";
}
