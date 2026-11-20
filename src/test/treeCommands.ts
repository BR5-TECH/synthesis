/**
 * Test-side answers to the tree reads of ASC-artifact-scanning.md, computed
 * from one nested `TreeNode` fixture, as the project index would give them
 * (ASC-FR-KVIE, ASC-FR-AGGG, ASC-FR-SKRN, ASC-FR-YNJR, ASC-FR-AVVY,
 * ASC-FR-QKXR, ASC-FR-OGRB, ASC-FR-YSWU).
 *
 * A test keeps its nested fixture, and its `invoke` mock asks
 * `treeCommand(cmd, args, tree)` first: a tree read gets its answer, and any
 * other command gets `undefined`, which leaves it to the mock.
 */
import type { ArtifactType, FilterResult, FolderChildren, PathSearch, TreeNode } from "../types";

/** The tree reads this helper answers. */
export const TREE_COMMANDS = new Set([
  "list_tree_children",
  "list_tree_path",
  "filter_project_tree",
  "list_project_artifacts",
  "search_project_paths",
  "existing_project_paths",
  "rescan_project_tree",
]);

const TYPE_ORDER: ArtifactType[] = [
  "skill",
  "agent",
  "prompt",
  "spec",
  "flow",
  "architecture",
  "instructions",
  "scenario",
  "scratchpad",
  "doc",
];

/** The types in a folder's subtree, its own assignment included. */
function subtreeTypes(node: TreeNode): ArtifactType[] {
  const found = new Set<ArtifactType>();
  const walk = (n: TreeNode) => {
    if (n.artifactType) found.add(n.artifactType);
    for (const c of n.children ?? []) walk(c);
  };
  walk(node);
  return TYPE_ORDER.filter((t) => found.has(t));
}

/**
 * The wire node of one fixture node: no `children`, and the folder flags
 * computed from the subtree as the index computes them (ASC-FR-08,
 * ASC-FR-OGRB, ASC-FR-YSWU), whatever flags the fixture states.
 */
export function wireNode(node: TreeNode): TreeNode {
  const { children, ...rest } = node;
  if (node.nodeKind !== "folder") return rest;
  const types = subtreeTypes(node);
  return {
    ...rest,
    hasArtifacts: types.length > 0,
    subtreeTypes: types,
    hasChildren: (children ?? []).length > 0,
  };
}

/** ASC-FR-KVIE: folders first, then files, each group by name in byte order. */
function byTreeOrder(a: TreeNode, b: TreeNode): number {
  if (a.nodeKind !== b.nodeKind) return a.nodeKind === "folder" ? -1 : 1;
  return a.name < b.name ? -1 : a.name > b.name ? 1 : 0;
}

/** A copy of the fixture with every folder's children in tree order. */
function ordered(node: TreeNode): TreeNode {
  if (!node.children) return node;
  return { ...node, children: node.children.map(ordered).sort(byTreeOrder) };
}

function find(tree: TreeNode, id: string): TreeNode | null {
  if (id === "" || id === tree.id) return tree;
  for (const c of tree.children ?? []) {
    const hit = find(c, id);
    if (hit) return hit;
  }
  return null;
}

function visit(tree: TreeNode, f: (n: TreeNode) => void) {
  for (const c of tree.children ?? []) {
    f(c);
    if (c.nodeKind === "folder") visit(c, f);
  }
}

function lensAdmitsFile(lens: string, t: ArtifactType | undefined): boolean {
  if (lens === "all_files") return true;
  if (lens === "all_artifacts") return t != null;
  return t === lens;
}

