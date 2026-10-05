/**
 * `FLO-flow.md`: the Flow document — its serialization, its deserialization,
 * and every graph rule the canvas answers with while the author draws.
 *
 * The document's *shape* is not owned here: `../core/FGV-flow-graph-validation.md`
 * defines it and is the authority on whether a given body is a Flow at all
 * (FGV-FR-01). This module serializes to that schema and deserializes from it,
 * and enforces the same rules locally so a refused connection answers
 * immediately and costs no round-trip (FLO NFR). The two describe one graph;
 * neither defers to the other.
 *
 * Kept free of React and of `api` so the rules are testable as the pure
 * functions they are. Every mutator returns a NEW document rather than editing
 * in place: the store hands the result to React, which compares identities to
 * decide what to re-render.
 */

import {
  absolutePosition,
  areSiblings,
  elements,
  findElement,
  parentOf,
  subtreeOf,
} from "./containment";
import {
  DEFAULT_LOOP_NAME,
  DEFAULT_LOOP_SIZE,
  DEFAULT_NODE_NAME,
  LOOP_HEADER_H,
  LOOP_PADDING,
  MEMBER_NODE_H,
  MEMBER_NODE_W,
  type FlowDocument,
  type FlowElementBase,
  type FlowEdge,
  type FlowNode,
  type FlowPosition,
  type FlowSize,
} from "./types";

export {
  DEFAULT_LOOP_NAME,
  DEFAULT_LOOP_SIZE,
  DEFAULT_NODE_NAME,
  emptyFlow,
  FLOW_VERSION,
  LOOP_HEADER_H,
  LOOP_PADDING,
  MEMBER_NODE_H,
  MEMBER_NODE_W,
} from "./types";
export type {
  FlowDocument,
  FlowEdge,
  FlowElement,
  FlowLoop,
  FlowNode,
  FlowPosition,
  FlowSize,
} from "./types";
export {
  absolutePosition,
  ancestorsOf,
  areSiblings,
  elements,
  findElement,
  innermostLoopAt,
  loopBounds,
  parentOf,
  subtreeOf,
} from "./containment";
export { parseFlowDocument } from "./parse";
export type { FlowParseResult } from "./parse";
export { serializeFlowDocument } from "./serialize";

// ---------------------------------------------------------------------------
// Graph mutation
// ---------------------------------------------------------------------------
/**
 * Mint an id with `prefix` that nothing in `taken` holds.
 *
 * Derived from what the document actually holds rather than from its size:
 * deleting one node and adding another would otherwise re-mint an id already in
 * use, colliding React keys and making an edge resolve to the wrong node.
 */
function mintId(prefix: string, taken: Set<string>, from: number): string {
  let n = from;
  while (taken.has(`${prefix}${n}`)) n += 1;
  return `${prefix}${n}`;
}

/** Every id in use, across both kinds of element (FLO-FR-06). */
function elementIdsOf(doc: FlowDocument): Set<string> {
  return new Set([...doc.loops.map((l) => l.id), ...doc.nodes.map((n) => n.id)]);
}

/**
 * FLO-FR-07 / FLO-FR-42: place a new, unconnected node with no artifact and no
 * prompt, in `parentId`'s coordinates when it lands inside a loop.
 */
export function addNode(
  doc: FlowDocument,
  position: FlowPosition,
  parentId?: string,
): FlowDocument {
  // Numbered within its own kind but checked against EVERY id: nodes and loops
  // share one space (FLO-FR-06), so `n3` is unavailable if a loop already holds
  // it, and a Flow still reads as n1, n2, l1, l2 rather than as one interleaved
  // sequence.
  const id = mintId("n", elementIdsOf(doc), doc.nodes.length + 1);
  return {
    ...doc,
    nodes: [
      ...doc.nodes,
      {
        id,
        name: DEFAULT_NODE_NAME,
        ...(parentId !== undefined ? { parentId } : {}),
        position,
      },
    ],
  };
}

/**
 * FLO-FR-07 / FLO-FR-37: place a new, empty loop — a default name, no artifact
 * reference, no pass bound, a default size, and nothing inside it.
 */
