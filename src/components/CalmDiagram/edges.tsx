/**
 * CLM-FR-ERXV, CLM-FR-ESCS, CAD-FR-KSEJ: the edges of a CALM canvas. An edge
 * follows the route of the layout, which keeps nodes off it and off its label.
 * An edge that the layout did not route (between a node and the group that
 * holds it) runs between the borders of its two nodes. Its label shows the
 * description, the protocol as a chip, and, in a chosen flow, the number of
 * each step.
 */
import {
  BaseEdge,
  EdgeLabelRenderer,
  Position,
  getBezierPath,
  type Edge,
  type EdgeProps,
} from "@xyflow/react";

import type { Box, CalmEdgeView, CalmRoute } from "../../calm/types";
import { besideEnd, routePath, sideOf } from "./geometry";
import { exitPoint, type Side, type StepMark } from "./model";

export interface CalmEdgeData extends Record<string, unknown> {
  edge: CalmEdgeView;
  /** The boxes of the two nodes in the coordinates of the diagram. */
  from: Box;
  to: Box;
  /** CLM-FR-GDML: the route and the label box that the layout gave, if any. */
  route?: CalmRoute;
  /** CAD-FR-KSEJ: the steps of the chosen flow on this edge. */
  steps: StepMark[];
  dim: boolean;
  selected: boolean;
  /** CAD-FR-KUBU: selects this edge; absent on a static canvas. */
  onPick?: () => void;
}

export type CalmFlowEdge = Edge<CalmEdgeData, "calmEdge">;

const POSITIONS: Record<Side, Position> = {
  left: Position.Left,
  right: Position.Right,
  top: Position.Top,
  bottom: Position.Bottom,
};

export function CalmEdgeLine({ id, data, markerEnd, markerStart }: EdgeProps<CalmFlowEdge>) {
  if (!data) return null;
  const { edge, from, to, route, steps, dim, selected, onPick } = data;
  let path: string;
  let start: { x: number; y: number; side: Side };
  let end: { x: number; y: number; side: Side };
  let labelStyle: React.CSSProperties;
  if (route && route.points.length >= 2) {
    const first = route.points[0];
    const last = route.points[route.points.length - 1];
    path = routePath(route.points);
    start = { ...first, side: sideOf(from, first) };
    end = { ...last, side: sideOf(to, last) };
    const box = route.label;
    labelStyle = box
      ? { transform: `translate(${box.x}px, ${box.y}px)`, width: box.width, height: box.height }
      : { transform: `translate(-50%, -50%) translate(${(first.x + last.x) / 2}px, ${(first.y + last.y) / 2}px)` };
  } else {
    start = exitPoint(from, to);
    end = exitPoint(to, from);
    const [bezier, labelX, labelY] = getBezierPath({
      sourceX: start.x,
      sourceY: start.y,
      sourcePosition: POSITIONS[start.side],
      targetX: end.x,
      targetY: end.y,
      targetPosition: POSITIONS[end.side],
    });
    path = bezier;
    labelStyle = { transform: `translate(-50%, -50%) translate(${labelX}px, ${labelY}px)` };
  }
  const inFlow = steps.length > 0;
  const className = [
    "calm-edge",
    `calm-edge--${edge.kind}`,
    dim ? "calm-dim" : "",
    inFlow ? "calm-in-flow" : "",
    selected ? "calm-selected" : "",
  ]
    .filter(Boolean)
    .join(" ");
  const hasLabel = edge.label !== undefined || edge.protocol !== undefined || inFlow;
  const sourceLabel = besideEnd(start);
  const targetLabel = besideEnd(end);
  return (
    <>
      <BaseEdge
        id={id}
        path={path}
        className={className}
        markerEnd={markerEnd}
        markerStart={markerStart}
        interactionWidth={14}
      />
      <EdgeLabelRenderer>
        {hasLabel && (
          <div
            className={`calm-edge-label ${className}`}
            data-calm-edge={edge.id}
            onClick={
              onPick
                ? (event) => {
                    event.stopPropagation();
                    onPick();
                  }
                : undefined
            }
            style={labelStyle}
          >
            {steps.map((step, index) => (
              <span
                key={index}
                className="calm-step"
                title={step.reversed ? "Step against the arrow" : "Step"}
              >
                {step.sequence}
              </span>
            ))}
            {edge.label !== undefined && <span className="calm-edge-label__text">{edge.label}</span>}
            {edge.protocol !== undefined && <span className="calm-chip">{edge.protocol}</span>}
          </div>
        )}
        {edge.sourceInterfaces.length > 0 && (
          <div
            className={`calm-end-label calm-end-label--${sourceLabel.align}${dim ? " calm-dim" : ""}`}
            style={{ transform: `translate(${sourceLabel.x}px, ${sourceLabel.y}px)` }}
          >
            {edge.sourceInterfaces.join(", ")}
          </div>
        )}
        {edge.targetInterfaces.length > 0 && (
          <div
            className={`calm-end-label calm-end-label--${targetLabel.align}${dim ? " calm-dim" : ""}`}
            style={{ transform: `translate(${targetLabel.x}px, ${targetLabel.y}px)` }}
          >
            {edge.targetInterfaces.join(", ")}
          </div>
        )}
      </EdgeLabelRenderer>
    </>
  );
}

export const CALM_EDGE_TYPES = { calmEdge: CalmEdgeLine };
