/**
 * What the shell does for the project's delivery while a project is open
 * (`MDL-map-delivery-lens.md`, `SMP-specification-map.md`):
 *
 * - LIB-FR-JTXR / MDL-FR-YSMB: it keeps the resolved delivery source in the map
 *   session, so a request to open the delivery's Flow file is routed to the
 *   Map tab at once.
 * - MDL-FR-EFQF: an open Flow tab of the file that becomes the delivery writes
 *   its pending changes and closes.
 * - SMP-FR-SWUL / SNV-FR-GNQB: the Map tab's dirty indicator and its Save follow
 *   the delivery's Flow file. The shell does this and not the lens, because the
 *   lens unmounts on a tab switch and a write can land after that.
 */
import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";

import * as events from "../../events";
import { logInfo, logWarn } from "../../logging";
import type { FlowSessionStore } from "../../state/flowSessions";
import type { SpecMapSessionStore } from "../../state/specMap/session";
import type { Tab } from "../../types";
import type { DeliverySource } from "../../types/delivery";

/** LIB-FR-JTXR: the Flow file that is the project's delivery, or null. */
export function deliveryFlowId(source: DeliverySource | null): string | null {
  return source?.kind === "file" ? source.flowId : null;
}

export interface DeliveryShellDeps {
  /** A project is open in the main window. */
  active: boolean;
  projectKey: string;
  contentRootEpoch: number;
  specMap: SpecMapSessionStore;
  flows: FlowSessionStore;
  tabs: Tab[];
  closeTab: (id: string) => Promise<boolean>;
}

export function useDeliveryShell(deps: DeliveryShellDeps): void {
  const { active, projectKey, contentRootEpoch, specMap, flows } = deps;
  const latest = useRef(deps);
  latest.current = deps;

  // LIB-FR-JTXR: read the source when the project or its content root opens,
  // and again on each change of the settings or of the tree.
  useEffect(() => {
    if (!active) return;
    void specMap.resolveDeliverySource();
    let cancelled = false;
    const off: Array<() => void> = [];
    const again = () => void specMap.resolveDeliverySource();
    for (const pending of [listen(events.DELIVERY_SOURCE_CHANGED, again), events.onProjectTreeChanged(again)]) {
      void Promise.resolve(pending)
        .then((fn) => {
          if (typeof fn !== "function") return;
          if (cancelled) fn();
          else off.push(fn);
        })
        .catch(() => {});
    }
    return () => {
      cancelled = true;
      off.forEach((fn) => fn());
    };
  }, [active, projectKey, contentRootEpoch, specMap]);

  // MDL-FR-EFQF: a Flow tab of the file that becomes the delivery closes.
  useEffect(() => {
    if (!active) return;
    let seen = deliveryFlowId(specMap.snapshot().deliverySource);
    const check = () => {
      const flowId = deliveryFlowId(specMap.snapshot().deliverySource);
      if (flowId === seen) return;
      seen = flowId;
      if (!flowId) return;
      const tab = latest.current.tabs.find((t) => t.kind === "flow" && t.artifactId === flowId);
      if (!tab) return;
      logInfo(["frontend"], "flow tab of the new delivery closes", {});
      void latest.current.closeTab(tab.id).then((closed) => {
        if (!closed) logWarn(["frontend"], "flow tab of the new delivery could not close", {});
      });
    };
    check();
    return specMap.subscribe(check);
  }, [active, specMap]);

  // SMP-FR-SWUL / SNV-FR-GNQB: the dirty indicator and the Save of the Map tab.
  useEffect(() => {
    const update = () => {
      const flowId = deliveryFlowId(specMap.snapshot().deliverySource);
      const dirty = flowId !== null && flows.get(flowId)?.dirty === true;
      specMap.setDeliveryDirty(dirty);
      specMap.setDeliverySave(
        flowId
          ? async () => {
              await flows.flush(flowId);
            }
          : null,
      );
    };
    update();
    const offFlows = flows.subscribe(update);
    const offMap = specMap.subscribe(update);
    return () => {
      offFlows();
      offMap();
    };
  }, [flows, specMap]);
}
