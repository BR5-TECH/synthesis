/**
 * The diagram viewer overlay (`EDT-editor.md` EDT-FR-KMSI – EDT-FR-EAOC).
 *
 * The frame, the geometry, and the focus trap are the ones of the diagram
 * editor overlay (`OverlayFrame`). The content is the diagram of the fence
 * alone, which the author zooms and pans. Zoom and pan are view state of this
 * opening: they change no byte of the buffer (EDT-FR-EAOC) and the next
 * opening starts fitted again (EDT-FR-JJDP).
 */
import { useEffect, useLayoutEffect, useRef, useState } from "react";

import { logDebug, logWarn } from "../../logging";
import { Icon } from "../icons";
import { diagramElement } from "../markdownMermaidRender";
import { OverlayFrame } from "./OverlayFrame";
import type { DiagramEditorRequest } from "./store";
import { useDiagramRender, type RenderedDiagram } from "./useDiagramRender";
import { useFence } from "./useFence";
import { sameSize, usePanZoom } from "./usePanZoom";
import { naturalSize, type Size } from "./viewerGeometry";

/** EDT-FR-CCZU: the accessible names of the viewer and its controls. */
export const DIAGRAM_VIEWER_LABEL = "View Mermaid diagram";
export const FIT_LABEL = "Fit to window";
/** EDT-FR-VWNJ: the accessible name of the diagram area. */
export const VIEWER_DIAGRAM_LABEL = "Mermaid diagram";

export function DiagramViewerOverlay({ request }: { request: DiagramEditorRequest }) {
  const areaRef = useRef<HTMLDivElement>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const shownSource = useRef<string | null>(null);
  const { source, finish } = useFence(request, () => shownSource.current);
  shownSource.current = source;
  const { empty, settled, lastRendered } = useDiagramRender(source);

  // EDT-FR-JRDD: a source that stops being a valid diagram closes the viewer.
  // A render in progress is not an error: the viewer keeps what it shows.
  const invalid = empty || settled?.state === "error";
  useEffect(() => {
    if (!invalid) return;
    logDebug(["frontend"], "the diagram viewer has no valid diagram", { empty });
    finish("not-a-diagram");
  }, [invalid, empty, finish]);

  // EDT-FR-LVFQ: the diagram that shows. While a changed source renders, the
  // last diagram of this opening stays.
  const shown: RenderedDiagram | null =
    settled?.state === "rendered" ? settled : invalid ? null : lastRendered;

  const [natural, setNatural] = useState<Size | null>(null);

  // EDT-FR-VWNJ: the drawing of the fence, with its natural size, so the
  // transform alone sets the size on screen.
  useLayoutEffect(() => {
    const stage = stageRef.current;
    if (!stage) return;
    const node = shown ? diagramElement(shown.svg, shown.id) : null;
    let size: Size | null = null;
    if (node) {
      size = naturalSize(node);
      if (size) {
        node.setAttribute("width", String(size.width));
        node.setAttribute("height", String(size.height));
      }
      (node as SVGElement).style.removeProperty("max-width");
      if (!size) logWarn(["frontend"], "the diagram viewer cannot find the size of the diagram");
    }
    stage.replaceChildren(...(node ? [node] : []));
    setNatural((last) => (sameSize(last, size) ? last : size));
  }, [shown]);

  const { transform, fitted, dragging, fit, onKeyDown, onPointerDown } = usePanZoom(
    areaRef,
    natural,
    shown,
  );

  // EDT-FR-ELUJ: focus moves into the diagram area.
  useEffect(() => {
    areaRef.current?.focus();
  }, []);

  return (
    <OverlayFrame
      label={DIAGRAM_VIEWER_LABEL}
      className="diagram-viewer"
      scrimTestId="diagram-viewer-scrim"
      home={areaRef}
      onClose={finish}
      onKeyDown={onKeyDown}
      controls={
        <>
          <span className="diagram-viewer__zoom" data-testid="diagram-viewer-zoom">
            {Math.round(transform.scale * 100)}%
          </span>
          <button
            type="button"
            className="btn btn--ghost btn--sm diagram-viewer__fit"
            aria-label={FIT_LABEL}
            title={FIT_LABEL}
            onClick={fit}
          >
            <Icon.Fit size={12} />
            <span>Fit</span>
          </button>
        </>
      }
    >
      <div
        ref={areaRef}
        className={`diagram-viewer__area doc${dragging ? " diagram-viewer__area--dragging" : ""}`}
        role="img"
        aria-label={VIEWER_DIAGRAM_LABEL}
        tabIndex={0}
        data-fitted={fitted ? "true" : undefined}
        onPointerDown={onPointerDown}
        // EDT-FR-JBZU: a link in a diagram does nothing here either.
        onClick={(event) => event.preventDefault()}
      >
        <div
          ref={stageRef}
          className="mermaid-diagram diagram-viewer__stage"
          data-testid="diagram-viewer-stage"
          style={{
            transform: `translate(${transform.x}px, ${transform.y}px) scale(${transform.scale})`,
          }}
        />
      </div>
    </OverlayFrame>
  );
}
