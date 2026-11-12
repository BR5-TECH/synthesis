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
  /** OCW-FR-YTGJ: the run whose strip the author closed, or null. */
  closedRunId: string | null;
  /**
   * OCW-FR-AKVG: the archive folder name a read named with `change_archived`,
   * or null while the change is not archived.
   */
  archivedAs: string | null;
}

const EMPTY: ChangeViewState = {
  detail: null,
  selected: null,
  appliedSeq: 0,
  modes: {},
  closedRunId: null,
  archivedAs: null,
};

const views = new Map<string, ChangeViewState>();

/**
 * OCW-FR-GVZT: the shell follows which tabs state an archived change, so it
 * records no working interval for them. The version moves only when that set
 * changes, so a selection or a mode kept here renders nothing outside the tab.
 */
const listeners = new Set<() => void>();
let archivedVersion = 0;

function noteArchivedChanged(): void {
  archivedVersion += 1;
  for (const listener of [...listeners]) listener();
}

/** Follow the set of tabs that state an archived change. */
export function subscribeChangeViews(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** The version of the set of tabs that state an archived change. */
export function archivedChangeViewsVersion(): number {
  return archivedVersion;
}

/** OCW-FR-AKVG: the archive folder name of the change of a tab, or null. */
export function archivedAsOf(tabId: string): string | null {
  return views.get(tabId)?.archivedAs ?? null;
}

/** The state a tab kept, or the empty state. */
export function changeViewOf(tabId: string): ChangeViewState {
  return views.get(tabId) ?? EMPTY;
}

/** Keep part of a tab's state. */
export function keepChangeView(tabId: string, patch: Partial<ChangeViewState>): void {
  const before = archivedAsOf(tabId);
  views.set(tabId, { ...changeViewOf(tabId), ...patch });
  if (archivedAsOf(tabId) !== before) noteArchivedChanged();
}

/** Drop the state of every tab that is not open. */
export function pruneChangeViews(openTabIds: ReadonlySet<string>): void {
  let archivedGone = false;
  for (const id of [...views.keys()]) {
    if (openTabIds.has(id)) continue;
    if (views.get(id)?.archivedAs != null) archivedGone = true;
    views.delete(id);
  }
  if (archivedGone) noteArchivedChanged();
}

/** Tests only: start from an empty store. */
export function resetChangeViewsForTest(): void {
  views.clear();
  listeners.clear();
  archivedVersion = 0;
}
