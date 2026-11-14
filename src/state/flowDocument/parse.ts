/**
 * Deserialization of a Flow document, version 3 (FLO-FR-03, FLO-FR-04,
 * FLO-FR-05).
 *
 * The backend validation (`"validate flow document"`, FGV-FR-02) is the
 * authority on whether a body is a Flow, and a Flow tab validates every body
 * before it calls this (FLO-FR-46). This reader is not a second validator: it
 * checks only what it needs to build the typed document, so that a body that
 * the backend did not see (a Git revision in the Diff tab, DFV-FR-33) gives an
 * error rather than a broken document.
 */

import {
  emptyFlow,
  FLOW_VERSION,
  ITEM_KINDS,
  type FlowDocument,
  type FlowEdge,
  type FlowItem,
  type FlowItemKind,
} from "./types";

export type FlowParseResult =
  | { ok: true; doc: FlowDocument }
  | { ok: false; error: string };

type Obj = Record<string, unknown>;

/** A failure that ends the parse; caught once, at the top. */
class ParseError extends Error {}

function fail(message: string): never {
  throw new ParseError(message);
}

function isObject(v: unknown): v is Obj {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function optString(v: unknown, what: string): string | undefined {
  if (v === undefined || v === null) return undefined;
  if (typeof v !== "string") fail(`${what} must be a string`);
  return v;
}

function reqString(v: unknown, what: string): string {
  if (typeof v !== "string") fail(`${what} must be a string`);
  return v;
}

/** A list of strings; an empty list reads as absent (FGV-FR-01). */
function optStrings(v: unknown, what: string): string[] | undefined {
  if (v === undefined || v === null) return undefined;
  if (!Array.isArray(v) || v.some((s) => typeof s !== "string")) {
    fail(`${what} must be a list of strings`);
  }
  return v.length > 0 ? [...(v as string[])] : undefined;
}

function readItem(entry: unknown, i: number): FlowItem {
  if (!isObject(entry)) fail(`item ${i} is not an object`);
  const id = entry.id;
  if (typeof id !== "string" || id === "") fail(`item ${i} has no id`);
  const place = `item "${id}"`;
  const kind = entry.kind;
  if (typeof kind !== "string" || !ITEM_KINDS.includes(kind as FlowItemKind)) {
    fail(`${place} has no item kind`);
  }
  const name = reqString(entry.name, `${place} name`);
  const description = optString(entry.description, `${place} description`);
  const artifactIds = optStrings(entry.artifactIds, `${place} artifactIds`);
  const parentId = optString(entry.parentId, `${place} parentId`);
  const draftId = optString(entry.draftId, `${place} draftId`);
  const footprint = optStrings(entry.footprint, `${place} footprint`);
  const due = optString(entry.due, `${place} due`);
  return {
    id,
    kind: kind as FlowItemKind,
    name,
    ...(description !== undefined ? { description } : {}),
    ...(artifactIds !== undefined ? { artifactIds } : {}),
    ...(parentId !== undefined ? { parentId } : {}),
    ...(draftId !== undefined ? { draftId } : {}),
    ...(footprint !== undefined ? { footprint } : {}),
    ...(due !== undefined ? { due } : {}),
  };
}

function readEdge(entry: unknown, i: number): FlowEdge {
  if (!isObject(entry)) fail(`edge ${i} is not an object`);
  const id = entry.id;
  if (typeof id !== "string" || id === "") fail(`edge ${i} has no id`);
  const from = entry.from;
  const to = entry.to;
  if (typeof from !== "string" || typeof to !== "string") {
    fail(`edge "${id}" has no endpoints`);
  }
  const type = entry.type;
  if (type !== "blocks" && type !== "relates") {
    fail(`edge "${id}" has no type of blocks or relates`);
  }
  return { id, from, to, type };
}

function readDocument(raw: unknown): FlowDocument {
  if (!isObject(raw)) fail("not a Flow document: expected an object");
  // FGV-FR-07: a version 1 or a version 2 document is an unknown version.
  if (raw.version !== FLOW_VERSION) {
    fail(
      `unknown Flow version ${JSON.stringify(raw.version ?? null)} (expected ${FLOW_VERSION})`,
    );
  }
  for (const list of ["items", "edges"] as const) {
    if (raw[list] !== undefined && raw[list] !== null && !Array.isArray(raw[list])) {
      fail(`\`${list}\` must be an array`);
    }
  }
  const name = optString(raw.name, "`name`") ?? "";
  const description = optString(raw.description, "`description`");
  const references = optStrings(raw.references, "`references`") ?? [];
  const items = ((raw.items as unknown[] | undefined) ?? []).map(readItem);
  const edges = ((raw.edges as unknown[] | undefined) ?? []).map(readEdge);
  return {
    version: FLOW_VERSION,
    name,
    // FLO-FR-25: an empty description is absent.
    ...(description ? { description } : {}),
    references,
    items,
    edges,
  };
}

/**
 * FLO-FR-04 / FLO-FR-05: turn a file body into a plan, or say why it cannot
 * be one. An empty or whitespace-only body is the empty plan (FGV-FR-05).
 */
export function parseFlowDocument(body: string): FlowParseResult {
  if (body.trim() === "") return { ok: true, doc: emptyFlow() };
  let raw: unknown;
  try {
    raw = JSON.parse(body);
  } catch (e) {
    return { ok: false, error: `not valid JSON: ${(e as Error).message}` };
  }
  try {
    return { ok: true, doc: readDocument(raw) };
  } catch (e) {
    if (e instanceof ParseError) return { ok: false, error: e.message };
    throw e;
  }
}
