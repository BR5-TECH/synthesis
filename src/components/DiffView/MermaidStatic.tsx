import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";

import { logDebug } from "../../logging";
import { INERT_DIAGRAM_LABEL } from "../markdownMermaid";
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

/**
 * DFV-FR-EGFA / EDT-FR-LEUI: a diagram fence drawn outside an editing surface —
 * an original block, or a target run that is not being edited. It shows the
 * diagram and takes no input: no toggle, no focus, no click.
 *
 * While the diagram renders, and when it cannot, the fence's source shows as
 * the static block always draws it, with the error under it.
 */
export function MermaidStatic({
  source,
  children,
}: {
  source: string;
  /** The block as a static fenced block, with its word marks (DFV-FR-32). */
  children: ReactNode;
}) {
  const [theme, setTheme] = useState(currentTheme);
  const [result, setResult] = useState<DiagramResult | undefined>(() =>
    cachedDiagram(source, theme),
  );
  const figure = useRef<HTMLSpanElement>(null);

  useEffect(() => subscribeTheme(setTheme), []);

  // This block's diagram stays in the cache while it shows.
  const [owner] = useState(() => ({}));
  useEffect(() => {
    showDiagrams(owner, [[source, theme]]);
  }, [owner, source, theme]);
  useEffect(() => () => releaseDiagrams(owner), [owner]);

  useEffect(() => {
    const known = cachedDiagram(source, theme);
    setResult(known);
    if (known || source.trim() === "") return;
    let live = true;
    renderDiagram(source, theme, () => live)
      .then((next) => {
        if (live && next) setResult(next);
      })
      .catch((error: unknown) => {
        logDebug(["frontend"], "a diagram render request failed", {
          error: error instanceof Error ? error.name : typeof error,
        });
      });
    return () => {
      live = false;
    };
  }, [source, theme]);

  const rendered = result?.state === "rendered" ? result : null;
  useLayoutEffect(() => {
    const el = figure.current;
    if (!el || rendered === null) return;
    const node = diagramElement(rendered.svg, rendered.id);
    el.replaceChildren(...(node ? [node] : []));
  }, [rendered]);

  if (rendered !== null) {
    return (
      <pre
        data-language="mermaid"
        className="mermaid-block mermaid-block--diagram"
        data-diagram-view="diagram"
      >
        <span className="mermaid-head">
          <span
            ref={figure}
            className="mermaid-diagram"
            role="img"
            aria-label={INERT_DIAGRAM_LABEL}
            // DFV-FR-EGFA: an inert diagram takes no click, not even on a link in it.
            onClick={(event) => event.preventDefault()}
          />
        </span>
      </pre>
    );
  }
  return (
    <>
      {children}
      {result?.state === "error" && (
        <span className="mermaid-error mermaid-error--static">
          <span className="mermaid-error__summary">Diagram error: {result.summary}</span>
          {result.detail !== undefined && (
            <span className="mermaid-error__detail">{result.detail}</span>
          )}
        </span>
      )}
    </>
  );
}
