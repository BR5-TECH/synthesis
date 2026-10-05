/**
 * The effective difference between two revisions of a Flow
 * (`DFV-diff-viewer.md` DFV-FR-33 – DFV-FR-35).
 *
 * A Flow is JSON, so its source diff is a diff of braces: a node dragged three
 * pixels rewrites the same lines a node deleted does, and "which prompt does
 * this step run now" is not a question a line diff can answer. This module
 * answers it — it compares the two revisions as the graphs they describe and
 * reports what changed about the Flow: its name and description, its nodes,
 * what each node references and prompts, and the connections between them.
 *
 * Pure, and free of React: the same entries feed all three visualization modes,
 * which differ only in which of them they show and on which side.
 */
import {
  parseFlowDocument,
  type FlowDocument,
  type FlowEdge,
  type FlowLoop,
  type FlowNode,
} from "../state/flowDocument";

/** How an entry, or one fact within it, stands between the two revisions. */
export type FlowChangeStatus = "added" | "removed" | "changed" | "unchanged";

/**
 * One statement about an entry — an artifact it references, its inline prompt,
 * an edge's label — with what it was and what it became.
 *
 * A fact of an added or removed entry is `unchanged`: the entry's own marking
 * already says the whole of it is new or gone, and marking every part of it
 * repeats that at higher contrast (the reasoning of DFV-FR-32).
 */
export interface FlowFact {
  label: string;
  status: FlowChangeStatus;
  before?: string;
  after?: string;
}

/** One thing the diff has something to say about. */
export interface FlowDiffEntry {
  /** Stable within a diff — the element id, the edge's endpoints, or `flow`. */
  key: string;
  kind: "flow" | "node" | "loop" | "edge";
  status: FlowChangeStatus;
  /** What it is called in the old revision; null when it had none. */
  titleBefore: string | null;
  /** What it is called in the new revision; null when it has none. */
  titleAfter: string | null;
  facts: FlowFact[];
}

export type FlowDiffResult =
  | { ok: true; entries: FlowDiffEntry[] }
  | { ok: false; error: string };

/**
 * A revision's Flow. A missing revision — the comparison added or deleted the
 * file — is an empty Flow, so an added file reads as every part of it added and
 * a deleted one as every part removed, which is what the two revisions say.
 */
function revisionDoc(
  body: string | null,
): { ok: true; doc: FlowDocument } | { ok: false; error: string } {
  return parseFlowDocument(body ?? "");
}

/**
 * Pair two revisions' items by identity, preserving both orders.
 *
 * Deliberately not the line pairing of `lineAlign`: that reads a removal
 * opposite an addition as a *replacement*, which is right for lines and wrong
 * here — a node deleted and a node added are two nodes, not one that changed,
 * and pairing them would report the wrong thing about both. An item pairs only
 * with the item carrying its own key, and one with no counterpart faces null.
 *
 * Order is the new revision's, with each removed item emitted at the point its
 * old neighbours put it, so a Flow reads in the order its author left it.
 */
