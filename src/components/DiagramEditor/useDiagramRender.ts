/**
 * EDT-FR-QSOQ, EDT-FR-LVFQ: the render of one source for a diagram overlay,
 * with the same render, the same sanitised drawing, and the same error
 * messages as a diagram fence on the page (EDT-FR-JIBV, EDT-FR-JBZU,
 * EDT-FR-XKMP), so an overlay and the fence behind the scrim always agree.
 */
import { useEffect, useState } from "react";

import { logDebug } from "../../logging";
import {
  cachedDiagram,
  currentTheme,
  releaseDiagrams,
  renderDiagram,
  showDiagrams,
  subscribeTheme,
  type DiagramResult,
} from "../markdownMermaidRender";

export type RenderedDiagram = Extract<DiagramResult, { state: "rendered" }>;

export interface DiagramRender {
  /** The source is empty or only whitespace. */
  empty: boolean;
  /**
   * The last result that came back for a source of this opening. While a
   * render is in progress, it is the result of an earlier source.
   */
  settled: DiagramResult | null;
  /** The last diagram that rendered during this opening. */
  lastRendered: RenderedDiagram | null;
}

export function useDiagramRender(source: string): DiagramRender {
  const [theme, setTheme] = useState(currentTheme);
  useEffect(() => subscribeTheme(setTheme), []);

  const empty = source.trim() === "";
  const [settled, setSettled] = useState<DiagramResult | null>(() =>
    empty ? null : (cachedDiagram(source, theme) ?? null),
  );
  const [lastRendered, setLastRendered] = useState<RenderedDiagram | null>(() =>
    settled?.state === "rendered" ? settled : null,
  );

  // The diagram this overlay asks for stays in the cache while it shows.
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
        logDebug(["frontend"], "a diagram overlay render request failed", {
          error: error instanceof Error ? error.name : typeof error,
        });
      });
    return () => {
      live = false;
    };
  }, [source, theme, empty]);

  return { empty, settled, lastRendered };
}
