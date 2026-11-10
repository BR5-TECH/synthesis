/**
 * Deserialization of a Flow document, version 2 (FLO-FR-03, FLO-FR-04,
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
  type FlowSync,
  type SyncBase,
  type SyncLink,
  type TrackerRef,
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

function numberPair<K extends string>(
  v: unknown,
  keys: readonly [K, K],
  what: string,
): Record<K, number> {
  if (!isObject(v)) fail(`${what} is not an object`);
  const out = {} as Record<K, number>;
  for (const key of keys) {
    const n = v[key];
    if (typeof n !== "number" || !Number.isFinite(n)) {
      fail(`${what} has no numeric ${key}`);
    }
    out[key] = n;
  }
  return out;
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
  let localOnly: boolean | undefined;
  if (entry.localOnly !== undefined && entry.localOnly !== null) {
    if (typeof entry.localOnly !== "boolean") {
      fail(`${place} localOnly must be a boolean`);
    }
    localOnly = entry.localOnly;
  }
  const draftId = optString(entry.draftId, `${place} draftId`);
  const footprint = optStrings(entry.footprint, `${place} footprint`);
  const due = optString(entry.due, `${place} due`);
  const position = numberPair(entry.position, ["x", "y"] as const, `${place} position`);
  const size =
    entry.size === undefined || entry.size === null
      ? undefined
      : numberPair(entry.size, ["width", "height"] as const, `${place} size`);
  return {
    id,
    kind: kind as FlowItemKind,
    name,
    ...(description !== undefined ? { description } : {}),
    ...(artifactIds !== undefined ? { artifactIds } : {}),
    ...(parentId !== undefined ? { parentId } : {}),
    ...(localOnly !== undefined ? { localOnly } : {}),
    ...(draftId !== undefined ? { draftId } : {}),
    ...(footprint !== undefined ? { footprint } : {}),
    ...(due !== undefined ? { due } : {}),
    position,
    ...(size !== undefined ? { size } : {}),
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

function readRef(v: unknown, what: string): TrackerRef {
  if (!isObject(v)) fail(`${what} is not an object`);
  const kind = v.kind;
  if (kind !== "issue" && kind !== "milestone") {
    fail(`${what} has no kind of issue or milestone`);
  }
  const number = v.number;
  if (typeof number !== "number" || !Number.isInteger(number)) {
    fail(`${what} has no number`);
  }
  return {
    kind,
    host: reqString(v.host, `${what} host`),
    owner: reqString(v.owner, `${what} owner`),
    repo: reqString(v.repo, `${what} repo`),
    number,
    url: reqString(v.url, `${what} url`),
  };
}

/**
 * A sync base. The issue-only and milestone-only fields keep the difference
 * between absent and `null`, so the base round-trips as it was written.
 */
function readBase(v: unknown, what: string): SyncBase {
  if (!isObject(v)) fail(`${what} is not an object`);
  const state = v.state;
  if (state !== "open" && state !== "closed") fail(`${what} has no state`);
  const base: SyncBase = {
    name: reqString(v.name, `${what} name`),
    description: reqString(v.description, `${what} description`),
    state,
  };
  if ("type" in v) {
    base.type = v.type === null ? null : reqString(v.type, `${what} type`);
  }
  if ("parent" in v) {
    base.parent = v.parent === null ? null : readRef(v.parent, `${what} parent`);
  }
  if ("milestone" in v) {
    if (v.milestone !== null && typeof v.milestone !== "number") {
      fail(`${what} milestone must be a number or null`);
    }
    base.milestone = v.milestone as number | null;
  }
  if ("blockedBy" in v) {
    if (!Array.isArray(v.blockedBy)) fail(`${what} blockedBy must be a list`);
    base.blockedBy = v.blockedBy.map((r, k) => readRef(r, `${what} blockedBy ${k}`));
  }
  if ("due" in v) {
    base.due = v.due === null ? null : reqString(v.due, `${what} due`);
  }
  return base;
}

function readSync(v: unknown): FlowSync | undefined {
  if (v === undefined || v === null) return undefined;
  if (!isObject(v)) fail("`sync` is not an object");
  if (v.tracker !== "github") fail("`sync.tracker` must be github");
  if (!Array.isArray(v.links)) fail("`sync.links` must be an array");
  const links: SyncLink[] = v.links.map((entry, i) => {
    if (!isObject(entry)) fail(`sync link ${i} is not an object`);
    return {
      itemId: reqString(entry.itemId, `sync link ${i} itemId`),
      ref: readRef(entry.ref, `sync link ${i} ref`),
      base: readBase(entry.base, `sync link ${i} base`),
    };
  });
  let ignored: TrackerRef[] | undefined;
  if (v.ignored !== undefined && v.ignored !== null) {
    if (!Array.isArray(v.ignored)) fail("`sync.ignored` must be an array");
    ignored = v.ignored.map((r, i) => readRef(r, `sync ignored ${i}`));
  }
  // FGV-FR-01: an empty list reads as absent.
  return {
    tracker: "github",
    links,
    ...(ignored && ignored.length > 0 ? { ignored } : {}),
  };
}

function readDocument(raw: unknown): FlowDocument {
  if (!isObject(raw)) fail("not a Flow document: expected an object");
  // FGV-FR-07: a version 1 document is an unknown version.
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
  const sync = readSync(raw.sync);
  return {
    version: FLOW_VERSION,
    name,
    // FLO-FR-25: an empty description is absent.
    ...(description ? { description } : {}),
    references,
    items,
    edges,
    ...(sync !== undefined ? { sync } : {}),
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
