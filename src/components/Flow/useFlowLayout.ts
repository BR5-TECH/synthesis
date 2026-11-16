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
): FlowLayoutView {
  useSyncExternalStore(layouts.subscribe, layouts.getVersion);
  const stored = scope ? layouts.get(scope) : null;
  const { cards, containers } = sizes;
  const boardLayout = useMemo(
    () => (board && doc && stored ? resolveBoardLayout(doc, stored) : null),
    [board, doc, stored],
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
      void layouts.save(scope, board && doc ? toBoardStoredLayout(doc, next) : toStoredLayout(next));
    },
    [scope, layout, layouts, board, doc],
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
