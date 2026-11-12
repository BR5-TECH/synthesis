/**
 * The Map tab (`SMP-specification-map.md`): the specification corpus as one
 * nested map, and a planning surface over it.
 *
 * Everything that must outlive a tab switch lives in the shell-owned
 * `SpecMapSessionStore` (SMP-FR-QNUH). What lives here is only what a gesture
 * needs while it runs: the hover, the drag, the open dialog, and the measured
 * canvas.
 */
import { useEffect, useMemo, useState, useSyncExternalStore } from "react";
import * as api from "../../api";
import { typeChip } from "../../artifactTypes";
import { logInfo, logWarn } from "../../logging";
import { formatSectionAddress } from "../../settingsWindow";
import { openOpenSpecChange } from "../../state/openspec/changeRoutes";
import {
  INITIAL_VIEW,
  type MapView,
  type SpecMapSessionStore,
} from "../../state/specMap/session";
import {
  indexChildren,
  projectPath,
  refExists,
} from "../../state/specMap/tree";
import { rollupAll, rollupTotal, type Rollup } from "../../state/specMap/rollups";
import { moveTargets, newNodeId } from "../../state/specMap/organize";
import {
  isOpenSpecIndex,
  type ChangeChip,
  type MapRef,
  type SpecificationIndex,
} from "../../state/specMap/types";
import type { OpenableArtifact } from "../../types";
import { Toolbar } from "./Toolbar";
import { Canvas } from "./Canvas";
import { Inspector } from "./Inspector";
import { inspectorModel } from "./inspectorModel";
import { DeleteNodeDialog, GroupDialog } from "./dialogs";
import { computeEdges } from "./edges";
import { holderRect, lastLevelOf, layoutMap, levelFor, scopeOf } from "./layout";
import { centerScale, centeredOn, firstPaintView, topAlignedView } from "./zoom";
import { useMapDrag } from "./useMapDrag";
import { useElementSize, useWindowWidth } from "./useElementSize";
import { Icon } from "../icons";

export interface SpecMapProps {
  store: SpecMapSessionStore;
  /** SMI-FR-GAJD: open a spec file in its Editor tab. */
  onOpenArtifact: (item: OpenableArtifact) => void;
  /** SMD-FR-HVBE: create a draft for an index node and open it. */
  onNewDraft: (nodeId: string) => void;
  /** SMD-FR-XEPS: open a planned draft in its New Artifact tab. */
  onOpenDraft: (draft: { id: string; name: string }) => void;
  /** SNV-FR-56: a dialog of the map closes every other floating overlay. */
  onOverlayOpening?: () => void;
}

type DialogState =
  | { kind: "create"; parentId: string | null }
  | { kind: "edit"; nodeId: string }
  | { kind: "delete"; nodeId: string };

/** SMI-FR-QWOP: the inspector docks from this window width up. */
export const DOCKED_INSPECTOR_WIDTH = 1160;

/**
 * SMP-FR-KVKD: what the notice line of an OpenSpec index says, or null where
 * the validation ran and every location was read.
 */
export function openSpecNotice(index: SpecificationIndex): string | null {
  if (!isOpenSpecIndex(index)) return null;
  const parts: string[] = [];
  if (index.validation === "unavailable") {
    parts.push("The OpenSpec validation could not run, so no requirement shows as a gap.");
  }
  const unread = index.unreadLocations ?? 0;
  if (unread > 0) {
    parts.push(
      `The changes of ${unread} ${unread === 1 ? "location" : "locations"} could not be read.`,
    );
  }
  return parts.length > 0 ? parts.join(" ") : null;
}

