/**
 * The Delivery lens of the Map tab (`MDL-map-delivery-lens.md`): the project's
 * one delivery plan on one canvas, from the system that holds it
 * (`DSR-delivery-source.md`).
 *
 * The frame keeps this component mounted through a lens switch (SMP-FR-TKUS).
 * What must outlive a lens switch or a tab switch lives in the map session:
 * the source, the GitHub plan and its pending edits, the layouts, the view, and
 * the selection (SMP-FR-QNUH, SMP-FR-RNLH). A Flow file's plan lives in the
 * shell's `FlowSessionStore` (FLO-FR-30).
 */
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from "react";

import { logInfo, logWarn } from "../../logging";
import { deliveryLensData } from "../../state/delivery/lensData";
import { deriveSpecLinks } from "../../state/delivery/specLinks";
import { deliveryNodeOf } from "../../state/specMap/nodeRefs";
import { DiscussionToggle } from "../MapDiscussion";
import { useElementSize, useWindowWidth } from "../SpecMap/useElementSize";
import { FlowSessionStore } from "../../state/flowSessions";
import type { SpecMapSessionStore } from "../../state/specMap/session";
import type { FlowItem } from "../../state/flowDocument";
import type { ItemDeliveryState, OpenableArtifact } from "../../types";
import { describeSource } from "../DeliverySettingsSection";
import { FlowExcerptsContext, type FlowCanvasHandle, type FlowViewState } from "../Flow";
import { DEFAULT_VIEW, type View } from "../Flow/geometry";
import { FileLens } from "./FileLens";
import { GithubLens } from "./GithubLens";
import type { DeliveryHost } from "./host";
import { SpecsList } from "./ItemPanel";
import { leafRollup, Rollup } from "./Rollup";
import { githubSourceOf, useDeliveryPlan } from "./useDeliveryPlan";
import { useLensEvents } from "./useLensEvents";
import { useSourceChangeFocus } from "./useSourceChangeFocus";

export type { DeliveryHost } from "./host";

/** SDL-FR-BVNS: what the Delivery lens asks of the Specifications lens. */
export interface SpecificationsRequest {
  /**
   * The project-relative path of a spec file. The Specifications lens selects
   * its spec node, shows the last level, and centres the node (SMI-FR-PRSL).
   */
  specPath?: string;
}

export interface DeliveryLensProps {
  /**
   * SMP-FR-TKUS: true while the Delivery lens is the active lens. The frame
   * mounts the lens once the default lens is known and keeps it mounted
   * through a lens switch. An inactive lens renders nothing.
   */
  isActive: boolean;
  /**
   * SMP-FR-QNUH / SMP-FR-RNLH: the map session. The lens keeps its state in
   * `store.snapshot().delivery` and reads the source in
   * `store.snapshot().deliverySource`.
   */
  store: SpecMapSessionStore;
  /** MDL-FR-GDUP: the lens switch, first in the toolbar of the lens. */
  lensSwitch: ReactNode;
  /** SDL-FR-BVNS: switch to the Specifications lens, with a spec node to show. */
  onRequestSpecifications: (request?: SpecificationsRequest) => void;
  /** Open a file in its Editor tab (LIB-FR-03). */
  onOpenArtifact: (item: OpenableArtifact) => void;
  /** SNV-FR-56: a dialog of the lens closes every other floating overlay. */
  onOverlayOpening?: () => void;
  /** The Flow sessions, the claim flow, and the routes of the shell. */
  host?: DeliveryHost;
  /** SMP-FR-WNLR: the Discussion column and its splitter, at the right edge of the body. */
  discussion?: ReactNode;
}

export function DeliveryLens(props: DeliveryLensProps) {
  const { store, isActive } = props;
  const [ownFlows] = useState(() => new FlowSessionStore());
  const host: DeliveryHost = props.host ?? { flows: ownFlows };
  const plan = useDeliveryPlan(store, host.flows, isActive);
  useLensEvents(store, plan.github);

  useEffect(() => {
    logInfo(["frontend"], "delivery lens mounted", {});
  }, []);

  // MDL-FR-YTAP: the lens reads the source when it first renders.
  const resolved = useRef(false);
  useEffect(() => {
    if (!isActive || resolved.current) return;
    resolved.current = true;
    void store.resolveDeliverySource();
  }, [isActive, store]);

  if (!isActive) return null;
  return <ActiveLens {...props} host={host} plan={plan} />;
}

