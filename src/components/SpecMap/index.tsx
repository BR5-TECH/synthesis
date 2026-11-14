/**
 * The Map tab (`SMP-specification-map.md`): the frame of its two lenses. The
 * **Delivery** lens shows the delivery plan (`MDL-map-delivery-lens.md`), and
 * the **Specifications** lens shows the specification corpus as one nested map.
 *
 * The frame asks for the default lens on the first render of a map session
 * (SMP-FR-TKUS). The active lens and the state of each lens live in the
 * shell-owned `SpecMapSessionStore` (SMP-FR-QNUH, SMP-FR-RNLH).
 */
import { useEffect, useState, useSyncExternalStore } from "react";
import type { SpecMapSessionStore } from "../../state/specMap/session";
import type { OpenableArtifact } from "../../types";
import { DeliveryLens, type DeliveryHost, type SpecificationsRequest } from "../DeliveryLens";
import { useSpecLinkView } from "../DeliveryLens/specLinkSlots";
import { FlowSessionStore } from "../../state/flowSessions";
import type { InspectorSlots } from "./Inspector";
import { LensSwitch } from "./LensSwitch";
import { SpecificationsLens } from "./SpecificationsLens";
import type { UnplannedToggle } from "./Toolbar";

export { DOCKED_INSPECTOR_WIDTH, openSpecNotice } from "./SpecificationsLens";

export interface SpecMapProps {
  store: SpecMapSessionStore;
  /** SMI-FR-GAJD: open a spec file in its Editor tab. */
  onOpenArtifact: (item: OpenableArtifact) => void;
  /** SNV-FR-56: a dialog of the map closes every other floating overlay. */
  onOverlayOpening?: () => void;
  /** SDL-FR-VAGU / SDL-FR-CFHH: the insertion points of the inspector. */
  inspectorSlots?: InspectorSlots;
  /** SDL-FR-HZLB: the **unplanned** toggle, or undefined where it does not show. */
  unplanned?: UnplannedToggle;
  /** MDL-FR-VLQJ / MDL-FR-QEXS: the Flow sessions, the claim flow, and the routes of the shell. */
  delivery?: DeliveryHost;
}

export function SpecMap({
  store,
  onOpenArtifact,
  onOverlayOpening,
  inspectorSlots,
  unplanned,
  delivery,
}: SpecMapProps) {
  useSyncExternalStore(store.subscribe, store.getVersion);
  const { lens } = store.snapshot();
  const [ownFlows] = useState(() => new FlowSessionStore());
  const host: DeliveryHost = delivery ?? { flows: ownFlows };
  // SDL-FR-VAGU .. SDL-FR-CKAA: what the Specifications lens shows of the delivery.
  const links = useSpecLinkView(store, host, lens === "specifications");

  // SMP-FR-TKUS: the first render of a map session asks for the default lens.
  useEffect(() => {
    void store.decideLens();
  }, [store]);

  const lensSwitch = <LensSwitch lens={lens} onLens={store.setLens} />;

  if (lens === null) {
    return (
      <div className="smap">
        <div className="smap-toolbar" role="toolbar" aria-label="Map controls">
          {lensSwitch}
        </div>
        <div className="smap-body">
          <div className="smap-main">
            <div className="smap-state" role="status">
              <p className="smap-state__line">Loading the map…</p>
            </div>
          </div>
        </div>
      </div>
    );
  }

  // SDL-FR-BVNS: the Delivery lens shows a spec node of the Specifications lens.
  const requestSpecifications = (request?: SpecificationsRequest) => {
    store.setLens("specifications");
    if (request?.specPath) store.selectByPath(request.specPath);
  };

  return (
    <>
      <DeliveryLens
        isActive={lens === "delivery"}
        store={store}
        lensSwitch={lensSwitch}
        onRequestSpecifications={requestSpecifications}
        onOpenArtifact={onOpenArtifact}
        onOverlayOpening={onOverlayOpening}
        host={host}
      />
      {lens === "specifications" && (
        <SpecificationsLens
          store={store}
          lensSwitch={lensSwitch}
          onOpenArtifact={onOpenArtifact}
          inspectorSlots={inspectorSlots ?? links.slots}
          unplanned={unplanned ?? links.unplanned}
          links={links.links}
        />
      )}
    </>
  );
}