function pairByKey<T>(
  a: readonly T[],
  b: readonly T[],
  key: (item: T) => string,
): Array<{ old: T | null; new: T | null }> {
  const rows: Array<{ old: T | null; new: T | null }> = [];
  // Position by key rather than a forward-only cursor: the two revisions are
  // arbitrary Git blobs, and a hand edit or a merge can list the same nodes in
  // a different order. Walking the old list in step with the new one would run
  // past a reordered item and pair it with nothing — reporting a node that is
  // present in both as added, and losing every change it carries.
  const oldIndex = new Map<string, number>();
  a.forEach((item, i) => oldIndex.set(key(item), i));
  const newKeys = new Set(b.map(key));

  let cursor = 0;
  /** Emit the removed items sitting before `stop` in the old revision. */
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

const position = (n: { position: { x: number; y: number } }) =>
  `${n.position.x}, ${n.position.y}`;

/**
 * A fact of an entry that exists in one revision only. It is unmarked — the
 * entry's own marking already says the whole of it is new or gone — and its
 * value belongs to the side the entry is on, which is the side that will render
 * it (DFV-FR-35).
 */
function ownFact(
  label: string,
  value: string | undefined,
  side: "before" | "after",
): FlowFact | null {
  if (value === undefined) return null;
  return {
    label,
    status: "unchanged",
    ...(side === "before" ? { before: value } : { after: value }),
  };
}

/** A fact present on both sides, marked only where the two disagree. */
function compare(
  label: string,
  before: string | undefined,
  after: string | undefined,
): FlowFact | null {
  if (before === after) {
    return before === undefined
      ? null
      : { label, status: "unchanged", before, after };
  }
  if (before === undefined) return { label, status: "added", after };
  if (after === undefined) return { label, status: "removed", before };
  return { label, status: "changed", before, after };
}

/**
 * Every artifact either side references, in new-revision order first. Read off
 * the field rather than off a node, a loop carrying references on exactly the
 * same terms (`FLO-flow.md` FLO-FR-37).
 */
function referenceFacts(
  before: { artifactIds?: string[] } | null,
  after: { artifactIds?: string[] } | null,
): FlowFact[] {
  const oldIds = before?.artifactIds ?? [];
  const newIds = after?.artifactIds ?? [];
  const paired = before && after;
  const seen = new Set<string>();
  const facts: FlowFact[] = [];
  for (const id of [...newIds, ...oldIds]) {
    if (seen.has(id)) continue;
    seen.add(id);
    const inOld = oldIds.includes(id);
    const inNew = newIds.includes(id);
    // Only an element present in both revisions can have gained or lost a
    // reference; on one that is itself new or gone, every reference is simply
    // part of it.
    const status: FlowChangeStatus =
      !paired || (inOld && inNew) ? "unchanged" : inNew ? "added" : "removed";
    facts.push({
      label: "Artifact",
      status,
      ...(inOld ? { before: id } : {}),
      ...(inNew ? { after: id } : {}),
    });
  }
  return facts;
}

/**
 * DFV-FR-33: which container an element sits in, named by the loop it left and
 * the one it joined.
 *
 * Reported ahead of any position fact because it changes what the step is part
 * of and what it may connect to (`FLO-flow.md` FLO-FR-40), where a position
 * changes only where it is drawn. A loop is named by its own name where the
 * revision holding it has one, so the entry reads as "moved into Review cycle"
 * rather than as a pair of ids.
 */
function containerFacts(
  before: { parentId?: string } | null,
  after: { parentId?: string } | null,
  oldDoc: FlowDocument,
  newDoc: FlowDocument,
): FlowFact[] {
  const name = (doc: FlowDocument, id: string | undefined) =>
    id === undefined
      ? "(top level)"
      : doc.loops.find((l) => l.id === id)?.name || id;
  if (before && after) {
    if (before.parentId === after.parentId) return [];
    return [
      {
        label: "Container",
        status: "changed",
        before: name(oldDoc, before.parentId),
        after: name(newDoc, after.parentId),
      },
    ];
  }
  // On an element that is itself new or gone, its container is simply part of
  // it — unmarked, and stated only where it is not the top level.
  const only = after ?? before;
  if (!only?.parentId) return [];
  const side = after ? "after" : "before";
  return [
    {
      label: "Container",
      status: "unchanged",
      ...(side === "after"
        ? { after: name(newDoc, only.parentId) }
        : { before: name(oldDoc, only.parentId) }),
    },
  ];
}

function nodeFacts(
  before: FlowNode | null,
  after: FlowNode | null,
  oldDoc: FlowDocument,
  newDoc: FlowDocument,
): FlowFact[] {
  const paired = before && after;
  const facts: FlowFact[] = [];
  if (paired && before.name !== after.name) {
    facts.push({
      label: "Name",
      status: "changed",
      before: before.name,
      after: after.name,
    });
  }
  facts.push(...referenceFacts(before, after));
  const prompt = paired
    ? compare("Prompt", before.prompt, after.prompt)
    : after
      ? ownFact("Prompt", after.prompt, "after")
      : ownFact("Prompt", before?.prompt, "before");
  if (prompt) facts.push(prompt);
  facts.push(...containerFacts(before, after, oldDoc, newDoc));
  // A node the author only dragged still changed the file, so the diff says so
  // — last, because where a step sits says the least about what it does.
  if (paired && position(before) !== position(after)) {
    facts.push({
      label: "Position",
      status: "changed",
      before: position(before),
      after: position(after),
    });
  }
  return facts;
}

const size = (l: FlowLoop) => `${l.size.width} × ${l.size.height}`;

/**
 * DFV-FR-33: what a loop's entry says. Its own fields first — the name, the
 * artifacts it references, and the pass bound, which are what the loop is for —
 * then what it holds, then where it sits and how big it is, which say the least.
 *
 * What it holds is stated by naming its members rather than by nesting their
 * entries inside it, so every element of the Flow is one entry at one level
 * however deeply the loops nest.
 */
function loopFacts(
  before: FlowLoop | null,
  after: FlowLoop | null,
  oldDoc: FlowDocument,
  newDoc: FlowDocument,
): FlowFact[] {
  const paired = before && after;
  const facts: FlowFact[] = [];
  if (paired && before.name !== after.name) {
    facts.push({
      label: "Name",
      status: "changed",
      before: before.name,
      after: after.name,
    });
  }
  facts.push(...referenceFacts(before, after));
  const passes = (l: FlowLoop | null | undefined) =>
    l?.maxPasses === undefined ? undefined : String(l.maxPasses);
  const maxPasses = paired
    ? compare("Max passes", passes(before), passes(after))
    : after
      ? ownFact("Max passes", passes(after), "after")
      : ownFact("Max passes", passes(before), "before");
  if (maxPasses) facts.push(maxPasses);

  const members = (doc: FlowDocument, id: string) =>
    [...doc.loops, ...doc.nodes]
      .filter((e) => e.parentId === id)
      .map((e) => e.name || e.id)
      .join(", ") || undefined;
  const holds = paired
    ? compare("Holds", members(oldDoc, before.id), members(newDoc, after.id))
    : after
      ? ownFact("Holds", members(newDoc, after.id), "after")
      : ownFact("Holds", before ? members(oldDoc, before.id) : undefined, "before");
  if (holds) facts.push(holds);

  facts.push(...containerFacts(before, after, oldDoc, newDoc));
  if (paired && position(before) !== position(after)) {
    facts.push({
      label: "Position",
      status: "changed",
      before: position(before),
      after: position(after),
    });
  }
  if (paired && size(before) !== size(after)) {
    facts.push({
      label: "Size",
      status: "changed",
      before: size(before),
      after: size(after),
    });
  }
  return facts;
}

function edgeTitle(
  edge: FlowEdge | null,
  doc: FlowDocument | null,
): string | null {
  if (!edge || !doc) return null;
  // Either endpoint may be a loop (`FLO-flow.md` FLO-FR-39), so the name is
  // looked up across every element rather than among the nodes alone.
  const name = (id: string) =>
    [...doc.loops, ...doc.nodes].find((e) => e.id === id)?.name || id;
  return `${name(edge.from)} → ${name(edge.to)}`;
}

function statusOf(
  before: unknown | null,
  after: unknown | null,
  changed: boolean,
): FlowChangeStatus {
  if (!before) return "added";
  if (!after) return "removed";
  return changed ? "changed" : "unchanged";
}

/**
 * DFV-FR-33: the two revisions as the Flows they describe.
 *
 * Nodes pair by id, which FLO-FR-06 makes stable across saves, so a renamed node
 * is one node that changed rather than one added and one removed. Edges pair by
 * their endpoints, because that — not the edge's id — is what a reader means by
 * "the same connection".
 */
export function flowDiff(
  oldBody: string | null,
  newBody: string | null,
): FlowDiffResult {
  const before = revisionDoc(oldBody);
  if (!before.ok) return { ok: false, error: `old revision: ${before.error}` };
  const after = revisionDoc(newBody);
  if (!after.ok) return { ok: false, error: `new revision: ${after.error}` };
  const oldDoc = before.doc;
  const newDoc = after.doc;

  const entries: FlowDiffEntry[] = [];

  // The Flow's own fields (FLO-FR-25), first: they say what the thing is. A
  // revision the comparison added or deleted has no Flow at all, so this entry
  // is added or removed with it rather than sitting on both sides of a file
  // that exists on one.
  const hadFlow = oldBody !== null;
  const hasFlow = newBody !== null;
  const metaFacts =
    hadFlow && hasFlow
      ? [
          compare("Name", oldDoc.name || undefined, newDoc.name || undefined),
          compare("Description", oldDoc.description, newDoc.description),
        ].filter((f): f is FlowFact => f !== null)
      : [
          ownFact(
            "Name",
            (hasFlow ? newDoc.name : oldDoc.name) || undefined,
            hasFlow ? "after" : "before",
          ),
          ownFact(
            "Description",
            hasFlow ? newDoc.description : oldDoc.description,
            hasFlow ? "after" : "before",
          ),
        ].filter((f): f is FlowFact => f !== null);
  entries.push({
    key: "flow",
    kind: "flow",
    status: statusOf(
      hadFlow || null,
      hasFlow || null,
      metaFacts.some((f) => f.status !== "unchanged"),
    ),
    titleBefore: hadFlow ? "Flow" : null,
    titleAfter: hasFlow ? "Flow" : null,
    facts: metaFacts,
  });

  // Loops before nodes: a container reads ahead of what it holds, and it is what
  // a reader needs in hand to make sense of a node's Container fact.
  for (const pair of pairByKey(oldDoc.loops, newDoc.loops, (l) => l.id)) {
    const facts = loopFacts(pair.old, pair.new, oldDoc, newDoc);
    entries.push({
      key: `loop:${pair.new?.id ?? pair.old?.id}`,
      kind: "loop",
      status: statusOf(
        pair.old,
        pair.new,
        facts.some((f) => f.status !== "unchanged"),
      ),
      titleBefore: pair.old?.name ?? null,
      titleAfter: pair.new?.name ?? null,
      facts,
    });
  }

  for (const pair of pairByKey(oldDoc.nodes, newDoc.nodes, (n) => n.id)) {
    const facts = nodeFacts(pair.old, pair.new, oldDoc, newDoc);
    entries.push({
      key: `node:${pair.new?.id ?? pair.old?.id}`,
      kind: "node",
      status: statusOf(
        pair.old,
        pair.new,
        facts.some((f) => f.status !== "unchanged"),
      ),
      titleBefore: pair.old?.name ?? null,
      titleAfter: pair.new?.name ?? null,
      facts,
    });
  }

  for (const pair of pairByKey(
    oldDoc.edges,
    newDoc.edges,
    (e) => `${e.from}\0${e.to}`,
  )) {
    const label =
      pair.old && pair.new
        ? compare("Label", pair.old.label, pair.new.label)
        : pair.new
          ? ownFact("Label", pair.new.label, "after")
          : ownFact("Label", pair.old?.label, "before");
    const facts = label ? [label] : [];
    entries.push({
      key: `edge:${pair.new?.from ?? pair.old?.from} ${pair.new?.to ?? pair.old?.to}`,
      kind: "edge",
      status: statusOf(
        pair.old,
        pair.new,
        facts.some((f) => f.status !== "unchanged"),
      ),
      titleBefore: edgeTitle(pair.old, oldDoc),
      titleAfter: edgeTitle(pair.new, newDoc),
      facts,
    });
  }

  return { ok: true, entries };
}

/** The entries a mode showing only change renders (DFV-FR-34). */
export function changedOnly(entries: FlowDiffEntry[]): FlowDiffEntry[] {
  return entries.filter((e) => e.status !== "unchanged");
}

/** The facts that mode shows within one of them: the ones that differ. */
export function changedFacts(entry: FlowDiffEntry): FlowFact[] {
  return entry.status === "changed"
    ? entry.facts.filter((f) => f.status !== "unchanged")
    : entry.facts;
}
