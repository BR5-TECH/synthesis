/**
 * The Specifications lens of the Map tab (`SMP-specification-map.md`): the
 * specification corpus as one nested map.
 *
 * Everything that must outlive a tab switch or a lens switch lives in the
 * shell-owned `SpecMapSessionStore` (SMP-FR-QNUH, SMP-FR-RNLH). What lives here
 * is only what a gesture needs while it runs: the hover and the measured
 * canvas.
 */
import {
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from "react";
import * as api from "../../api";
import { typeChip } from "../../artifactTypes";
import { logInfo, logWarn } from "../../logging";
import { openOpenSpecChange } from "../../state/openspec/changeRoutes";
import {
  INITIAL_VIEW,
  type MapView,
  type SpecMapSessionStore,
} from "../../state/specMap/session";
import { projectPath, refExists } from "../../state/specMap/tree";
import { rollupAll, rollupTotal, type Rollup } from "../../state/specMap/rollups";
import {
  isOpenSpecIndex,
  type ChangeChip,
  type MapRef,
  type SpecificationIndex,
} from "../../state/specMap/types";
import type { OpenableArtifact } from "../../types";
import { Toolbar, type UnplannedToggle } from "./Toolbar";
import type { SpecLinks } from "../../state/delivery/specLinks";
import { Canvas } from "./Canvas";
import { Inspector, type InspectorSlots } from "./Inspector";
import { inspectorModel } from "./inspectorModel";
import { computeEdges } from "./edges";
import { holderRect, lastLevelOf, layoutMap, levelFor, scopeOf } from "./layout";
import { centerScale, centeredOn, firstPaintView, topAlignedView } from "./zoom";
import { useElementSize, useWindowWidth } from "./useElementSize";
import { Icon } from "../icons";
import { OpenSpecFailureNotice } from "../OpenSpecFailureNotice";
import { noticeSentence } from "../../state/openspec/failureNotice";
import { useShortFormLog } from "../../state/openspec/useShortFormLog";

export interface SpecificationsLensProps {
  store: SpecMapSessionStore;
  /** SMP-FR-TKUS: the lens switch, first in the toolbar. */
  lensSwitch: ReactNode;
  /** SMI-FR-GAJD: open a spec file in its Editor tab. */
  onOpenArtifact: (item: OpenableArtifact) => void;
  /** SDL-FR-VAGU / SDL-FR-CFHH: the insertion points of the inspector. */
  inspectorSlots?: InspectorSlots;
  /** SDL-FR-HZLB: the **unplanned** toggle, or undefined where it does not show. */
  unplanned?: UnplannedToggle;
  /** SDL-FR-SGQM / SDL-FR-KXCY: the delivery links, or null where none show. */
  links?: SpecLinks | null;
}

/** SMI-FR-QWOP: the inspector docks from this window width up. */
export const DOCKED_INSPECTOR_WIDTH = 1160;

/**
 * SMP-FR-KVKD: what the notice line of an OpenSpec index says, or null where
 * the validation ran and every location was read. The sentence of the notice
 * of `validationFailure` stands in the line, and its Details beside it.
 */
export function openSpecNotice(index: SpecificationIndex): string | null {
  if (!isOpenSpecIndex(index)) return null;
  const parts: string[] = [];
  if (index.validation === "unavailable") {
    parts.push("The OpenSpec validation could not run, so no requirement shows as a gap.");
    const sentence = index.validationFailure ? noticeSentence(index.validationFailure) : null;
    if (sentence) parts.push(sentence);
  }
  const unread = index.unreadLocations ?? 0;
  if (unread > 0) {
    parts.push(
      `The changes of ${unread} ${unread === 1 ? "location" : "locations"} could not be read.`,
    );
  }
  return parts.length > 0 ? parts.join(" ") : null;
}

/**
 * SMP-FR-KBXW: Retry of the error state. After an OpenSpec failure it first
 * re-checks the CLI (`"get openspec status"` with `recheck`), then loads.
 */
async function retryLoad(store: SpecMapSessionStore): Promise<void> {
  if (store.snapshot().errorFailure) {
    try {
      await api.getOpenspecStatus(true);
    } catch (e) {
      logWarn(["frontend"], "the openspec status re-check of the map retry failed", {
        reason: e instanceof Error ? e.name : typeof e,
      });
    }
  }
  await store.retry();
}

/** SMI-FR-UKIV: open the change of a chip in the location of the chip. */
function openChangeOfChip(chip: ChangeChip): void {
  logInfo(["frontend"], "specification map opens a change", {
    locationKind: chip.location.kind,
  });
  if (chip.location.kind === "stream") {
    openOpenSpecChange(
      { kind: "stream", streamId: chip.location.streamId },
      chip.change,
      { streamName: chip.location.streamName },
    );
  } else {
    openOpenSpecChange({ kind: "active_worktree" }, chip.change);
  }
}

export function SpecificationsLens({
  store,
  lensSwitch,
  onOpenArtifact,
  inspectorSlots,
  unplanned,
  links = null,
}: SpecificationsLensProps) {
  useSyncExternalStore(store.subscribe, store.getVersion);
  const state = store.snapshot();
  const tree = store.tree();
  const windowWidth = useWindowWidth();
  // SMP-FR-GJEW: the toolbar's rules follow the width of the tab itself.
  const [rootEl, setRootEl] = useState<HTMLDivElement | null>(null);
  const tabSize = useElementSize(rootEl);
  const [canvasEl, setCanvasEl] = useState<HTMLDivElement | null>(null);
  const size = useElementSize(canvasEl);
  const [hoverRef, setHover] = useState<MapRef | null>(null);

  // SMP-FR-PMCX: when this lens renders, a session without an index loads it
  // once.
  useEffect(() => {
    void store.load();
  }, [store]);

  // SMP-FR-MNZW: an OpenSpec index has no dependency.
  const openspec = tree !== null && isOpenSpecIndex(tree.index);
  const scope = useMemo(() => (tree ? scopeOf(tree, state.focusId) : null), [tree, state.focusId]);
  const lastLevel = scope ? lastLevelOf(scope) : 0;
  const level = Math.min(state.view.level, lastLevel);
  const layout = useMemo(
    () => (scope ? layoutMap(scope, level, size.width) : null),
    [scope, level, size.width],
  );
  // SMZ-FR-NUOB: until the author pans or zooms, the view is derived from the
  // measured canvas, so it follows the first non-zero height.
  const view: MapView = state.view.touched
    ? { ...state.view, level }
    : firstPaintView(layout?.height ?? 0, size.height);
  const rollups = useMemo(() => (tree ? rollupAll(tree) : new Map<string, Rollup>()), [tree]);
  const total = useMemo(() => (tree ? rollupTotal(tree) : null), [tree]);
  const hover = hoverRef && tree && refExists(tree, hoverRef) ? hoverRef : null;
  const active = hover ?? state.selection;

  // The edges do not depend on the translation or the scale, so a pan re-renders
  // the map without computing them again.
  // SME-FR-XLUQ: no edge draws for an OpenSpec index.
  const showDeps = state.showDeps && !openspec;
  const edges = useMemo(
    () =>
      tree && layout && scope
        ? computeEdges({ tree, layout, scope, level, active, showDeps })
        : null,
    [tree, layout, scope, level, active, showDeps],
  );

  // SMI-FR-RPCO / SMI-FR-PRSL: show a node at its own level, centred.
  const zoom = state.zoom;
  useEffect(() => {
    if (!zoom || !tree || !size.measured) return;
    let target = scopeOf(tree, state.focusId);
    // SMI-FR-QCUC: a capability a spec file names shows at the last level.
    const levelOf = (within: typeof target) => {
      const own = levelFor(tree, within, zoom.ref);
      return own !== null && zoom.lastLevel ? lastLevelOf(within) : own;
    };
    let targetLevel = levelOf(target);
    if (targetLevel === null && state.focusId !== null) {
      // A node outside the focused subtree is reached by ending the focus.
      store.setFocus(null);
      target = scopeOf(tree, null);
      targetLevel = levelOf(target);
    }
    if (targetLevel !== null) {
      const targetLayout = layoutMap(target, targetLevel, size.width);
      const rect = holderRect(tree, targetLayout, zoom.ref);
      const scale = centerScale(targetLevel, lastLevelOf(target));
      store.setView(
        rect
          ? centeredOn(rect, targetLevel, scale)
          : topAlignedView(targetLevel, scale, targetLayout.height, size.height, true),
      );
    }
    store.consumeZoom(zoom.nonce);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [zoom?.nonce, tree, size.measured]);

  /** SMZ-FR-VYOS / SMZ-FR-EPTR */
  const chooseLevel = (next: number) => {
    if (!tree || !scope) return;
    if (next === 0) {
      store.setView(INITIAL_VIEW);
      return;
    }
    const scale = centerScale(next, lastLevel);
    const nextLayout = layoutMap(scope, next, size.width);
    const rect = state.selection ? holderRect(tree, nextLayout, state.selection) : null;
    // SMZ-FR-QOTA: with nothing to centre on, the level is framed top-aligned.
    store.setView(
      rect
        ? centeredOn(rect, next, scale)
        : topAlignedView(next, scale, nextLayout.height, size.height, true),
    );
  };

  const openSpec = (code: string) => {
    const spec = tree?.specs.get(code);
    if (!spec) return;
    // SMI-FR-GAJD: the file of the node's path, relative to the project root.
    const path = projectPath(tree!.index, spec);
    onOpenArtifact({
      id: path,
      name: path.split("/").pop() ?? path,
      artifactType: "spec",
      chip: typeChip("spec"),
    });
  };

  // SMP-FR-KBXW: Retry replaces the error state with the loading state, so
  // its button leaves the page. Where the load fails again, the focus goes
  // back to the new Retry, and does not fall to the page.
  const retryButton = useRef<HTMLButtonElement | null>(null);
  const retried = useRef(false);
  useEffect(() => {
    if (state.status === "loading") return;
    if (retried.current && state.status === "error") retryButton.current?.focus();
    retried.current = false;
  }, [state.status]);

  const ready = state.status === "ready" && tree !== null && scope !== null && layout !== null;
  // OFN-FR-LKAT: the line of SMP-FR-KVKD states the sentence of the
  // validation failure, so the map logs that notice once when it appears.
  useShortFormLog(
    "spec_map_validation",
    ready && tree.index.validation === "unavailable" ? (tree.index.validationFailure ?? null) : null,
  );
  const empty = ready && tree.index.roots.length === 0;
  const docked = windowWidth >= DOCKED_INSPECTOR_WIDTH;
  const inspectorOpen = state.inspectorOpen ?? docked;

  let content;
  if (state.status === "error") {
    // SMP-FR-KBXW: an OpenSpec failure renders its notice in place of the
    // statement. The Retry of the error state stays, so the notice offers
    // none (OFN-FR-DPIJ). The map knows no location of a change, so the
    // notice offers no Open change.
    const failure = state.errorFailure;
    content = (
      <div
        className={failure ? "smap-state smap-state--notice" : "smap-state"}
        role={failure ? undefined : "alert"}
      >
        {failure ? (
          <OpenSpecFailureNotice
            failure={failure}
            surface="spec_map_load"
            className="smap-state__notice"
          />
        ) : (
          <p className="smap-state__line">The specification map could not be loaded.</p>
        )}
        <div className="smap-state__actions">
          <button
            ref={retryButton}
            type="button"
            className="btn btn--sm"
            onClick={() => {
              retried.current = true;
              void retryLoad(store);
            }}
          >
            Retry
          </button>
        </div>
      </div>
    );
  } else if (!ready) {
    content = (
      <div className="smap-state" role="status">
        <p className="smap-state__line">Loading the specification map…</p>
      </div>
    );
  } else if (empty) {
    content = (
      <div className="smap-state" role="status">
        <p className="smap-state__line">No specifications are indexed.</p>
      </div>
    );
  } else {
    content = (
      <Canvas
        canvasEl={canvasEl}
        setCanvasEl={setCanvasEl}
        width={size.width}
        height={size.height}
        tree={tree}
        scope={scope}
        layout={layout}
        rollups={rollups}
        edges={edges!}
        view={view}
        lastLevel={lastLevel}
        selection={state.selection}
        hover={hover}
        active={active}
        gapsOnly={state.gapsOnly}
        links={links}
        unplanned={!!unplanned?.on && links !== null}
        focusId={state.focusId}
        handlers={{ select: store.select, hover: setHover }}
        onView={store.setView}
        onResetView={() => store.setView(INITIAL_VIEW)}
        onFocus={store.setFocus}
        showInspectorButton={!inspectorOpen}
        inspectorOverlay={inspectorOpen && !docked}
        onOpenInspector={() => store.setInspectorOpen(true)}
      />
    );
  }

  const notice = ready ? openSpecNotice(tree.index) : null;

  return (
    <div className="smap" ref={setRootEl}>
      <Toolbar
        lensSwitch={lensSwitch}
        levels={scope?.levels ?? []}
        level={level}
        total={ready ? total : null}
        showDeps={state.showDeps}
        depsToggle={!openspec}
        gapsOnly={state.gapsOnly}
        unplanned={unplanned}
        width={tabSize.width}
        disabled={!ready || empty}
        onLevel={chooseLevel}
        onToggleDeps={store.toggleDeps}
        onToggleGaps={store.toggleGaps}
      />
      {/* SMP-FR-KVKD: one line above the canvas, so it hides no node. It
          stands outside the body, so the inspector that overlays the body
          below the docking width (SMI-FR-QWOP) does not cover it. */}
      {notice && (
        <div className="smap-notice" data-testid="spec-map-notice">
          <Icon.Warning size={12} />
          {/* OFN-FR-ONNF: the sentence alone is live, so opening Details does
              not announce the list. */}
          <span role="status">{notice}</span>
          {/* SMP-FR-KVKD: the Details of the validation failure, in the line. */}
          {ready && tree.index.validationFailure && (
            <ValidationDetails failure={tree.index.validationFailure} />
          )}
        </div>
      )}
      <div className="smap-body">
        <div className="smap-main">
          {content}
        </div>
        {ready && inspectorOpen && (
          <Inspector
            model={inspectorModel(tree, rollups, state.selection)}
            overlay={!docked}
            slots={inspectorSlots}
            actions={{
              onClose: () => store.setInspectorOpen(false),
              onSelect: store.select,
              onFocus: store.setFocus,
              onZoomTo: store.zoomTo,
              onOpenSpec: openSpec,
              onOpenChange: openChangeOfChip,
            }}
          />
        )}
      </div>
    </div>
  );
}

/**
 * SMP-FR-KVKD: the Details of the failure of a validation that could not
 * run. The line states the sentence itself, so this holds Details alone.
 */
function ValidationDetails({ failure }: { failure: NonNullable<SpecificationIndex["validationFailure"]> }) {
  if ((failure.diagnostics ?? []).length === 0) return null;
  return (
    <div className="smap-notice__details">
      <OpenSpecFailureNotice
        failure={failure}
        surface="spec_map_validation"
        actions={false}
        withSentence={false}
        log={false}
      />
    </div>
  );
}