export function addLoop(
  doc: FlowDocument,
  position: FlowPosition,
  parentId?: string,
): FlowDocument {
  const id = mintId("l", elementIdsOf(doc), doc.loops.length + 1);
  return {
    ...doc,
    loops: [
      ...doc.loops,
      {
        id,
        name: DEFAULT_LOOP_NAME,
        ...(parentId !== undefined ? { parentId } : {}),
        position,
        size: { ...DEFAULT_LOOP_SIZE },
      },
    ],
  };
}

/** FLO-FR-08: remove a node together with every edge incident to it. */
export function removeNode(doc: FlowDocument, nodeId: string): FlowDocument {
  return {
    ...doc,
    nodes: doc.nodes.filter((n) => n.id !== nodeId),
    edges: doc.edges.filter((e) => e.from !== nodeId && e.to !== nodeId),
  };
}

/** FLO-FR-45: whether removing this loop would take anything with it. */
export function loopHolds(doc: FlowDocument, loopId: string): boolean {
  return elements(doc).some((e) => e.parentId === loopId);
}

/**
 * FLO-FR-45: remove a loop, either way the author meant it.
 *
 * `cascade` takes every element and edge inside it, nested loops and all, on the
 * same terms removing a node removes its edges. `release` keeps them, moving
 * each immediate member into whatever contained the loop — where the edges among
 * them stay valid because they stay siblings — and rebasing their positions so
 * nothing appears to jump. Either way the loop's own edges go with it.
 */
export function removeLoop(
  doc: FlowDocument,
  loopId: string,
  mode: "cascade" | "release",
): FlowDocument {
  const loop = doc.loops.find((l) => l.id === loopId);
  if (!loop) return doc;

  if (mode === "cascade") {
    const gone = subtreeOf(doc, loopId);
    return {
      ...doc,
      loops: doc.loops.filter((l) => !gone.has(l.id)),
      nodes: doc.nodes.filter((n) => !gone.has(n.id)),
      edges: doc.edges.filter((e) => !gone.has(e.from) && !gone.has(e.to)),
    };
  }

  // Released members keep their place on the canvas: their positions were
  // relative to the loop's origin, so they take the loop's own offset with them.
  const rehome = <T extends FlowElementBase>(e: T): T => {
    if (e.parentId !== loopId) return e;
    const { parentId: _drop, ...rest } = e;
    return {
      ...(rest as T),
      ...(loop.parentId !== undefined ? { parentId: loop.parentId } : {}),
      position: {
        x: e.position.x + loop.position.x,
        y: e.position.y + loop.position.y,
      },
    };
  };
  return {
    ...doc,
    loops: doc.loops.filter((l) => l.id !== loopId).map(rehome),
    nodes: doc.nodes.map(rehome),
    edges: doc.edges.filter((e) => e.from !== loopId && e.to !== loopId),
  };
}

function patchNode(
  doc: FlowDocument,
  nodeId: string,
  patch: (node: FlowNode) => FlowNode,
): FlowDocument {
  return {
    ...doc,
    nodes: doc.nodes.map((n) => (n.id === nodeId ? patch(n) : n)),
  };
}

/** FLO-FR-09: free text, not required to be unique — identity is the id. */
export function renameNode(
  doc: FlowDocument,
  nodeId: string,
  name: string,
): FlowDocument {
  return patchNode(doc, nodeId, (n) => ({ ...n, name }));
}

/**
 * FLO-FR-25: name the Flow. Free text, and the only field the editor marks as
 * required — a Flow is run by name, so an unnamed one is incomplete even though
 * it is perfectly saveable.
 */
export function setFlowName(doc: FlowDocument, name: string): FlowDocument {
  if (doc.name === name) return doc;
  return { ...doc, name };
}

/**
 * FLO-FR-25: describe the Flow. Optional; emptying it removes the field rather
 * than storing `""`, so a description the user cleared serializes identically to
 * one that was never written.
 */
export function setFlowDescription(
  doc: FlowDocument,
  description: string,
): FlowDocument {
  if ((doc.description ?? "") === description) return doc;
  const { description: _drop, ...rest } = doc;
  return description === "" ? rest : { ...rest, description };
}

