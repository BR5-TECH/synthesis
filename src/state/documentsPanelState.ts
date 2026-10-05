/**
 * The Documents panel's session memory: the filter text and the expanded or
 * collapsed state of each folder (`../../specifications/ui/DPN-documents-panel.md`
 * DPN-FR-AREM).
 *
 * Held in a module-level store rather than in the panel for the reason the other
 * panels hold theirs above them: choosing another surface unmounts the panel
 * outright, and a change of the active worktree remounts the whole shell subtree.
 * Both must leave this state as it is. The state lives for the application
 * session, so a relaunch starts empty, and nothing here is written to disk.
 *
 * A folder that has never been toggled is not in the map, and a folder not in
 * the map starts expanded.
 *
 * While the filter holds text, the folders with a match render expanded. A
 * folder that the user closes in that time is closed for that filter text
 * only: the set of such folders is session memory too, and it starts empty
 * again when the filter text changes.
 */
import { useSyncExternalStore } from "react";

/** What the panel renders from and mutates. */
export interface DocumentsPanelState {
  filter: string;
  setFilter: (text: string) => void;
  /** Whether the folder with this key is expanded when no filter forces it. */
  isExpanded: (folderKey: string) => boolean;
  setExpanded: (folderKey: string, expanded: boolean) => void;
  /** Whether the user closed this folder while the current filter text shows. */
  isFilterCollapsed: (folderKey: string) => boolean;
  setFilterCollapsed: (folderKey: string, collapsed: boolean) => void;
}

let filterText = "";
const toggled = new Map<string, boolean>();
const filterCollapsed = new Set<string>();
const listeners = new Set<() => void>();
let snapshot: DocumentsPanelState | null = null;

function changed(): void {
  snapshot = null;
  for (const listener of listeners) listener();
}

function setFilter(text: string): void {
  if (text === filterText) return;
  filterText = text;
  filterCollapsed.clear();
  changed();
}

function setExpanded(folderKey: string, expanded: boolean): void {
  if (toggled.get(folderKey) === expanded) return;
  toggled.set(folderKey, expanded);
  changed();
}

function setFilterCollapsed(folderKey: string, collapsed: boolean): void {
  if (filterCollapsed.has(folderKey) === collapsed) return;
  if (collapsed) filterCollapsed.add(folderKey);
  else filterCollapsed.delete(folderKey);
  changed();
}

const isExpanded = (folderKey: string): boolean =>
  toggled.get(folderKey) ?? true;

const isFilterCollapsed = (folderKey: string): boolean =>
  filterCollapsed.has(folderKey);

function read(): DocumentsPanelState {
  snapshot ??= {
    filter: filterText,
    setFilter,
    isExpanded,
    setExpanded,
    isFilterCollapsed,
    setFilterCollapsed,
  };
  return snapshot;
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function useDocumentsPanelState(): DocumentsPanelState {
  return useSyncExternalStore(subscribe, read, read);
}

/** Forget everything, as a relaunch does. Tests only. */
export function resetDocumentsPanelStateForTest(): void {
  filterText = "";
  toggled.clear();
  filterCollapsed.clear();
  changed();
}