function filter(tree: TreeNode, text: string, lens: string, limit: number): FilterResult {
  const needle = text.trim().toLowerCase();
  let matched = 0;
  let truncated = false;
  const go = (folder: TreeNode): TreeNode[] => {
    const out: TreeNode[] = [];
    for (const c of folder.children ?? []) {
      if (truncated) break;
      const hit = needle === "" || c.name.toLowerCase().includes(needle);
      if (c.nodeKind === "file") {
        if (hit && lensAdmitsFile(lens, c.artifactType)) {
          if (matched >= limit) {
            truncated = true;
            break;
          }
          matched += 1;
          out.push(wireNode(c));
        }
        continue;
      }
      const own = hit && (lens === "all_files" || (lens === "all_artifacts" ? c.artifactType != null : c.artifactType === lens));
      if (own) {
        if (matched >= limit) {
          truncated = true;
          break;
        }
        matched += 1;
      }
      const kids = go(c);
      if (own || kids.length > 0) out.push({ ...wireNode(c), children: kids });
    }
    return out;
  };
  return { root: go(tree), truncated };
}

/**
 * The answer of one tree read against `tree`, or `undefined` when `cmd` is not
 * a tree read. A folder the fixture does not hold rejects with
 * `path_not_in_tree`, as the command does.
 */
export function treeCommand(
  cmd: string,
  args: unknown,
  tree: TreeNode,
): Promise<unknown> | undefined {
  if (!TREE_COMMANDS.has(cmd)) return undefined;
  const a = (args ?? {}) as Record<string, unknown>;
  tree = ordered(tree);
  switch (cmd) {
    case "list_tree_children":
    case "rescan_project_tree": {
      // The command takes a path; a mock that drops the arguments must not
      // read as a request for the root.
      if (cmd === "list_tree_children" && typeof a.path !== "string") {
        return Promise.reject("list_tree_children: no path");
      }
      const path = cmd === "rescan_project_tree" ? "" : (a.path as string);
      // The command trims the slashes of its path, as the index does.
      const node = find(tree, path.replace(/^\/+|\/+$/g, ""));
      if (!node || node.nodeKind !== "folder") return Promise.reject("path_not_in_tree");
      return Promise.resolve((node.children ?? []).map(wireNode));
    }
    case "list_tree_path": {
      const id = String(a.path ?? "");
      const node = find(tree, id);
      if (!node) return Promise.resolve(null);
      const folders: string[] = [""];
      const segs = id.split("/").filter(Boolean);
      const depth = node.nodeKind === "folder" ? segs.length : segs.length - 1;
      for (let i = 1; i <= depth; i++) folders.push(segs.slice(0, i).join("/"));
      const out: FolderChildren[] = folders.map((folder) => ({
        folder,
        children: (find(tree, folder)?.children ?? []).map(wireNode),
      }));
      return Promise.resolve(out);
    }
    case "filter_project_tree":
      // The command has no default for any argument.
      if (typeof a.text !== "string" || typeof a.lens !== "string" || typeof a.limit !== "number") {
        return Promise.reject("filter_project_tree: missing argument");
      }
      return Promise.resolve(filter(tree, a.text, a.lens, a.limit));
    case "list_project_artifacts": {
      const out: TreeNode[] = [];
      visit(tree, (n) => {
        if (n.artifactType) out.push(wireNode(n));
      });
      return Promise.resolve(out);
    }
    case "search_project_paths": {
      const needle = String(a.text ?? "").trim().toLowerCase();
      if (typeof a.text !== "string" || typeof a.kind !== "string") {
        return Promise.reject("search_project_paths: missing argument");
      }
      const kind = a.kind;
      const limit = typeof a.limit === "number" ? a.limit : null;
      const paths: string[] = [];
      let truncated = false;
      visit(tree, (n) => {
        if (truncated) return;
        const kindOk = kind === "any" || n.nodeKind === kind;
        if (!kindOk || (needle && !n.path.toLowerCase().includes(needle))) return;
        if (limit !== null && paths.length >= limit) {
          truncated = true;
          return;
        }
        paths.push(n.path);
      });
      const result: PathSearch = { paths, truncated };
      return Promise.resolve(result);
    }
    case "existing_project_paths": {
      const asked = (a.paths as string[] | undefined) ?? [];
      return Promise.resolve(
        asked.filter((p) => {
          const bare = p.replace(/^\/+|\/+$/g, "");
          return bare !== "" && find(tree, bare) !== null;
        }),
      );
    }
  }
  return undefined;
}