/** SMP-FR-KBXW: the OpenSpec section of Project settings. */
function openOpenSpecSettings(): void {
  void api.openSettingsWindow(formatSectionAddress("project", "openspec")).catch((e) => {
    logWarn(["frontend"], "Settings window could not be opened", {
      reason: e instanceof Error ? e.message : String(e),
    });
  });
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

export function SpecMap({
  store,
  onOpenArtifact,
  onNewDraft,
  onOpenDraft,
  onOverlayOpening,
}: SpecMapProps) {
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
  const [dialog, setDialog] = useState<DialogState | null>(null);

  // SMP-FR-PMCX: a session without an index loads it once.
  useEffect(() => {
    void store.load();
  }, [store]);

  // SMP-FR-MNZW: an OpenSpec index has no dependency and no organization.
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

  const drag = useMapDrag({
    canvas: canvasEl,
    tree,
    layout,
    view,
    enabled: !openspec,
    onDrop: (ref, target) =>
      store.dispatch({ kind: "move", ref, parentId: target.parentId, index: target.index }),
  });

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

  // SMO-FR-NXDL / SMI-FR-RPCO / SMI-FR-PRSL: show a node at its own level, centred.
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

  const openDialog = (next: DialogState) => {
    onOverlayOpening?.();
    setDialog(next);
  };
  const closeDialog = () => setDialog(null);

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

  const ready = state.status === "ready" && tree !== null && scope !== null && layout !== null;
  const empty = ready && tree.index.roots.length === 0;
  const docked = windowWidth >= DOCKED_INSPECTOR_WIDTH;
  const inspectorOpen = state.inspectorOpen ?? docked;

  let content;
  if (state.status === "error") {
    // SMP-FR-KBXW: a missing CLI also says that OpenSpec cannot be read, and
    // routes to where the author sets it up.
    const cli = state.errorCode === "cli_unavailable";
    content = (
      <div className="smap-state" role="alert">
        <p className="smap-state__line">The specification map could not be loaded.</p>
        {cli && (
          <p className="smap-state__line">
            OpenSpec cannot be read: the OpenSpec CLI is not available on this machine.
          </p>
        )}
        <div className="smap-state__actions">
          <button type="button" className="btn btn--sm" onClick={() => void store.retry()}>
            Retry
          </button>
          {cli && (
            <button type="button" className="btn btn--sm" onClick={openOpenSpecSettings}>
              Open OpenSpec settings
            </button>
          )}
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
        focusId={state.focusId}
        drag={drag.drag}
        handlers={{
          pointerDown: drag.pointerDown,
          select: (ref) => {
            if (drag.consumeClick()) return;
            store.select(ref);
          },
          hover: setHover,
        }}
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
        levels={scope?.levels ?? []}
        level={level}
        total={ready ? total : null}
        showDeps={state.showDeps}
        depsToggle={!openspec}
        gapsOnly={state.gapsOnly}
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
        <p className="smap-notice" role="status" data-testid="spec-map-notice">
          <Icon.Warning size={12} />
          <span>{notice}</span>
        </p>
      )}
      <div className="smap-body">
        <div className="smap-main">
          {content}
        </div>
        {ready && inspectorOpen && (
          <Inspector
            model={inspectorModel(tree, rollups, state.selection)}
            overlay={!docked}
            actions={{
              onClose: () => store.setInspectorOpen(false),
              onSelect: store.select,
              onFocus: store.setFocus,
              onZoomTo: store.zoomTo,
              onCreate: (parentId) => openDialog({ kind: "create", parentId }),
              onEdit: (nodeId) => openDialog({ kind: "edit", nodeId }),
              onDelete: (nodeId) => openDialog({ kind: "delete", nodeId }),
              onNewDraft,
              onOpenSpec: openSpec,
              onOpenDraft: (id, name) => onOpenDraft({ id, name }),
              onOpenChange: openChangeOfChip,
            }}
          />
        )}
      </div>
      {dialog && tree && <MapDialog dialog={dialog} store={store} onClose={closeDialog} />}
    </div>
  );
}

function MapDialog({
  dialog,
  store,
  onClose,
}: {
  dialog: DialogState;
  store: SpecMapSessionStore;
  onClose: () => void;
}) {
  const tree = store.tree()!;
  const levels = tree.index.levels;

  if (dialog.kind === "create") {
    const parentDepth = dialog.parentId === null ? -1 : (tree.depth.get(dialog.parentId) ?? -1);
    return (
      <GroupDialog
        title={`New ${levels[parentDepth + 1]?.singular ?? "node"}`}
        confirmLabel="Create"
        initial={{ label: "", summary: "" }}
        moveTargets={null}
        onCancel={onClose}
        onConfirm={({ label, summary }) => {
          store.dispatch({
            kind: "create",
            parentId: dialog.parentId,
            node: { id: newNodeId(tree), label, summary },
          });
          onClose();
        }}
      />
    );
  }

  const node = tree.nodes.get(dialog.nodeId);
  if (!node) return null;
  const singular = levels[tree.depth.get(node.id) ?? 0]?.singular ?? "node";

  if (dialog.kind === "delete") {
    return (
      <DeleteNodeDialog
        title={`Delete ${singular}`}
        label={node.label}
        childCount={node.children.length}
        onCancel={onClose}
        onConfirm={() => {
          store.dispatch({ kind: "delete", nodeId: node.id });
          onClose();
        }}
      />
    );
  }

  return (
    <GroupDialog
      title={`Rename ${singular}`}
      confirmLabel="Save"
      initial={{ label: node.label, summary: node.summary }}
      moveTargets={moveTargets(tree, node.id).map((id) => ({
        id,
        label: tree.nodes.get(id)!.label,
      }))}
      onCancel={onClose}
      onConfirm={({ label, summary, moveTo }) => {
        if (label !== node.label || summary !== node.summary) {
          store.dispatch({ kind: "edit", nodeId: node.id, label, summary });
        }
        if (moveTo) {
          // SMO-FR-SLNC: the subject becomes the last child of its new parent.
          const parent = store.tree()!.nodes.get(moveTo)!;
          store.dispatch({
            kind: "move",
            ref: { kind: "index", id: node.id },
            parentId: moveTo,
            index: indexChildren(parent).length,
          });
        }
        onClose();
      }}
    />
  );
}
