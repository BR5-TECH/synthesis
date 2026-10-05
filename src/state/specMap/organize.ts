/**
 * The edits the map session applies to its index
 * (`SMO-specification-map-organization.md`, `SMD-specification-map-drafts.md`).
 *
 * Every edit is pure: it returns a new index and never changes the one it was
 * given. An edit the tree rules refuse returns the same index object, so a
 * caller can tell a refusal by identity.
 *
 * The tree always keeps its levels. An index node stays at the depth of its
 * level, and spec nodes stay in leaf groups (SMO Intent).
 */
import { buildTree, indexChildren, type TreeIndex } from "./tree";
import {
  isSpecNode,
  type IndexNode,
  type MapEdit,
  type MapRef,
  type PlannedDraft,
  type SpecNode,
  type SpecificationIndex,
} from "./types";

type Child = IndexNode | SpecNode;

/** The index is plain data, so a JSON round trip is a full deep copy. */
export function cloneIndex(index: SpecificationIndex): SpecificationIndex {
  return JSON.parse(JSON.stringify(index)) as SpecificationIndex;
}

/** SMO-FR-UAKE: a new index node goes under a node above the leaf depth, or at the root. */
export function canCreateUnder(tree: TreeIndex, parentId: string | null): boolean {
  if (parentId === null) return true;
  const depth = tree.depth.get(parentId);
  return depth !== undefined && depth < tree.leafDepth;
}

/** SMO-FR-WGOF: whether `ref` may move to be a child of `parentId`. */
export function canMove(
  tree: TreeIndex,
  ref: MapRef,
  parentId: string | null,
): boolean {
  switch (ref.kind) {
    case "spec":
      return (
        tree.specs.has(ref.code) &&
        parentId !== null &&
        tree.depth.get(parentId) === tree.leafDepth
      );
    case "planned":
      return (
        tree.planned.has(ref.draftId) &&
        parentId !== null &&
        tree.nodes.has(parentId)
      );
    case "index": {
      const depth = tree.depth.get(ref.id);
      if (depth === undefined) return false;
      if (parentId === null) return depth === 0;
      const parentDepth = tree.depth.get(parentId);
      // A parent one level up can never lie inside the moved subtree, so the
      // depth rule alone also rules out every cycle.
      return parentDepth !== undefined && parentDepth === depth - 1;
    }
  }
}

/** SMO-FR-SLNC: every valid new parent of an index node, in index order. */
export function moveTargets(tree: TreeIndex, id: string): string[] {
  return tree.order.filter(
    (candidate) =>
      candidate !== tree.parent.get(id) &&
      canMove(tree, { kind: "index", id }, candidate),
  );
}

export interface DeleteOutcome {
  allowed: boolean;
  /** SMO-FR-OBRF: the sibling that takes the children, when there is one. */
  siblingId: string | null;
}

/** SMO-FR-OBRF / SMO-FR-GQTS: what a delete of `nodeId` does. */
export function deleteOutcome(tree: TreeIndex, nodeId: string): DeleteOutcome {
  const node = tree.nodes.get(nodeId);
  if (!node) return { allowed: false, siblingId: null };
  const parentId = tree.parent.get(nodeId) ?? null;
  const siblings =
    parentId === null
      ? tree.index.roots
      : indexChildren(tree.nodes.get(parentId)!);
  const position = siblings.findIndex((n) => n.id === nodeId);
  const sibling = siblings[position - 1] ?? siblings[position + 1] ?? null;
  if (sibling) return { allowed: true, siblingId: sibling.id };
  return { allowed: node.children.length === 0, siblingId: null };
}

/** An id no index node holds yet. */
export function newNodeId(tree: TreeIndex): string {
  let n = tree.nodes.size + 1;
  while (tree.nodes.has(`node-${n}`)) n++;
  return `node-${n}`;
}

function isValid(tree: TreeIndex, edit: MapEdit): boolean {
  switch (edit.kind) {
    case "create":
      return (
        canCreateUnder(tree, edit.parentId) &&
        !tree.nodes.has(edit.node.id) &&
        edit.node.label.trim() !== ""
      );
    case "edit":
      return tree.nodes.has(edit.nodeId) && edit.label.trim() !== "";
    case "move":
      return canMove(tree, edit.ref, edit.parentId);
    case "delete":
      return deleteOutcome(tree, edit.nodeId).allowed;
    case "attachDraft":
      return tree.nodes.has(edit.nodeId);
    case "renameDraft":
    case "detachDraft":
      return tree.planned.has(edit.draftId);
  }
}

