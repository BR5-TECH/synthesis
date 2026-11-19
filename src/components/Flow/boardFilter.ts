/**
 * `MDL-map-delivery-lens.md` MDL-FR-WDMP / MDL-FR-ZKXX: which items of the
 * board the status filter of the Delivery lens hides. Pure, so each rule is
 * tested without a canvas.
 */
import { ancestorsOf, type FlowDocument } from "../../state/flowDocument";
import type { BoardDot } from "./boardStatus";

/** MDL-FR-MIMF: the filter of the board: the hidden statuses, and Blocked only. */
export interface BoardStatusFilter {
  hidden: readonly BoardDot[];
  blockedOnly: boolean;
}

/** The filter that hides nothing (DMS-FR-TXMH, the empty view). */
export const SHOW_ALL: BoardStatusFilter = { hidden: [], blockedOnly: false };

/** Whether `filter` hides nothing. */
export const filterIsEmpty = (filter: BoardStatusFilter | undefined): boolean =>
  !filter || (filter.hidden.length === 0 && !filter.blockedOnly);

export const NO_OMITTED: ReadonlySet<string> = new Set<string>();

export interface BoardFilterInput {
  doc: FlowDocument;
  filter: BoardStatusFilter;
  /** DBR-FR-QZPA: the status dot of an item, or null where it shows none. */
  dotOf: (id: string) => BoardDot | null;
  /** FLO-FR-IKMM: whether an item is blocked. */
  blockedOf: (id: string) => boolean;
  /**
   * MDL-FR-ZKXX: the items that always show: a ghost card, an item with a
   * proposal mark, each selected item, and the item of the open issue view.
   */
  keep: ReadonlySet<string>;
}

/**
 * MDL-FR-WDMP / MDL-FR-ZKXX: the ids of the items the filter hides.
 *
 * An item shows where its status is not hidden and, while Blocked only is on,
 * where it is blocked. A group shows where it passes or a descendant shows,
 * and each ancestor of a shown item shows. A milestone, an item with no
 * status dot, and an item of `keep` always show.
 */
export function omittedItems(input: BoardFilterInput): ReadonlySet<string> {
  const { doc, filter, dotOf, blockedOf, keep } = input;
  if (filterIsEmpty(filter)) return NO_OMITTED;
  const hidden = new Set<BoardDot>(filter.hidden);
  const passes = (id: string): boolean => {
    const dot = dotOf(id);
    if (dot === null) return true;
    if (hidden.has(dot)) return false;
    return !filter.blockedOnly || blockedOf(id);
  };
  const shown = new Set<string>();
  const show = (id: string) => {
    shown.add(id);
    for (const a of ancestorsOf(doc, id)) shown.add(a);
  };
  for (const item of doc.items) {
    if (item.kind === "milestone" || keep.has(item.id) || passes(item.id)) show(item.id);
  }
  const out = new Set<string>();
  for (const item of doc.items) if (!shown.has(item.id)) out.add(item.id);
  return out.size === 0 ? NO_OMITTED : out;
}
