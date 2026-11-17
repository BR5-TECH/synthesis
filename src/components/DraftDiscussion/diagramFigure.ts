/**
 * DCR-FR-XNPE: the new diagram of a diagram hunk, drawn into a plain DOM node.
 *
 * A widget of the decoration set is built and dropped by ProseMirror, outside
 * React, so this draws with the same render as the fence of the Editor
 * (`../markdownMermaidRender`) and without a React root: bundled, offline, at
 * the strict security level, in the colours of the active theme, and again when
 * the theme changes. What comes back is a node, never markup that is assigned.
 */
import { logDebug } from "../../logging";
import { DIAGRAM_LAYOUT_EVENT } from "../markdownMermaid";
import {
  DIAGRAM_ERROR,
  cachedDiagram,
  currentTheme,
  diagramElement,
  releaseDiagrams,
  renderDiagram,
  showDiagrams,
  subscribeTheme,
  type DiagramResult,
  type DiagramTheme,
} from "../markdownMermaidRender";

/** EDT-FR-UDVE: what the figure says when the source holds no diagram. */
const NO_DIAGRAM = "No diagram to show";

/** EDT-FR-XTEB: the source and the message of a source that does not render. */
function errorNodes(source: string, error: Extract<DiagramResult, { state: "error" }>): Node[] {
  const line = document.createElement("div");
  line.className = "mermaid-error";
  line.setAttribute("role", "alert");
  // The message can quote the source, so it is text.
  const summary = document.createElement("span");
  summary.className = "mermaid-error__summary";
  summary.textContent = `Diagram error: ${error.summary}`;
  line.appendChild(summary);
  if (error.detail !== undefined) {
    const detail = document.createElement("span");
    detail.className = "mermaid-error__detail";
    detail.textContent = error.detail;
    line.appendChild(detail);
  }
  const text = document.createElement("pre");
  text.className = "hunk__diagram-source";
  text.textContent = source;
  return [line, text];
}

/**
 * Draws the diagram of `source` into `host`, and again when the theme changes.
 * Returns the function that stops it. The cache of renders keeps the diagram
 * that is on screen (`showDiagrams`).
 */
export function mountDiagramFigure(host: HTMLElement, source: string): () => void {
  const owner = {};
  let live = true;
  let generation = 0;
  let theme: DiagramTheme = currentTheme();
  const empty = source.trim() === "";

  const settle = (nodes: Node[]) => {
    host.replaceChildren(...nodes);
    // The comment rail and the chip align themselves again to the new height.
    host.dispatchEvent(new CustomEvent(DIAGRAM_LAYOUT_EVENT, { bubbles: true }));
  };

  const draw = (result: DiagramResult) => {
    if (result.state === "error") {
      settle(errorNodes(source, result));
      return;
    }
    const svg = diagramElement(result.svg, result.id);
    if (svg === null) {
      settle(errorNodes(source, { state: "error", summary: DIAGRAM_ERROR.read }));
      return;
    }
    const figure = document.createElement("div");
    figure.className = "mermaid-diagram hunk__diagram-figure";
    figure.setAttribute("role", "img");
    figure.setAttribute("aria-label", "Proposed Mermaid diagram");
    // EDT-FR-JBZU: a link in a diagram does nothing here either.
    figure.addEventListener("click", (event) => event.preventDefault());
    figure.appendChild(svg);
    settle([figure]);
  };

  const run = () => {
    const mine = ++generation;
    if (empty) {
      const statement = document.createElement("p");
      statement.className = "hunk__diagram-statement";
      statement.textContent = NO_DIAGRAM;
      host.replaceChildren(statement);
      return;
    }
    showDiagrams(owner, [[source, theme]]);
    const known = cachedDiagram(source, theme);
    if (known) {
      draw(known);
      return;
    }
    // While the render is in progress the figure keeps what it shows.
    if (host.childNodes.length === 0) {
      const pending = document.createElement("p");
      pending.className = "hunk__diagram-statement";
      pending.setAttribute("role", "status");
      pending.textContent = "Rendering diagram…";
      host.replaceChildren(pending);
    }
    renderDiagram(source, theme, () => live && mine === generation)
      .then((result) => {
        if (live && mine === generation && result) draw(result);
      })
      .catch((error: unknown) => {
        logDebug(["frontend"], "a diagram hunk render request failed", {
          error: error instanceof Error ? error.name : typeof error,
        });
      });
  };

  const stopTheme = subscribeTheme((next) => {
    theme = next;
    run();
  });
  run();
  return () => {
    live = false;
    stopTheme();
    releaseDiagrams(owner);
  };
}
