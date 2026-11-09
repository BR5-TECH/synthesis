/**
 * CAD-FR-KUYJ: the diagram pane of the editor overlay on a CALM fence. It
 * follows `EDT-editor.md` EDT-FR-QSOQ, EDT-FR-STFK, and EDT-FR-UDVE as the
 * Mermaid pane does: it keeps the last diagram of this opening while a read
 * runs, and dims it under the error message when the source has an error.
 */
import { NO_DIAGRAM, PREVIEW_LABEL } from "../DiagramEditor/DiagramPreview";
import { CalmFitted } from "./CalmFitted";
import { CALM_DIAGRAM_LABEL } from "./CalmInteractive";
import { useCalmRead, type RenderedCalm } from "./useCalmRead";

export function CalmPreview({ source }: { source: string }) {
  const { empty, settled, lastRendered } = useCalmRead(source);
  const failed = !empty && settled?.state === "error" ? settled : null;
  let shown: RenderedCalm | null = null;
  if (!empty && settled !== null) {
    shown = settled.state === "rendered" ? settled : lastRendered;
  }
  const stale = failed !== null && shown !== null;
  return (
    <div className="diagram-editor__preview doc" role="region" aria-label={PREVIEW_LABEL}>
      {shown !== null && (
        <div
          className={`diagram-editor__figure calm-preview${stale ? " diagram-editor__figure--stale" : ""}`}
          role="img"
          aria-label={CALM_DIAGRAM_LABEL}
          data-stale={stale ? "true" : undefined}
        >
          <CalmFitted layout={shown.layout} />
        </div>
      )}
      {shown === null && <p className="diagram-editor__statement">{NO_DIAGRAM}</p>}
      {failed !== null && (
        <div className="mermaid-error diagram-editor__error" role="alert">
          <span className="mermaid-error__summary">Diagram error: {failed.summary}</span>
          {failed.detail !== undefined && (
            <span className="mermaid-error__detail">{failed.detail}</span>
          )}
        </div>
      )}
    </div>
  );
}
