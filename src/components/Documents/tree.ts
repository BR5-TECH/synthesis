/**
 * The document tree of the Documents panel
 * (`../../../specifications/ui/DPN-documents-panel.md` DPN-FR-NHDZ, DPN-FR-TELU).
 *
 * Pure functions over the entries the collection reported. The tree holds every
 * folder on the way to a document, each filesystem path once. A chain of folders
 * that holds one folder and no document is one row labelled with the joined
 * names. Folders sort before documents, and each group sorts by name ignoring
 * case. A path that the collection did not report is never built.
 */
import type { DocumentEntry } from "../../types";

export interface FolderNode {
  kind: "folder";
  /** The full path of the deepest folder of the row. */
  key: string;
  /** The names of the row's folders, joined with a slash. */
  label: string;
  children: TreeNode[];
}

export interface DocumentNode {
  kind: "document";
  /** The document id. */
  key: string;
  label: string;
  entry: DocumentEntry;
}

export type TreeNode = FolderNode | DocumentNode;

/** One visible row of the tree, in the order it is drawn. */
export interface TreeRow {
  node: TreeNode;
  depth: number;
  /** Whether the row is a folder drawn open. Always false for a document. */
  expanded: boolean;
  /** The key of the folder row that holds this row, or null at the top. */
  parentKey: string | null;
}

interface RawFolder {
  name: string;
  key: string;
  folders: Map<string, RawFolder>;
  documents: DocumentEntry[];
}

function leadingSlashes(path: string): string {
  const match = /^\/+/.exec(path);
  return match ? match[0] : "";
}

/** The sort order of a name: case is ignored, and an exact tie falls back to bytes. */
export function compareNames(a: string, b: string): number {
  const x = a.toLowerCase();
  const y = b.toLowerCase();
  if (x < y) return -1;
  if (x > y) return 1;
  return a < b ? -1 : a > b ? 1 : 0;
}

function compact(folder: RawFolder): FolderNode {
  let label = folder.name;
  let key = folder.key;
  let current = folder;
  // A chain that holds one folder and no document is one row.
  while (current.documents.length === 0 && current.folders.size === 1) {
    const [only] = [...current.folders.values()];
    label = `${label}/${only.name}`;
    key = only.key;
    current = only;
  }
  const children: TreeNode[] = [
    ...[...current.folders.values()].map(compact),
    ...current.documents.map(
      (entry): DocumentNode => ({
        kind: "document",
        key: entry.id,
        label: entry.name,
        entry,
      }),
    ),
  ];
  return { kind: "folder", key, label, children: sortNodes(children) };
}

function sortNodes(nodes: TreeNode[]): TreeNode[] {
  return [...nodes].sort((a, b) => {
    if (a.kind !== b.kind) return a.kind === "folder" ? -1 : 1;
    return compareNames(a.label, b.label) || compareNames(a.key, b.key);
  });
}

/** Build the tree of every document of the collection. */
export function buildTree(documents: DocumentEntry[]): TreeNode[] {
  const root: RawFolder = {
    name: "",
    key: "",
    folders: new Map(),
    documents: [],
  };
  for (const entry of documents) {
    const prefix = leadingSlashes(entry.path);
    const segments = entry.path.split("/").filter((s) => s !== "");
    // The last segment is the file; the rest are the folders above it.
    const folders = segments.slice(0, -1);
    let current = root;
    let path = prefix;
    folders.forEach((segment, index) => {
      path = index === 0 ? `${prefix}${segment}` : `${path}/${segment}`;
      let next = current.folders.get(segment);
      if (!next) {
        next = { name: segment, key: path, folders: new Map(), documents: [] };
        current.folders.set(segment, next);
      }
      current = next;
    });
    current.documents.push(entry);
  }
  const top: TreeNode[] = [
    ...[...root.folders.values()].map(compact),
    ...root.documents.map(
      (entry): DocumentNode => ({
        kind: "document",
        key: entry.id,
        label: entry.name,
        entry,
      }),
    ),
  ];
  return sortNodes(top);
}

export interface FilteredTree {
  nodes: TreeNode[];
  /** The folders that hold a match, which render open while the filter has text. */
  open: Set<string>;
}

const matches = (label: string, needle: string): boolean =>
  label.toLowerCase().includes(needle);

/**
 * DPN-FR-TELU: narrow the tree to the rows whose name contains the text. A
 * folder stays when its own name matches, and then it shows everything below it.
 * A folder also stays when a row below it matches.
 */
export function filterTree(nodes: TreeNode[], text: string): FilteredTree {
  const needle = text.trim().toLowerCase();
  if (needle === "") return { nodes, open: new Set() };
  const open = new Set<string>();
  const walk = (list: TreeNode[]): TreeNode[] => {
    const kept: TreeNode[] = [];
    for (const node of list) {
      if (node.kind === "document") {
        if (matches(node.label, needle)) kept.push(node);
        continue;
      }
      if (matches(node.label, needle)) {
        open.add(node.key);
        kept.push(node);
        continue;
      }
      const children = walk(node.children);
      if (children.length > 0) {
        open.add(node.key);
        kept.push({ ...node, children });
      }
    }
    return kept;
  };
  return { nodes: walk(nodes), open };
}

/** The rows a tree draws, which are the rows of its expanded folders only. */
export function visibleRows(
  nodes: TreeNode[],
  isExpanded: (folderKey: string) => boolean,
): TreeRow[] {
  const rows: TreeRow[] = [];
  const walk = (list: TreeNode[], depth: number, parentKey: string | null) => {
    for (const node of list) {
      if (node.kind === "document") {
        rows.push({ node, depth, expanded: false, parentKey });
        continue;
      }
      const expanded = isExpanded(node.key);
      rows.push({ node, depth, expanded, parentKey });
      if (expanded) walk(node.children, depth + 1, node.key);
    }
  };
  walk(nodes, 0, null);
  return rows;
}

/** The ids of every document of a snapshot, for the report of a removal. */
export function documentIds(documents: DocumentEntry[]): Set<string> {
  return new Set(documents.map((d) => d.id));
}
