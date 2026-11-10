/**
 * The effective difference between two revisions of a Flow
 * (`DFV-diff-viewer.md` DFV-FR-33, DFV-FR-RGBK, DFV-FR-34, DFV-FR-35).
 *
 * A Flow is JSON, so its source diff is a diff of braces: an item moved by
 * three pixels changes the same lines as an item that is removed. This module
 * compares the two revisions as the plans they describe: the Flow's own
 * fields, each item paired by id, and each edge paired by its two endpoints.
 *
 * Pure, and free of React: the same entries feed all three visualization
 * modes, which differ only in which entries they show and on which side.
 */
import {
  parseFlowDocument,
  type FlowDocument,
  type FlowEdge,
  type FlowItem,
  type FlowItemKind,
  type SyncLink,
} from "../state/flowDocument";

/** How an entry, or one fact in it, stands between the two revisions. */
export type FlowChangeStatus = "added" | "removed" | "changed" | "unchanged";

/**
 * One statement about an entry, with what it was and what it became.
 *
 * A fact of an added or removed entry is `unchanged`: the entry's own marking
 * already says that all of it is new or gone (the reasoning of DFV-FR-32).
 */
export interface FlowFact {
  label: string;
  status: FlowChangeStatus;
  before?: string;
  after?: string;
}

/** One thing the diff has something to say about. */
export interface FlowDiffEntry {
  /** Stable in a diff: `flow`, `item:<id>`, or `edge:<a> <b>`. */
  key: string;
  kind: "flow" | "item" | "edge";
  /** The item's kind in the old revision, for an item entry that has one. */
  itemKindBefore?: FlowItemKind;
  /** The item's kind in the new revision, for an item entry that has one. */
  itemKindAfter?: FlowItemKind;
  status: FlowChangeStatus;
  /** What it is called in the old revision; null when it has no counterpart. */
  titleBefore: string | null;
  /** What it is called in the new revision; null when it has no counterpart. */
  titleAfter: string | null;
  facts: FlowFact[];
}

export type FlowDiffResult =
  | { ok: true; entries: FlowDiffEntry[] }
  | { ok: false; error: string };

/**
 * Pair two revisions' entries by key, keeping both orders. An entry pairs only
 * with the entry that has its own key; one with no counterpart faces null.
 *
 * The order is the new revision's, with each removed entry put at the point
 * its old neighbours put it, so a Flow reads in the order its author left it.
 * The old index is by key, not a forward cursor, so a reordered list still
 * pairs each entry with its counterpart.
 */
function pairByKey<T>(
  a: readonly T[],
  b: readonly T[],
  key: (item: T) => string,
): Array<{ old: T | null; new: T | null }> {
  const rows: Array<{ old: T | null; new: T | null }> = [];
  const oldIndex = new Map<string, number>();
  a.forEach((item, i) => {
    if (!oldIndex.has(key(item))) oldIndex.set(key(item), i);
  });
  const newKeys = new Set(b.map(key));
  let cursor = 0;
  const drainRemovedBefore = (stop: number) => {
    for (; cursor < stop; cursor += 1) {
      if (!newKeys.has(key(a[cursor]))) rows.push({ old: a[cursor], new: null });
    }
  };
  for (const item of b) {
    const at = oldIndex.get(key(item));
    if (at === undefined) {
      rows.push({ old: null, new: item });
      continue;
    }
    if (at >= cursor) {
      drainRemovedBefore(at);
      cursor = at + 1;
    }
    rows.push({ old: a[at], new: item });
  }
  drainRemovedBefore(a.length);
  return rows;
}

/**
 * A fact of an entry that only one revision has (`paired` false: unmarked, on
 * its own side), or a fact of a paired entry, marked only where the two sides
 * disagree.
 */
function fact(
  label: string,
  before: string | undefined,
  after: string | undefined,
  paired: boolean,
): FlowFact | null {
  if (!paired) {
    if (after !== undefined) return { label, status: "unchanged", after };
    if (before !== undefined) return { label, status: "unchanged", before };
    return null;
  }
  if (before === after) {
    return before === undefined ? null : { label, status: "unchanged", before, after };
  }
  if (before === undefined) return { label, status: "added", after };
  if (after === undefined) return { label, status: "removed", before };
  return { label, status: "changed", before, after };
}

/**
 * DFV-FR-RGBK: each entry of a list, gained, lost, or kept. The new order
 * comes first, then what only the old revision held.
 */
function listFacts(
  label: string,
  before: readonly string[] | undefined,
  after: readonly string[] | undefined,
  paired: boolean,
): FlowFact[] {
  const oldList = before ?? [];
  const newList = after ?? [];
  const seen = new Set<string>();
  const out: FlowFact[] = [];
  for (const v of [...newList, ...oldList]) {
    if (seen.has(v)) continue;
    seen.add(v);
    const inOld = oldList.includes(v);
    const inNew = newList.includes(v);
    const status: FlowChangeStatus =
      !paired || (inOld && inNew) ? "unchanged" : inNew ? "added" : "removed";
    out.push({
      label,
      status,
      ...(inOld ? { before: v } : {}),
      ...(inNew ? { after: v } : {}),
    });
  }
  return out;
}

