/**
 * The notice lines of a GitHub plan (`MDL-map-delivery-lens.md`): the stale
 * read (MDL-FR-LSKL), the unmapped Types (MDL-FR-IEWL), and the misplaced
 * items and omitted relations (MDL-FR-MWMO). Each is one line below the band,
 * and none covers the canvas.
 */
import { useState } from "react";

import type { GithubDeliveryPlan, TrackerFacts } from "../../types/delivery";
import { Icon } from "../icons";

/** MDL-FR-KKYH / MDL-FR-LSKL: the time of a read, in words. */
export function readTimeText(iso: string, now: number): string {
  const at = Date.parse(iso);
  if (Number.isNaN(at)) return "read at an unknown time";
  const minutes = Math.floor((now - at) / 60_000);
  if (minutes < 1) return "read just now";
  if (minutes < 60) return `read ${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `read ${hours} h ago`;
  return `read ${new Date(at).toISOString().slice(0, 10)}`;
}

/** The issue or milestone number of an item, for a notice. */
function numberOf(tracker: ReadonlyMap<string, TrackerFacts>, itemId: string): string {
  const t = tracker.get(itemId);
  if (!t) return itemId;
  return t.ref.kind === "milestone" ? `milestone ${t.ref.number}` : `#${t.ref.number}`;
}

/** MDL-FR-LSKL: a plan answered from the cache after a failed read. */
export function StaleNotice({
  answer,
  retrying,
  onRetry,
}: {
  answer: GithubDeliveryPlan;
  retrying: boolean;
  onRetry: () => void;
}) {
  return (
    <div className="smap-notice dlens-notice" data-testid="delivery-stale-notice">
      <Icon.Warning size={12} />
      <span role="status">
        This plan is from the last read ({readTimeText(answer.readAt, Date.now())}
        <span className="dlens-notice__time"> at {new Date(answer.readAt).toLocaleString()}</span>).
        GitHub could not be read: {answer.error?.message ?? "the read failed"}.
      </span>
      <button type="button" className="btn btn--sm dlens-notice__action" disabled={retrying} onClick={onRetry}>
        {retrying ? "Retrying…" : "Retry"}
      </button>
    </div>
  );
}

/** MDL-FR-IEWL: the Types the mapping gives no kind, with their issue counts. */
export function UnmappedNotice({
  types,
  onMapTypes,
}: {
  types: GithubDeliveryPlan["notices"]["unmappedTypes"];
  onMapTypes: () => void;
}) {
  const total = types.reduce((n, t) => n + t.count, 0);
  const list = types.map((t) => `${t.type ?? "No Type"} (${t.count})`).join(", ");
  return (
    <div className="smap-notice dlens-notice" data-testid="delivery-unmapped-notice">
      <Icon.Warning size={12} />
      <span role="status">
        {total} {total === 1 ? "issue has" : "issues have"} no kind: {list}
      </span>
      <button type="button" className="btn btn--sm dlens-notice__action" onClick={onMapTypes}>
        Map Types
      </button>
    </div>
  );
}

/** MDL-FR-MWMO: the misplaced items and the omitted relations. */
export function StructureNotice({
  notices,
  tracker,
}: {
  notices: GithubDeliveryPlan["notices"];
  tracker: ReadonlyMap<string, TrackerFacts>;
}) {
  const [open, setOpen] = useState(false);
  const misplaced = notices.misplaced.length;
  const omitted = notices.omittedRelations.length;
  const parts = [
    ...(misplaced > 0 ? [`${misplaced} ${misplaced === 1 ? "item is" : "items are"} misplaced`] : []),
    ...(omitted > 0 ? [`${omitted} ${omitted === 1 ? "relation is" : "relations are"} left out`] : []),
  ];
  return (
    <div className="smap-notice dlens-notice" data-testid="delivery-structure-notice">
      <Icon.Warning size={12} />
      <button
        type="button"
        className="dlens-notice__toggle"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        {parts.join(" · ")}
        <span aria-hidden="true"> {open ? "▾" : "▸"}</span>
      </button>
      {open && (
        <ul className="dlens-notice__list" data-testid="delivery-structure-list">
          {notices.misplaced.map((m) => (
            <li key={`m:${m.itemId}`}>
              {numberOf(tracker, m.itemId)} is under {numberOf(tracker, m.parentItemId)}, which cannot
              hold it, so it shows at the top level.
            </li>
          ))}
          {notices.omittedRelations.map((r) => (
            <li key={`r:${r.blockingItemId}:${r.blockedItemId}`}>
              {numberOf(tracker, r.blockingItemId)} blocks {numberOf(tracker, r.blockedItemId)}, but
              {r.reason === "lineage"
                ? " one holds the other, so the plan shows no edge."
                : " the relation closes a cycle, so the plan shows no edge."}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
