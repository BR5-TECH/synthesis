import { useCallback, useMemo, useSyncExternalStore } from "react";

import {
  resolveBoardLayout,
  resolveLayout,
  toBoardStoredLayout,
  type StoredLayout,
  type LayoutMeasures,
  toStoredLayout,
  type FlowDocument,
  type FlowLayout,
} from "../../state/flowDocument";
import type { FlowLayoutStore } from "../../state/flowLayouts";

/** What a container's view measured of itself (`measureContainer`). */
export interface ContainerMeasure {
  headWidth: number;
  memberTop: number;
}

/** The sizes the canvas measured: card heights, and the containers' needs. */
export interface MeasuredSizes {
  cards: Readonly<Record<string, number>>;
  containers: Readonly<Record<string, ContainerMeasure>>;
}

const NO_SIZES: MeasuredSizes = { cards: {}, containers: {} };
const NO_OMITTED: ReadonlySet<string> = new Set<string>();

/**
 * DBR-FR-NIGZ / DBR-FR-BKWE: the layout a board commit stores. The board
 * stores the stack order and the collapse alone, and the stack order must be
 * the one of every item, the hidden ones included. So each item that the
 * full layout places takes its position from it, and only the collapse comes
 * from `next`. A hidden item (`omitted`) is not walked in `next`, so it takes
 * its whole entry from the full layout, its collapse included. An item that
 * the full layout does not hold (an item just added) keeps its position in
 * `next`.
 */
export function boardCommitLayout(
  full: FlowLayout,
  next: FlowLayout,
  omitted: ReadonlySet<string> = NO_OMITTED,
): FlowLayout {
  const out: Record<string, FlowLayout[string]> = {};
  for (const [id, entry] of Object.entries(next)) {
    const placed = full[id];
    if (!placed) out[id] = entry;
    else if (omitted.has(id)) out[id] = placed;
    else out[id] = { ...entry, position: placed.position };
  }
  return out;
}

/** The layout the canvas draws, and the one way to change it. */
export interface FlowLayoutView {
  /** The layout of every item, or null until the stored layout is read. */
  layout: FlowLayout | null;
  /** The containers whose layout keeps them collapsed (FLO-FR-ZASL). */
  collapsed: ReadonlySet<string>;
  /**
   * FLO-FR-21 / FLO-FR-TDHR: keep `next` and store it once for the scope. A
   * layout change marks nothing dirty and schedules no write of the plan.
   */
  commit: (next: FlowLayout) => void;
  /** MDL-FR-QAJN: store the empty layout; the plan is laid out again. */
  reset: () => void;
  /** DBR-FR-HTCE: the layout of the scope as the machine stores it, or null until read. */
  stored: StoredLayout | null;
  /** DBR-FR-KTJS: store `next` for the scope as it is. */
  commitStored: (next: StoredLayout) => void;
  /** FLO-FR-IAOQ / DBR-FR-ZGNB: the No milestone column of the board, or null. */
  boardNone: { x: number; y: number; width: number; height: number } | null;
}

/**
 * FLO-FR-PLUZ / FLO-FR-HBBF: the layout of `doc` in `scope`: the stored layout
 * of this machine, and an automatic place for every item it does not place.
 */
export function useFlowLayout(
  doc: FlowDocument | null,
  scope: string | null,
  layouts: FlowLayoutStore,
  /**
   * MDL-FR-ECPZ / FLO-FR-PLUZ: what the canvas measured of the items, so the
   * automatic layout fits the content its binding renders.
   */
  sizes: MeasuredSizes = NO_SIZES,
  /** DBR-FR-ERHW: lay the plan out as the board of the Delivery lens. */
  board = false,
  /** DBR-FR-SKSO / FLO-FR-OHZB: the items the host hides on the board. */
  omitted: ReadonlySet<string> = NO_OMITTED,
): FlowLayoutView {
  useSyncExternalStore(layouts.subscribe, layouts.getVersion);
  const stored = scope ? layouts.get(scope) : null;
  const { cards, containers } = sizes;
  // DBR-FR-LNZI: on the board, `cards` holds the measured height of each
  // card, group face, and column header.
  const fullBoard = useMemo(
    () => (board && doc && stored ? resolveBoardLayout(doc, stored, { heights: cards }) : null),
    [board, doc, stored, cards],
  );
  // DBR-FR-SKSO: the board as it shows, with the hidden items out of their stacks.
  const boardLayout = useMemo(
    () =>
      fullBoard && doc && stored && omitted.size > 0
        ? resolveBoardLayout(doc, stored, { heights: cards, omitted })
        : fullBoard,
    [fullBoard, doc, stored, cards, omitted],
  );
  const layout = useMemo(() => {
    if (!doc || !stored) return null;
    if (boardLayout) return boardLayout.layout;
    const measures: LayoutMeasures = {
      cardHeight: (id) => cards[id],
      headWidth: (id) => containers[id]?.headWidth,
      memberTop: (id) => containers[id]?.memberTop,
    };
    return resolveLayout(doc, stored, measures);
  }, [doc, stored, cards, containers, boardLayout]);
  const collapsed = useMemo(
    () =>
      new Set(
        Object.entries(layout ?? {})
          .filter(([, entry]) => entry.collapsed)
          .map(([id]) => id),
      ),
    [layout],
  );
  const commit = useCallback(
    (next: FlowLayout) => {
      if (!scope || next === layout) return;
      // DBR-FR-BKWE: the board stores the stack order and the collapse alone.
      // DBR-FR-NIGZ: a hidden item has no slot in `next`, so the stack order
      // comes from the full layout.
      const full = fullBoard && fullBoard !== boardLayout ? boardCommitLayout(fullBoard.layout, next, omitted) : next;
      void layouts.save(scope, board && doc ? toBoardStoredLayout(doc, full) : toStoredLayout(next));
    },
    [scope, layout, layouts, board, doc, fullBoard, boardLayout, omitted],
  );
  const commitStored = useCallback(
    (next: StoredLayout) => {
      if (scope) void layouts.save(scope, next);
    },
    [scope, layouts],
  );
  const reset = useCallback(() => {
    if (scope) void layouts.reset(scope);
  }, [scope, layouts]);
  return { layout, collapsed, commit, reset, stored, commitStored, boardNone: boardLayout?.none ?? null };
}
