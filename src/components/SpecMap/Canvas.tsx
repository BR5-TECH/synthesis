import { useEffect, useRef, useState, type PointerEvent } from "react";
import type { TreeIndex } from "../../state/specMap/tree";
import type { Rollup } from "../../state/specMap/rollups";
import type { MapRef } from "../../state/specMap/types";
import type { MapView } from "../../state/specMap/session";
import { Icon } from "../icons";
import type { MapLayout, MapScope } from "./layout";
import type { MapEdges } from "./edges";
import type { DragState } from "./useMapDrag";
import { EdgeLayer } from "./EdgeLayer";
import { NodeLayer, type NodeHandlers } from "./nodes";
import { HoverCard } from "./HoverCard";
import { Breadcrumb, Legend, ZoomControls, keepFromCanvas } from "./CanvasChrome";
import { BUTTON_STEP, WHEEL_IN, WHEEL_OUT, zoomStep } from "./zoom";

interface CanvasProps {
  canvasEl: HTMLDivElement | null;
  setCanvasEl: (el: HTMLDivElement | null) => void;
  width: number;
  height: number;
  tree: TreeIndex;
  scope: MapScope;
  layout: MapLayout;
  rollups: Map<string, Rollup>;
  edges: MapEdges;
  view: MapView;
  lastLevel: number;
  selection: MapRef | null;
  hover: MapRef | null;
  active: MapRef | null;
  gapsOnly: boolean;
  focusId: string | null;
  drag: DragState | null;
  handlers: NodeHandlers;
  onView: (view: MapView) => void;
  onResetView: () => void;
  onFocus: (id: string | null) => void;
  showInspectorButton: boolean;
  /** SMI-FR-QWOP: the inspector is open as an overlay over this canvas. */
  inspectorOverlay: boolean;
  onOpenInspector: () => void;
}

function refLabel(tree: TreeIndex, ref: MapRef): string {
  switch (ref.kind) {
    case "index":
      return tree.nodes.get(ref.id)?.label ?? "";
    case "spec":
      return ref.code;
    case "planned":
      return tree.planned.get(ref.draftId)?.name ?? "";
  }
}

/** SMZ-FR-DPWC: the canvas, its one transformed stage, and the controls over it. */
export function Canvas(props: CanvasProps) {
  const { tree, layout, view, drag, width, height } = props;
  const pan = useRef<{ x: number; y: number; view: MapView } | null>(null);
  const [panning, setPanning] = useState(false);
  const latest = useRef(props);
  latest.current = props;

  // SMZ-FR-ZQLP: a wheel over the canvas zooms it and scrolls nothing else.
  useEffect(() => {
    const el = props.canvasEl;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const current = latest.current;
      // SMZ-FR-CUTY
      if (current.drag) return;
      current.onView(zoomStep(current.view, e.deltaY > 0 ? WHEEL_OUT : WHEEL_IN, current.lastLevel));
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [props.canvasEl]);

  // SMZ-FR-KAHU: a drag that starts on empty canvas pans the stage.
  const startPan = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    pan.current = { x: e.clientX, y: e.clientY, view };
    e.currentTarget.setPointerCapture?.(e.pointerId);
    setPanning(true);
    props.handlers.hover(null);
  };
  const movePan = (e: PointerEvent<HTMLDivElement>) => {
    const start = pan.current;
    if (!start) return;
    props.onView({
      ...start.view,
      tx: start.view.tx + (e.clientX - start.x),
      ty: start.view.ty + (e.clientY - start.y),
      touched: true,
    });
  };
  const endPan = () => {
    if (!pan.current) return;
    pan.current = null;
    setPanning(false);
  };

  const bar = drag?.target?.bar;
  return (
    <div
      ref={props.setCanvasEl}
      className="smap-canvas"
      data-testid="spec-map-canvas"
      data-panning={panning || undefined}
      data-inspector-overlay={props.inspectorOverlay || undefined}
      onPointerDown={startPan}
      onPointerMove={movePan}
      onPointerUp={endPan}
      onPointerCancel={endPan}
      onMouseOver={() => props.handlers.hover(null)}
      onMouseLeave={() => props.handlers.hover(null)}
    >
      <div
        className="smap-stage"
        data-testid="spec-map-stage"
        style={{ transform: `translate(${view.tx}px, ${view.ty}px) scale(${view.scale})` }}
      >
        <EdgeLayer edges={props.edges} />
        <NodeLayer
          tree={tree}
          layout={layout}
          rollups={props.rollups}
          selection={props.selection}
          active={props.active}
          gapsOnly={props.gapsOnly}
          related={props.edges.related}
          dragging={drag?.ref ?? null}
          dropId={drag?.target?.highlightId ?? null}
          handlers={props.handlers}
        />
        {bar && (
          <div
            className="smap-drop-bar"
            style={{ left: bar.x, top: bar.y, width: bar.w, height: bar.h }}
          />
        )}
        {drag && (
          <div className="smap-ghost" style={{ left: drag.world.x, top: drag.world.y }}>
            {refLabel(tree, drag.ref)}
          </div>
        )}
      </div>
      {props.hover && !panning && !drag && (
        <HoverCard
          tree={tree}
          layout={layout}
          rollups={props.rollups}
          target={props.hover}
          view={view}
          width={width}
          height={height}
        />
      )}
      <Breadcrumb tree={tree} focusId={props.focusId} onFocus={props.onFocus} />
      {props.showInspectorButton && (
        <button
          type="button"
          className="smap-inspector-open"
          onPointerDown={keepFromCanvas}
          onClick={props.onOpenInspector}
        >
          <Icon.Doc size={12} /> inspector
        </button>
      )}
      <Legend levelName={props.scope.levels[view.level]?.plural ?? ""} />
      <ZoomControls
        scale={view.scale}
        disabled={drag !== null}
        onZoomIn={() => props.onView(zoomStep(view, BUTTON_STEP, props.lastLevel))}
        onZoomOut={() => props.onView(zoomStep(view, 1 / BUTTON_STEP, props.lastLevel))}
        onReset={props.onResetView}
      />
    </div>
  );
}
