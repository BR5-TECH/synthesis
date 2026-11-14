/**
 * The delivery plan the map session holds, whichever lens asks for it
 * (`MDL-map-delivery-lens.md`, `SDL-specification-delivery-links.md`
 * SDL-FR-VYIF).
 *
 * - A `file` source is the Flow file's editing session in the shell's
 *   `FlowSessionStore`. The Map tab takes the role of its Flow tab
 *   (MDL-FR-VLQJ), so a lens that needs the plan opens that session.
 * - A `github` source is the `GithubPlanSession` of the map session, which
 *   loads on the terms of MDL-FR-LZIE.
 */
import { useEffect, useSyncExternalStore } from "react";

import type { FlowDocument } from "../../state/flowDocument";
import type { FlowSessionStore } from "../../state/flowSessions";
import { deliveryLensData } from "../../state/delivery/lensData";
import type { GithubPlanSession } from "../../state/delivery/githubPlan";
import type { SpecMapSessionStore } from "../../state/specMap/session";
import type { DeliverySource } from "../../types/delivery";

const noSubscribe = () => () => {};
const noVersion = () => 0;

export interface DeliveryPlanView {
  source: DeliverySource | null;
  /** The plan to render, or null while it loads or where there is none. */
  doc: FlowDocument | null;
  /** The plan's session where the source is `github`. */
  github: GithubPlanSession | null;
  /** The Flow file where the source is `file`. */
  flowId: string | null;
}

/** The GitHub source of a valid configuration, or null. */
export function githubSourceOf(source: DeliverySource | null) {
  return source?.kind === "github" && source.configuration.state === "valid" ? source : null;
}

/**
 * The plan of the session's delivery source. With `load` false, nothing is
 * asked for: the view reads only what the session holds.
 */
export function useDeliveryPlan(
  store: SpecMapSessionStore,
  flows: FlowSessionStore,
  load: boolean,
): DeliveryPlanView {
  useSyncExternalStore(store.subscribe, store.getVersion);
  useSyncExternalStore(flows.subscribe, flows.getVersion);
  const source = store.snapshot().deliverySource;
  const github = githubSourceOf(source);
  const session = github ? deliveryLensData(store).githubFor(github.project.nodeId) : null;
  useSyncExternalStore(session?.subscribe ?? noSubscribe, session?.getVersion ?? noVersion);
  const flowId = source?.kind === "file" ? source.flowId : null;

  // SDL-FR-VYIF / MDL-FR-LZIE: a session that holds no plan loads it.
  useEffect(() => {
    if (!load) return;
    if (flowId) flows.openTab(flowId);
    session?.start();
  }, [load, flowId, flows, session]);

  const doc = flowId ? (flows.get(flowId)?.doc ?? null) : (session?.snapshot().shown ?? null);
  return { source, doc, github: session, flowId };
}
