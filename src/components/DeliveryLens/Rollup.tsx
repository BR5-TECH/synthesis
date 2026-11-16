/**
 * MDL-FR-GDUP: the roll-up of the toolbar of the Delivery lens: a 160 × 4px
 * progress track of the done, in progress, and blocked work items, and its
 * legend.
 */
import { isContainerKind, type FlowDocument } from "../../state/flowDocument";
import type { FlowItemState, ItemDeliveryState } from "../../types";

export interface LeafRollup {
  counts: Record<FlowItemState, number>;
  leaves: number;
  blocked: number;
  /** MDL-FR-GDUP: the segments of the track: a blocked item counts as blocked alone. */
  segments: { done: number; active: number; blocked: number };
}

/**
 * MDL-FR-GDUP / FDL-FR-KWMT: the work items of the plan by state, and the
 * blocked work items. A work item is a leaf, or a container that holds no item.
 */
export function leafRollup(
  doc: FlowDocument | null,
  states: ReadonlyMap<string, ItemDeliveryState> | null,
): LeafRollup | null {
  if (!doc || !states) return null;
  const counts: Record<FlowItemState, number> = {
    done: 0,
    in_progress: 0,
    drafting: 0,
    not_started: 0,
    dropped: 0,
  };
  let leaves = 0;
  let blocked = 0;
  const segments = { done: 0, active: 0, blocked: 0 };
  const holders = new Set(doc.items.map((i) => i.parentId).filter((p) => p !== undefined));
  for (const item of doc.items) {
    if (isContainerKind(item.kind) && holders.has(item.id)) continue;
    const state = states.get(item.id);
    if (!state) continue;
    leaves += 1;
    counts[state.state] += 1;
    if (state.blocked) blocked += 1;
    if (state.state === "done") segments.done += 1;
    else if (state.blocked) segments.blocked += 1;
    else if (state.state === "in_progress") segments.active += 1;
  }
  return { counts, leaves, blocked, segments };
}

const SEGMENTS = [
  { key: "done", dot: "done" },
  { key: "active", dot: "in progress" },
  { key: "blocked", dot: "blocked" },
] as const;

export function Rollup({ rollup }: { rollup: LeafRollup | null }) {
  if (!rollup) return null;
  const { counts, leaves, blocked, segments } = rollup;
  const label = `${counts.done} done, ${counts.in_progress} in progress, ${blocked} blocked`;
  const legend = { done: counts.done, active: counts.in_progress, blocked };
  const words = { done: "done", active: "in progress", blocked: "blocked" };
  return (
    <span className="dm-progress">
      <span className="dm-progress__track" role="img" aria-label={label} data-testid="delivery-rollup-bar">
        {SEGMENTS.map(({ key }) => (
          <span
            key={key}
            className="dm-progress__seg"
            data-k={key}
            style={{ width: `${leaves > 0 ? (segments[key] / leaves) * 100 : 0}%` }}
          />
        ))}
      </span>
      <span className="dm-progress__legend" data-testid="delivery-rollup-counts">
        {SEGMENTS.map(({ key, dot }) => (
          <span key={key}>
            <i className="dm-dot" data-s={dot} aria-hidden="true" />
            <b>{legend[key]}</b> {words[key]}
          </span>
        ))}
      </span>
    </span>
  );
}
