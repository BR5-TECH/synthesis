import type { ReactNode } from "react";

import { CalmInline } from "../CalmDiagram/CalmInline";
import { useCalmRead } from "../CalmDiagram/useCalmRead";
import { CALM_DIAGRAM_LABEL } from "../markdownCalm";

/**
 * DFV-FR-EGFA / EDT-FR-LEUI, CAD-FR-PFNM: a CALM fence drawn outside an editing
 * surface. It shows the inline canvas and takes no input. While its source
 * reads, and when the read fails, the fence shows its source as the static
 * block draws it, with the error under it (CAD-FR-ITUS).
 */
export function CalmStatic({ source, children }: { source: string; children: ReactNode }) {
  const { settled } = useCalmRead(source);
  if (settled?.state === "rendered") {
    return (
      <pre data-language="calm" className="mermaid-block mermaid-block--diagram" data-diagram-view="diagram">
        <span className="mermaid-head">
          <span className="mermaid-diagram calm-figure" role="img" aria-label={CALM_DIAGRAM_LABEL}>
            <CalmInline layout={settled.layout} />
          </span>
        </span>
      </pre>
    );
  }
  return (
    <>
      {children}
      {settled?.state === "error" && (
        <span className="mermaid-error mermaid-error--static">
          <span className="mermaid-error__summary">Diagram error: {settled.summary}</span>
          {settled.detail !== undefined && (
            <span className="mermaid-error__detail">{settled.detail}</span>
          )}
        </span>
      )}
    </>
  );
}
