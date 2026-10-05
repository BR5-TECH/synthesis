import { useId } from "react";
import type { MapEdges } from "./edges";

/** Large enough that no layout of a real index reaches its edge. */
const EXTENT = 20000;

const WEIGHTS = ["light", "medium", "heavy"] as const;

/**
 * SME-FR-SGUC / SME-FR-TJMY / SME-FR-VAEC: one path per edge, so every edge
 * carries its own arrowhead. Colours come from the stylesheet, so a theme change
 * recolours the edges without a new render.
 */
export function EdgeLayer({ edges }: { edges: MapEdges }) {
  const uid = useId().replace(/[^A-Za-z0-9_-]/g, "");
  const marker = (name: string) => `url(#smap-${uid}-${name})`;
  return (
    <svg
      className="smap-edges"
      width={EXTENT}
      height={EXTENT}
      viewBox={`${-EXTENT / 2} ${-EXTENT / 2} ${EXTENT} ${EXTENT}`}
      style={{ left: -EXTENT / 2, top: -EXTENT / 2 }}
      aria-hidden="true"
    >
      <defs>
        {(["dim", "hot", "bad"] as const).map((name) => (
          <marker
            key={name}
            id={`smap-${uid}-${name}`}
            viewBox="0 0 10 10"
            refX="9"
            refY="5"
            markerWidth="5"
            markerHeight="5"
            orient="auto-start-reverse"
          >
            <path className="smap-marker" data-marker={name} d="M 0 1 L 9 5 L 0 9 z" />
          </marker>
        ))}
      </defs>
      {WEIGHTS.map((weight) =>
        edges[weight].map((d, i) => (
          <path
            key={`${weight}-${i}`}
            className="smap-edge"
            data-weight={weight}
            d={d}
            markerEnd={marker("dim")}
          />
        )),
      )}
      {edges.unresolved.map((d, i) => (
        <path
          key={`unresolved-${i}`}
          className="smap-edge"
          data-weight="unresolved"
          d={d}
          markerEnd={marker("bad")}
        />
      ))}
      {edges.active.map((d, i) => (
        <path
          key={`active-${i}`}
          className="smap-edge"
          data-weight="active"
          d={d}
          markerEnd={marker("hot")}
        />
      ))}
    </svg>
  );
}
