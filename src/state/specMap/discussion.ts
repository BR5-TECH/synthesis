/**
 * What the Map session keeps of the Discussion column
 * (`SMP-specification-map.md` SMP-FR-AUFA, SMP-FR-SWCL): whether it shows, how
 * the tab is split, which thread it reads, whether it holds the opening
 * composer of a new thread, and whether its resolved disclosure is expanded.
 * The unsent text of its composers is in the discussion session store, which a
 * project switch clears in the same step as the map session (SMP-FR-MEZK).
 */
import { documentFraction, type SplitRatio } from "../../components/DraftDiscussion/ratio";
import type { MapNodeRef } from "../../types";

/** SMP-FR-SWCL: the split the column opens at, canvas 70. */
export const DISCUSSION_RATIO_DEFAULT: SplitRatio = "doc70";

/** SMP-FR-SWCL: below this Map tab width the column replaces the inspector. */
export const DISCUSSION_REPLACES_INSPECTOR_BELOW = 900;

export interface MapDiscussionState {
  /** SMP-FR-WNLR: shown on the first open of a map session. */
  visible: boolean;
  /** SMP-FR-SWCL: the split between the canvas side and the column. */
  ratio: SplitRatio;
  /** SMP-FR-WYZA: the thread the column reads, or null for the first open tab. */
  threadId: string | null;
  /**
   * SMP-FR-KHKC / SMP-FR-IHFM: the column holds the opening composer of a new
   * thread, about `node` or about the whole map.
   */
  composing: boolean;
  /** The node of the opening composer, or null for a whole-map thread. */
  node: MapNodeRef | null;
  /** SMP-FR-HSZM / CMT-FR-17: the resolved disclosure is expanded. */
  showResolved: boolean;
  /** SMP-FR-IHFM: advances at each Discuss, so each puts the caret in the composer. */
  focusSignal: number;
  /** SMP-FR-IHFM: advances at each Discuss, so the column asks for the thread of the node. */
  discussSignal: number;
  /**
   * SMP-FR-IHFM: the Discuss whose look-up has answered. A column that mounts
   * again does not ask again, so it keeps the composer the author writes in.
   */
  lookedSignal: number;
}

export function initialDiscussionState(): MapDiscussionState {
  return {
    visible: true,
    ratio: DISCUSSION_RATIO_DEFAULT,
    threadId: null,
    composing: false,
    node: null,
    showResolved: false,
    focusSignal: 0,
    discussSignal: 0,
    lookedSignal: 0,
  };
}

/** SMP-FR-SWCL: how much of the tab body the column takes, as a fraction. */
export function columnFraction(ratio: SplitRatio): number {
  return 1 - documentFraction(ratio);
}

/** MPR-FR-MYLA: two references name the same node when lens, kind, and id are equal. */
export function sameMapNode(a: MapNodeRef | null | undefined, b: MapNodeRef | null | undefined): boolean {
  if (!a || !b) return !a && !b;
  return a.lens === b.lens && a.kind === b.kind && a.id === b.id;
}

/** The key of an opening composer in the session stores: the whole map, or one node. */
export function mapThreadKey(node: MapNodeRef | null | undefined): string {
  return node ? `map-thread:${node.lens}:${node.kind}:${node.id}` : "map-thread";
}
