/**
 * The edit operations on items and on the Flow's own fields (FLO-FR-07 to
 * FLO-FR-14, FLO-FR-25, FLO-FR-YDWT, FLO-FR-45, FLO-FR-HGIY, FLO-FR-NRBV,
 * FLO-FR-RMVR, FLO-FR-ZEJU, FLO-FR-XAPK, FLO-FR-TZSU).
 *
 * Each operation returns a new document, or the same document when it changes
 * nothing, or a typed refusal (`result.ts`). None changes its input.
 */

import {
  absolutePosition,
  childrenOf,
  findItem,
  permitsParent,
  subtreeOf,
} from "./containment";
import { accept, refuse, type FlowEditResult } from "./result";
import { ITEM_ID_PREFIX, mintId, patchItem, without } from "./shared";
import {
  DEFAULT_CONTAINER_SIZES,
  DEFAULT_ITEM_NAMES,
  isContainerKind,
  ITEM_KINDS,
  PERMITTED_PARENTS,
  type FlowDocument,
  type FlowItem,
  type FlowItemKind,
  type FlowPosition,
} from "./types";

/** The kind of the container `parentId` names, or `null` for the top level. */
function parentKindOf(
  doc: FlowDocument,
  parentId: string | undefined,
): FlowItemKind | null {
  return parentId === undefined ? null : (findItem(doc, parentId)?.kind ?? null);
}

// ---------------------------------------------------------------------------
// Add and remove
// ---------------------------------------------------------------------------

/**
 * FLO-FR-07 / FLO-FR-42 / FLO-FR-OAOZ: place a new item of `kind`, with its
 * default name and, for a container kind, its default size.
 *
 * `position` is in canvas coordinates; it is stored in the coordinates of
 * `parentId` (FLO-FR-21). A container the kind does not permit refuses the
 * placement and names the containers the kind can join.
 */
export function addItem(
  doc: FlowDocument,
  kind: FlowItemKind,
  position: FlowPosition,
  parentId?: string,
): FlowEditResult<{ itemId: string }> {
  if (parentId !== undefined && !findItem(doc, parentId)) {
    return refuse("unknown_parent");
  }
  if (!permitsParent(kind, parentKindOf(doc, parentId))) {
    return refuse("invalid_parent_kind", { permitted: PERMITTED_PARENTS[kind] });
  }
  // The id of a removed item that keeps its sync link is not free: that link
  // is the record a sync reads as a local removal (FGV-FR-15).
  const taken = new Set([
    ...doc.items.map((i) => i.id),
    ...(doc.sync?.links.map((l) => l.itemId) ?? []),
  ]);
  const sameKind = doc.items.filter((i) => i.kind === kind).length;
  const id = mintId(ITEM_ID_PREFIX[kind], taken, sameKind + 1);
  const origin =
    parentId === undefined ? { x: 0, y: 0 } : absolutePosition(doc, parentId);
  const item: FlowItem = {
    id,
    kind,
    name: DEFAULT_ITEM_NAMES[kind],
    ...(parentId !== undefined ? { parentId } : {}),
    position: { x: position.x - origin.x, y: position.y - origin.y },
    ...(isContainerKind(kind)
      ? { size: { ...DEFAULT_CONTAINER_SIZES[kind as keyof typeof DEFAULT_CONTAINER_SIZES] } }
      : {}),
  };
  return { ok: true, doc: { ...doc, items: [...doc.items, item] }, itemId: id };
}

/**
 * FLO-FR-45: what a removal of `id` takes with it. `itemCount` counts the items
 * under it, not the item itself; `edgeCount` counts every edge of every item
 * that goes. A container with `itemCount` 0 is removed without a question.
 */
export function removalScope(
  doc: FlowDocument,
  id: string,
): { itemCount: number; edgeCount: number } {
  const gone = subtreeOf(doc, id);
  return {
    itemCount: Math.max(0, gone.size - 1),
    edgeCount: doc.edges.filter((e) => gone.has(e.from) || gone.has(e.to)).length,
  };
}

/**
 * FLO-FR-08 / FLO-FR-45: remove an item with every item under it, and every
 * edge of every item that goes. A sync link of a removed item stays: it is the
 * record a sync reads as a local removal (FGV-FR-15).
 */
export function removeItem(doc: FlowDocument, id: string): FlowDocument {
  const gone = subtreeOf(doc, id);
  if (gone.size === 0) return doc;
  return {
    ...doc,
    items: doc.items.filter((i) => !gone.has(i.id)),
    edges: doc.edges.filter((e) => !gone.has(e.from) && !gone.has(e.to)),
  };
}

// ---------------------------------------------------------------------------
// Name, description, references
// ---------------------------------------------------------------------------

/** FLO-FR-09: free text, not required to be unique. */
export function renameItem(doc: FlowDocument, id: string, name: string): FlowDocument {
  return patchItem(doc, id, (i) => (i.name === name ? i : { ...i, name }));
}

