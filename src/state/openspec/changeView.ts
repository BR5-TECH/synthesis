/**
 * What a change workspace tab keeps between two mounts
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-LABL,
 * OCW-FR-PFUQ).
 *
 * The viewport mounts the active tab alone, so a tab that becomes active again
 * mounts again. The last read that succeeded and the selected artifact stay
 * here, so the tab shows them while it reads again, and a read that fails
 * keeps them. The store is pruned against the tab strip, so nothing outlives
 * its tab.
 */
import type { ChangeDetail } from "../../types";

export interface ChangeViewState {
  /** The last read that succeeded, or null. */
  detail: ChangeDetail | null;
  /** The id of the selected artifact, or null. */
  selected: string | null;
  /** The `openSeq` of the last open whose selection the tab applied. */
  appliedSeq: number;
  /** OCW-FR-ABUR: the Rich or Source choice of each file, by its path. */
  modes: Readonly<Record<string, "rich" | "source">>;
}

const EMPTY: ChangeViewState = { detail: null, selected: null, appliedSeq: 0, modes: {} };

const views = new Map<string, ChangeViewState>();

/** The state a tab kept, or the empty state. */
export function changeViewOf(tabId: string): ChangeViewState {
  return views.get(tabId) ?? EMPTY;
}

/** Keep part of a tab's state. */
export function keepChangeView(tabId: string, patch: Partial<ChangeViewState>): void {
  views.set(tabId, { ...changeViewOf(tabId), ...patch });
}

/** Drop the state of every tab that is not open. */
export function pruneChangeViews(openTabIds: ReadonlySet<string>): void {
  for (const id of [...views.keys()]) {
    if (!openTabIds.has(id)) views.delete(id);
  }
}

/** Tests only: start from an empty store. */
export function resetChangeViewsForTest(): void {
  views.clear();
}
