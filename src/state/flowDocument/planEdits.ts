/**
 * MDL-FR-XJBV: the edits of `"apply github delivery edit (edit)"` that turn one
 * plan into the next. The canvas makes every change as a change of the
 * document (FLO-FR-26); a GitHub plan takes that change as these semantic
 * edits in place of a write (`GDP-github-delivery-plan.md`).
 *
 * A removal is not an edit here: in a GitHub plan the host asks how to remove
 * (MDL-FR-XQGD), and makes the `remove_item` edit itself.
 */

import type { GithubDeliveryEdit } from "../../types/delivery";
import { isColumnMilestone, milestoneOrder } from "./milestoneOrder";
import type { FlowDocument, FlowEdge, FlowItem } from "./types";

/** The same list of strings, in the same order. */
function sameList(a: readonly string[] | undefined, b: readonly string[] | undefined): boolean {
  const x = a ?? [];
  const y = b ?? [];
  return x.length === y.length && x.every((v, i) => v === y[i]);
}

/** An edge as GitHub knows it: its two ends and its type, not its id. */
const edgeKey = (e: FlowEdge) => `${e.type} ${e.from} ${e.to}`;

/**
 * The edits of one item that is in both plans. The overlay removals come
 * first, so a kind change to a container finds no draft link and no
 * footprint left on the item (FGV-FR-MZIA).
 */
function itemEdits(
  before: FlowItem,
  after: FlowItem,
  updatedAt: string,
): GithubDeliveryEdit[] {
  const out: GithubDeliveryEdit[] = [];
  const itemId = after.id;
  if (before.draftId !== undefined && after.draftId === undefined) {
    out.push({ kind: "unlink_draft", itemId });
  }
  if (!sameList(before.footprint, after.footprint)) {
    out.push({ kind: "set_footprint", itemId, footprint: [...(after.footprint ?? [])] });
  }
  if (before.kind !== after.kind) {
    out.push({ kind: "set_kind", itemId, itemKind: after.kind, updatedAt });
  }
  if (before.parentId !== after.parentId) {
    out.push({ kind: "move", itemId, containerId: after.parentId ?? null, updatedAt });
  }
  if (before.name !== after.name) {
    out.push({ kind: "rename", itemId, name: after.name, updatedAt });
  }
  if ((before.description ?? "") !== (after.description ?? "")) {
    out.push({
      kind: "set_description",
      itemId,
      description: after.description ?? "",
      updatedAt,
    });
  }
  if (before.due !== after.due) {
    out.push({ kind: "set_due", itemId, due: after.due ?? null, updatedAt });
  }
  if (!sameList(before.artifactIds, after.artifactIds)) {
    out.push({ kind: "set_references", itemId, artifactIds: [...(after.artifactIds ?? [])] });
  }
  if (after.draftId !== undefined && before.draftId !== after.draftId) {
    out.push({ kind: "link_draft", itemId, draftId: after.draftId });
  }
  return out;
}

/**
 * The edits that turn `before` into `after`, in the order they apply: new
 * items, the changes of each item in document order, the edges that go, the
 * edges that come, and the plan's references. `updatedAt` gives the tracker's
 * `updated_at` of an item (GDP-FR-GBDF); an edge carries the value of its
 * blocked end, the issue a blocked-by relation is written on (GDP-FR-YGXA).
 */
export function planEditsBetween(
  before: FlowDocument,
  after: FlowDocument,
  updatedAt: (itemId: string) => string,
): GithubDeliveryEdit[] {
  const out: GithubDeliveryEdit[] = [];
  const old = new Map(before.items.map((i) => [i.id, i]));
  for (const item of after.items) {
    const was = old.get(item.id);
    if (was) {
      out.push(...itemEdits(was, item, updatedAt(item.id)));
      continue;
    }
    out.push({
      kind: "create_item",
      itemKind: item.kind,
      name: item.name,
      containerId: item.parentId ?? null,
      ...(item.description !== undefined ? { description: item.description } : {}),
      ...(item.due !== undefined ? { due: item.due } : {}),
    });
  }
  const oldEdges = new Map(before.edges.map((e) => [edgeKey(e), e]));
  const newEdges = new Map(after.edges.map((e) => [edgeKey(e), e]));
  for (const [key, e] of oldEdges) {
    if (newEdges.has(key)) continue;
    out.push({ kind: "remove_edge", from: e.from, to: e.to, type: e.type, updatedAt: updatedAt(e.to) });
  }
  for (const [key, e] of newEdges) {
    if (oldEdges.has(key)) continue;
    out.push({ kind: "add_edge", from: e.from, to: e.to, type: e.type, updatedAt: updatedAt(e.to) });
  }
  if (!sameList(before.references, after.references)) {
    out.push({ kind: "set_plan_references", references: [...after.references] });
  }
  // FLO-FR-OGPF: a move of a milestone column changes the order of the
  // milestone items, and the overlay keeps the order it gives.
  if (milestonesMoved(before, after)) {
    out.push({ kind: "set_milestone_order", milestoneIds: milestoneOrder(after).map((m) => m.id) });
  }
  return out;
}

/** FLO-FR-OGPF: the same milestone columns stand in another order of the plan. */
function milestonesMoved(before: FlowDocument, after: FlowDocument): boolean {
  const a = before.items.filter(isColumnMilestone).map((m) => m.id);
  const b = after.items.filter(isColumnMilestone).map((m) => m.id);
  return a.length === b.length && new Set([...a, ...b]).size === a.length && !sameList(a, b);
}

/**
 * MDL-FR-PUCM: the items an edit touches, which carry the pending mark: each
 * item whose fields changed, each end of an edge that came or went, and each
 * new item.
 */
export function touchedItems(before: FlowDocument, after: FlowDocument): string[] {
  const out = new Set<string>();
  const old = new Map(before.items.map((i) => [i.id, i]));
  for (const item of after.items) {
    const was = old.get(item.id);
    if (!was || itemEdits(was, item, "").length > 0) out.add(item.id);
  }
  if (milestonesMoved(before, after)) {
    for (const m of after.items.filter(isColumnMilestone)) out.add(m.id);
  }
  const oldKeys = new Set(before.edges.map(edgeKey));
  const newKeys = new Set(after.edges.map(edgeKey));
  for (const e of [...before.edges, ...after.edges]) {
    if (oldKeys.has(edgeKey(e)) !== newKeys.has(edgeKey(e))) {
      out.add(e.from);
      out.add(e.to);
    }
  }
  return [...out];
}