/**
 * Apply a patch to whichever list holds the element, node or loop, leaving the
 * other untouched. One id space (FLO-FR-06) means one lookup: a caller that knew
 * which kind it held would have to keep on knowing every time the kinds gained a
 * shared field.
 */
function patchElement(
  doc: FlowDocument,
  elementId: string,
  patch: <T extends FlowElementBase>(element: T) => T,
): FlowDocument {
  return {
    ...doc,
    loops: doc.loops.map((l) => (l.id === elementId ? patch(l) : l)),
    nodes: doc.nodes.map((n) => (n.id === elementId ? patch(n) : n)),
  };
}

/**
 * FLO-FR-10: add an artifact to an element's references — a loop's as much as a
 * node's (FLO-FR-37) — appended after the ones it already carries.
 *
 * Returns the document unchanged — the same object, not an equal one — when the
 * element already references that artifact or does not exist, so a re-selection
 * cannot raise the unsaved state (FLO-FR-26) over a graph nobody changed.
 */
export function addElementArtifact(
  doc: FlowDocument,
  elementId: string,
  artifactId: string,
): FlowDocument {
  if (!artifactId) return doc;
  const element = findElement(doc, elementId);
  if (!element || (element.artifactIds ?? []).includes(artifactId)) return doc;
  return patchElement(doc, elementId, (e) => ({
    ...e,
    artifactIds: [...(e.artifactIds ?? []), artifactId],
  }));
}

/**
 * FLO-FR-10: drop one of an element's artifact references, leaving its others in
 * place. Removing the last one removes the field rather than storing an empty
 * list, so an element the user cleared serializes identically to one that never
 * referenced anything (FLO-FR-14).
 */
export function removeElementArtifact(
  doc: FlowDocument,
  elementId: string,
  artifactId: string,
): FlowDocument {
  const element = findElement(doc, elementId);
  if (!element || !(element.artifactIds ?? []).includes(artifactId)) return doc;
  return patchElement(doc, elementId, (e) => {
    const { artifactIds: _drop, ...rest } = e;
    const kept = (e.artifactIds ?? []).filter((id) => id !== artifactId);
    return (kept.length > 0 ? { ...rest, artifactIds: kept } : rest) as typeof e;
  });
}

/**
 * FLO-FR-13: set a node's inline prompt. Emptying it removes the field rather
 * than storing `""`, so a node the user cleared serializes identically to one
 * that never carried a prompt (FLO-FR-14).
 */
export function setNodePrompt(
  doc: FlowDocument,
  nodeId: string,
  prompt: string,
): FlowDocument {
  return patchNode(doc, nodeId, (n) => {
    const { prompt: _drop, ...rest } = n;
    return prompt === "" ? rest : { ...rest, prompt };
  });
}

/** FLO-FR-38: a loop's name, renamed in place like a node's. */
export function renameLoop(
  doc: FlowDocument,
  loopId: string,
  name: string,
): FlowDocument {
  const loop = doc.loops.find((l) => l.id === loopId);
  if (!loop || loop.name === name) return doc;
  return {
    ...doc,
    loops: doc.loops.map((l) => (l.id === loopId ? { ...l, name } : l)),
  };
}

/**
 * FLO-FR-37: the pass bound. `null` clears it — a loop bounded by nothing but
 * the prompt it references is an ordinary loop — and a value that is not a whole
 * number greater than zero is not stored, because the document may not carry one
 * (`FGV-FR-14`).
 */
export function setLoopMaxPasses(
  doc: FlowDocument,
  loopId: string,
  maxPasses: number | null,
): FlowDocument {
  const loop = doc.loops.find((l) => l.id === loopId);
  if (!loop) return doc;
  const next =
    maxPasses === null ||
    !Number.isInteger(maxPasses) ||
    (maxPasses as number) < 1
      ? undefined
      : (maxPasses as number);
  if ((loop.maxPasses ?? undefined) === next) return doc;
  return {
    ...doc,
    loops: doc.loops.map((l) => {
      if (l.id !== loopId) return l;
      const { maxPasses: _drop, ...rest } = l;
      return next === undefined ? rest : { ...rest, maxPasses: next };
    }),
  };
}

