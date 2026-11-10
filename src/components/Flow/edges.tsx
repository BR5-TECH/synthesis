import { findItem, type FlowDocument, type FlowEdge } from "../../state/flowDocument";
import type { EdgeRoute, Frames } from "./frames";
import { edgeMidpoint, edgePath } from "./geometry";

/** FLO-FR-48: what an edge is called, by its type, in words. */
export function edgeName(doc: FlowDocument, e: FlowEdge): string {
  const nameOf = (id: string) => findItem(doc, id)?.name || id;
  return e.type === "blocks"
    ? `${nameOf(e.from)} blocks ${nameOf(e.to)}`
    : `${nameOf(e.from)} relates to ${nameOf(e.to)}`;
}

/** The route each drawn edge takes; an edge inside one collapsed container is not drawn. */
export function routeEdges(doc: FlowDocument, frames: Frames) {
  const out: Array<{ edge: FlowEdge; route: EdgeRoute }> = [];
  for (const edge of doc.edges) {
    const route = frames.route(edge.from, edge.to);
    if (route) out.push({ edge, route });
  }
  return out;
}

/**
 * FLO-FR-48 / FLO-FR-32 / FLO-FR-39: the edge layer. A `blocks` edge is solid
 * with an arrowhead at the blocked end, on the border of the item it lands on.
 * A `relates` edge is dashed with no arrowhead. The type is in the shape, and a
 * selected edge takes the selection emphasis.
 */
export function EdgeLayer({
  routes,
  selectedEdge,
  canvasId,
  preview,
  refused,
}: {
  routes: Array<{ edge: FlowEdge; route: EdgeRoute }>;
  selectedEdge: string | null;
  canvasId: string;
  preview: { d: string; snapped: boolean } | null;
  refused: EdgeRoute | null;
}) {
  const arrow = `flow-arrow-${canvasId}`;
  const arrowActive = `flow-arrow-active-${canvasId}`;
  const arrowRefused = `flow-arrow-refused-${canvasId}`;
  return (
    <svg className="flow-edges" width="1" height="1" aria-hidden="true">
      <defs>
        {[
          { id: arrow, cls: "flow-arrow" },
          { id: arrowActive, cls: "flow-arrow flow-arrow--active" },
          { id: arrowRefused, cls: "flow-arrow flow-arrow--refused" },
        ].map((m) => (
          <marker
            key={m.id}
            id={m.id}
            viewBox="0 0 10 10"
            refX="9"
            refY="5"
            markerWidth="7"
            markerHeight="7"
            markerUnits="userSpaceOnUse"
            orient="auto"
          >
            <path className={m.cls} d="M 0 0 L 10 5 L 0 10 z" />
          </marker>
        ))}
      </defs>
      {routes.map(({ edge, route }) => {
        const d = edgePath(route.ax, route.ay, route.bx, route.by, route.faceA, route.faceB);
        const active = selectedEdge === edge.id;
        return (
          <g key={edge.id}>
            <path className="flow-edge-hit" d={d} data-edge-id={edge.id} />
            <path
              className={`flow-edge flow-edge--${edge.type}`}
              data-testid={`flow-edge-path-${edge.id}`}
              data-type={edge.type}
              data-selected={active}
              d={d}
              markerEnd={
                edge.type === "blocks" ? `url(#${active ? arrowActive : arrow})` : undefined
              }
            />
          </g>
        );
      })}
      {refused && (
        <path
          className="flow-edge flow-edge--refused"
          data-testid="flow-edge-refused"
          d={edgePath(refused.ax, refused.ay, refused.bx, refused.by, refused.faceA, refused.faceB)}
          markerEnd={`url(#${arrowRefused})`}
        />
      )}
      {preview && (
        <path
          className="flow-edge flow-edge--preview"
          data-testid="flow-connection"
          data-magnet={preview.snapped}
          d={preview.d}
          markerEnd={`url(#${arrowActive})`}
        />
      )}
    </svg>
  );
}

/**
 * FLO-FR-CEVI: the handle at the middle of each edge, which selects it. It
 * shows on hover and on focus, so a keyboard author can reach every edge.
 */
export function EdgeHandles({
  doc,
  routes,
  selectedEdge,
  onSelect,
}: {
  doc: FlowDocument;
  routes: Array<{ edge: FlowEdge; route: EdgeRoute }>;
  selectedEdge: string | null;
  onSelect: (edgeId: string) => void;
}) {
  return (
    <>
      {routes.map(({ edge, route }) => {
        const mid = edgeMidpoint(route.ax, route.ay, route.bx, route.by, route.faceA, route.faceB);
        return (
          <button
            key={edge.id}
            type="button"
            className="flow-edge-handle"
            data-edge-id={edge.id}
            data-testid={`flow-edge-${edge.id}`}
            data-selected={selectedEdge === edge.id}
            aria-label={`Edge: ${edgeName(doc, edge)}`}
            aria-pressed={selectedEdge === edge.id}
            style={{ left: mid.x, top: mid.y }}
            onClick={() => onSelect(edge.id)}
          />
        );
      })}
    </>
  );
}

/**
 * FLO-FR-CEVI / FLO-FR-20: the controls of a selected edge: its type as a
 * two-way choice, Reverse on a `blocks` edge, and Remove.
 */
export function EdgePanel({
  doc,
  edge,
  at,
  inert,
  onType,
  onReverse,
  onRemove,
}: {
  doc: FlowDocument;
  edge: FlowEdge;
  at: { left: number; top: number };
  inert: boolean;
  onType: (type: "blocks" | "relates") => void;
  onReverse: () => void;
  onRemove: () => void;
}) {
  return (
    <div
      className="flow-edge-panel"
      role="group"
      aria-label={`Edge ${edgeName(doc, edge)}`}
      style={{ left: at.left, top: at.top }}
    >
      <div className="flow-seg" role="radiogroup" aria-label="Edge type">
        {(["blocks", "relates"] as const).map((type) => (
          <button
            key={type}
            type="button"
            role="radio"
            aria-checked={edge.type === type}
            className="flow-seg__opt"
            disabled={inert}
            onClick={() => {
              if (edge.type !== type) onType(type);
            }}
          >
            {type === "blocks" ? "Blocks" : "Relates"}
          </button>
        ))}
      </div>
      {edge.type === "blocks" && (
        <button type="button" className="flow-btn flow-btn--xs" disabled={inert} onClick={onReverse}>
          Reverse
        </button>
      )}
      <button
        type="button"
        className="flow-btn flow-btn--xs flow-btn--danger"
        aria-label="Remove edge"
        disabled={inert}
        onClick={onRemove}
      >
        Remove
      </button>
    </div>
  );
}
