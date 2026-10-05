/**
 * The shapes the selector and its row share
 * (`../../../specifications/ui/WSS-work-stream-selector.md` WSS-FR-HGWL,
 * WSS-FR-BDMU).
 *
 * A merge call and an update each outlive the confirmation that started them,
 * so what the row shows while one runs is held by the selector rather than by
 * the surface that opened it: the author closes and reopens the dropdown while
 * they run.
 */

import type { StreamMergeResult, WorkStreamUpdateProgress } from "../../types";

/**
 * WSS-FR-HGWL: a merge call this surface started and is waiting on.
 *
 * Held for the length of the call alone. It ends when the call settles,
 * whatever the call returns.
 */
export interface RunningMerge {
  streamName: string;
}

/**
 * WSS-FR-KMHD / WSS-FR-PLVE: what a settled merge call answered, kept on the
 * row until the dropdown closes or the author starts another act on the stream.
 * It is the answer to a finished call and is no record.
 */
export type MergeOutcome = StreamMergeResult;

/** WSS-FR-BDMU: an update this surface started and is showing. */
export interface RunningUpdate {
  streamName: string;
  /** The latest turn reported, or null before the first one begins. */
  progress: WorkStreamUpdateProgress | null;
}

/** What confirmation or window a row has open, if any. */
export type RowSurface =
  | { kind: "none" }
  | { kind: "merge"; streamId: string }
  | { kind: "update"; streamId: string }
  | { kind: "delete"; streamId: string };

/**
 * WSS-FR-JMWA: what a row says about a path set it cannot fit.
 *
 * The first few paths, then a count of the rest. The whole set is on the row's
 * `title` and in the update resolution window; a row that listed every path
 * would grow past the listing's own scrolling region and take the stream's name
 * and its action out of view with it.
 */
export function namedPaths(paths: string[], shown = 3): string {
  if (paths.length <= shown) return paths.join(", ");
  const rest = paths.length - shown;
  return `${paths.slice(0, shown).join(", ")} and ${rest} more`;
}