/** FLO-FR-13: Markdown. An empty description is absent. */
export function setItemDescription(
  doc: FlowDocument,
  id: string,
  description: string,
): FlowDocument {
  return patchItem(doc, id, (i) => {
    if ((i.description ?? "") === description) return i;
    const rest = without(i, "description");
    return description === "" ? rest : { ...rest, description };
  });
}

/** FLO-FR-10: add a reference after the others. A repeat changes nothing. */
export function addItemArtifact(
  doc: FlowDocument,
  id: string,
  artifactId: string,
): FlowDocument {
  if (artifactId === "") return doc;
  return patchItem(doc, id, (i) =>
    (i.artifactIds ?? []).includes(artifactId)
      ? i
      : { ...i, artifactIds: [...(i.artifactIds ?? []), artifactId] },
  );
}

/** FLO-FR-10: remove one reference. The last one removes the field. */
export function removeItemArtifact(
  doc: FlowDocument,
  id: string,
  artifactId: string,
): FlowDocument {
  return patchItem(doc, id, (i) => {
    if (!(i.artifactIds ?? []).includes(artifactId)) return i;
    const kept = (i.artifactIds ?? []).filter((a) => a !== artifactId);
    const rest = without(i, "artifactIds");
    return kept.length > 0 ? { ...rest, artifactIds: kept } : rest;
  });
}

/** FLO-FR-25: the Flow's name; always written, empty or not. */
export function setFlowName(doc: FlowDocument, name: string): FlowDocument {
  return doc.name === name ? doc : { ...doc, name };
}

/** FLO-FR-25: the Flow's description. An empty description is absent. */
export function setFlowDescription(
  doc: FlowDocument,
  description: string,
): FlowDocument {
  if ((doc.description ?? "") === description) return doc;
  const { description: _drop, ...rest } = doc;
  return description === "" ? rest : { ...rest, description };
}

/** FLO-FR-YDWT: add a Flow reference after the others. A repeat changes nothing. */
export function addFlowReference(doc: FlowDocument, artifactId: string): FlowDocument {
  if (artifactId === "" || doc.references.includes(artifactId)) return doc;
  return { ...doc, references: [...doc.references, artifactId] };
}

/** FLO-FR-YDWT: remove one Flow reference. */
export function removeFlowReference(doc: FlowDocument, artifactId: string): FlowDocument {
  if (!doc.references.includes(artifactId)) return doc;
  return { ...doc, references: doc.references.filter((r) => r !== artifactId) };
}

// ---------------------------------------------------------------------------
// Kind
// ---------------------------------------------------------------------------

/** The fields a kind change removes from an item (FGV-FR-MZIA). */
export type KindDroppedField = "draftId" | "footprint" | "due";

/**
 * FLO-FR-HGIY: the fields that a change of `id` to `kind` removes. The canvas
 * asks before it removes a draft link or a footprint.
 */
export function kindChangeDrops(
  doc: FlowDocument,
  id: string,
  kind: FlowItemKind,
): KindDroppedField[] {
  const item = findItem(doc, id);
  if (!item) return [];
  const out: KindDroppedField[] = [];
  if (isContainerKind(kind)) {
    if (item.draftId !== undefined) out.push("draftId");
    if (item.footprint !== undefined) out.push("footprint");
  }
  if (kind !== "milestone" && item.due !== undefined) out.push("due");
  return out;
}

/**
 * FLO-FR-HGIY: change an item's kind. Refused when the item's container does
 * not permit the kind, when a member cannot sit in it, when a container that
 * holds items would become a leaf, or when a sync link would join a milestone
 * to an issue (FGV-FR-LWPD). The fields the new kind does not carry are
 * removed (FGV-FR-MZIA); a leaf that becomes a container gets the default size
 * of its new kind.
 *
 * A kind change cannot make a dependency cycle: an empty container waits as a
 * leaf does (FGV-FR-PMTY), and only an empty container becomes a leaf.
 */
export function setItemKind(
  doc: FlowDocument,
  id: string,
  kind: FlowItemKind,
): FlowEditResult {
  const item = findItem(doc, id);
  if (!item) return refuse("unknown_item");
  if (item.kind === kind) return accept(doc);
  if (!permitsParent(kind, parentKindOf(doc, item.parentId))) {
    return refuse("invalid_parent_kind", { permitted: PERMITTED_PARENTS[kind] });
  }
  const members = childrenOf(doc, id);
  if (!isContainerKind(kind) && members.length > 0) {
    return refuse("not_empty", { itemIds: members.map((m) => m.id) });
  }
  const refused = members.filter((m) => !permitsParent(m.kind, kind));
  if (refused.length > 0) {
    return refuse("member_not_permitted", { itemIds: refused.map((m) => m.id) });
  }
  const link = doc.sync?.links.find((l) => l.itemId === id);
  if (link && (kind === "milestone") !== (link.ref.kind === "milestone")) {
    return refuse("tracker_kind_mismatch");
  }

  let next: FlowItem = { ...item, kind };
  if (isContainerKind(kind)) {
    next = without(next, "draftId", "footprint");
    if (!next.size) {
      const size = DEFAULT_CONTAINER_SIZES[kind as keyof typeof DEFAULT_CONTAINER_SIZES];
      next = { ...next, size: { ...size } };
    }
  } else {
    next = without(next, "size");
  }
  if (kind !== "milestone") next = without(next, "due");
  return accept(patchItem(doc, id, () => next));
}

