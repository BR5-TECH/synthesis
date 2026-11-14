/**
 * MDL-FR-GDUP: the roll-up of the toolbar of the Delivery lens: a 170 × 8px
 * bar of the leaf states and the counts line.
 */
import { isContainerKind, type FlowDocument } from "../../state/flowDocument";
import type { FlowItemState, ItemDeliveryState } from "../../types";

/** MDL-FR-GDUP: the order of the segments of the bar. */
export const ROLLUP_ORDER: readonly FlowItemState[] = [
  "done",
  "in_progress",
  "drafting",
  "not_started",
  "dropped",
];

const WORDS: Record<FlowItemState, string> = {
  done: "done",
  in_progress: "in progress",
  drafting: "drafting",
  not_started: "not started",
  dropped: "dropped",
};

export interface LeafRollup {
  counts: Record<FlowItemState, number>;
  leaves: number;
  blocked: number;
}

/** MDL-FR-GDUP: the leaves of the plan by state, and the blocked leaves. */
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
  for (const item of doc.items) {
    if (isContainerKind(item.kind)) continue;
    const state = states.get(item.id);
    if (!state) continue;
    leaves += 1;
    counts[state.state] += 1;
    if (state.blocked) blocked += 1;
  }
  return { counts, leaves, blocked };
}

export function Rollup({ rollup }: { rollup: LeafRollup | null }) {
  if (!rollup) return null;
  const { counts, leaves, blocked } = rollup;
  const label = ROLLUP_ORDER.map((s) => `${counts[s]} ${WORDS[s]}`).join(", ");
  return (
    <>
      <span className="smap-toolbar__divider" aria-hidden="true" />
      <span className="dlens-bar" role="img" aria-label={label} data-testid="delivery-rollup-bar">
        {ROLLUP_ORDER.map((state) => (
          <span
            key={state}
            className="dlens-bar__segment"
            data-state={state}
            style={{ width: `${leaves > 0 ? (counts[state] / leaves) * 100 : 0}%` }}
          />
        ))}
      </span>
      <span className="smap-toolbar__counts" data-testid="delivery-rollup-counts">
        {counts.done} done · {counts.in_progress} in progress · {blocked} blocked
      </span>
    </>
  );
}
