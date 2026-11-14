/**
 * What the Specifications lens shows of the delivery
 * (`SDL-specification-delivery-links.md`): the **Delivered by** list
 * (SDL-FR-VAGU, SDL-FR-MCGI), **Plan work** (SDL-FR-CFHH), the planned marks
 * and the **unplanned** toggle (SDL-FR-SGQM .. SDL-FR-VUQH). Nothing of it
 * shows while the delivery source is `none` (SDL-FR-CKAA).
 */
import { useMemo, useSyncExternalStore } from "react";

import { logInfo } from "../../logging";
import {
  deriveSpecLinks,
  itemsLinking,
  NO_LINKS,
  planWorkFiles,
  type SpecLinks,
} from "../../state/delivery/specLinks";
import { findItem } from "../../state/flowDocument";
import type { SpecMapSessionStore } from "../../state/specMap/session";
import { roleOf } from "../../state/specMap/tree";
import type { MapRef } from "../../state/specMap/types";
import { StatusChip } from "../Flow/chips";
import { KIND_GLYPHS } from "../Flow/kindGlyphs";
import { useDeliveryState } from "../Flow/useDeliveryState";
import type { InspectorSlots } from "../SpecMap/Inspector";
import type { UnplannedToggle } from "../SpecMap/Toolbar";
import type { DeliveryHost } from "./host";
import { useDeliveryPlan } from "./useDeliveryPlan";

export interface SpecLinkView {
  /** SDL-FR-SGQM / SDL-FR-KXCY: the links, or null while the source is `none`. */
  links: SpecLinks | null;
  slots: InspectorSlots | undefined;
  unplanned: UnplannedToggle | undefined;
}

/**
 * The delivery side of the Specifications lens. `active` is true while that
 * lens shows, so the plan loads only then (SDL-FR-VYIF).
 */
export function useSpecLinkView(
  store: SpecMapSessionStore,
  host: DeliveryHost,
  active: boolean,
): SpecLinkView {
  useSyncExternalStore(store.subscribe, store.getVersion);
  const state = store.snapshot();
  const source = state.deliverySource;
  const planned = source !== null && source.kind !== "none";
  const plan = useDeliveryPlan(store, host.flows, active && planned);
  const tree = state.status === "ready" ? store.tree() : null;
  const doc = plan.doc;

  // FLO-FR-KQCE: the status chips of a Flow file's items. A GitHub plan's
  // states come with each answer.
  const fileStates = useDeliveryState(active && plan.flowId ? doc : null, active && plan.flowId !== null);
  const answer = plan.github?.snapshot().answer ?? null;
  const states = useMemo(
    () => (plan.github ? new Map((answer?.states ?? []).map((s) => [s.itemId, s])) : fileStates.states),
    [plan.github, answer, fileStates.states],
  );

  const links = useMemo(() => (planned ? deriveSpecLinks(doc, tree) : null), [planned, doc, tree]);

  if (!planned || !tree) {
    return { links: planned ? (links ?? NO_LINKS) : null, slots: undefined, unplanned: planned ? toggle(store) : undefined };
  }

  const deliveredBy = (subject: MapRef) => {
    if (subject.kind === "change") return null;
    return (
      <div data-testid="spec-map-delivered-by">
        <div className="smap-section-label">Delivered by</div>
        <div className="smap-rows">
          {!doc ? (
            <div className="smap-row dlens-loading-row" role="status">
              <span className="smap-row__label">Loading the delivery plan…</span>
            </div>
          ) : (
            (() => {
              const ids = itemsLinking(links ?? NO_LINKS, tree, doc, subject);
              if (ids.length === 0) {
                return (
                  <div className="smap-row">
                    <span className="smap-row__label dlens-muted">No plan item delivers it yet.</span>
                  </div>
                );
              }
              return ids.map((id) => {
                const item = findItem(doc, id)!;
                return (
                  <button
                    key={id}
                    type="button"
                    className="smap-row dlens-delivered"
                    aria-label={`Show ${item.kind} ${item.name || "Unnamed"} on the delivery plan`}
                    onClick={() => {
                      logInfo(["frontend"], "specification map shows a delivery item", {});
                      store.showDeliveryItem(id);
                    }}
                  >
                    <span className="flow-glyph" aria-hidden="true">
                      {KIND_GLYPHS[item.kind]}
                    </span>
                    <span className="smap-row__label">{item.name || "Unnamed"}</span>
                    <StatusChip state={states?.get(id)?.state} />
                  </button>
                );
              });
            })()
          )}
        </div>
      </div>
    );
  };

  const planWork = (subject: MapRef) => {
    const isSpec = subject.kind === "spec";
    const isLeafGroup = subject.kind === "index" && tree.nodes.has(subject.id) && roleOf(tree, subject.id) === "leafGroup";
    if (!isSpec && !isLeafGroup) return null;
    const label =
      subject.kind === "spec"
        ? (tree.specs.get(subject.code)?.label ?? subject.code)
        : (tree.nodes.get((subject as { id: string }).id)?.label ?? "");
    return (
      <button
        type="button"
        className="btn btn--sm smap-action"
        onClick={() => {
          logInfo(["frontend"], "specification map plans work", {});
          store.requestPlanWork(label, planWorkFiles(tree, subject));
        }}
      >
        Plan work
      </button>
    );
  };

  return { links: links ?? NO_LINKS, slots: { deliveredBy, planWork }, unplanned: toggle(store) };
}

/** SDL-FR-HZLB: the **unplanned** toggle of the session. */
function toggle(store: SpecMapSessionStore): UnplannedToggle {
  return { on: store.snapshot().unplanned, onToggle: store.toggleUnplanned };
}