const position = (i: FlowItem) => `${i.position.x}, ${i.position.y}`;
const size = (i: FlowItem) =>
  i.size ? `${i.size.width} × ${i.size.height}` : undefined;

/** FLO-FR-ULEM: a tracker link as a reader names it. */
function trackerLink(link: SyncLink | undefined): string | undefined {
  if (!link) return undefined;
  const r = link.ref;
  const where = `${r.host === "github.com" ? "" : `${r.host}/`}${r.owner}/${r.repo}`;
  return r.kind === "milestone"
    ? `${where} milestone ${r.number}`
    : `${where}#${r.number}`;
}

function linkOf(doc: FlowDocument, itemId: string): SyncLink | undefined {
  return doc.sync?.links.find((l) => l.itemId === itemId);
}

/** The container an item sits in, named by its name where it has one. */
function containerName(doc: FlowDocument, parentId: string | undefined): string {
  if (parentId === undefined) return "(top level)";
  return doc.items.find((i) => i.id === parentId)?.name || parentId;
}

/**
 * DFV-FR-RGBK: what an item's entry says. What the item is comes first, then
 * what it links, then where it sits, which says the least. The name and the
 * kind are in the entry's title, so only a difference in them is a fact.
 */
function itemFacts(
  before: FlowItem | null,
  after: FlowItem | null,
  oldDoc: FlowDocument,
  newDoc: FlowDocument,
): FlowFact[] {
  const paired = before !== null && after !== null;
  const facts: Array<FlowFact | null> = [];
  if (paired && before.name !== after.name) {
    facts.push(fact("Name", before.name, after.name, true));
  }
  if (paired && before.kind !== after.kind) {
    facts.push(fact("Kind", before.kind, after.kind, true));
  }
  facts.push(fact("Description", before?.description, after?.description, paired));
  facts.push(...listFacts("Reference", before?.artifactIds, after?.artifactIds, paired));
  facts.push(...listFacts("Footprint", before?.footprint, after?.footprint, paired));
  facts.push(fact("Draft", before?.draftId, after?.draftId, paired));
  facts.push(fact("Due", before?.due, after?.due, paired));
  const local = (i: FlowItem | null) => (i?.localOnly ? "Yes" : undefined);
  facts.push(fact("Local only", local(before), local(after), paired));
  facts.push(
    fact(
      "Tracker link",
      before ? trackerLink(linkOf(oldDoc, before.id)) : undefined,
      after ? trackerLink(linkOf(newDoc, after.id)) : undefined,
      paired,
    ),
  );
  if (paired) {
    if (before.parentId !== after.parentId) {
      facts.push({
        label: "Container",
        status: "changed",
        before: containerName(oldDoc, before.parentId),
        after: containerName(newDoc, after.parentId),
      });
    }
    if (position(before) !== position(after)) {
      facts.push({
        label: "Position",
        status: "changed",
        before: position(before),
        after: position(after),
      });
    }
    const sizeFact = fact("Size", size(before), size(after), true);
    if (sizeFact && sizeFact.status !== "unchanged") facts.push(sizeFact);
  } else if (after?.parentId !== undefined) {
    // On an item that is itself new or gone, its container is part of it,
    // stated only when it is not the top level.
    facts.push(fact("Container", undefined, containerName(newDoc, after.parentId), false));
  } else if (before?.parentId !== undefined) {
    facts.push(fact("Container", containerName(oldDoc, before.parentId), undefined, false));
  }
  return facts.filter((f): f is FlowFact => f !== null);
}

/** The two endpoints of an edge, in an order that ignores direction. */
const edgeKey = (e: FlowEdge) =>
  e.from < e.to ? `${e.from} ${e.to}` : `${e.to} ${e.from}`;

/** FLO-FR-15: `blocks` reads from the blocking item; `relates` has no order. */
function edgeTitle(edge: FlowEdge | null, doc: FlowDocument): string | null {
  if (!edge) return null;
  const name = (id: string) => doc.items.find((i) => i.id === id)?.name || id;
  return `${name(edge.from)} ${edge.type === "blocks" ? "→" : "—"} ${name(edge.to)}`;
}

/** DFV-FR-RGBK: an edge's type and, for a `blocks` edge, its direction. */
function edgeFacts(
  before: FlowEdge | null,
  after: FlowEdge | null,
  oldDoc: FlowDocument,
  newDoc: FlowDocument,
): FlowFact[] {
  if (!before || !after) {
    const f = fact("Type", before?.type, after?.type, false);
    return f ? [f] : [];
  }
  const facts: FlowFact[] = [];
  if (before.type !== after.type) {
    facts.push({ label: "Type", status: "changed", before: before.type, after: after.type });
  }
  // A `relates` edge has no direction; a reversed `blocks` edge is a change.
  if (before.type === "blocks" && after.type === "blocks" && before.from !== after.from) {
    facts.push({
      label: "Direction",
      status: "changed",
      before: edgeTitle(before, oldDoc)!,
      after: edgeTitle(after, newDoc)!,
    });
  }
  return facts;
}