function merged(
  a: PlannedDraft[] | undefined,
  b: PlannedDraft[] | undefined,
): PlannedDraft[] | undefined {
  const out = [...(a ?? []), ...(b ?? [])];
  return out.length > 0 ? out : undefined;
}

export function applyEdit(
  index: SpecificationIndex,
  edit: MapEdit,
): SpecificationIndex {
  const tree = buildTree(index);
  if (!isValid(tree, edit)) return index;
  const next = cloneIndex(index);
  const nt = buildTree(next);
  const listOf = (parentId: string | null): Child[] =>
    (parentId === null ? next.roots : nt.nodes.get(parentId)!.children) as Child[];

  const removeDraft = (draftId: string): PlannedDraft | null => {
    const holder = nt.plannedParent.get(draftId);
    if (!holder) return null;
    const node = nt.nodes.get(holder)!;
    const list = node.planned ?? [];
    const at = list.findIndex((d) => d.draftId === draftId);
    const [draft] = list.splice(at, 1);
    if (list.length === 0) delete node.planned;
    return draft;
  };

  switch (edit.kind) {
    case "create":
      listOf(edit.parentId).push({
        id: edit.node.id,
        label: edit.node.label.trim(),
        summary: edit.node.summary,
        children: [],
      });
      break;
    case "edit": {
      const node = nt.nodes.get(edit.nodeId)!;
      node.label = edit.label.trim();
      node.summary = edit.summary;
      break;
    }
    case "move": {
      const { ref } = edit;
      if (ref.kind === "planned") {
        const draft = removeDraft(ref.draftId)!;
        const target = nt.nodes.get(edit.parentId!)!;
        const list = target.planned ?? [];
        list.splice(clamp(edit.index, list.length), 0, draft);
        target.planned = list;
        break;
      }
      const from =
        ref.kind === "spec"
          ? listOf(nt.specParent.get(ref.code)!)
          : listOf(nt.parent.get(ref.id) ?? null);
      const at = from.findIndex((c) =>
        ref.kind === "spec"
          ? isSpecNode(c) && c.code === ref.code
          : !isSpecNode(c) && c.id === ref.id,
      );
      const [item] = from.splice(at, 1);
      // The index is a position in the target list with the item already out.
      const to = listOf(edit.parentId);
      to.splice(clamp(edit.index, to.length), 0, item);
      break;
    }
    case "delete": {
      const parentId = nt.parent.get(edit.nodeId) ?? null;
      const list = listOf(parentId) as IndexNode[];
      const at = list.findIndex((n) => n.id === edit.nodeId);
      const node = list[at];
      const previous = list[at - 1];
      const following = list[at + 1];
      if (previous) {
        previous.children = [...previous.children, ...node.children] as Child[] as
          | IndexNode[]
          | SpecNode[];
        previous.planned = merged(previous.planned, node.planned);
      } else if (following) {
        following.children = [...node.children, ...following.children] as Child[] as
          | IndexNode[]
          | SpecNode[];
        following.planned = merged(node.planned, following.planned);
      } else if (parentId !== null) {
        const parent = nt.nodes.get(parentId)!;
        parent.planned = merged(parent.planned, node.planned);
      }
      list.splice(at, 1);
      break;
    }
    case "attachDraft": {
      removeDraft(edit.draft.draftId);
      const node = nt.nodes.get(edit.nodeId)!;
      node.planned = [...(node.planned ?? []), { ...edit.draft }];
      break;
    }
    case "renameDraft": {
      const node = nt.nodes.get(nt.plannedParent.get(edit.draftId)!)!;
      node.planned = (node.planned ?? []).map((d) =>
        d.draftId === edit.draftId ? { ...d, name: edit.name } : d,
      );
      break;
    }
    case "detachDraft":
      removeDraft(edit.draftId);
      break;
  }
  return next;
}

function clamp(index: number, length: number): number {
  return Math.max(0, Math.min(index, length));
}
