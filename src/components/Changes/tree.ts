/**
 * The Changes panel's tree assembly and check logic (CHG-FR-08, CHG-FR-09,
 * CHG-FR-14, CHG-FR-27, CHG-FR-28, CHG-FR-50). Pure functions, no rendering.
 */
import { matchesLens, matchesText, type TypeLens } from "../../artifactTypes";
import type { ChangeEntry } from "../../types";
import type { CommitFile } from "../CommitMessageModal";

export interface ChangeNode {
  /** Stable key for expand/collapse and selection state (CHG-FR-17). */
  key: string;
  name: string;
  /** Project-relative path of the folder or file. */
  path: string;
  kind: "folder" | "file";
  /** Files only. */
  entry?: ChangeEntry;
  children: ChangeNode[];
}

/**
 * Build the nested folder tree for a set of already-filtered entries, with the
 * changed files as the only leaves (CHG-FR-08). Because the entries are filtered
 * first, a folder with no visible changed descendant simply never gets created —
 * which is exactly the rule CHG-FR-08 states.
 *
 * `keyPrefix` namespaces the node keys so the same path under **Revisioned** and
 * under **Unrevisioned** keeps independent expand state.
 */
export function buildChangeTree(
  entries: ChangeEntry[],
  keyPrefix: string,
): ChangeNode[] {
  const roots: ChangeNode[] = [];
  // Folder nodes by path, so repeated visits to the same folder reuse one node.
  const folders = new Map<string, ChangeNode>();

  const folderAt = (path: string): ChangeNode[] => {
    if (path === "") return roots;
    const existing = folders.get(path);
    if (existing) return existing.children;
    const slash = path.lastIndexOf("/");
    const parent = slash === -1 ? "" : path.slice(0, slash);
    const name = slash === -1 ? path : path.slice(slash + 1);
    const node: ChangeNode = {
      // Keys carry the node kind: a branch comparison can hold both a deleted
      // file `a` and a new file `a/b.md`, and two sibling nodes named `a` with
      // one key would collide in React and share expand state.
      key: `${keyPrefix}d:${path}`,
      name,
      path,
      kind: "folder",
      children: [],
    };
    folders.set(path, node);
    folderAt(parent).push(node);
    return node.children;
  };

  for (const entry of entries) {
    const slash = entry.path.lastIndexOf("/");
    const parent = slash === -1 ? "" : entry.path.slice(0, slash);
    folderAt(parent).push({
      key: `${keyPrefix}f:${entry.path}`,
      name: entry.name,
      path: entry.path,
      kind: "file",
      entry,
      children: [],
    });
  }

  // Folders first, then files, each alphabetical — matching the Library's order.
  const sort = (nodes: ChangeNode[]) => {
    nodes.sort((a, b) => {
      if (a.kind !== b.kind) return a.kind === "folder" ? -1 : 1;
      return a.name.localeCompare(b.name);
    });
    for (const n of nodes) if (n.kind === "folder") sort(n.children);
  };
  sort(roots);
  return roots;
}

/** Whether an entry survives the two AND-combined filters (CHG-FR-14). */
export function entryVisible(
  entry: ChangeEntry,
  lens: TypeLens,
  text: string,
): boolean {
  return matchesLens(lens, entry.artifactType) && matchesText(text, entry.name);
}

/**
 * Split a change set into the two top-level groups (CHG-FR-09): **Revisioned**,
 * holding every entry Git already tracks, and **Unrevisioned**, holding the
 * entries whose change status is untracked. The filters apply inside both groups
 * identically (CHG-FR-16).
 */
export function partitionEntries(
  entries: ChangeEntry[],
  lens: TypeLens,
  text: string,
): { revisioned: ChangeEntry[]; unrevisioned: ChangeEntry[] } {
  const visible = entries.filter((e) => entryVisible(e, lens, text));
  return {
    revisioned: visible.filter((e) => e.changeStatus !== "untracked"),
    unrevisioned: visible.filter((e) => e.changeStatus === "untracked"),
  };
}

/**
 * Every changed file at or below `node`, in tree order. Because the tree is
 * built from already-filtered entries, this is exactly the node's *currently
 * visible* descendants — which is what a folder's check cascades over and what
 * its tri-state is computed from (CHG-FR-28).
 */
export function visibleFilesUnder(node: ChangeNode): ChangeEntry[] {
  if (node.kind === "file") return node.entry ? [node.entry] : [];
  return node.children.flatMap(visibleFilesUnder);
}

/**
 * CHG-FR-50 / CHG-FR-52: how many changed files are currently visible beneath a
 * node, counted recursively over its whole subtree rather than over its direct
 * children. Because the tree is built from already-filtered entries, this is by
 * construction the count the active filters admit — it is the same number the
 * node reveals when it is expanded, and it re-computes with the filters rather
 * than on a reload.
 */
export function visibleFileCount(node: ChangeNode): number {
  // Counted rather than collected: every folder and group row asks for this on
  // every render, and `visibleFilesUnder` allocates an array per level.
  if (node.kind === "file") return node.entry ? 1 : 0;
  return node.children.reduce((sum, child) => sum + visibleFileCount(child), 0);
}

/** How a folder or group node's checkbox renders (CHG-FR-28). */
export type CheckState = "checked" | "unchecked" | "indeterminate";

/**
 * CHG-FR-28: a folder or group is checked when every visible changed file
 * beneath it is checked, unchecked when none is, and indeterminate when some
 * are. A folder with nothing visible beneath it cannot be rendered at all
 * (CHG-FR-08), so the empty case only arises defensively.
 */
export function folderCheckState(
  node: ChangeNode,
  checked: ReadonlySet<string>,
): CheckState {
  const files = visibleFilesUnder(node);
  if (files.length === 0) return "unchecked";
  const ticked = files.filter((e) => checked.has(e.path)).length;
  if (ticked === 0) return "unchecked";
  return ticked === files.length ? "checked" : "indeterminate";
}

/**
 * CHG-FR-27: the **commit set** is exactly the file rows that are both checked
 * and currently visible under the active filters. A row the filters hide
 * contributes nothing whether or not it was checked while visible, so the panel
 * never commits a path the author cannot see.
 */
export function commitSetOf(
  visible: ChangeEntry[],
  checked: ReadonlySet<string>,
): CommitFile[] {
  return visible
    .filter((e) => checked.has(e.path))
    .map((e) => ({ path: e.path, untracked: e.changeStatus === "untracked" }));
}
