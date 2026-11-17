/**
 * DCR-FR-JKVA / DCR-FR-XNPE / DCR-FR-BYSX: what a diagram hunk draws in the
 * document.
 *
 * A diagram hunk is drawn as a diagram and never as text. This is the diagram
 * half of `hunkDecorations.ts`, which hands each diagram hunk to it. It adds no
 * node and changes no byte: the old fence stays in the document as it is, so the
 * buffer, the file, and the undo history do not change (DCR-FR-02, DCR-FR-UKMP).
 *
 *   change    the old fence is hidden, and the new diagram stands in its place
 *   addition  the new diagram stands at the place of the insertion
 *   removal   the old diagram stays and is marked as removed
 *   rejected  the old diagram stays, in the held tone, with a mark that says
 *             the hunk is rejected; no new diagram is drawn
 *
 * Every mark says its meaning in words or in a glyph as well as in a colour.
 */
import type { Node as PMNode } from "@tiptap/pm/model";
import { Decoration } from "@tiptap/pm/view";

import { logDebug } from "../../logging";
import { DIAGRAM_LAYOUT_EVENT, isDiagramFence } from "../markdownMermaid";
import { mountDiagramFigure } from "./diagramFigure";
import type { DecoratedHunk } from "./hunkDecorations";
import { HUNK_ATTR, HUNK_CLASS } from "./hunkConstants";

/**
 * DCR-FR-12: tells the chip that its block is in the document now. A widget is
 * built before ProseMirror puts it in the page, so the chip, which measures the
 * block, cannot find it until the build is done.
 */
function announce(node: HTMLElement): void {
  queueMicrotask(() => {
    if (node.isConnected) node.dispatchEvent(new CustomEvent(DIAGRAM_LAYOUT_EVENT, { bubbles: true }));
  });
}

/** The teardown of each figure, by the widget it is drawn in. */
const teardowns = new WeakMap<Node, () => void>();

/** DCR-FR-07: the classes of the state of a hunk, apart from its kind. */
function stateClasses(hunk: DecoratedHunk): string[] {
  const out: string[] = [];
  if (hunk.state === "discussing") out.push(`${HUNK_CLASS}--discussing`);
  if (hunk.state === "rejected") out.push(`${HUNK_CLASS}--rejected`);
  if (hunk.lost) out.push(`${HUNK_CLASS}--lost`);
  return out;
}

/** The visible name of what the hunk does, for the mark in the block. */
function kindWords(hunk: DecoratedHunk): string {
  return hunk.kind === "add"
    ? "Diagram addition"
    : hunk.kind === "del"
      ? "Diagram removal"
      : "Diagram change";
}

/**
 * DCR-FR-JKVA: the block that holds the new diagram. It has the classes of an
 * added text, so the chip finds its place and the focus mark reaches it.
 */
function figureNode(hunk: DecoratedHunk, focused: boolean, label: string): HTMLElement {
  const box = document.createElement("div");
  box.className = [
    HUNK_CLASS,
    `${HUNK_CLASS}--add`,
    `${HUNK_CLASS}--diagram`,
    ...stateClasses(hunk),
  ].join(" ");
  box.setAttribute(HUNK_ATTR, hunk.id);
  box.setAttribute("role", "group");
  box.setAttribute("aria-label", label);
  box.dataset.focused = String(focused);
  const gutter = document.createElement("span");
  gutter.className = `${HUNK_CLASS}__gutter`;
  gutter.setAttribute("aria-hidden", "true");
  gutter.textContent = "+";
  // A mark that says what the block is, to the right of the chip's place.
  const tag = document.createElement("span");
  tag.className = `${HUNK_CLASS}__tag t-ui-xs`;
  tag.setAttribute("aria-hidden", "true");
  tag.textContent = kindWords(hunk);
  const body = document.createElement("div");
  body.className = `${HUNK_CLASS}__body`;
  const figure = document.createElement("div");
  figure.className = `${HUNK_CLASS}__diagram`;
  body.appendChild(figure);
  box.append(gutter, tag, body);
  teardowns.set(box, mountDiagramFigure(figure, hunk.diagram?.newSource ?? ""));
  announce(box);
  return box;
}

