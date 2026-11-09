/**
 * CAD-FR-KUYJ: a static CALM drawing fitted into the box that holds it, at
 * most at its natural size, as the diagram pane of the editor overlay shows it.
 */
import { useLayoutEffect, useRef, useState } from "react";

import { NO_NODES_STATEMENT, type CalmLayout } from "../../calm/types";
import { CalmCanvas } from "./CalmCanvas";

export function CalmFitted({ layout }: { layout: CalmLayout }) {
  const boxRef = useRef<HTMLDivElement>(null);
  const [box, setBox] = useState({ width: 0, height: 0 });

  useLayoutEffect(() => {
    const el = boxRef.current;
    if (!el) return;
    const measure = () => setBox({ width: el.clientWidth, height: el.clientHeight });
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const scale =
    box.width > 0 && box.height > 0
      ? Math.min(1, box.width / layout.width, box.height / layout.height)
      : 1;
  return (
    <div ref={boxRef} className="calm-fitted">
      {layout.graph.nodes.length === 0 ? (
        <p className="calm-fitted__empty">{NO_NODES_STATEMENT}</p>
      ) : (
        <div
          className="calm-fitted__stage"
          style={{ width: layout.width, height: layout.height, transform: `scale(${scale})` }}
        >
          <CalmCanvas layout={layout} />
        </div>
      )}
    </div>
  );
}
