/** The controls that float over the canvas: legend, zoom, and breadcrumb. */
import { Fragment, type PointerEvent } from "react";
import { ancestorIds, type TreeIndex } from "../../state/specMap/tree";
import { COMPLETENESS_STATES } from "../../state/specMap/types";
import { Icon } from "../icons";
import { scaleLabel } from "./zoom";

/** A press on a control must not start a pan of the canvas beneath it. */
export const keepFromCanvas = (e: PointerEvent) => e.stopPropagation();

/** SMZ-FR-AILK */
export function Legend({ levelName }: { levelName: string }) {
  return (
    <div className="smap-legend" onPointerDown={keepFromCanvas}>
      <span className="smap-legend__level">{levelName}</span>
      <span className="smap-legend__divider" aria-hidden="true" />
      {COMPLETENESS_STATES.map((state) => (
        <span key={state} className="smap-legend__item">
          <span className="smap-dot" data-state={state} />
          {state}
        </span>
      ))}
    </div>
  );
}

/** SMZ-FR-JWXA / SMZ-FR-CUTY */
export function ZoomControls({
  scale,
  disabled,
  onZoomIn,
  onZoomOut,
  onReset,
}: {
  scale: number;
  disabled: boolean;
  onZoomIn: () => void;
  onZoomOut: () => void;
  onReset: () => void;
}) {
  return (
    <div className="smap-zoom" onPointerDown={keepFromCanvas}>
      <button
        type="button"
        className="smap-zoom__btn"
        aria-label="Zoom out"
        title="Zoom out"
        disabled={disabled}
        onClick={onZoomOut}
      >
        <Icon.Minus size={13} />
      </button>
      <button
        type="button"
        className="smap-zoom__btn"
        aria-label="Zoom in"
        title="Zoom in"
        disabled={disabled}
        onClick={onZoomIn}
      >
        <Icon.Plus size={13} />
      </button>
      <button
        type="button"
        className="smap-zoom__btn smap-zoom__label"
        aria-label="Reset view"
        title="Fit"
        disabled={disabled}
        onClick={onReset}
      >
        {scaleLabel(scale)}
      </button>
    </div>
  );
}

/** SMZ-FR-BRCT */
export function Breadcrumb({
  tree,
  focusId,
  onFocus,
}: {
  tree: TreeIndex;
  focusId: string | null;
  onFocus: (id: string | null) => void;
}) {
  const focused = focusId ? tree.nodes.get(focusId) : undefined;
  if (!focused) return null;
  return (
    <nav className="smap-crumbs" aria-label="Focused subtree" onPointerDown={keepFromCanvas}>
      <button type="button" className="smap-crumbs__item" onClick={() => onFocus(null)}>
        specifications
      </button>
      {ancestorIds(tree, focused.id).map((id) => (
        <Fragment key={id}>
          <span className="smap-crumbs__sep" aria-hidden="true">
            →
          </span>
          <button type="button" className="smap-crumbs__item" onClick={() => onFocus(id)}>
            {tree.nodes.get(id)!.label}
          </button>
        </Fragment>
      ))}
      <span className="smap-crumbs__sep" aria-hidden="true">
        →
      </span>
      <span className="smap-crumbs__current" aria-current="location">
        {focused.label}
      </span>
    </nav>
  );
}