/** FLO-FR-HGIY: the kinds `id` can change to, its own kind among them. */
export function allowedKinds(doc: FlowDocument, id: string): FlowItemKind[] {
  return ITEM_KINDS.filter((k) => setItemKind(doc, id, k).ok);
}

// ---------------------------------------------------------------------------
// Draft, footprint, due date, local only
// ---------------------------------------------------------------------------

/** FGV-FR-CTFX: the item that links `draftId`, if one does. */
export function itemLinkingDraft(
  doc: FlowDocument,
  draftId: string,
): string | undefined {
  return doc.items.find((i) => i.draftId === draftId)?.id;
}

/**
 * FLO-FR-NRBV / FLO-FR-RMVR: link a task or a defect to a draft, or unlink it
 * with `null`. A draft that another item links is refused (FGV-FR-CTFX), and
 * the refusal names that item.
 */
export function setItemDraft(
  doc: FlowDocument,
  id: string,
  draftId: string | null,
): FlowEditResult {
  const item = findItem(doc, id);
  if (!item) return refuse("unknown_item");
  if (draftId === null || draftId === "") {
    return accept(
      item.draftId === undefined ? doc : patchItem(doc, id, (i) => without(i, "draftId")),
    );
  }
  if (isContainerKind(item.kind)) return refuse("not_leaf");
  if (item.draftId === draftId) return accept(doc);
  const other = itemLinkingDraft(doc, draftId);
  if (other !== undefined) {
    return refuse("draft_already_linked", { itemIds: [other] });
  }
  return accept(patchItem(doc, id, (i) => ({ ...i, draftId })));
}

/**
 * FGV-FR-RQEM: a footprint entry is a project-relative path: not empty, not
 * absolute (a leading `/` or `\`, or a drive letter), and with no `..`
 * segment.
 */
export function isFootprintPath(path: string): boolean {
  if (path === "" || path.startsWith("/") || path.startsWith("\\")) return false;
  if (/^[A-Za-z]:/.test(path)) return false;
  return !path.split(/[/\\]/).some((segment) => segment === "..");
}

/** FLO-FR-ZEJU: add a path to a leaf's footprint. */
export function addFootprintEntry(
  doc: FlowDocument,
  id: string,
  path: string,
): FlowEditResult {
  const item = findItem(doc, id);
  if (!item) return refuse("unknown_item");
  if (isContainerKind(item.kind)) return refuse("not_leaf");
  if (!isFootprintPath(path)) return refuse("invalid_footprint_path");
  if ((item.footprint ?? []).includes(path)) return refuse("duplicate_footprint");
  return accept(
    patchItem(doc, id, (i) => ({ ...i, footprint: [...(i.footprint ?? []), path] })),
  );
}

/** FLO-FR-ZEJU: remove one path. The last one removes the field. */
export function removeFootprintEntry(
  doc: FlowDocument,
  id: string,
  path: string,
): FlowDocument {
  return patchItem(doc, id, (i) => {
    if (!(i.footprint ?? []).includes(path)) return i;
    const kept = (i.footprint ?? []).filter((p) => p !== path);
    const rest = without(i, "footprint");
    return kept.length > 0 ? { ...rest, footprint: kept } : rest;
  });
}

/** FGV-FR-RRND: a real calendar date in the form `YYYY-MM-DD`. */
export function isCalendarDate(s: string): boolean {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(s);
  if (!m) return false;
  const y = Number(m[1]);
  const month = Number(m[2]);
  const d = Number(m[3]);
  const leap = (y % 4 === 0 && y % 100 !== 0) || y % 400 === 0;
  const days = [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][month - 1];
  return days !== undefined && d >= 1 && d <= days;
}

/**
 * FLO-FR-XAPK: set a milestone's due date, or clear it with `null` or an empty
 * string.
 */
export function setDue(
  doc: FlowDocument,
  id: string,
  due: string | null,
): FlowEditResult {
  const item = findItem(doc, id);
  if (!item) return refuse("unknown_item");
  if (due === null || due === "") {
    return accept(
      item.due === undefined ? doc : patchItem(doc, id, (i) => without(i, "due")),
    );
  }
  if (item.kind !== "milestone") return refuse("not_milestone");
  if (!isCalendarDate(due)) return refuse("invalid_due_date");
  if (item.due === due) return accept(doc);
  return accept(patchItem(doc, id, (i) => ({ ...i, due })));
}

/** FLO-FR-TZSU: mark an item local only, or clear the mark (absent when off). */
export function setLocalOnly(doc: FlowDocument, id: string, on: boolean): FlowDocument {
  return patchItem(doc, id, (i) => {
    if (on) return i.localOnly === true ? i : { ...i, localOnly: true };
    return i.localOnly === undefined ? i : without(i, "localOnly");
  });
}
