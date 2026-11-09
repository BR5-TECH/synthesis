/**
 * The "waits for" graph of FGV-FR-PMTY, the local copy of the backend's
 * `WaitGraph` (`src-tauri/src/flow_validation/model.rs`). The canvas refuses a
 * connection that closes a dependency cycle at once, with no backend call
 * (FLO-FR-GZCH, FLO NFR), so the two must agree on every plan, and they read
 * the same `blocks` edges of an invalid one.
 *
 * Each item is two vertices: its **waiter**, which waits for what blocks the
 * item or any of its ancestors, and its **completion**, which the item reaches
 * when its own waits are over and, for a container, when every member is
 * complete. A `blocks` edge `F → T` is the arc `waiter(T) → completion(F)`.
 * Every item's completion waits for its own waiter, so an empty container
 * counts as a leaf. A cycle in this graph is a leaf that waits, directly or
 * through other leaves, for itself.
 */

import { inLineage } from "./containment";
import { isContainerKind, type FlowDocument, type FlowItemKind } from "./types";

/** A `blocks` edge as the graph reads it. */
export interface BlocksArc {
  id: string;
  from: string;
  to: string;
}

interface Arc {
  to: number;
  /** The `blocks` edge that made this arc, or null for a hierarchy arc. */
  edge: string | null;
}

interface Graph {
  index: Map<string, number>;
  arcs: Arc[][];
  blocks: BlocksArc[];
}

/**
 * Build the graph of `doc`, with `blocks` as its `blocks` edges. The caller
 * gives the edges so that a candidate edge can be tested before it exists.
 */
function build(doc: FlowDocument, blocks: readonly BlocksArc[]): Graph {
  // The first of two items that share an id wins, as in the backend.
  const index = new Map<string, number>();
  const kinds = new Map<string, FlowItemKind>();
  const order: string[] = [];
  for (const item of doc.items) {
    if (index.has(item.id)) continue;
    index.set(item.id, order.length);
    kinds.set(item.id, item.kind);
    order.push(item.id);
  }
  // A parent that names no item is left out; each item is placed once.
  const parent = new Map<string, string>();
  const children = new Map<string, string[]>();
  for (const item of doc.items) {
    const p = item.parentId;
    if (p === undefined || !index.has(p) || parent.has(item.id)) continue;
    parent.set(item.id, p);
    const list = children.get(p) ?? [];
    list.push(item.id);
    children.set(p, list);
  }

  const n = order.length;
  const arcs: Arc[][] = Array.from({ length: 2 * n }, () => []);
  order.forEach((id, i) => {
    const p = parent.get(id);
    if (p !== undefined) arcs[i].push({ to: index.get(p)!, edge: null });
    arcs[n + i].push({ to: i, edge: null });
    if (isContainerKind(kinds.get(id)!)) {
      for (const c of children.get(id) ?? []) {
        arcs[n + i].push({ to: n + index.get(c)!, edge: null });
      }
    }
  });
  const kept: BlocksArc[] = [];
  for (const b of blocks) {
    const f = index.get(b.from);
    const t = index.get(b.to);
    if (f === undefined || t === undefined) continue;
    arcs[t].push({ to: n + f, edge: b.id });
    kept.push(b);
  }
  return { index, arcs, blocks: kept };
}

/**
 * The `blocks` edges of `doc` that the backend's rules give to the graph
 * (`rules.rs::edge_rules`): not an edge to a missing item, not a self edge,
 * not the second or a later edge on the same two items, and not an edge
 * within a lineage. A valid plan has none of these, but the filter keeps the
 * two sides in agreement on an invalid one too.
 */
function blocksOf(doc: FlowDocument): BlocksArc[] {
  const ids = new Set(doc.items.map((i) => i.id));
  const pairs = new Set<string>();
  const out: BlocksArc[] = [];
  for (const e of doc.edges) {
    let sound = ids.has(e.from) && ids.has(e.to);
    if (e.from === e.to) continue;
    const pair = e.from < e.to ? `${e.from}\0${e.to}` : `${e.to}\0${e.from}`;
    if (pairs.has(pair)) sound = false;
    pairs.add(pair);
    if (sound && inLineage(doc, e.from, e.to)) sound = false;
    if (sound && e.type === "blocks") out.push({ id: e.id, from: e.from, to: e.to });
  }
  return out;
}