/**
 * DCR-FR-12 / DCR-FR-JKVA: the block under the old diagram that holds the space
 * of the chip and says, in words, what the mark on the old diagram means.
 */
function footNode(hunk: DecoratedHunk, focused: boolean, label: string, words: string): HTMLElement {
  const foot = document.createElement("div");
  foot.className = [
    HUNK_CLASS,
    `${HUNK_CLASS}--del-foot`,
    `${HUNK_CLASS}--diagram-foot`,
    ...stateClasses(hunk),
  ].join(" ");
  foot.setAttribute(HUNK_ATTR, hunk.id);
  foot.setAttribute("role", "group");
  foot.setAttribute("aria-label", label);
  foot.dataset.focused = String(focused);
  const mark = document.createElement("span");
  mark.className = `${HUNK_CLASS}__foot-mark`;
  mark.setAttribute("aria-hidden", "true");
  mark.textContent = hunk.state === "rejected" ? "!" : "−";
  const text = document.createElement("span");
  text.className = `${HUNK_CLASS}__foot-words t-ui-xs`;
  text.textContent = words;
  foot.append(mark, text);
  announce(foot);
  return foot;
}

/** The words of the mark under an old diagram that stays in the document. */
function footWords(hunk: DecoratedHunk): string {
  if (hunk.state === "rejected") {
    return hunk.kind === "del"
      ? "Diagram removal rejected"
      : hunk.kind === "add"
        ? "Diagram addition rejected"
        : "Diagram change rejected";
  }
  return "Diagram removed";
}

/** The old fence of a hunk, where the document holds it at `from`. */
function fenceAt(doc: PMNode, hunk: DecoratedHunk): PMNode | null {
  const node = doc.nodeAt(hunk.from);
  if (node === null || !isDiagramFence(node) || hunk.from + node.nodeSize !== hunk.to) {
    return null;
  }
  return node;
}

/**
 * DCR-FR-JKVA: every decoration of one diagram hunk that is still drawn.
 * `boundary` is the block boundary an insertion is drawn at.
 */
export function diagramDecorations(
  doc: PMNode,
  hunk: DecoratedHunk,
  focused: boolean,
  boundary: number,
  label: string,
): Decoration[] {
  const held = hunk.state === "rejected";
  const widget = (
    at: number,
    side: number,
    build: () => HTMLElement,
    key: string,
    destroy?: (node: Node) => void,
  ) =>
    Decoration.widget(at, build, {
      side,
      ignoreSelection: true,
      stopEvent: () => true,
      key,
      destroy,
    });
  const key = [hunk.id, hunk.state, hunk.lost, `${hunk.position}/${hunk.total}`].join(":");

  const figure = () => figureNode(hunk, focused, label);
  const foot = () => footNode(hunk, focused, label, footWords(hunk));
  const diagramKey = `${key}:diagram:${hunk.diagram?.newSource ?? ""}`;

  if (hunk.kind === "add") {
    // A rejected addition has no new diagram to show, and holds the chip's place.
    return held
      ? [widget(boundary, 1, foot, `${key}:foot:${focused}`)]
      : [widget(boundary, 1, figure, diagramKey, destroyFigure)];
  }

  // A change or a removal names the fence it alters, and draws nothing where
  // the document holds no such fence.
  if (fenceAt(doc, hunk) === null) {
    logDebug(["frontend"], "a diagram hunk names no fence of the document", { kind: hunk.kind });
    return [];
  }
  if (hunk.kind === "replace" && !held) {
    // The new diagram stands in the place of the old one, which is hidden.
    return [
      Decoration.node(hunk.from, hunk.to, { class: `${HUNK_CLASS}--diagram-old` }),
      widget(hunk.from, -1, figure, diagramKey, destroyFigure),
    ];
  }
  // A removal marks the old diagram. A rejected hunk keeps it, in the held tone.
  return [
    Decoration.node(hunk.from, hunk.to, {
      class: [HUNK_CLASS, `${HUNK_CLASS}--diagram-marked`, ...stateClasses(hunk)].join(" "),
      [HUNK_ATTR]: hunk.id,
      "data-focused": String(focused),
    }),
    widget(hunk.to, 1, foot, `${key}:foot:${focused}`),
  ];
}

function destroyFigure(node: Node): void {
  teardowns.get(node)?.();
  teardowns.delete(node);
}
