/**
 * CAD-FR-LLNQ – CAD-FR-NNHM: the interactive CALM canvas, shared by the
 * diagram viewer overlay and the Diagram view of an Architecture tab. It holds
 * the canvas with the fit, zoom, and pan of the Mermaid viewer (CAD-FR-MBML),
 * the flow selector (CAD-FR-LQLI), and the details panel (CAD-FR-KUBU).
 */
import { useEffect, useMemo, useRef, useState, type RefObject } from "react";

import { logDebug } from "../../logging";
import type { CalmLayout } from "../../calm/types";
import { NO_NODES_STATEMENT } from "../../calm/types";
import { Icon } from "../icons";
import { usePanZoom, type PanZoom } from "../DiagramEditor/usePanZoom";
import { CalmCanvas } from "./CalmCanvas";
import { CalmDetails } from "./CalmDetails";
import {
  absoluteBoxes,
  flowHighlight,
  selectionExists,
  type CalmSelection,
} from "./model";

export { CALM_DIAGRAM_LABEL } from "./labels";
import { CALM_DIAGRAM_LABEL } from "./labels";
/** CAD-FR-LQLI: the accessible name of the flow selector and its first option. */
export const FLOW_LABEL = "Flow";
export const NO_FLOW = "No flow";
export const FIT_LABEL = "Fit to window";

/** The state of one interactive canvas. */
export interface CalmInteraction {
  layout: CalmLayout;
  areaRef: RefObject<HTMLDivElement | null>;
  view: PanZoom;
  flowId: string | null;
  setFlowId: (id: string | null) => void;
  selected: CalmSelection | null;
  select: (selection: CalmSelection | null) => void;
  /** CAD-FR-MXHO: selects an item and pans so that it shows. */
  reveal: (selection: CalmSelection) => void;
}

/**
 * The state of an interactive canvas of `layout`. CAD-FR-MZSV: it starts with
 * no flow and nothing selected. CAD-FR-NNHM: a new layout keeps the flow and
 * the selection while it still holds them.
 */
export function useCalmInteraction(layout: CalmLayout): CalmInteraction {
  const areaRef = useRef<HTMLDivElement>(null);
  const natural = useMemo(() => ({ width: layout.width, height: layout.height }), [layout]);
  const view = usePanZoom(areaRef, natural, layout);
  const [flowId, setFlowId] = useState<string | null>(null);
  const [selected, select] = useState<CalmSelection | null>(null);

  useEffect(() => {
    if (flowId !== null && !layout.graph.flows.some((flow) => flow.id === flowId)) {
      setFlowId(null);
    }
    if (selected !== null && !selectionExists(layout, selected)) select(null);
  }, [layout, flowId, selected]);

  const { reveal: revealBox } = view;
  const reveal = (selection: CalmSelection) => {
    select(selection);
    const boxes = absoluteBoxes(layout);
    if (selection.kind === "node") {
      const box = boxes[selection.id];
      if (box) revealBox(box);
      return;
    }
    const edge = layout.graph.edges.find((item) => item.id === selection.id);
    const from = edge ? boxes[edge.source] : undefined;
    const to = edge ? boxes[edge.target] : undefined;
    if (!from || !to) return;
    const x = Math.min(from.x, to.x);
    const y = Math.min(from.y, to.y);
    revealBox({
      x,
      y,
      width: Math.max(from.x + from.width, to.x + to.width) - x,
      height: Math.max(from.y + from.height, to.y + to.height) - y,
    });
  };

  return { layout, areaRef, view, flowId, setFlowId, selected, select, reveal };
}

/** CAD-FR-LQLI: the flow selector; absent when the document has no flows. */
export function CalmFlowSelect({ ix }: { ix: CalmInteraction }) {
  const flows = ix.layout.graph.flows;
  if (flows.length === 0) return null;
  return (
    <label className="calm-flow-select">
      <span>{FLOW_LABEL}</span>
      <select
        aria-label={FLOW_LABEL}
        value={ix.flowId ?? ""}
        onChange={(event) => {
          const next = event.target.value || null;
          logDebug(["frontend"], "a CALM flow was chosen", {
            chosen: next !== null,
            steps: flows.find((flow) => flow.id === next)?.steps.length ?? 0,
          });
          ix.setFlowId(next);
        }}
      >
        <option value="">{NO_FLOW}</option>
        {flows.map((flow) => (
          <option key={flow.id} value={flow.id}>
            {flow.name}
          </option>
        ))}
      </select>
    </label>
  );
}

/** CAD-FR-MBML: the zoom readout and the Fit control of `EDT-editor.md` EDT-FR-CCZU. */
export function CalmZoomControls({ ix }: { ix: CalmInteraction }) {
  return (
    <>
      <span className="diagram-viewer__zoom" data-testid="calm-zoom">
        {Math.round(ix.view.transform.scale * 100)}%
      </span>
      <button
        type="button"
        className="btn btn--ghost btn--sm diagram-viewer__fit"
        aria-label={FIT_LABEL}
        title={FIT_LABEL}
        onClick={ix.view.fit}
      >
        <Icon.Fit size={12} />
        <span>Fit</span>
      </button>
    </>
  );
}

/** CAD-FR-LLNQ, CAD-FR-KUBU: the diagram area and the details panel. */
export function CalmStage({ ix }: { ix: CalmInteraction }) {
  const { layout, view } = ix;
  const flow = layout.graph.flows.find((item) => item.id === ix.flowId) ?? null;
  const highlight = useMemo(() => (flow ? flowHighlight(flow) : null), [flow]);
  const empty = layout.graph.nodes.length === 0;
  return (
    <div className="calm-stage">
      <div
        ref={ix.areaRef}
        className={`calm-stage__area diagram-viewer__area doc${view.dragging ? " diagram-viewer__area--dragging" : ""}`}
        role="group"
        aria-label={CALM_DIAGRAM_LABEL}
        tabIndex={0}
        data-fitted={view.fitted ? "true" : undefined}
        onPointerDown={view.onPointerDown}
        onKeyDown={view.onKeyDown}
      >
        {empty ? (
          <p className="calm-stage__empty">{NO_NODES_STATEMENT}</p>
        ) : (
          <div
            className="calm-stage__drawing"
            data-testid="calm-stage"
            style={{
              transform: `translate(${view.transform.x}px, ${view.transform.y}px) scale(${view.transform.scale})`,
            }}
          >
            <CalmCanvas
              layout={layout}
              highlight={highlight}
              selected={ix.selected}
              onSelect={ix.select}
            />
          </div>
        )}
      </div>
      <CalmDetails
        graph={layout.graph}
        selected={ix.selected}
        flow={flow}
        onSelect={ix.reveal}
      />
    </div>
  );
}
