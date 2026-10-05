/**
 * Lookups over one specification index (`SMP-specification-map.md`
 * SMP-FR-FEJV): where each node sits, how deep, and what lies beneath it.
 *
 * Built once per index value. The index is immutable — every edit returns a new
 * one — so a tree built from it never goes stale.
 */
import {
  isSpecNode,
  type IndexNode,
  type MapRef,
  type PlannedDraft,
  type SpecNode,
  type SpecificationIndex,
} from "./types";

/** A spec node's path is relative to this folder of the project. */
export const SPEC_ROOT = "specifications/";

export type NodeRole = "root" | "branch" | "leafGroup";

export interface TreeIndex {
  index: SpecificationIndex;
  nodes: Map<string, IndexNode>;
  parent: Map<string, string | null>;
  depth: Map<string, number>;
  specs: Map<string, SpecNode>;
  specParent: Map<string, string>;
  /** Keyed by the project-relative path, `specifications/<path>`. */
  specByPath: Map<string, string>;
  planned: Map<string, PlannedDraft>;
  plannedParent: Map<string, string>;
  /** SMP-FR-FEJV: the depth of every leaf group; spec nodes sit one deeper. */
  leafDepth: number;
  /** Index node ids in pre-order, which is the order of the index. */
  order: string[];
}

export function buildTree(index: SpecificationIndex): TreeIndex {
  const tree: TreeIndex = {
    index,
    nodes: new Map(),
    parent: new Map(),
    depth: new Map(),
    specs: new Map(),
    specParent: new Map(),
    specByPath: new Map(),
    planned: new Map(),
    plannedParent: new Map(),
    leafDepth: Math.max(index.levels.length - 2, 0),
    order: [],
  };
  const walk = (node: IndexNode, parentId: string | null, depth: number) => {
    tree.nodes.set(node.id, node);
    tree.parent.set(node.id, parentId);
    tree.depth.set(node.id, depth);
    tree.order.push(node.id);
    for (const draft of node.planned ?? []) {
      tree.planned.set(draft.draftId, draft);
      tree.plannedParent.set(draft.draftId, node.id);
    }
    for (const child of node.children) {
      if (isSpecNode(child)) {
        tree.specs.set(child.code, child);
        tree.specParent.set(child.code, node.id);
        tree.specByPath.set(SPEC_ROOT + child.path, child.code);
      } else {
        walk(child, node.id, depth + 1);
      }
    }
  };
  for (const root of index.roots) walk(root, null, 0);
  return tree;
}

/** SMN-FR-WAGD: the top-level folder of a spec under `specifications/`. */
export const specFolder = (spec: SpecNode): string => spec.path.split("/")[0];

/** SMP-FR-FEJV: the role an index node has, from its depth. */
export function roleOf(tree: TreeIndex, id: string): NodeRole {
  const depth = tree.depth.get(id) ?? 0;
  if (depth === tree.leafDepth) return "leafGroup";
  return depth === 0 ? "root" : "branch";
}

export const indexChildren = (node: IndexNode): IndexNode[] =>
  node.children.filter((c): c is IndexNode => !isSpecNode(c));

export const specChildren = (node: IndexNode): SpecNode[] =>
  node.children.filter(isSpecNode);

/** Every spec node beneath an index node, in index order. */
export function specsUnder(tree: TreeIndex, id: string): SpecNode[] {
  const node = tree.nodes.get(id);
  if (!node) return [];
  const out: SpecNode[] = [];
  const walk = (n: IndexNode) => {
    for (const child of n.children) {
      if (isSpecNode(child)) out.push(child);
      else walk(child);
    }
  };
  walk(node);
  return out;
}

/** SMD-FR-ULTF: how many planned drafts sit on a node or beneath it. */
export function plannedUnder(tree: TreeIndex, id: string): number {
  const node = tree.nodes.get(id);
  if (!node) return 0;
  let count = 0;
  const walk = (n: IndexNode) => {
    count += n.planned?.length ?? 0;
    for (const child of indexChildren(n)) walk(child);
  };
  walk(node);
  return count;
}

/** The index node a ref is, or sits in. */
export function nodeOfRef(tree: TreeIndex, ref: MapRef): string | null {
  switch (ref.kind) {
    case "index":
      return tree.nodes.has(ref.id) ? ref.id : null;
    case "spec":
      return tree.specParent.get(ref.code) ?? null;
    case "planned":
      return tree.plannedParent.get(ref.draftId) ?? null;
  }
}

export function refExists(tree: TreeIndex, ref: MapRef): boolean {
  switch (ref.kind) {
    case "index":
      return tree.nodes.has(ref.id);
    case "spec":
      return tree.specs.has(ref.code);
    case "planned":
      return tree.planned.has(ref.draftId);
  }
}

/** The ancestors of an index node, root first, the node itself excluded. */
export function ancestorIds(tree: TreeIndex, id: string): string[] {
  const out: string[] = [];
  let current = tree.parent.get(id) ?? null;
  while (current !== null) {
    out.unshift(current);
    current = tree.parent.get(current) ?? null;
  }
  return out;
}

/**
 * The index node at `depth` that is, or contains, what `ref` names — or null
 * when the ref sits shallower than that depth.
 */
export function ancestorAtDepth(
  tree: TreeIndex,
  ref: MapRef,
  depth: number,
): string | null {
  let id = nodeOfRef(tree, ref);
  while (id !== null) {
    const d = tree.depth.get(id) ?? 0;
    if (d === depth) return id;
    if (d < depth) return null;
    id = tree.parent.get(id) ?? null;
  }
  return null;
}

/** Whether `ref` is the index node `ancestorId` or lies beneath it. */
export function isWithin(
  tree: TreeIndex,
  ref: MapRef,
  ancestorId: string,
): boolean {
  const depth = tree.depth.get(ancestorId);
  if (depth === undefined) return false;
  return ancestorAtDepth(tree, ref, depth) === ancestorId;
}
