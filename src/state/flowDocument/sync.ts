/**
 * The edits a tracker connection makes to the document (FLO-FR-AKMV,
 * FLO-FR-TYEZ, FLO-FR-ULEM). The sync itself is the backend's
 * (`FTS-flow-tracker-sync.md`).
 */

import { findItem } from "./containment";
import { accept, refuse, type FlowEditResult } from "./result";
import { patchItem, without } from "./shared";
import type { FlowDocument, SyncBase, SyncLink, TrackerRef } from "./types";

/** FLO-FR-AKMV: add the sync section. A Flow that has one is unchanged. */
export function connectSync(doc: FlowDocument): FlowDocument {
  if (doc.sync) return doc;
  return { ...doc, sync: { tracker: "github", links: [] } };
}

/** FLO-FR-ULEM: the sync link of an item, if it has one. */
export function syncLinkOf(doc: FlowDocument, itemId: string): SyncLink | undefined {
  return doc.sync?.links.find((l) => l.itemId === itemId);
}

/** FGV-FR-LWPD: two references name the same tracker item. */
export function sameTrackerRef(a: TrackerRef, b: TrackerRef): boolean {
  return (
    a.kind === b.kind &&
    a.host === b.host &&
    a.owner === b.owner &&
    a.repo === b.repo &&
    a.number === b.number
  );
}

/**
 * FLO-FR-TYEZ: link an item to an existing tracker item. The item takes the
 * base's name and description (an empty description is absent), and the link
 * stores the reference with that base.
 *
 * Refused when the Flow has no sync section, when the item already has a link,
 * when another item links the same reference (the refusal names it), and when
 * a milestone would link an issue or the reverse (FGV-FR-LWPD).
 */
export function setSyncLink(
  doc: FlowDocument,
  itemId: string,
  ref: TrackerRef,
  base: SyncBase,
): FlowEditResult {
  const item = findItem(doc, itemId);
  if (!item) return refuse("unknown_item");
  if (!doc.sync) return refuse("not_synced");
  if (syncLinkOf(doc, itemId)) return refuse("already_linked");
  const other = doc.sync.links.find((l) => sameTrackerRef(l.ref, ref));
  if (other) return refuse("duplicate_tracker_link", { itemIds: [other.itemId] });
  if ((item.kind === "milestone") !== (ref.kind === "milestone")) {
    return refuse("tracker_kind_mismatch");
  }
  const named = patchItem(doc, itemId, (i) => {
    const rest = without(i, "description");
    return {
      ...rest,
      name: base.name,
      ...(base.description !== "" ? { description: base.description } : {}),
    };
  });
  // FGV-FR-TLMV: a reference is never both linked and ignored, so a link to
  // an ignored tracker item takes it off the ignored list.
  const ignored = (doc.sync.ignored ?? []).filter((r) => !sameTrackerRef(r, ref));
  return accept({
    ...named,
    sync: {
      tracker: doc.sync.tracker,
      links: [...doc.sync.links, { itemId, ref, base }],
      ...(ignored.length > 0 ? { ignored } : {}),
    },
  });
}