function statusOf(
  hasBefore: boolean,
  hasAfter: boolean,
  facts: readonly FlowFact[],
): FlowChangeStatus {
  if (!hasBefore) return "added";
  if (!hasAfter) return "removed";
  return facts.some((f) => f.status !== "unchanged") ? "changed" : "unchanged";
}

/**
 * DFV-FR-33: why a revision cannot be compared by identity, or null. The
 * comparison pairs items by id and edges by their two endpoints, so two items
 * with one id (FGV-FR-09) or two edges on one pair of items (FGV-FR-11) make an
 * invalid revision, which renders the inline state, not a wrong pairing.
 */
function pairingError(doc: FlowDocument): string | null {
  const itemIds = new Set<string>();
  for (const i of doc.items) {
    if (itemIds.has(i.id)) return `more than one item carries the id "${i.id}"`;
    itemIds.add(i.id);
  }
  const pairs = new Set<string>();
  for (const e of doc.edges) {
    const key = edgeKey(e);
    if (pairs.has(key)) return `more than one edge joins "${e.from}" and "${e.to}"`;
    pairs.add(key);
  }
  return null;
}

/**
 * DFV-FR-33: the two revisions as the plans they describe. A missing revision
 * (the comparison added or deleted the file) reads as an empty plan, so an
 * added file reads as every part of it added. Sync bases are not compared
 * (DFV-FR-RGBK): a sync that only refreshed them changed no plan.
 */
export function flowDiff(
  oldBody: string | null,
  newBody: string | null,
): FlowDiffResult {
  const before = parseFlowDocument(oldBody ?? "");
  if (!before.ok) return { ok: false, error: `old revision: ${before.error}` };
  const after = parseFlowDocument(newBody ?? "");
  if (!after.ok) return { ok: false, error: `new revision: ${after.error}` };
  const oldDoc = before.doc;
  const newDoc = after.doc;
  const oldError = pairingError(oldDoc);
  if (oldError) return { ok: false, error: `old revision: ${oldError}` };
  const newError = pairingError(newDoc);
  if (newError) return { ok: false, error: `new revision: ${newError}` };
  const entries: FlowDiffEntry[] = [];

  // The Flow's own fields come first: they say what the plan is. A revision
  // the comparison added or deleted has no Flow, so this entry is added or
  // removed with the file.
  const hadFlow = oldBody !== null;
  const hasFlow = newBody !== null;
  const paired = hadFlow && hasFlow;
  const oldMeta = hadFlow ? oldDoc : null;
  const newMeta = hasFlow ? newDoc : null;
  const flowFacts = [
    fact("Name", oldMeta?.name || undefined, newMeta?.name || undefined, paired),
    fact("Description", oldMeta?.description, newMeta?.description, paired),
    ...listFacts("Reference", oldMeta?.references, newMeta?.references, paired),
  ].filter((f): f is FlowFact => f !== null);
  entries.push({
    key: "flow",
    kind: "flow",
    status: statusOf(hadFlow, hasFlow, flowFacts),
    titleBefore: hadFlow ? "Flow" : null,
    titleAfter: hasFlow ? "Flow" : null,
    facts: flowFacts,
  });

  for (const pair of pairByKey(oldDoc.items, newDoc.items, (i) => i.id)) {
    const facts = itemFacts(pair.old, pair.new, oldDoc, newDoc);
    entries.push({
      key: `item:${(pair.new ?? pair.old)!.id}`,
      kind: "item",
      ...(pair.old ? { itemKindBefore: pair.old.kind } : {}),
      ...(pair.new ? { itemKindAfter: pair.new.kind } : {}),
      status: statusOf(pair.old !== null, pair.new !== null, facts),
      titleBefore: pair.old ? pair.old.name : null,
      titleAfter: pair.new ? pair.new.name : null,
      facts,
    });
  }

  for (const pair of pairByKey(oldDoc.edges, newDoc.edges, edgeKey)) {
    const facts = edgeFacts(pair.old, pair.new, oldDoc, newDoc);
    entries.push({
      key: `edge:${edgeKey((pair.new ?? pair.old)!)}`,
      kind: "edge",
      status: statusOf(pair.old !== null, pair.new !== null, facts),
      titleBefore: edgeTitle(pair.old, oldDoc),
      titleAfter: edgeTitle(pair.new, newDoc),
      facts,
    });
  }

  return { ok: true, entries };
}

/** The entries a mode that shows only change renders (DFV-FR-34). */
export function changedOnly(entries: FlowDiffEntry[]): FlowDiffEntry[] {
  return entries.filter((e) => e.status !== "unchanged");
}

/** The facts that mode shows in one of them: the ones that differ. */
export function changedFacts(entry: FlowDiffEntry): FlowFact[] {
  return entry.status === "changed"
    ? entry.facts.filter((f) => f.status !== "unchanged")
    : entry.facts;
}
