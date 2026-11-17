/**
 * DBR-FR-AGIN .. DBR-FR-PMGS: what the open plan proposals show on the board
 * of a GitHub plan: a mark on each item a row changes or withdraws, a ghost
 * card for each issue a row adds, and the selected ring on the card of the row
 * the review sheet selects.
 */
import { useMemo, useSyncExternalStore } from "react";

import type { FlowDocument, FlowItem } from "../../state/flowDocument";
import type { TrackerFacts } from "../../types";
import { ghostCards, mapMarks } from "../../state/mapPlan/derive";
import type { FlowPlanChange, FlowProposalView } from "../Flow/binding";
import { useMapPlan } from "../MapPlanProposal";

/**
 * The plan with a ghost item in the column of each added row, and
 * the view the board reads. A ghost of an unknown milestone goes to the
 * **No milestone** column.
 */
/**
 * The plan item id of `issue-N` or `milestone-N`, read from the tracker facts,
 * so the board does not depend on how the plan names its items.
 */
const NO_TRACKER: readonly TrackerFacts[] = [];

export function itemIdResolver(tracker: readonly TrackerFacts[]): (id: string) => string {
  const byRef = new Map(tracker.map((t) => [`${t.ref.kind}-${t.ref.number}`, t.itemId]));
  return (id) => byRef.get(id) ?? id;
}

export function useProposalBoard(
  document: FlowDocument | null,
  tracker: readonly TrackerFacts[] = NO_TRACKER,
): {
  document: FlowDocument | null;
  proposal: FlowProposalView;
} {
  const { store, host } = useMapPlan();
  const version = useSyncExternalStore(store.subscribe, store.getVersion);
  return useMemo(() => {
    const proposals = store.boardProposals();
    const busy = store.snapshot().busy;
    const resolve = itemIdResolver(tracker);
    const ghosts = ghostCards(proposals, busy);
    const reviewed = store.reviewedItemId();
    const view: FlowProposalView = {
      marks: new Map([...mapMarks(proposals, busy)].map(([id, mark]) => [resolve(id), mark])),
      ghosts: new Map(),
      reviewed: reviewed === null ? null : resolve(reviewed),
    };
    if (!document || ghosts.length === 0) return { document, proposal: view };
    const known = new Set(document.items.map((i) => i.id));
    const ghostItems: FlowItem[] = [];
    const ghostMap = new Map<string, { kindKnown: boolean }>();
    for (const ghost of ghosts) {
      if (known.has(ghost.id)) continue;
      const milestone = ghost.milestoneId === null ? null : resolve(ghost.milestoneId);
      const kind = host?.kindOfType(ghost.type) ?? null;
      ghostMap.set(ghost.id, { kindKnown: kind !== null });
      ghostItems.push({
        id: ghost.id,
        kind: kind ?? "task",
        name: ghost.title,
        ...(milestone !== null && known.has(milestone) ? { parentId: milestone } : {}),
      });
    }
    return {
      document: { ...document, items: [...ghostItems, ...document.items] },
      proposal: { ...view, ghosts: ghostMap },
    };
    // `version` stands for every change of the review store.
  }, [document, tracker, store, host, version]);
}

/**
 * DBR-FR-WKNI: an edit of the board without the ghost cards. A ghost card is
 * not part of the plan, so no edit names it and the plan the host shows next
 * holds none.
 */
export function withoutGhosts(
  change: FlowPlanChange,
  ghosts: ReadonlyMap<string, unknown>,
): FlowPlanChange {
  if (ghosts.size === 0) return change;
  const names = (edit: object) =>
    Object.values(edit).some((value) => typeof value === "string" && ghosts.has(value));
  return {
    edits: change.edits.filter((edit) => !names(edit)),
    next: { ...change.next, items: change.next.items.filter((i) => !ghosts.has(i.id)) },
    itemIds: change.itemIds.filter((id) => !ghosts.has(id)),
  };
}
