/**
 * The folder tree a run's path lists render as
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-TXLW,
 * GRU-FR-HEQB).
 *
 * A pure function of the paths the backend reports. Which folders are open is
 * the surface's own state; this module only says which are open at first.
 */

export interface PathFolder {
  kind: "folder";
  /** Unique in one tree, and different from the key of a file at the same path. */
  key: string;
  /** The folder's name, or the whole chain of names a compacted row stands for. */
  label: string;
  /** The full path of the deepest folder the row stands for. */
  path: string;
  /** How many paths the folder holds at every depth. */
  count: number;
  children: PathNode[];
}

export interface PathFile {
  kind: "file";
  key: string;
  name: string;
  path: string;
}

export type PathNode = PathFolder | PathFile;

/** GRU-FR-HEQB: a list this long or shorter renders with every folder open. */
export const OPEN_ALL_LIMIT = 24;

/**
 * GRU-FR-TXLW: the paths as a folder tree. Folders come before files, each in
 * name order. A folder that holds only one folder and no file is one row with
 * the folder under it, so a deep and narrow chain costs one row and not many.
 */
export function buildPathTree(paths: readonly string[]): PathNode[] {
  const roots: PathNode[] = [];
  const folders = new Map<string, PathFolder>();
  const seen = new Set<string>();

  const childrenAt = (path: string): PathNode[] => {
    if (path === "") return roots;
    const existing = folders.get(path);
    if (existing) return existing.children;
    const slash = path.lastIndexOf("/");
    const node: PathFolder = {
      kind: "folder",
      // The kind is part of the key: a file `a` and a folder `a/` can both
      // be in one list, and two rows with one key share their state.
      key: `d:${path}`,
      label: slash === -1 ? path : path.slice(slash + 1),
      path,
      count: 0,
      children: [],
    };
    folders.set(path, node);
    childrenAt(slash === -1 ? "" : path.slice(0, slash)).push(node);
    return node.children;
  };

  for (const raw of paths) {
    // A trailing slash names a folder the ignore rules hide as a whole. The
    // row keeps the name the backend gave it.
    const trimmed = raw.endsWith("/") ? raw.slice(0, -1) : raw;
    if (trimmed === "" || seen.has(raw)) continue;
    seen.add(raw);
    const slash = trimmed.lastIndexOf("/");
    childrenAt(slash === -1 ? "" : trimmed.slice(0, slash)).push({
      kind: "file",
      key: `f:${raw}`,
      name: raw.slice(slash + 1),
      path: raw,
    });
  }

  return finish(roots);
}

/** Sorts, counts and compacts one level, and every level under it. */
function finish(nodes: PathNode[]): PathNode[] {
  const done = nodes.map((node) => (node.kind === "folder" ? compact(node) : node));
  done.sort((a, b) => {
    if (a.kind !== b.kind) return a.kind === "folder" ? -1 : 1;
    const left = a.kind === "folder" ? a.label : a.name;
    const right = b.kind === "folder" ? b.label : b.name;
    return left.localeCompare(right, "en", { numeric: true });
  });
  return done;
}

function compact(folder: PathFolder): PathFolder {
  let label = folder.label;
  let current = folder;
  while (current.children.length === 1 && current.children[0].kind === "folder") {
    current = current.children[0];
    label = `${label}/${current.label}`;
  }
  const children = finish(current.children);
  return {
    kind: "folder",
    key: current.key,
    label,
    path: current.path,
    count: pathTotal(children),
    children,
  };
}

/** GRU-FR-TXLW: how many distinct paths a tree holds. */
export function pathTotal(nodes: readonly PathNode[]): number {
  return nodes.reduce(
    (sum, node) => sum + (node.kind === "folder" ? node.count : 1),
    0,
  );
}

/**
 * GRU-FR-HEQB: the folders open when a list is first rendered. A short list
 * opens every folder. A longer one opens its root folders and keeps every
 * deeper folder closed, so the first view is a summary and not the whole list.
 */
export function defaultOpenFolders(
  nodes: readonly PathNode[],
  total: number,
): Set<string> {
  const open = new Set<string>();
  const walk = (level: readonly PathNode[], all: boolean) => {
    for (const node of level) {
      if (node.kind !== "folder") continue;
      open.add(node.key);
      if (all) walk(node.children, true);
    }
  };
  walk(nodes, total <= OPEN_ALL_LIMIT);
  return open;
}
