/**
 * The Map tab (`SMP-specification-map.md`): the frame of its two lenses. The
 * **Delivery** lens shows the delivery plan (`MDL-map-delivery-lens.md`), and
 * the **Specifications** lens shows the specification corpus as one nested map.
 *
 * The frame asks for the default lens on the first render of a map session
 * (SMP-FR-TKUS). The active lens and the state of each lens live in the
 * shell-owned `SpecMapSessionStore` (SMP-FR-QNUH, SMP-FR-RNLH).
 */
import { useEffect, useMemo, useState, useSyncExternalStore } from "react";
import type { SpecMapSessionStore } from "../../state/specMap/session";
import type { OpenableArtifact } from "../../types";
import { DeliveryLens, type DeliveryHost, type SpecificationsRequest } from "../DeliveryLens";
import { useSpecLinkView } from "../DeliveryLens/specLinkSlots";
import { FlowSessionStore } from "../../state/flowSessions";
import type { InspectorSlots } from "./Inspector";
import { LensSwitch } from "./LensSwitch";
import { SpecificationsLens } from "./SpecificationsLens";
import type { UnplannedToggle } from "./Toolbar";
import { githubSourceOf, useDeliveryPlan } from "../DeliveryLens/useDeliveryPlan";
import { itemIdResolver } from "../DeliveryLens/useProposalBoard";
import { DiscussionPane } from "../MapDiscussion";
import { MapPlanContext, type MapPlanContextValue, type MapPlanHost } from "../MapPlanProposal";
import { deliveryNodeOf, issueItemId, specNodeOf } from "../../state/specMap/nodeRefs";
import { MapPlanReviewStore } from "../../state/mapPlan/reviewStore";
import { focusDiscussion, requestPendingFocus } from "../../state/discussionFocus";
import { getDeliverySettings } from "../../api";
import { logWarn } from "../../logging";
import type { DeliverySettings } from "../../types";
import { kindOfType } from "../../state/mapPlan/kinds";

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

  // SMP-FR-WNLR: the Discussion column, shared by both lenses.
  const plan = useDeliveryPlan(store, host.flows, false);
  const planDoc = plan.doc;
  const planTool = githubSourceOf(plan.source) !== null;
  const state = store.snapshot();
  // SMP-FR-DUUU: the node selected on the canvas of the active lens.
  const tree = store.tree();
  const selected =
    lens === "delivery" ? state.deliveryNode : tree ? specNodeOf(tree, state.selection) : null;
  const openDraftById = host.onOpenDraftById;
  // MPV-FR-CZDO: the one review state of the plan proposals of this map session.
  const [reviewStore] = useState(() => new MapPlanReviewStore());
  useEffect(() => reviewStore.retain(), [reviewStore]);
  // MPV-FR-SNCR: the Type mapping gives the kind chip of a new issue.
  const [settings, setSettings] = useState<DeliverySettings | null>(null);
  useEffect(() => {
    if (!planTool) return;
    let live = true;
    getDeliverySettings().then(
      (read) => {
        if (live) setSettings(read);
      },
      () => logWarn(["frontend"], "delivery settings could not be read for the plan review", {}),
    );
    return () => {
      live = false;
    };
  }, [planTool]);
  const github = githubSourceOf(plan.source);
  const repository = github ? `${github.repository.owner}/${github.repository.name}` : null;
  const planSession = plan.github;
  const planHost = useMemo<MapPlanHost>(() => {
    // MPV-FR-NMYT: the plan item of an issue, read from the tracker facts.
    const resolve = itemIdResolver(planSession?.snapshot().answer?.tracker ?? []);
    const itemOf = (number: number) => resolve(issueItemId(number));
    return {
      itemOnCanvas: (number) => planDoc?.items.some((i) => i.id === itemOf(number)) ?? false,
      selectItem: (number) => store.showDeliveryItem(itemOf(number)),
      ...(openDraftById ? { openDraft: openDraftById } : {}),
      discuss: (number) => {
        const item = number === null ? undefined : planDoc?.items.find((i) => i.id === itemOf(number));
        if (item) {
          store.discuss(deliveryNodeOf(item));
          return;
        }
        // The thread the column shows, which can be its first thread when none was chosen.
        const thread = reviewStore.snapshot().activeDiscussionId;
        // The column already shows it, so focus it at once; a column that mounts later takes the request.
        if (thread && focusDiscussion(thread, "composer") === "closed") requestPendingFocus(thread, "composer");
      },
      showDelivery: () => store.setLens("delivery"),
      repository,
      kindOfIssue: (number) => {
        const kind = planDoc?.items.find((i) => i.id === itemOf(number))?.kind;
        return kind && kind !== "milestone" ? kind : null;
      },
      kindOfType: (type) => kindOfType(settings, type),
    };
  }, [planDoc, planSession, store, reviewStore, openDraftById, repository, settings]);
  const planContext = useMemo<MapPlanContextValue>(
    () => ({ store: reviewStore, host: planHost }),
    [reviewStore, planHost],
  );
  const discussion = <DiscussionPane store={store} selected={selected} planTool={planTool} />;

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
    <MapPlanContext.Provider value={planContext}>
      <DeliveryLens
        isActive={lens === "delivery"}
        store={store}
        lensSwitch={lensSwitch}
        onRequestSpecifications={requestSpecifications}
        onOpenArtifact={onOpenArtifact}
        onOverlayOpening={onOverlayOpening}
        host={host}
        discussion={discussion}
      />
      {lens === "specifications" && (
        <SpecificationsLens
          store={store}
          lensSwitch={lensSwitch}
          onOpenArtifact={onOpenArtifact}
          inspectorSlots={inspectorSlots ?? links.slots}
          unplanned={unplanned ?? links.unplanned}
          links={links.links}
          discussion={discussion}
          onOverlayOpening={onOverlayOpening}
        />
      )}
    </MapPlanContext.Provider>
  );
}
