/**
 * FLO-FR-MCFR / FLO-FR-OGPF: the milestone order of a plan, and the change of
 * the document that a move of a milestone column makes.
 */

import type { FlowDocument, FlowItem } from "./types";

/** A milestone of the top level: a milestone column (FLO-FR-SQUS). */
export const isColumnMilestone = (item: FlowItem): boolean =>
  item.kind === "milestone" && item.parentId === undefined;

/**
 * FLO-FR-MCFR: the milestone order. It starts from the plan's order of its
 * milestone items. Dated milestones then take the places of the dated
 * milestones in that order, sorted by due date, and each undated milestone
 * keeps its place. Two milestones of one due date keep the plan's order.
 */
export function milestoneOrder(doc: FlowDocument): FlowItem[] {
  const milestones = doc.items.filter(isColumnMilestone);
  const dated = milestones
    .map((m, i) => ({ m, i }))
    .filter(({ m }) => m.due !== undefined)
    .sort((a, b) => (a.m.due! < b.m.due! ? -1 : a.m.due! > b.m.due! ? 1 : a.i - b.i))
    .map(({ m }) => m);
  let next = 0;
  return milestones.map((m) => (m.due !== undefined ? dated[next++] : m));
}

/**
 * FLO-FR-OGPF: the document whose milestone items stand in `ids` order, each
 * in a place a milestone item held before, so every other item keeps its
 * place. Gives `doc` itself where nothing moves or `ids` does not name each
 * milestone column once.
 */
export function orderMilestones(doc: FlowDocument, ids: readonly string[]): FlowDocument {
  const slots: number[] = [];
  doc.items.forEach((item, i) => {
    if (isColumnMilestone(item)) slots.push(i);
  });
  const byId = new Map(slots.map((i) => [doc.items[i].id, doc.items[i]]));
  if (ids.length !== slots.length || new Set(ids).size !== ids.length || !ids.every((id) => byId.has(id))) {
    return doc;
  }
  if (slots.every((slot, k) => doc.items[slot].id === ids[k])) return doc;
  const items = [...doc.items];
  slots.forEach((slot, k) => {
    items[slot] = byId.get(ids[k])!;
  });
  return { ...doc, items };
}

/**
 * FLO-FR-OGPF: move the milestone column `id` to `index` of the milestone
 * order. Gives `doc` itself where the order does not change.
 */
export function moveMilestone(doc: FlowDocument, id: string, index: number): FlowDocument {
  const order = milestoneOrder(doc).map((m) => m.id);
  const from = order.indexOf(id);
  if (from < 0) return doc;
  const to = Math.max(0, Math.min(order.length - 1, index));
  if (to === from) return doc;
  const next = [...order];
  next.splice(from, 1);
  next.splice(to, 0, id);
  const moved = orderMilestones(doc, next);
  // FLO-FR-MCFR: a dated milestone keeps its place by due date, so a move that
  // the order gives back changes nothing.
  const after = milestoneOrder(moved).map((m) => m.id);
  return after.every((m, k) => m === order[k]) ? doc : moved;
}
