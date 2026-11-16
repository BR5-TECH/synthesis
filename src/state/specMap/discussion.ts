/**
 * What the Map session keeps of the Discussion column
 * (`SMP-specification-map.md` SMP-FR-AUFA, SMP-FR-SWCL): whether it shows, how
 * wide it is, and which thread it reads. The unsent text of its composer is in
 * the discussion session store, which a project switch clears in the same step
 * as the map session (SMP-FR-MEZK).
 */
import type { MapNodeRef } from "../../types";

/** SMP-FR-SWCL: the width of the column, in pixels. */
export const DISCUSSION_WIDTH_DEFAULT = 360;
export const DISCUSSION_WIDTH_MIN = 280;
export const DISCUSSION_WIDTH_MAX = 560;

/** SMP-FR-SWCL: below this Map tab width the column replaces the inspector. */
export const DISCUSSION_REPLACES_INSPECTOR_BELOW = 900;

export interface MapDiscussionState {
  /** SMP-FR-WNLR: shown on the first open of a map session. */
  visible: boolean;
  width: number;
  /** The node of the thread the column reads, or null for the Map thread (SMP-FR-WYZA). */
  node: MapNodeRef | null;
  /** SMP-FR-IHFM: advances at each Discuss, so each puts the caret in the composer. */
  focusSignal: number;
}

export function initialDiscussionState(): MapDiscussionState {
  return { visible: true, width: DISCUSSION_WIDTH_DEFAULT, node: null, focusSignal: 0 };
}

export function clampDiscussionWidth(width: number): number {
  return Math.min(DISCUSSION_WIDTH_MAX, Math.max(DISCUSSION_WIDTH_MIN, Math.round(width)));
}

/** MPR-FR-MYLA: two references name the same node when lens, kind, and id are equal. */
export function sameMapNode(a: MapNodeRef | null | undefined, b: MapNodeRef | null | undefined): boolean {
  if (!a || !b) return !a && !b;
  return a.lens === b.lens && a.kind === b.kind && a.id === b.id;
}

/** The key of a thread in the session stores: the Map thread, or one node. */
export function mapThreadKey(node: MapNodeRef | null | undefined): string {
  return node ? `map-thread:${node.lens}:${node.kind}:${node.id}` : "map-thread";
}
