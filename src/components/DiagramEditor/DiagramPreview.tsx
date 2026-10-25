/**
 * EDT-FR-QSOQ, EDT-FR-STFK, EDT-FR-UDVE: the right pane of the diagram editor
 * overlay — the live diagram of the source in the left pane.
 *
 * It uses the same render, the same sanitised drawing, and the same error
 * messages as a diagram fence on the page (EDT-FR-JIBV, EDT-FR-JBZU,
 * EDT-FR-XKMP), so the pane and the fence behind the scrim always agree.
 */
import { useEffect, useLayoutEffect, useRef, useState } from "react";

import { logDebug } from "../../logging";
import {
  cachedDiagram,
  currentTheme,
  diagramElement,
  releaseDiagrams,
  renderDiagram,
  showDiagrams,
  subscribeTheme,
  type DiagramResult,
} from "../markdownMermaidRender";

/** EDT-FR-UDVE: the statement the pane shows when it has no diagram. */
export const NO_DIAGRAM = "No diagram to show";
/** EDT-FR-HVGG: the accessible name of the pane. */
export const PREVIEW_LABEL = "Diagram preview";

type Rendered = Extract<DiagramResult, { state: "rendered" }>;
type Failed = Extract<DiagramResult, { state: "error" }>;

export function DiagramPreview({ source }: { source: string }) {
  const [theme, setTheme] = useState(currentTheme);
  useEffect(() => subscribeTheme(setTheme), []);

  const empty = source.trim() === "";
  /**
   * The last result that came back during this opening. While a render is in
   * progress, the pane keeps what it shows (EDT-FR-QSOQ).
   */
  const [settled, setSettled] = useState<DiagramResult | null>(() =>
    empty ? null : (cachedDiagram(source, theme) ?? null),
  );
  /** EDT-FR-STFK: the last diagram that rendered during this opening. */
  const [lastRendered, setLastRendered] = useState<Rendered | null>(() =>
    settled?.state === "rendered" ? settled : null,
  );

  // The diagram this pane asks for stays in the cache while it shows.
  const [owner] = useState(() => ({}));
  useEffect(() => {
    showDiagrams(owner, empty ? [] : [[source, theme]]);
  }, [owner, source, theme, empty]);
  useEffect(() => () => releaseDiagrams(owner), [owner]);

  useEffect(() => {
    if (empty) {
      // An empty source shows no diagram, so no earlier result stands for it.
      setSettled(null);
      return;
    }
    const take = (result: DiagramResult) => {
      setSettled(result);
      if (result.state === "rendered") setLastRendered(result);
    };
    const known = cachedDiagram(source, theme);
    if (known) {
      take(known);
      return;
    }
    let live = true;
    renderDiagram(source, theme, () => live)
      .then((result) => {
        if (live && result) take(result);
      })
      .catch((error: unknown) => {
        logDebug(["frontend"], "a diagram editor render request failed", {
          error: error instanceof Error ? error.name : typeof error,
        });
      });
    return () => {
      live = false;
    };
  }, [source, theme, empty]);

  const failed: Failed | null = !empty && settled?.state === "error" ? settled : null;
  // EDT-FR-STFK: an invalid source keeps the last diagram, dimmed.
  // EDT-FR-UDVE: nothing to show until a result came back for a source that
  // is not empty.
  let shown: Rendered | null = null;
  if (!empty && settled !== null) {
    shown = settled.state === "rendered" ? settled : lastRendered;
  }
  const stale = failed !== null && shown !== null;

  const figure = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const el = figure.current;
    if (!el) return;
    const node = shown ? diagramElement(shown.svg, shown.id) : null;
    el.replaceChildren(...(node ? [node] : []));
  }, [shown]);

  return (
    <div className="diagram-editor__preview doc" role="region" aria-label={PREVIEW_LABEL}>
      {shown !== null && (
        <div
          ref={figure}
          className={`mermaid-diagram diagram-editor__figure${stale ? " diagram-editor__figure--stale" : ""}`}
          role="img"
          aria-label="Mermaid diagram"
          data-stale={stale ? "true" : undefined}
          // EDT-FR-JBZU: a link in a diagram does nothing here either.
          onClick={(event) => event.preventDefault()}
        />
      )}
      {/* EDT-FR-UDVE: also while the first render of this opening is in
          progress, because no diagram has rendered yet. */}
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
