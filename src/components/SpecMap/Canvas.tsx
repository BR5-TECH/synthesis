import { useEffect, useRef, useState, type PointerEvent } from "react";
import type { TreeIndex } from "../../state/specMap/tree";
import type { Rollup } from "../../state/specMap/rollups";
import type { MapRef } from "../../state/specMap/types";
import type { MapView } from "../../state/specMap/session";
import { Icon } from "../icons";
import type { MapLayout, MapScope } from "./layout";
import type { MapEdges } from "./edges";
import { EdgeLayer } from "./EdgeLayer";
import { NodeLayer, type NodeHandlers } from "./nodes";
import { HoverCard } from "./HoverCard";
import type { SpecLinks } from "../../state/delivery/specLinks";
import { Breadcrumb, Legend, ZoomControls, keepFromCanvas } from "./CanvasChrome";
import { bindCanvasGestures } from "../canvasGestures";
import { BUTTON_STEP, WHEEL_IN, WHEEL_OUT, pinchZoom, zoomStep } from "./zoom";

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
  /** SDL-FR-SGQM: the delivery links, or null where none show. */
  links: SpecLinks | null;
  /** SDL-FR-KXCY: the **unplanned** filter is on. */
  unplanned: boolean;
  focusId: string | null;
  handlers: NodeHandlers;
  onView: (view: MapView) => void;
  onResetView: () => void;
  onFocus: (id: string | null) => void;
  showInspectorButton: boolean;
  /** SMI-FR-QWOP: the inspector is open as an overlay over this canvas. */
  inspectorOverlay: boolean;
  onOpenInspector: () => void;
}

/** SMZ-FR-DPWC: the canvas, its one transformed stage, and the controls over it. */
export function Canvas(props: CanvasProps) {
  const { tree, layout, view, width, height } = props;
  const pan = useRef<{ x: number; y: number; view: MapView } | null>(null);
  const [panning, setPanning] = useState(false);
  const latest = useRef(props);
  latest.current = props;

  // SMZ-FR-ZQLP: a wheel over the canvas zooms it by one step and scrolls
  // nothing else. SMZ-FR-DLES: a pinch, or a wheel with Ctrl or ⌘, zooms in
  // proportion to the gesture, about the point under the pointer.
  useEffect(() => {
    const el = props.canvasEl;
    if (!el) return;
    return bindCanvasGestures(el, {
      onPinch: (factor, clientX, clientY) => {
        const current = latest.current;
        const rect = el.getBoundingClientRect();
        const anchor = {
          x: clientX - rect.left - rect.width / 2,
          y: clientY - rect.top - rect.height / 2,
        };
        current.onView(pinchZoom(current.view, factor, anchor, current.lastLevel));
      },
      onScroll: (e) => {
        e.preventDefault();
        if (e.deltaY === 0) return;
        const current = latest.current;
        current.onView(
          zoomStep(current.view, e.deltaY > 0 ? WHEEL_OUT : WHEEL_IN, current.lastLevel),
        );
      },
    });
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
          links={props.links}
          unplanned={props.unplanned}
          related={props.edges.related}
          handlers={props.handlers}
        />
      </div>
      {props.hover && !panning && (
        <HoverCard
          tree={tree}
          layout={layout}
          rollups={props.rollups}
          target={props.hover}
          links={props.links}
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
        onZoomIn={() => props.onView(zoomStep(view, BUTTON_STEP, props.lastLevel))}
        onZoomOut={() => props.onView(zoomStep(view, 1 / BUTTON_STEP, props.lastLevel))}
        onReset={props.onResetView}
      />
    </div>
  );
}
