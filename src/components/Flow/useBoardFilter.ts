/**
 * FLO-FR-OHZB / MDL-FR-WDMP / MDL-FR-ZKXX: the items of the board that the
 * host's status filter hides. Only the Delivery lens gives a filter
 * (MDL-FR-XHNZ), so a Flow tab hides nothing.
 */
import { useMemo, useRef } from "react";

import type { FlowDocument } from "../../state/flowDocument";
import { NO_OMITTED, filterIsEmpty, omittedItems, type BoardStatusFilter } from "./boardFilter";
import { boardDot, NO_STATUS_NAMES, type BoardStatusNames } from "./boardStatus";
import type { FlowSource } from "./useFlowSource";

export interface BoardFilterContext {
  board: boolean;
  doc: FlowDocument | null;
  source: FlowSource;
  filter: BoardStatusFilter | undefined;
  statusNames: BoardStatusNames | undefined;
  /** MDL-FR-ZKXX: the selection, which always shows. */
  selection: readonly string[];
}

/** The same set as `next` when it holds the same ids, so the layout does not change. */
function stable(prev: ReadonlySet<string>, next: ReadonlySet<string>): ReadonlySet<string> {
  if (prev.size !== next.size) return next;
  for (const id of next) if (!prev.has(id)) return next;
  return prev;
}

/** The ids of the items the board does not show. */
export function useBoardFilter(ctx: BoardFilterContext): ReadonlySet<string> {
  const { board, doc, source, filter, selection } = ctx;
  const names = ctx.statusNames ?? NO_STATUS_NAMES;
  const states = source.delivery.states;
  const { tracker, repository, proposal } = source;
  const selectionKey = selection.join("\n");
  const kept = useRef<ReadonlySet<string>>(NO_OMITTED);
  const omitted = useMemo(() => {
    if (!board || !doc || !filter || filterIsEmpty(filter)) return NO_OMITTED;
    const keep = new Set<string>(selectionKey === "" ? [] : selectionKey.split("\n"));
    for (const id of proposal.ghosts.keys()) keep.add(id);
    for (const id of proposal.marks.keys()) keep.add(id);
    return omittedItems({
      doc,
      filter,
      // DBR-FR-QZPA: the dot the card shows (as `factsOf` of the canvas gives it).
      dotOf: (id) =>
        boardDot(
          { tracker: repository ? tracker.get(id) : undefined, state: states?.get(id)?.state },
          names,
        ),
      blockedOf: (id) => states?.get(id)?.blocked === true,
      keep,
    });
  }, [board, doc, filter, selectionKey, proposal, repository, tracker, states, names]);
  kept.current = stable(kept.current, omitted);
  return kept.current;
}
