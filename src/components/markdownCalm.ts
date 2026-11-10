/**
 * `CAD-calm-architecture-diagrams.md` CAD-FR-PFNM – CAD-FR-JBUT: the parts of
 * the diagram fence plugin (`markdownMermaid.ts`) that are different for a
 * CALM fence: the read of its source and the inline canvas it shows.
 *
 * The inline canvas is a React tree in the DOM of a ProseMirror widget. The
 * widget mounts it when it is drawn and unmounts it when it is destroyed.
 */
import { createElement } from "react";
import { createRoot, type Root } from "react-dom/client";

import {
  cachedCalm,
  forgetCalmTransientFailures,
  readCalm,
  releaseCalm,
  showCalm,
} from "../calm/read";
import type { CalmLayout, CalmResult } from "../calm/types";
import { CalmInline } from "./CalmDiagram/CalmInline";
import type { DiagramKind } from "./diagramKind";
import {
  cachedDiagram,
  forgetTransientFailures,
  releaseDiagrams,
  renderDiagram,
  showDiagrams,
  type DiagramResult,
  type DiagramTheme,
} from "./markdownMermaidRender";

/** What one read or render of a fence source made, of either kind. */
export type FenceResult = DiagramResult | CalmResult;

export { CALM_DIAGRAM_LABEL } from "./CalmDiagram/labels";

/**
 * The kept result of a fence source. A CALM read does not depend on the
 * theme (CAD-FR-IXAC), so its result is kept by source alone.
 */
export function cachedFence(
  kind: DiagramKind,
  source: string,
  theme: DiagramTheme,
): FenceResult | undefined {
  return kind === "calm" ? cachedCalm(source) : cachedDiagram(source, theme);
}

/** A new read or render of a fence source; null when nobody wants it any more. */
export function renderFence(
  kind: DiagramKind,
  source: string,
  theme: DiagramTheme,
  wanted: () => boolean,
): Promise<FenceResult | null> {
  return kind === "calm" ? readCalm(source, wanted) : renderDiagram(source, theme, wanted);
}

/** The diagrams that `owner` shows now, so the kept results keep them. */
export function showFences(
  owner: object,
  mermaid: Array<[string, DiagramTheme]>,
  calm: string[],
): void {
  showDiagrams(owner, mermaid);
  showCalm(owner, calm);
}

export function releaseFences(owner: object): void {
  releaseDiagrams(owner);
  releaseCalm(owner);
}

/** CLM-FR-HPJT, EDT-FR-XKMP: forget each failed library load of both kinds. */
export function forgetFenceTransientFailures(): void {
  forgetTransientFailures();
  forgetCalmTransientFailures();
}

const roots = new WeakMap<Element, Root>();

/** CAD-FR-IPRR: mounts the inline canvas of `layout` into `figure`. */
export function mountCalmFigure(figure: HTMLElement, layout: CalmLayout): void {
  figure.classList.add("calm-figure");
  const root = createRoot(figure);
  roots.set(figure, root);
  root.render(createElement(CalmInline, { layout }));
}

/** Unmounts every inline canvas under `dom`, when its widget is destroyed. */
export function unmountCalmFigures(dom: Node): void {
  if (!(dom instanceof Element)) return;
  const figures = dom.matches(".calm-figure")
    ? [dom]
    : Array.from(dom.querySelectorAll(".calm-figure"));
  for (const figure of figures) {
    const root = roots.get(figure);
    if (!root) continue;
    roots.delete(figure);
    // A root cannot unmount while React renders; the widget can go mid-render.
    queueMicrotask(() => root.unmount());
  }
}
