/** Deserialization (FLO-FR-03 / FLO-FR-04 / FLO-FR-05). */

import { } from "./containment";
import {
  emptyFlow,
  FLOW_VERSION,
  type FlowDocument,
  type FlowEdge,
  type FlowLoop,
  type FlowNode } from "./types";

export type FlowParseResult =
  | { ok: true; doc: FlowDocument }
  | { ok: false; error: string };

function isObject(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function optionalString(
  v: unknown,
  what: string,
): { ok: true; value?: string } | { ok: false; error: string } {
  if (v === undefined || v === null) return { ok: true };
  if (typeof v !== "string") return { ok: false, error: `${what} must be a string` };
  return { ok: true, value: v };
}

/**
 * FLO-FR-10: an element's artifact references — a loop's as much as a node's
 * (FLO-FR-37) — from either the list the field holds now or the single
 * `artifactId` a node carried before it could reference more than one artifact.
 * Accepting the older shape is what keeps a Flow authored against it opening
 * rather than landing in the FLO-FR-05 error state; the next write emits the
 * list form. Only a node is read for it: no loop was ever written with it, so
 * accepting it there would be inventing history rather than keeping it.
 *
 * Repeats are folded rather than refused: referencing the same artifact twice
 * says nothing a single reference does not, and an unreadable-file error over it
 * would cost the author their whole Flow to fix by hand.
 */
function artifactIdsOf(
  entry: Record<string, unknown>,
  id: string,
  kind: "node" | "loop",
): { ok: true; value?: string[] } | { ok: false; error: string } {
  const raw = entry.artifactIds;
  if (raw === undefined || raw === null) {
    if (kind === "loop") return { ok: true, value: undefined };
    const one = optionalString(entry.artifactId, `node "${id}" artifactId`);
    if (!one.ok) return one;
    return { ok: true, value: one.value ? [one.value] : undefined };
  }
  if (!Array.isArray(raw)) {
    return { ok: false, error: `${kind} "${id}" artifactIds must be an array` };
  }
  const out: string[] = [];
  for (const v of raw) {
    if (typeof v !== "string" || v === "") {
      return {
        ok: false,
        error: `${kind} "${id}" has an artifact reference that is not a string` };
    }
    if (!out.includes(v)) out.push(v);
  }
  return { ok: true, value: out.length > 0 ? out : undefined };
}

/**
 * FLO-FR-04 / FLO-FR-05: turn a file body into a graph, or say why it cannot be
 * one.
 *
 * An empty or whitespace-only body is an empty graph, not a failure — that is
 * what a Flow created through New Artifact, New File, or an external `touch`
 * looks like on disk, and such a file must open ready to edit (FLO-FR-04).
 * Anything else that will not parse, or that parses to something other than the
 * document shape, is an error the caller renders as the tab's error state; the
 * caller writes nothing while in it, so an unreadable file is never replaced by
 * an empty graph (FLO-FR-05).
 *
 * Validation is deliberately strict about the invariants the canvas relies on —
 * unique node ids, edges whose endpoints both exist, no self-edge, at most one
 * edge per ordered pair — because a document violating one of them is not
 * something the editor can render and then faithfully write back. Reporting it
 * is the only answer that does not lose the author's file.
 */
export function parseFlowDocument(body: string): FlowParseResult {
  if (body.trim() === "") return { ok: true, doc: emptyFlow() };

  let raw: unknown;
  try {
    raw = JSON.parse(body);
  } catch (e) {
    return { ok: false, error: `not valid JSON: ${(e as Error).message}` };
  }

  if (!isObject(raw)) return { ok: false, error: "not a Flow document: expected an object" };
  if (raw.version !== FLOW_VERSION) {
    return {
      ok: false,
      error: `unsupported Flow version ${JSON.stringify(raw.version)} (expected ${FLOW_VERSION})` };
  }
  if (!Array.isArray(raw.nodes)) return { ok: false, error: "`nodes` must be an array" };
  if (!Array.isArray(raw.edges)) return { ok: false, error: "`edges` must be an array" };
  // FLO-FR-37: `loops` is absent from a document holding none.
  if (raw.loops !== undefined && raw.loops !== null && !Array.isArray(raw.loops)) {
    return { ok: false, error: "`loops` must be an array" };
  }
  const rawLoops: unknown[] = Array.isArray(raw.loops) ? raw.loops : [];

  // FLO-FR-25: a Flow written before it could carry a name reads as one with an
  // empty name rather than as an unreadable file — the alternative is refusing
  // to open every Flow that already exists.
  const name = optionalString(raw.name, "`name`");
  if (!name.ok) return name;
  const description = optionalString(raw.description, "`description`");
  if (!description.ok) return description;

  /** A `{ x, y }` or `{ width, height }` block of finite numbers. */
  function numberPair<K extends string>(
    v: unknown,
    keys: readonly [K, K],
    what: string,
  ): { ok: true; value: Record<K, number> } | { ok: false; error: string } {
    if (!isObject(v)) return { ok: false, error: `${what} is not an object` };
    const out = {} as Record<K, number>;
    for (const key of keys) {
      const n = v[key];
      if (typeof n !== "number" || !Number.isFinite(n)) {
        return { ok: false, error: `${what} has no numeric ${key}` };
      }
      out[key] = n;
    }
    return { ok: true, value: out };
  }

  // FLO-FR-06: nodes and loops share ONE id space, so both are collected here
  // before either is used as an edge endpoint or as a parent.
  const elementIds = new Set<string>();
  const parents = new Map<string, string | undefined>();

  const loops: FlowLoop[] = [];
  for (const [i, entry] of rawLoops.entries()) {
    if (!isObject(entry)) return { ok: false, error: `loop ${i} is not an object` };
    const { id, name } = entry;
    if (typeof id !== "string" || id === "") {
      return { ok: false, error: `loop ${i} has no id` };
    }
    if (elementIds.has(id)) return { ok: false, error: `duplicate element id "${id}"` };
    if (typeof name !== "string") return { ok: false, error: `loop "${id}" has no name` };
    const position = numberPair(entry.position, ["x", "y"] as const, `loop "${id}" position`);
    if (!position.ok) return position;
    const size = numberPair(
      entry.size,
      ["width", "height"] as const,
      `loop "${id}" size`,
    );
    if (!size.ok) return size;
    const artifactIds = artifactIdsOf(entry, id, "loop");
    if (!artifactIds.ok) return artifactIds;
    const parentId = optionalString(entry.parentId, `loop "${id}" parentId`);
    if (!parentId.ok) return parentId;
    // FLO-FR-37: a pass bound is a whole number greater than zero, or absent.
    const maxPassesRaw = entry.maxPasses;
    let maxPasses: number | undefined;
    if (maxPassesRaw !== undefined && maxPassesRaw !== null) {
      if (
        typeof maxPassesRaw !== "number" ||
        !Number.isInteger(maxPassesRaw) ||
        maxPassesRaw < 1
      ) {
        return {
          ok: false,
          error: `loop "${id}" maxPasses must be a whole number greater than zero` };
      }
      maxPasses = maxPassesRaw;
    }

    elementIds.add(id);
    parents.set(id, parentId.value);
    loops.push({
      id,
      name,
      ...(artifactIds.value !== undefined ? { artifactIds: artifactIds.value } : {}),
      ...(maxPasses !== undefined ? { maxPasses } : {}),
      ...(parentId.value !== undefined ? { parentId: parentId.value } : {}),
      position: position.value,
      size: size.value });
  }

  const nodes: FlowNode[] = [];
  for (const [i, entry] of raw.nodes.entries()) {
    if (!isObject(entry)) return { ok: false, error: `node ${i} is not an object` };
    const { id, name } = entry;
    if (typeof id !== "string" || id === "") {
      return { ok: false, error: `node ${i} has no id` };
    }
    if (elementIds.has(id)) return { ok: false, error: `duplicate element id "${id}"` };
    if (typeof name !== "string") {
      return { ok: false, error: `node "${id}" has no name` };
    }
    const position = numberPair(entry.position, ["x", "y"] as const, `node "${id}" position`);
    if (!position.ok) return position;
    const artifactIds = artifactIdsOf(entry, id, "node");
    if (!artifactIds.ok) return artifactIds;
    const prompt = optionalString(entry.prompt, `node "${id}" prompt`);
    if (!prompt.ok) return prompt;
    const parentId = optionalString(entry.parentId, `node "${id}" parentId`);
    if (!parentId.ok) return parentId;

    elementIds.add(id);
    parents.set(id, parentId.value);
    nodes.push({
      id,
      name,
      ...(artifactIds.value !== undefined ? { artifactIds: artifactIds.value } : {}),
      ...(prompt.value !== undefined ? { prompt: prompt.value } : {}),
      ...(parentId.value !== undefined ? { parentId: parentId.value } : {}),
      position: position.value });
  }

  // FLO-FR-41: a `parentId` names a loop, and containment is acyclic.
  const loopIds = new Set(loops.map((l) => l.id));
  for (const [id, parent] of parents) {
    if (parent === undefined) continue;
    if (!loopIds.has(parent)) {
      return { ok: false, error: `"${id}" names a parent "${parent}" that is not a loop` };
    }
    const seen = new Set<string>([id]);
    let at: string | undefined = parent;
    while (at !== undefined) {
      if (seen.has(at)) {
        return { ok: false, error: `loop "${id}" contains itself through its parents` };
      }
      seen.add(at);
      at = parents.get(at);
    }
  }

  const edges: FlowEdge[] = [];
  const edgeIds = new Set<string>();
  const pairs = new Set<string>();
  for (const [i, entry] of raw.edges.entries()) {
    if (!isObject(entry)) return { ok: false, error: `edge ${i} is not an object` };
    const { id, from, to } = entry;
    if (typeof id !== "string" || id === "") {
      return { ok: false, error: `edge ${i} has no id` };
    }
    if (edgeIds.has(id)) return { ok: false, error: `duplicate edge id "${id}"` };
    if (typeof from !== "string" || typeof to !== "string") {
      return { ok: false, error: `edge "${id}" has no endpoints` };
    }
    // FLO-FR-08: no edge exists without both of its endpoints. Either may be a
    // node or a loop — both are elements the graph connects (FLO-FR-39).
    if (!elementIds.has(from)) {
      return { ok: false, error: `edge "${id}" starts at unknown element "${from}"` };
    }
    if (!elementIds.has(to)) {
      return { ok: false, error: `edge "${id}" ends at unknown element "${to}"` };
    }
    // FLO-FR-16 / FLO-FR-17 / FLO-FR-40: the three rules that bound what a
    // graph may hold.
    if (from === to) {
      return { ok: false, error: `edge "${id}" connects element "${from}" to itself` };
    }
    if (parents.get(from) !== parents.get(to)) {
      return {
        ok: false,
        error: `edge "${id}" joins "${from}" and "${to}", which are in different containers` };
    }
    const pair = `${from}\0${to}`;
    if (pairs.has(pair)) {
      return { ok: false, error: `more than one edge from "${from}" to "${to}"` };
    }
    const label = optionalString(entry.label, `edge "${id}" label`);
    if (!label.ok) return label;

    edgeIds.add(id);
    pairs.add(pair);
    edges.push({
      id,
      from,
      to,
      ...(label.value !== undefined ? { label: label.value } : {}) });
  }

  return {
    ok: true,
    doc: {
      version: FLOW_VERSION,
      name: name.value ?? "",
      ...(description.value ? { description: description.value } : {}),
      loops,
      nodes,
      edges } };
}
