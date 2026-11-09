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

/** The side of `box` that `point` lies on. */
function sideOf(box: Box, point: { x: number; y: number }): Side {
  const distances: Array<[Side, number]> = [
    ["left", Math.abs(point.x - box.x)],
    ["right", Math.abs(point.x - (box.x + box.width))],
    ["top", Math.abs(point.y - box.y)],
    ["bottom", Math.abs(point.y - (box.y + box.height))],
  ];
  return distances.reduce((best, next) => (next[1] < best[1] ? next : best))[0];
}

/** An orthogonal route as a path, with small round corners. */
function routePath(points: Array<{ x: number; y: number }>): string {
  if (points.length === 0) return "";
  const radius = 6;
  let path = `M ${points[0].x} ${points[0].y}`;
  for (let i = 1; i < points.length - 1; i++) {
    const [a, b, c] = [points[i - 1], points[i], points[i + 1]];
    const inLength = Math.hypot(b.x - a.x, b.y - a.y);
    const outLength = Math.hypot(c.x - b.x, c.y - b.y);
    const r = Math.min(radius, inLength / 2, outLength / 2);
    const before = { x: b.x - ((b.x - a.x) / (inLength || 1)) * r, y: b.y - ((b.y - a.y) / (inLength || 1)) * r };
    const after = { x: b.x + ((c.x - b.x) / (outLength || 1)) * r, y: b.y + ((c.y - b.y) / (outLength || 1)) * r };
    path += ` L ${before.x} ${before.y} Q ${b.x} ${b.y} ${after.x} ${after.y}`;
  }
  const last = points[points.length - 1];
  return `${path} L ${last.x} ${last.y}`;
}

/** A point a short way from an end, for the interface label of that end. */
const besideEnd = (point: { x: number; y: number; side: Side }) => {
  const shift = 10;
  switch (point.side) {
    case "left":
      return { x: point.x - shift, y: point.y - 14, align: "end" as const };
    case "right":
      return { x: point.x + shift, y: point.y - 14, align: "start" as const };
    case "top":
      return { x: point.x + 6, y: point.y - 16, align: "start" as const };
    case "bottom":
      return { x: point.x + 6, y: point.y + 4, align: "start" as const };
  }
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
