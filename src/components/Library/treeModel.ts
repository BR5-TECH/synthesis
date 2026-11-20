/**
 * The rows the Project panel shows, computed from the folders it has loaded
 * (LIB-FR-EMOO, LIB-FR-09, LIB-FR-JZNT, LIB-FR-PPAT, LIB-FR-BXMZ).
 *
 * Pure: it reads the loaded folders, the expanded set, and the lens, and gives
 * back one flat list of rows in tree order. The panel renders only the rows in
 * view, so this list is the whole cost of one render that the size of the
 * project can reach.
 */
import { matchesLens, matchesText, type TypeLens as TypeFilter } from "../../artifactTypes";
import type { FilterResult, TreeLens, TreeNode } from "../../types";

/** One rendered row of the tree. */
export type TreeRowModel =
  | { kind: "node"; node: TreeNode; depth: number; open: boolean }
  | { kind: "loading"; key: string; depth: number }
  | { kind: "truncated" };

/** LIB-FR-04: the panel's lens in the spelling the backend reads. */
export function lensOf(typeFilter: TypeFilter): TreeLens {
  if (typeFilter === "artifacts") return "all_artifacts";
  if (typeFilter === "files") return "all_files";
  return typeFilter;
}

/**
 * LIB-FR-09: whether a folder shows under the lens with an empty text filter.
 * The backend's flags decide it (ASC-FR-08, ASC-FR-OGRB), so the panel never
 * walks a subtree it has not loaded.
 */
export function folderVisible(node: TreeNode, typeFilter: TypeFilter): boolean {
  if (typeFilter === "files") return true;
  if (typeFilter === "artifacts") return node.hasArtifacts === true;
  return (node.subtreeTypes ?? []).includes(typeFilter);
}

/** LIB-FR-05: whether a node shows under the lens with an empty text filter. */
export function nodeVisible(node: TreeNode, typeFilter: TypeFilter): boolean {
  return node.nodeKind === "folder"
    ? folderVisible(node, typeFilter)
    : matchesLens(typeFilter, node.artifactType);
}

export interface RowsInput {
  /** The children of the project root, or null before the first load. */
  root: TreeNode[] | null;
  /** The loaded children of each folder, by folder path. */
  childrenOf: ReadonlyMap<string, TreeNode[]>;
  expanded: ReadonlySet<string>;
  typeFilter: TypeFilter;
  /**
   * LIB-FR-PPAT: the nested result of a text filter. When it is set, the rows
   * come from it alone, as the backend already applied the text and the lens.
   */
  filtered: { root: TreeNode[]; truncated: boolean } | null;
}

/**
 * The visible rows in tree order. An expanded folder whose children are not
 * loaded yet shows one loading row under it (LIB-FR-JZNT).
 */
export function visibleRows(input: RowsInput): TreeRowModel[] {
  const out: TreeRowModel[] = [];
  const { expanded, typeFilter, filtered, childrenOf } = input;
  const walk = (nodes: TreeNode[], depth: number, fromFilter: boolean) => {
    for (const node of nodes) {
      if (!fromFilter && !nodeVisible(node, typeFilter)) continue;
      const isFolder = node.nodeKind === "folder";
      const open = isFolder && expanded.has(node.id);
      out.push({ kind: "node", node, depth, open });
      if (!open) continue;
      if (fromFilter) {
        walk(node.children ?? [], depth + 1, true);
        continue;
      }
      const kids = childrenOf.get(node.id);
      if (kids) walk(kids, depth + 1, false);
      else if (node.hasChildren !== false) {
        out.push({ kind: "loading", key: `loading:${node.id}`, depth: depth + 1 });
      }
    }
  };
  if (filtered) {
    walk(filtered.root, 0, true);
    if (filtered.truncated) out.push({ kind: "truncated" });
  } else if (input.root) {
    walk(input.root, 0, false);
  }
  return out;
}

/**
 * The expanded folders whose children the rows need and the panel does not
 * hold: each expanded folder that is itself on a row (LIB-FR-EMOO). A folder
 * inside a collapsed one is not on a row, so it is not loaded.
 */
export function foldersToLoad(rows: TreeRowModel[]): string[] {
  const out: string[] = [];
  for (let i = 0; i < rows.length; i++) {
    const row = rows[i];
    if (row.kind !== "loading") continue;
    const owner = rows[i - 1];
    if (owner?.kind === "node") out.push(owner.node.id);
  }
  return out;
}

/** The ancestor folder paths of a node id, root first. */
export function ancestorsOf(id: string): string[] {
  const segs = id.split("/");
  const out: string[] = [];
  for (let i = 1; i < segs.length; i++) out.push(segs.slice(0, i).join("/"));
  return out;
}

/** The parent folder path of a node id; `""` for a top-level node. */
export function parentOf(id: string): string {
  const at = id.lastIndexOf("/");
  return at < 0 ? "" : id.slice(0, at);
}

/** LIB-FR-PPAT: whether a nested filter result holds the node `id`. */
export function resultHolds(nodes: TreeNode[], id: string): boolean {
  for (const node of nodes) {
    if (node.id === id) return true;
    if (id.startsWith(`${node.id}/`) && resultHolds(node.children ?? [], id)) return true;
  }
  return false;
}

/** LIB-FR-18: the local filters a reveal must relax so the node shows. */
export interface FiltersToRelax {
  lens: boolean;
  text: boolean;
}

/**
 * LIB-FR-18: which filters hide `node`, and no more than those.
 *
 * With an empty text filter, the lens decides alone, from the backend's flags
 * (LIB-FR-09). With text, the backend shows a node that matches the text and
 * the lens on its own, and each ancestor folder of such a node (ASC-FR-SKRN).
 * The panel does not hold the subtree of a folder it has not opened, so a
 * folder shown only through a descendant is known from the current `result`.
 *
 * Dropping the lens is preferred where it suffices: **All files** admits every
 * node, so a name that matches the text shows under it. The text filter is
 * something the user typed and would have to type again.
 */
export function filtersToRelax(
  node: TreeNode,
  typeFilter: TypeFilter,
  text: string,
  result: FilterResult | null,
): FiltersToRelax {
  const none = { lens: false, text: false };
  if (text.trim() === "") {
    return nodeVisible(node, typeFilter) ? none : { lens: true, text: false };
  }
  const byText = matchesText(text, node.name);
  if (byText && matchesLens(typeFilter, node.artifactType)) return none;
  if (result && resultHolds(result.root, node.id)) return none;
  if (byText) return { lens: true, text: false };
  // The text goes. What is left is the lens with an empty text filter.
  return { lens: !nodeVisible(node, typeFilter), text: true };
}
