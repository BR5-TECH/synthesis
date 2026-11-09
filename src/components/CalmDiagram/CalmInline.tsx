/**
 * CAD-FR-IPRR, CAD-FR-IRIT, CAD-FR-JBUT: the inline canvas of a CALM fence.
 * It is a static picture: the drawing scales down to the width of the text
 * column, and its frame is between 160 px and 480 px high. The fence around
 * it owns the click and the keys that show the source (CAD-FR-IRPP).
 */
import { useLayoutEffect, useRef, useState } from "react";

import { NO_NODES_STATEMENT, type CalmLayout } from "../../calm/types";
import { CalmCanvas } from "./CalmCanvas";

/** CAD-FR-IPRR: the bounds of the height of the inline canvas. */
export const INLINE_MIN_HEIGHT = 160;
export const INLINE_MAX_HEIGHT = 480;

/** CAD-FR-IPRR: the scale and the frame height of a drawing in a column. */
export function inlineFit(
  column: number,
  natural: { width: number; height: number },
): { scale: number; height: number } {
  const across = column > 0 ? column / natural.width : 1;
  const scale = Math.min(1, across, INLINE_MAX_HEIGHT / natural.height);
  const height = Math.min(
    INLINE_MAX_HEIGHT,
    Math.max(INLINE_MIN_HEIGHT, Math.round(natural.height * scale)),
  );
  return { scale, height };
}

export function CalmInline({ layout }: { layout: CalmLayout }) {
  const frameRef = useRef<HTMLDivElement>(null);
  const [column, setColumn] = useState(0);

  useLayoutEffect(() => {
    const el = frameRef.current;
    if (!el) return;
    const measure = () => setColumn(el.clientWidth);
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  if (layout.graph.nodes.length === 0) {
    return (
      <div
        ref={frameRef}
        className="calm-inline calm-inline--empty"
        style={{ height: INLINE_MIN_HEIGHT }}
      >
        {NO_NODES_STATEMENT}
      </div>
    );
  }
  const { scale, height } = inlineFit(column, layout);
  return (
    <div ref={frameRef} className="calm-inline" style={{ height }} data-scale={scale.toFixed(3)}>
      <div
        className="calm-inline__stage"
        // CAD-FR-IRPP: the fence names the picture; its parts are not controls.
        aria-hidden="true"
        style={{
          width: layout.width,
          height: layout.height,
          // The scale is about the top-left corner, so the offsets that
          // centre the drawing use its scaled size.
          left: Math.max(0, (column - layout.width * scale) / 2),
          top: Math.max(0, (height - layout.height * scale) / 2),
          transform: `scale(${scale})`,
        }}
      >
        <CalmCanvas layout={layout} />
      </div>
    </div>
  );
}