/**
 * FLO-FR-44: the smallest a loop may be made — the bounds of what it holds, plus
 * the room the container keeps around them and the header above them.
 *
 * A loop holding nothing has no contents to clear, so its floor is a size small
 * enough to be useful and large enough to drop something into.
 */
export function minLoopSize(doc: FlowDocument, loopId: string): FlowSize {
  const floor = { width: 160, height: LOOP_HEADER_H + LOOP_PADDING * 2 };
  const members = elements(doc).filter((e) => e.parentId === loopId);
  if (members.length === 0) return floor;
  let right = 0;
  let bottom = 0;
  for (const m of members) {
    const w = m.kind === "loop" ? m.size.width : MEMBER_NODE_W;
    const h = m.kind === "loop" ? m.size.height : MEMBER_NODE_H;
    right = Math.max(right, m.position.x + w);
    bottom = Math.max(bottom, m.position.y + h);
  }
  return {
    width: Math.max(floor.width, right + LOOP_PADDING),
    height: Math.max(floor.height, bottom + LOOP_PADDING),
  };
}

/**
 * FLO-FR-44: resize a loop, never smaller than what it holds — so no element is
 * put outside its container by a gesture aimed at the container. Membership
 * changes only by the drop of FLO-FR-42.
 */
export function resizeLoop(
  doc: FlowDocument,
  loopId: string,
  size: FlowSize,
): FlowDocument {
  const loop = doc.loops.find((l) => l.id === loopId);
  if (!loop) return doc;
  const min = minLoopSize(doc, loopId);
  const next = {
    width: Math.max(min.width, Math.round(size.width)),
    height: Math.max(min.height, Math.round(size.height)),
  };
  if (loop.size.width === next.width && loop.size.height === next.height) {
    return doc;
  }
  return {
    ...doc,
    loops: doc.loops.map((l) => (l.id === loopId ? { ...l, size: next } : l)),
  };
}

/**
 * FLO-FR-43: the edges a move would sever — every edge joining `id` to an
 * element it is leaving behind.
 *
 * A move within the same container severs nothing. A move out of one severs
 * every edge `id` carries, because FLO-FR-40 makes all of them edges to its
 * current siblings, and none of them survives the boundary. Edges *inside* a
 * moved loop are untouched: its contents travel with it and stay siblings.
 */
export function edgesSeveredByMove(
  doc: FlowDocument,
  id: string,
  newParentId: string | undefined,
): FlowEdge[] {
  if (parentOf(doc, id) === newParentId) return [];
  return doc.edges.filter((e) => e.from === id || e.to === id);
}

/**
 * FLO-FR-21 / FLO-FR-42 / FLO-FR-43: move an element to `absolute` on the
 * canvas, into `newParentId`.
 *
 * `absolute` is in canvas coordinates and is rebased into the new container's
 * frame here, so callers never have to know where a loop happens to sit. A move
 * that changes the container drops the edges `edgesSeveredByMove` names — the
 * caller is expected to have asked the author first.
 *
 * Returns the document unchanged when nothing about the element moves, so a drag
 * that wandered off and came back raises no unsaved state (FLO-FR-26).
 */
