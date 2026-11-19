/**
 * MPV-FR-HGLN: the review of plan proposals in a tab other than the Map tab.
 * Each tab holds one scope: its own review store (MPV-FR-CZDO), so its own
 * sheet, and an in-place host (MPV-FR-CUWB). The summary rows of the
 * discussions of the tab and the sheet over the content of the tab
 * (MPV-FR-ZQRE) read it.
 *
 * The scope calls nothing while no proposal shows. When the first summary row
 * asks for the proposals of a discussion, it reads the delivery source once,
 * for the repository of an issue link and for the Type mapping of a kind chip.
 */
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";

import { getDeliverySettings, getDeliverySource } from "../../api";
import { logDebug, logWarn } from "../../logging";
import { MapPlanReviewStore } from "../../state/mapPlan/reviewStore";
import { githubSourceOf } from "../DeliveryLens/useDeliveryPlan";
import { MapPlanContext, type MapPlanContextValue } from "./host";
import { inPlaceHost, NO_DELIVERY, type InPlaceDelivery } from "./inPlaceHost";

export function InPlaceReviewScope({
  openDraft,
  children,
}: {
  /** MPV-FR-CUWB: open a draft that a row created, in the New Artifact tab. */
  openDraft?: (draftId: string) => void;
  children: ReactNode;
}) {
  const [store] = useState(() => new MapPlanReviewStore());
  // The store follows the event from its first `want`. The tab that goes stops it.
  useEffect(() => () => store.dispose(), [store]);

  const [delivery, setDelivery] = useState<InPlaceDelivery>(NO_DELIVERY);
  useEffect(() => {
    let live = true;
    let started = false;
    const start = () => {
      if (started || store.snapshot().proposals.size === 0) return;
      started = true;
      logDebug(["frontend"], "plan review reads the delivery source", {});
      getDeliverySource().then(
        (source) => {
          const github = githubSourceOf(source);
          if (!github) return;
          if (live) {
            const repository = `${github.repository.owner}/${github.repository.name}`;
            setDelivery((d) => ({ ...d, repository }));
          }
          getDeliverySettings().then(
            (settings) => {
              if (live) setDelivery((d) => ({ ...d, settings }));
            },
            () => logWarn(["frontend"], "delivery settings could not be read for the plan review", {}),
          );
        },
        () => logWarn(["frontend"], "delivery source could not be read for the plan review", {}),
      );
    };
    const unsubscribe = store.subscribe(start);
    start();
    return () => {
      live = false;
      unsubscribe();
    };
  }, [store]);

  // The route of the shell can be a new function on each render; the host
  // stays the same, so the rows and the sheet do not draw again for it.
  const openRef = useRef(openDraft);
  openRef.current = openDraft;
  const routed = openDraft !== undefined;
  const open = useCallback((draftId: string) => openRef.current?.(draftId), []);
  const value = useMemo<MapPlanContextValue>(
    () => ({ store, host: inPlaceHost(store, delivery, routed ? open : undefined), inPlace: true }),
    [store, delivery, routed, open],
  );
  return <MapPlanContext.Provider value={value}>{children}</MapPlanContext.Provider>;
}
