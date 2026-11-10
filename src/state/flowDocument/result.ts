/**
 * What an edit operation returns: a new document, or a typed refusal that says
 * why the plan stays as it was (FLO-FR-16, FLO-FR-OAOZ, FLO-FR-GZCH).
 *
 * An operation never changes the document it is given. When it changes
 * nothing, it returns the same object, so the session store does not raise the
 * unsaved state for an edit that did nothing (FLO-FR-26).
 */

import type { FlowDocument, FlowItemKind } from "./types";

export type FlowRefusalCode =
  /** No item has the given id. */
  | "unknown_item"
  /** No edge has the given id. */
  | "unknown_edge"
  /** The container to place an item in is not an item of the plan. */
  | "unknown_parent"
  /** FGV-FR-YHFX: the container does not permit the kind. */
  | "invalid_parent_kind"
  /** FLO-FR-HGIY: a member of the item cannot sit in the new kind. */
  | "member_not_permitted"
  /** FLO-FR-HGIY: a container that holds items cannot become a leaf. */
  | "not_empty"
  /** An item cannot go into itself or into an item under it (FGV-FR-13). */
  | "into_own_subtree"
  /** FLO-FR-16: an edge from an item to the same item. */
  | "self"
  /** FLO-FR-17: an edge already joins the two items. */
  | "duplicate_edge"
  /** FLO-FR-ZEPI, FLO-FR-OAOZ: an edge would join an item and its ancestor. */
  | "edge_within_lineage"
  /** FLO-FR-GZCH: the change would give the plan a dependency cycle. */
  | "dependency_cycle"
  /** FLO-FR-CEVI: only a `blocks` edge has a direction to reverse. */
  | "not_blocks"
  /** A draft link or a footprint on a container (FGV-FR-MZIA). */
  | "not_leaf"
  /** A due date on an item that is not a milestone (FGV-FR-MZIA). */
  | "not_milestone"
  /** FGV-FR-CTFX: another item already links the draft. */
  | "draft_already_linked"
  /** FGV-FR-RQEM: an empty, absolute, or `..` footprint path. */
  | "invalid_footprint_path"
  /** FGV-FR-RQEM: the item's footprint already holds the path. */
  | "duplicate_footprint"
  /** FGV-FR-RRND: not a calendar date `YYYY-MM-DD`. */
  | "invalid_due_date"
  /** FLO-FR-TYEZ: the Flow has no sync section. */
  | "not_synced"
  /** FLO-FR-TYEZ: the item already has a sync link. */
  | "already_linked"
  /** FGV-FR-LWPD: another item already links the tracker reference. */
  | "duplicate_tracker_link"
  /** FGV-FR-LWPD: a milestone and an issue, or the reverse. */
  | "tracker_kind_mismatch";

export interface FlowRefusal {
  code: FlowRefusalCode;
  /**
   * The items the refusal names: the items of a cycle (FLO-FR-GZCH), the
   * members a kind does not permit, or the item that already links a draft or
   * a tracker reference.
   */
  itemIds?: string[];
  /** The edges the refusal names: an existing edge, or a lineage edge. */
  edgeIds?: string[];
  /** FLO-FR-OAOZ: the containers the kind can join; `null` is the top level. */
  permitted?: readonly (FlowItemKind | null)[];
}

export type FlowEditResult<Extra extends object = object> =
  | ({ ok: true; doc: FlowDocument } & Extra)
  | { ok: false; refusal: FlowRefusal };

export function refuse(
  code: FlowRefusalCode,
  details: Omit<FlowRefusal, "code"> = {},
): { ok: false; refusal: FlowRefusal } {
  return { ok: false, refusal: { code, ...details } };
}

export function accept(doc: FlowDocument): { ok: true; doc: FlowDocument } {
  return { ok: true, doc };
}

/**
 * The document an edit gives, or `doc` itself when the edit was refused. For
 * a caller such as `FlowSessionStore.applyEdit`, which wants a document.
 */
export function docOrUnchanged(
  doc: FlowDocument,
  result: FlowEditResult,
): FlowDocument {
  return result.ok ? result.doc : doc;
}
