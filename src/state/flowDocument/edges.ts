/**
 * The edit operations on edges (FLO-FR-15 to FLO-FR-17, FLO-FR-CEVI,
 * FLO-FR-GZCH, FLO-FR-ZEPI, FLO-FR-20). The canvas answers each of them
 * locally and at once (FLO-FR-46, FLO NFR).
 */

import { findItem, inLineage } from "./containment";
import { accept, refuse, type FlowEditResult, type FlowRefusal } from "./result";
import { mintId } from "./shared";
import type { FlowDocument, FlowEdge, FlowEdgeType } from "./types";
import { cycleClosedBy } from "./waitGraph";

/** FLO-FR-17: the edge that joins the two items, in either direction. */
export function edgeBetween(
  doc: FlowDocument,
  a: string,
  b: string,
): FlowEdge | undefined {
  return doc.edges.find(
    (e) => (e.from === a && e.to === b) || (e.from === b && e.to === a),
  );
}

/**
 * FLO-FR-16 / FLO-FR-17 / FLO-FR-ZEPI / FLO-FR-GZCH: why a new `blocks` edge
 * `from → to` is refused, or null when it is not. The canvas uses this to
 * decide which items a drawn connection may land on (FLO-FR-33).
 */
export function connectRefusal(
  doc: FlowDocument,
  from: string,
  to: string,
): FlowRefusal | null {
  if (!findItem(doc, from) || !findItem(doc, to)) return { code: "unknown_item" };
  if (from === to) return { code: "self" };
  const existing = edgeBetween(doc, from, to);
  if (existing) return { code: "duplicate_edge", edgeIds: [existing.id] };
  if (inLineage(doc, from, to)) return { code: "edge_within_lineage" };
  const cycle = cycleClosedBy(doc, from, to);
  if (cycle) return { code: "dependency_cycle", itemIds: cycle };
  return null;
}

/**
 * FLO-FR-CEVI: a completed connection creates a `blocks` edge from the item
 * the drag started on. A refusal leaves the plan as it was.
 */
export function connect(
  doc: FlowDocument,
  from: string,
  to: string,
): FlowEditResult<{ edgeId: string }> {
  const refusal = connectRefusal(doc, from, to);
  if (refusal) return { ok: false, refusal };
  const id = mintId("e", new Set(doc.edges.map((e) => e.id)), doc.edges.length + 1);
  const edge: FlowEdge = { id, from, to, type: "blocks" };
  return { ok: true, doc: { ...doc, edges: [...doc.edges, edge] }, edgeId: id };
}

function replaceEdge(doc: FlowDocument, edge: FlowEdge): FlowDocument {
  return { ...doc, edges: doc.edges.map((e) => (e.id === edge.id ? edge : e)) };
}

/**
 * FLO-FR-CEVI / FLO-FR-GZCH: change an edge's type. A change to `blocks` that
 * closes a cycle is refused and names its items; the edge stays as it was. A
 * `relates` edge never forms a cycle.
 */
export function setEdgeType(
  doc: FlowDocument,
  edgeId: string,
  type: FlowEdgeType,
): FlowEditResult {
  const edge = doc.edges.find((e) => e.id === edgeId);
  if (!edge) return refuse("unknown_edge");
  if (edge.type === type) return accept(doc);
  if (type === "blocks") {
    const cycle = cycleClosedBy(doc, edge.from, edge.to, edgeId);
    if (cycle) return refuse("dependency_cycle", { itemIds: cycle });
  }
  return accept(replaceEdge(doc, { ...edge, type }));
}

/**
 * FLO-FR-CEVI / FLO-FR-GZCH: reverse a `blocks` edge. A reversal that closes a
 * cycle is refused and names its items; the edge stays as it was.
 */
export function reverseEdge(doc: FlowDocument, edgeId: string): FlowEditResult {
  const edge = doc.edges.find((e) => e.id === edgeId);
  if (!edge) return refuse("unknown_edge");
  if (edge.type !== "blocks") return refuse("not_blocks");
  const cycle = cycleClosedBy(doc, edge.to, edge.from, edgeId);
  if (cycle) return refuse("dependency_cycle", { itemIds: cycle });
  return accept(replaceEdge(doc, { ...edge, from: edge.to, to: edge.from }));
}

/** FLO-FR-20: remove one edge; both items and their other edges stay. */
export function removeEdge(doc: FlowDocument, edgeId: string): FlowDocument {
  if (!doc.edges.some((e) => e.id === edgeId)) return doc;
  return { ...doc, edges: doc.edges.filter((e) => e.id !== edgeId) };
}