function ActiveLens({
  store,
  lensSwitch,
  onRequestSpecifications,
  onOpenArtifact,
  onOverlayOpening,
  host,
  plan,
  discussion,
}: DeliveryLensProps & { host: DeliveryHost; plan: ReturnType<typeof useDeliveryPlan> }) {
  useSyncExternalStore(store.subscribe, store.getVersion);
  const state = store.snapshot();
  const source = plan.source;
  const data = deliveryLensData(store);
  const canvas = useRef<FlowCanvasHandle | null>(null);
  const [canvasMounted, setCanvasMounted] = useState(false);
  const canvasRef = useCallback((handle: FlowCanvasHandle | null) => {
    canvas.current = handle;
    setCanvasMounted(handle !== null);
  }, []);
  const [fileStates, setFileStates] = useState<ReadonlyMap<string, ItemDeliveryState> | null>(null);

  // SDL-FR-VYIF: the lens loads the index on the terms of SMP-FR-PMCX.
  const hasPlan = plan.doc !== null;
  useEffect(() => {
    if (hasPlan) void store.load();
  }, [hasPlan, store]);
  const tree = state.status === "ready" ? store.tree() : null;
  const links = useMemo(() => deriveSpecLinks(plan.doc, tree), [plan.doc, tree]);

  // MDL-FR-JIKK / SDL-FR-MCGI: show the item the session asks for.
  const reveal = state.deliveryReveal;
  useEffect(() => {
    if (!reveal || !canvasMounted || !canvas.current) return;
    canvas.current.reveal(reveal.itemId);
    store.consumeDeliveryReveal(reveal.nonce);
    logInfo(["frontend"], "delivery lens shows a requested item", {});
  }, [reveal, canvasMounted, store]);

  // MDL-FR-EEDO / SDL-FR-CFHH: add the task the Specifications lens asks for.
  const planWork = state.planWork;
  useEffect(() => {
    if (!planWork || !canvasMounted || !canvas.current || !hasPlan) return;
    store.consumePlanWork(planWork.nonce);
    void canvas.current.addTask({ name: planWork.name, references: planWork.references }).then((id) => {
      if (id === null) {
        logWarn(["frontend"], "planned work was not added to the delivery", {});
        return;
      }
      // SDL-FR-CFHH: the new task shows selected and centred, as MDL-FR-JIKK does.
      canvas.current?.reveal(id);
    });
  }, [planWork, canvasMounted, hasPlan, store]);

  // SMP-FR-RNLH: the view and the selection outlive each mount of the canvas.
  // The canvas reads it only when it mounts.
  const keptView = state.delivery.view as View | null;
  const initialView: FlowViewState | undefined =
    keptView || data.selection.length > 0
      ? { view: keptView ?? DEFAULT_VIEW, selection: data.selection }
      : undefined;
  // SMP-FR-DUUU: the node the author selected, for **Use selected node**.
  const docRef = useRef(plan.doc);
  docRef.current = plan.doc;
  const onViewChange = useCallback(
    (next: FlowViewState) => {
      store.keepDeliveryView(next.view);
      data.selection = next.selection;
      const only = next.selection.length === 1 ? next.selection[0] : null;
      const item = only === null ? undefined : docRef.current?.items.find((i) => i.id === only);
      store.setDeliveryNode(item ? deliveryNodeOf(item) : null);
    },
    [store, data],
  );
  // MDL-FR-QELI: **Discuss** hands the node of an item or a milestone to the column.
  const onDiscuss = useCallback(
    (item: FlowItem) => store.discuss(deliveryNodeOf(item)),
    [store],
  );
  const windowWidth = useWindowWidth();
  // SMP-FR-VDND: the width of the Map tab decides which split controls show.
  const [rootEl, setRootEl] = useState<HTMLDivElement | null>(null);
  const tabWidth = useElementSize(rootEl).width;

  const panelSpecs = (itemId: string) => (
    <SpecsList
      codes={links.byItem.get(itemId) ?? null}
      tree={tree}
      indexLoading={state.status === "idle" || state.status === "loading"}
      onOpenSpec={(specPath) => onRequestSpecifications({ specPath })}
    />
  );

  const github = githubSourceOf(source);
  // The lens that shows: the frame keys its lens by these same values.
  const sourceKey =
    source === null
      ? ""
      : github
        ? `github:${github.project.nodeId}`
        : source.kind === "file"
          ? `file:${source.flowId}`
          : source.kind;
  const lensRoot = useRef<HTMLDivElement>(null);
  const bindRoot = useCallback((el: HTMLDivElement | null) => {
    lensRoot.current = el;
    setRootEl(el);
  }, []);
  const onSourceChanged = useSourceChangeFocus(store, lensRoot, sourceKey);
  const githubPlan = plan.github?.snapshot();
  const states: ReadonlyMap<string, ItemDeliveryState> | null = github
    ? githubPlan?.answer
      ? new Map(githubPlan.answer.states.map((s) => [s.itemId, s]))
      : null
    : fileStates;

  let body: ReactNode;
  if (source === null) {
    body = (
      <div className="smap-state" role="status">
        <p className="smap-state__line">Reading the delivery source…</p>
      </div>
    );
  } else if (source.kind === "none") {
    // MDL-FR-XMQU: the empty state, its reason, and Configure delivery.
    body = (
      <div className="smap-state dlens-empty" data-testid="delivery-empty">
        <p className="smap-state__line dlens-empty__title">This project has no delivery.</p>
        <p className="smap-state__line dlens-empty__reason">{describeSource(source)}</p>
        <div className="smap-state__actions">
          <button
            type="button"
            className="btn btn--sm btn--primary"
            onClick={() => host.onOpenProjectSettings?.("delivery")}
          >
            Configure delivery
          </button>
        </div>
      </div>
    );
  } else if (source.kind === "github" && !github) {
    // MDL-FR-WAOZ: the configuration error, and a route to its settings.
    body = (
      <div className="smap-state" role="alert" data-testid="delivery-config-error">
        <p className="smap-state__line">
          {source.configuration.error ??
            "The GitHub Project settings are not checked yet. Open them to check the Project."}
        </p>
        <div className="smap-state__actions">
          <button
            type="button"
            className="btn btn--sm"
            onClick={() => host.onOpenProjectSettings?.("github-polling")}
          >
            Open GitHub Project settings
          </button>
        </div>
      </div>
    );
  } else if (github && plan.github) {
    body = (
      <GithubLens
        key={github.project.nodeId}
        source={github}
        session={plan.github}
        data={data}
        host={host}
        canvasRef={canvasRef}
        getCanvas={() => canvas.current}
        initialView={initialView}
        onViewChange={onViewChange}
        panelSpecs={panelSpecs}
        onOpenArtifact={onOpenArtifact}
        onOverlayOpening={onOverlayOpening}
        onSourceChanged={onSourceChanged}
        onDiscuss={onDiscuss}
      />
    );
  } else if (source.kind === "file") {
    body = (
      <FileLens
        key={source.flowId}
        source={source}
        host={host}
        canvasRef={canvasRef}
        getCanvas={() => canvas.current}
        initialView={initialView}
        onViewChange={onViewChange}
        panelSpecs={panelSpecs}
        onOpenArtifact={onOpenArtifact}
        onStates={setFileStates}
        onOverlayOpening={onOverlayOpening}
        onSourceChanged={onSourceChanged}
        onDiscuss={onDiscuss}
      />
    );
  }

  return (
    <div
      ref={bindRoot}
      className="smap"
      data-testid="delivery-lens"
    >
      <div className="smap-toolbar" role="toolbar" aria-label="Map controls">
        {lensSwitch}
        <Rollup rollup={leafRollup(plan.doc, states)} />
        {discussion !== undefined && (
          <>
            <span className="smap-toolbar__spacer" />
            <DiscussionToggle
              store={store}
              labelled={windowWidth >= 1000}
              presets={tabWidth >= 1000}
              hint={tabWidth >= 1180}
            />
          </>
        )}
      </div>
      <div className="smap-body">
        <div className="smap-main dlens-main">
          {/* MDL-FR-RILW: an item of the plan reads as its name, with no excerpt. */}
          <FlowExcerptsContext.Provider value={false}>{body}</FlowExcerptsContext.Provider>
        </div>
        {discussion}
      </div>
    </div>
  );
}
