/**
 * CLM-FR-GDML, CLM-FR-ERXV, CAD-FR-QHEJ: the geometry of a CALM edge that the
 * canvas and the print drawing share, so both draw the same route and put the
 * interface labels in the same place.
 */
import type { Box } from "../../calm/types";
import type { Side } from "./model";

/** The side of `box` that `point` lies on. */
export function sideOf(box: Box, point: { x: number; y: number }): Side {
  const distances: Array<[Side, number]> = [
    ["left", Math.abs(point.x - box.x)],
    ["right", Math.abs(point.x - (box.x + box.width))],
    ["top", Math.abs(point.y - box.y)],
    ["bottom", Math.abs(point.y - (box.y + box.height))],
  ];
  return distances.reduce((best, next) => (next[1] < best[1] ? next : best))[0];
}

/** An orthogonal route as a path, with small round corners. */
export function routePath(points: Array<{ x: number; y: number }>): string {
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
export const besideEnd = (point: { x: number; y: number; side: Side }) => {
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
