/**
 * EDT-FR-NZLC, EDT-FR-JMTD: one pane of the comparison pop-up — a rendered
 * diagram under a heading, or the empty state that stands for a diagram that
 * does not exist.
 *
 * It renders with the same render as the fence (`useDiagramRender`), fits the
 * diagram in its area, and lets the author zoom and pan it with the hooks of the
 * diagram viewer. A source that does not render shows its source and the
 * message of the error in this pane alone.
 */
import { useLayoutEffect, useRef, useState } from "react";

import { logWarn } from "../../logging";
import { Icon } from "../icons";
import { diagramElement } from "../markdownMermaidRender";
import { NO_DIAGRAM } from "./DiagramPreview";
import { useDiagramRender, type RenderedDiagram } from "./useDiagramRender";
import { sameSize, usePanZoom } from "./usePanZoom";
import { naturalSize, type Size } from "./viewerGeometry";

/** EDT-FR-NZLC: the accessible names and the headings of the two panes. */
export const CURRENT_PANE_LABEL = "Current diagram";
export const PROPOSED_PANE_LABEL = "Proposed diagram";
export const CURRENT_HEADING = "Current";
export const PROPOSED_HEADING = "Proposed";
/** EDT-FR-NZLC: the statements of the empty states. */
export const NO_DIAGRAM_YET = "No diagram yet";
export const DIAGRAM_REMOVED = "Diagram removed";

export function ComparisonPane({
  label,
  heading,
  source,
  emptyStatement,
}: {
  label: string;
  heading: string;
  /** The source of the diagram, or null where the pane holds no diagram. */
  source: string | null;
  /** What the pane says when it holds no diagram. */
  emptyStatement: string;
}) {
  return (
    <section className="diagram-comparison__pane" role="region" aria-label={label}>
      <h3 className="diagram-comparison__heading t-ui-xs">{heading}</h3>
      {source === null ? (
        <p className="diagram-comparison__empty" data-testid="comparison-empty">
          {emptyStatement}
        </p>
      ) : (
        <PaneDiagram source={source} label={label} />
      )}
    </section>
  );
}

function PaneDiagram({ source, label }: { source: string; label: string }) {
  // EDT-FR-JMTD: while a render is in progress, the pane keeps what it shows.
  const { empty, settled, lastRendered } = useDiagramRender(source);
  const failed = !empty && settled?.state === "error" ? settled : null;
  const shown: RenderedDiagram | null =
    empty || failed
      ? null
      : settled?.state === "rendered"
        ? settled
        : lastRendered;

  if (failed !== null) {
    // EDT-FR-XTEB: the source and the message. Both are text, so nothing in the
    // message of the library or in the source is read as markup.
    return (
      <div className="diagram-comparison__failed doc">
        <div className="mermaid-error diagram-editor__error" role="alert">
          <span className="mermaid-error__summary">Diagram error: {failed.summary}</span>
          {failed.detail !== undefined && (
            <span className="mermaid-error__detail">{failed.detail}</span>
          )}
        </div>
        <pre className="diagram-comparison__source" aria-label={`${label} source`}>
          {source}
        </pre>
      </div>
    );
  }
  if (empty) return <p className="diagram-comparison__empty">{NO_DIAGRAM}</p>;
  return <PaneFigure shown={shown} label={label} />;
}

/** The drawing of a pane, which the author fits, zooms, and pans. */
function PaneFigure({ shown, label }: { shown: RenderedDiagram | null; label: string }) {
  const areaRef = useRef<HTMLDivElement>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const [natural, setNatural] = useState<Size | null>(null);

  // EDT-FR-JMTD: the drawing with its natural size, so the transform alone sets
  // the size on screen.
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
      if (!size) logWarn(["frontend"], "the comparison pane cannot find the size of the diagram");
    }
    stage.replaceChildren(...(node ? [node] : []));
    setNatural((last) => (sameSize(last, size) ? last : size));
  }, [shown]);

  const { transform, dragging, fit, onKeyDown, onPointerDown } = usePanZoom(
    areaRef,
    natural,
    shown,
  );

  return (
    <>
      <div className="diagram-comparison__tools">
        <span className="diagram-viewer__zoom">{Math.round(transform.scale * 100)}%</span>
        <button
          type="button"
          className="btn btn--ghost btn--sm diagram-viewer__fit"
          aria-label={`Fit ${label.toLowerCase()} to its pane`}
          title="Fit to pane"
          onClick={fit}
        >
          <Icon.Fit size={12} />
          <span>Fit</span>
        </button>
      </div>
      <div
        ref={areaRef}
        className={`diagram-viewer__area diagram-comparison__area doc${dragging ? " diagram-viewer__area--dragging" : ""}`}
        role="img"
        aria-label="Mermaid diagram"
        tabIndex={0}
        onPointerDown={onPointerDown}
        onKeyDown={onKeyDown}
        // EDT-FR-JBZU: a link in a diagram does nothing here either.
        onClick={(event) => event.preventDefault()}
      >
        <div
          ref={stageRef}
          className="mermaid-diagram diagram-viewer__stage"
          style={{
            transform: `translate(${transform.x}px, ${transform.y}px) scale(${transform.scale})`,
          }}
        />
      </div>
    </>
  );
}