/** Kosaraju's algorithm, iterative. The component number of every vertex. */
function stronglyConnected(arcs: readonly Arc[][]): number[] {
  const n = arcs.length;
  const finished: number[] = [];
  const seen = new Array<boolean>(n).fill(false);
  for (let start = 0; start < n; start += 1) {
    if (seen[start]) continue;
    seen[start] = true;
    const stack: Array<[number, number]> = [[start, 0]];
    while (stack.length > 0) {
      const top = stack[stack.length - 1];
      const next = arcs[top[0]][top[1]];
      if (next) {
        top[1] += 1;
        if (!seen[next.to]) {
          seen[next.to] = true;
          stack.push([next.to, 0]);
        }
      } else {
        finished.push(top[0]);
        stack.pop();
      }
    }
  }
  const reverse: number[][] = Array.from({ length: n }, () => []);
  arcs.forEach((out, v) => out.forEach((a) => reverse[a.to].push(v)));
  const comp = new Array<number>(n).fill(-1);
  let count = 0;
  for (let k = finished.length - 1; k >= 0; k -= 1) {
    const start = finished[k];
    if (comp[start] !== -1) continue;
    comp[start] = count;
    const stack = [start];
    while (stack.length > 0) {
      const v = stack.pop()!;
      for (const w of reverse[v]) {
        if (comp[w] === -1) {
          comp[w] = count;
          stack.push(w);
        }
      }
    }
    count += 1;
  }
  return comp;
}

/**
 * FGV-FR-PMTY: the ids of the `blocks` edges that lie on a dependency cycle, in
 * document order. Empty for a plan with no cycle.
 */
export function dependencyCycleEdges(doc: FlowDocument): string[] {
  const g = build(doc, blocksOf(doc));
  const comp = stronglyConnected(g.arcs);
  const out = new Set<string>();
  g.arcs.forEach((list, v) => {
    for (const a of list) {
      if (a.edge !== null && comp[v] === comp[a.to]) out.add(a.edge);
    }
  });
  return doc.edges.filter((e) => out.has(e.id)).map((e) => e.id);
}

/**
 * The items of a cycle that a `blocks` edge `from → to` closes; null when the
 * edge closes none. The list starts at `from`, then gives each item it waits
 * for in turn (the containers the wait runs through included), and ends at
 * `to`, which the new edge makes wait for `from`.
 *
 * The edge is the arc `waiter(to) → completion(from)`, so it closes a cycle
 * when `completion(from)` already reaches `waiter(to)`. `skipEdge` names an
 * edge of `doc` to leave out, for an edge that changes its type or direction.
 */
export function cycleClosedBy(
  doc: FlowDocument,
  from: string,
  to: string,
  skipEdge?: string,
): string[] | null {
  const g = build(
    doc,
    blocksOf(doc).filter((b) => b.id !== skipEdge),
  );
  const f = g.index.get(from);
  const t = g.index.get(to);
  if (f === undefined || t === undefined) return null;
  if (from === to) return [from];
  const n = g.arcs.length / 2;
  const start = n + f;
  // Breadth first, so the cycle that is named is a short one.
  const via = new Map<number, { prev: number; edge: string | null }>();
  via.set(start, { prev: -1, edge: null });
  const queue = [start];
  let found = false;
  for (let k = 0; k < queue.length && !found; k += 1) {
    const v = queue[k];
    for (const a of g.arcs[v]) {
      if (via.has(a.to)) continue;
      via.set(a.to, { prev: v, edge: a.edge });
      if (a.to === t) {
        found = true;
        break;
      }
      queue.push(a.to);
    }
  }
  if (!found) return null;
  const edges: string[] = [];
  for (let v = t; v !== start; v = via.get(v)!.prev) {
    const e = via.get(v)!.edge;
    if (e !== null) edges.push(e);
  }
  edges.reverse();
  const byId = new Map(g.blocks.map((b) => [b.id, b]));
  const items: string[] = [];
  const add = (id: string) => {
    if (!items.includes(id)) items.push(id);
  };
  add(from);
  for (const id of edges) {
    const b = byId.get(id)!;
    add(b.to);
    add(b.from);
  }
  add(to);
  return items;
}

/**
 * FOA-FR-XOAR: whether `later` waits, directly or through other items, for
 * `earlier`.
 */
export function waitsFor(
  doc: FlowDocument,
  later: string,
  earlier: string,
): boolean {
  const g = build(doc, blocksOf(doc));
  const l = g.index.get(later);
  const e = g.index.get(earlier);
  if (l === undefined || e === undefined) return false;
  const target = g.arcs.length / 2 + e;
  const seen = new Set<number>();
  const stack = [l];
  while (stack.length > 0) {
    const v = stack.pop()!;
    if (v === target) return true;
    if (seen.has(v)) continue;
    seen.add(v);
    for (const a of g.arcs[v]) stack.push(a.to);
  }
  return false;
}