export function moveElement(
  doc: FlowDocument,
  id: string,
  absolute: FlowPosition,
  newParentId?: string,
): FlowDocument {
  const element = findElement(doc, id);
  if (!element) return doc;
  if (newParentId !== undefined) {
    // A loop is never placed inside itself, directly or through any chain
    // (FLO-FR-41).
    if (subtreeOf(doc, id).has(newParentId)) return doc;
    // The container has to still be there. A caller can hold a container id
    // across an edit — the confirmation of FLO-FR-43 is answered after the drag
    // that proposed it — and reparenting onto a loop that has since been removed
    // would leave the element naming nothing, which is a document the validator
    // refuses to write (`FGV-FR-13`) and the author cannot save their way out of.
    if (!doc.loops.some((l) => l.id === newParentId)) return doc;
  }

  const origin =
    newParentId === undefined
      ? { x: 0, y: 0 }
      : absolutePosition(doc, newParentId);
  const position = { x: absolute.x - origin.x, y: absolute.y - origin.y };
  const reparented = element.parentId !== newParentId;
  if (
    !reparented &&
    element.position.x === position.x &&
    element.position.y === position.y
  ) {
    return doc;
  }

  const severed = reparented ? new Set(edgesSeveredByMove(doc, id, newParentId).map((e) => e.id)) : null;
  const patch = <T extends FlowElementBase>(e: T): T => {
    if (e.id !== id) return e;
    const { parentId: _drop, ...rest } = e;
    return {
      ...(rest as T),
      ...(newParentId !== undefined ? { parentId: newParentId } : {}),
      position,
    };
  };
  return {
    ...doc,
    loops: doc.loops.map(patch),
    nodes: doc.nodes.map(patch),
    edges: severed ? doc.edges.filter((e) => !severed.has(e.id)) : doc.edges,
  };
}

/**
 * FLO-FR-21: kept as the node-only spelling of `moveElement` for callers that
 * are only ever moving a node within its own container.
 */
export function moveNode(
  doc: FlowDocument,
  nodeId: string,
  position: FlowPosition,
): FlowDocument {
  const node = doc.nodes.find((n) => n.id === nodeId);
  if (!node) return doc;
  if (node.position.x === position.x && node.position.y === position.y) {
    return doc;
  }
  return patchNode(doc, nodeId, (n) => ({ ...n, position }));
}

/** Why `connect` declined to create an edge. */
export type ConnectRefusal =
  | "self"
  | "duplicate"
  | "unknown-node"
  | "cross-container";

export type ConnectResult =
  | { ok: true; doc: FlowDocument; edgeId: string }
  | { ok: false; refused: ConnectRefusal };

/**
 * FLO-FR-15 – FLO-FR-18 / FLO-FR-39 / FLO-FR-40: connect two elements.
 *
 * Either endpoint may be a node or a loop — a loop is connectable as an element
 * of whatever contains it, and an edge arriving at one is the workflow entering
 * it (FLO-FR-39).
 *
 * Three refusals and no more: an element joined to itself, a second edge on an
 * ordered pair that already has one, and two elements that do not share a
 * container. In particular a connection that closes a cycle is created like any
 * other, because a loop or a retry is exactly what a back-edge expresses
 * (FLO-FR-18). `B→A` is a different ordered pair from `A→B`, so both may exist
 * at once.
 */
export function connect(
  doc: FlowDocument,
  from: string,
  to: string,
): ConnectResult {
  if (from === to) return { ok: false, refused: "self" };
  const ids = elementIdsOf(doc);
  if (!ids.has(from) || !ids.has(to)) return { ok: false, refused: "unknown-node" };
  if (!areSiblings(doc, from, to)) {
    return { ok: false, refused: "cross-container" };
  }
  if (doc.edges.some((e) => e.from === from && e.to === to)) {
    return { ok: false, refused: "duplicate" };
  }
  const id = mintId("e", new Set(doc.edges.map((e) => e.id)), doc.edges.length + 1);
  return { ok: true, doc: { ...doc, edges: [...doc.edges, { id, from, to }] }, edgeId: id };
}

/** FLO-FR-20: remove one edge, leaving both endpoint nodes and their others. */
export function removeEdge(doc: FlowDocument, edgeId: string): FlowDocument {
  return { ...doc, edges: doc.edges.filter((e) => e.id !== edgeId) };
}

/**
 * FLO-FR-19: an edge's label is optional and free text. Emptying it removes the
 * field, so the edge renders as the bare arrow an unlabelled edge is.
 */
export function renameEdge(
  doc: FlowDocument,
  edgeId: string,
  label: string,
): FlowDocument {
  return {
    ...doc,
    edges: doc.edges.map((e) => {
      if (e.id !== edgeId) return e;
      const { label: _drop, ...rest } = e;
      return label === "" ? rest : { ...rest, label };
    }),
  };
}
