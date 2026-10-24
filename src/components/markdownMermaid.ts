/**
 * EDT-FR-MOYS – EDT-FR-UYEN, EFR-FR-DGBG: a fenced code block whose language is
 * `mermaid` shows on the WYSIWYG surface as the diagram its source describes.
 *
 * The fence stays an ordinary `codeBlock` node, so the Markdown behind it is the
 * plain fence it always was and serialises exactly as any fenced block does
 * (EDT-FR-67). The diagram, the view toggle, and the error message are
 * **decorations**: they add no node and change no byte. Which view a fence shows
 * is plugin state, changed by transactions that carry meta alone, so a change of
 * view never fires the surface's update, never marks the artifact dirty, and is
 * never an undo step (EDT-FR-CQBG).
 *
 * In the diagram view the source text stays in the document, set at zero size,
 * so Find, comment fragments and the selection keep working on it (EDT-FR-UYEN).
 * Whenever the author could otherwise act on text they cannot see — the current
 * Find match is in it, or the keyboard moved the caret into it — the fence shows
 * its source as a **peek** until that stops.
 */
import { Extension } from "@tiptap/react";
import type { Node as PMNode } from "@tiptap/pm/model";
import {
  NodeSelection,
  Plugin,
  PluginKey,
  Selection,
  TextSelection,
  type EditorState,
} from "@tiptap/pm/state";
import { Decoration, DecorationSet, type EditorView } from "@tiptap/pm/view";

import { logDebug } from "../logging";
import { commentHighlightKey } from "./commentHighlight";
import { findHighlightKey } from "./findHighlight";
import {
  cachedDiagram,
  currentTheme,
  diagramElement,
  forgetTransientFailures,
  releaseDiagrams,
  renderDiagram,
  showDiagrams,
  subscribeTheme,
  textTag,
  type DiagramResult,
  type DiagramTheme,
} from "./markdownMermaidRender";

/** EDT-FR-MOYS: the language that makes a fenced block a diagram fence. */
export const DIAGRAM_LANGUAGE = "mermaid";

/** EDT-FR-USWS: the accessible names of the view toggle, by the view it shows. */
export const SHOW_SOURCE_LABEL = "Show diagram source";
export const SHOW_DIAGRAM_LABEL = "Show diagram";
/** EDT-FR-KQXZ: the accessible name of a diagram that takes input. */
export const DIAGRAM_LABEL = "Mermaid diagram, edit source";
/** EDT-FR-LEUI: the accessible name of an inert diagram. */
export const INERT_DIAGRAM_LABEL = "Mermaid diagram";

/**
 * The event a surface's DOM receives when the height of a fence can have
 * changed: a change of view, or a diagram that finished rendering. The comment
 * rail aligns its cards again on it (EDT-FR-UYEN).
 */
export const DIAGRAM_LAYOUT_EVENT = "synthesis-diagram-layout";

/** How one fence shows at this moment. */
export type DiagramView = "diagram" | "source" | "error" | "pending";

interface MermaidState {
  /** Start positions of the fences the author set to show their source. */
  sources: readonly number[];
  theme: DiagramTheme;
}

type MermaidMeta =
  | { type: "show"; pos: number; view: "diagram" | "source" }
  | { type: "theme"; theme: DiagramTheme }
  /** A render finished, or the focus moved: draw again from what is known. */
  | { type: "refresh" };

export const mermaidKey = new PluginKey<MermaidState>("synthesisMermaid");

export function isDiagramFence(node: PMNode): boolean {
  if (node.type.name !== "codeBlock") return false;
  const language = node.attrs.language;
  return typeof language === "string" && language.trim() === DIAGRAM_LANGUAGE;
}

/** Each diagram fence in the document, with its start position. */
function eachFence(doc: PMNode, visit: (node: PMNode, pos: number) => void): void {
  doc.descendants((node, pos) => {
    if (isDiagramFence(node)) {
      visit(node, pos);
      return false;
    }
    // A textblock holds no fence; anything else (a list, a quote) may.
    return !node.isTextblock;
  });
}

/** The start of the diagram fence that holds `pos`, or null. */
function fenceAround(doc: PMNode, pos: number): number | null {
  if (pos < 0 || pos > doc.content.size) return null;
  const $pos = doc.resolve(pos);
  for (let depth = $pos.depth; depth > 0; depth--) {
    if (isDiagramFence($pos.node(depth))) return $pos.before(depth);
  }
  return null;
}

function dispatchMeta(view: EditorView, meta: MermaidMeta, select?: Selection): void {
  const tr = view.state.tr.setMeta(mermaidKey, meta);
  if (select) tr.setSelection(select);
  tr.setMeta("addToHistory", false);
  view.dispatch(tr);
}

/**
 * A selection outside the fence at `fence`: the nearest text position after
 * it, else before it, else the fence selected whole — never a caret in text
 * the author cannot see.
 */
function selectionOutside(doc: PMNode, fence: number, node: PMNode): Selection {
  const after = Selection.findFrom(doc.resolve(fence + node.nodeSize), 1, true);
  if (after) return after;
  const before = Selection.findFrom(doc.resolve(fence), -1, true);
  if (before) return before;
  return NodeSelection.create(doc, fence);
}

/**
 * EDT-FR-USWS: after a change of view the widget is drawn anew, so the focus
 * the author had on the old toggle moves to the new one.
 */
function focusToggle(view: EditorView, fence: number): void {
  const dom = view.nodeDOM(fence);
  if (dom instanceof HTMLElement) {
    dom.querySelector<HTMLElement>(".mermaid-toggle")?.focus();
  }
}

/** EDT-FR-USWS: switch a fence between its diagram and its source. */
export function showFenceView(
  view: EditorView,
  fence: number,
  next: "diagram" | "source",
): void {
  const node = view.state.doc.nodeAt(fence);
  if (!node || !isDiagramFence(node)) return;
  let select: Selection | undefined;
  if (next === "diagram") {
    const { from, to } = view.state.selection;
    if (to > fence && from < fence + node.nodeSize) {
      select = selectionOutside(view.state.doc, fence, node);
    }
  }
  dispatchMeta(view, { type: "show", pos: fence, view: next }, select);
}

/** EDT-FR-KQXZ: show a fence's source in place and put the caret in it. */
export function editFenceSource(view: EditorView, fence: number): void {
  const node = view.state.doc.nodeAt(fence);
  if (!node || !isDiagramFence(node)) return;
  const caret = TextSelection.create(view.state.doc, fence + 1);
  dispatchMeta(view, { type: "show", pos: fence, view: "source" }, caret);
  view.focus();
}

/**
 * EDT-FR-UYEN: the top of the diagram a position lies in, in viewport pixels,
 * or null when the position is not in a fence that shows its diagram. The
 * comment rail aligns a card here, since the source text has no height there.
 */
export function diagramTopAt(view: EditorView, pos: number): number | null {
  const fence = fenceAround(view.state.doc, pos);
  if (fence === null) return null;
  const dom = view.nodeDOM(fence);
  if (!(dom instanceof HTMLElement)) return null;
  const shown = dom.getAttribute("data-diagram-view");
  if (shown !== "diagram" && shown !== "pending") return null;
  return dom.getBoundingClientRect().top;
}

/**
 * EDT-FR-UYEN: the viewport top a comment card aligns with for a fragment that
 * starts at `pos` — the top of the diagram when the fragment is in a fence that
 * shows its diagram, else the top of the text.
 */
export function fragmentTop(view: EditorView, pos: number): number {
  return diagramTopAt(view, pos) ?? view.coordsAtPos(pos).top;
}

/** What decides how one fence is drawn. */
interface FenceLook {
  view: DiagramView;
  /** The toggle is drawn; false on an inert surface, an empty fence, an error and a Find peek. */
  toggle: boolean;
}

interface FenceDecorationInput extends FenceLook {
  node: PMNode;
  pos: number;
  result: DiagramResult | undefined;
  editable: boolean;
  theme: DiagramTheme;
  commented: boolean;
}

function fenceOfWidget(view: EditorView, getPos: () => number | undefined): number | null {
  const at = getPos();
  return at === undefined ? null : fenceAround(view.state.doc, at);
}

function toggleButton(
  view: EditorView,
  getPos: () => number | undefined,
  shown: DiagramView,
): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "mermaid-toggle";
  const toSource = shown !== "source";
  button.textContent = toSource ? "Source" : "Diagram";
  button.setAttribute("aria-label", toSource ? SHOW_SOURCE_LABEL : SHOW_DIAGRAM_LABEL);
  button.addEventListener("mousedown", (event) => event.preventDefault());
  button.addEventListener("click", (event) => {
    event.preventDefault();
    const fence = fenceOfWidget(view, getPos);
    if (fence === null) return;
    showFenceView(view, fence, toSource ? "source" : "diagram");
    focusToggle(view, fence);
  });
  return button;
}

/** The diagram itself, or the state that stands in for it while it renders. */
function diagramFigure(
  view: EditorView,
  getPos: () => number | undefined,
  input: FenceDecorationInput,
): HTMLElement {
  const figure = document.createElement("span");
  figure.className = "mermaid-diagram";
  const rendered = input.result?.state === "rendered" ? input.result : null;
  if (rendered) {
    const svg = diagramElement(rendered.svg, rendered.id);
    if (svg) figure.appendChild(svg);
  } else {
    figure.classList.add("mermaid-diagram--pending");
    figure.setAttribute("role", "status");
    figure.textContent = "Rendering diagram…";
  }
  if (!input.editable) {
    // EDT-FR-LEUI: an inert diagram takes no click, not even on a link in it.
    figure.addEventListener("click", (event) => event.preventDefault());
    if (rendered) {
      figure.setAttribute("role", "img");
      figure.setAttribute("aria-label", INERT_DIAGRAM_LABEL);
    }
    return figure;
  }
  figure.tabIndex = 0;
  figure.setAttribute("role", "button");
  figure.setAttribute("aria-label", DIAGRAM_LABEL);
  const edit = () => {
    const fence = fenceOfWidget(view, getPos);
    if (fence !== null) editFenceSource(view, fence);
  };
  figure.addEventListener("mousedown", (event) => event.preventDefault());
  figure.addEventListener("click", (event) => {
    event.preventDefault();
    edit();
  });
  figure.addEventListener("keydown", (event) => {
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    edit();
  });
  return figure;
}

function fenceDecorations(input: FenceDecorationInput): Decoration[] {
  const { node, pos, view: shown, editable, toggle } = input;
  const end = pos + node.nodeSize;
  const attrs: Record<string, string> = {
    class: `mermaid-block mermaid-block--${shown}`,
    "data-diagram-view": shown,
  };
  if (input.commented) attrs["data-diagram-commented"] = "true";
  const decos = [Decoration.node(pos, end, attrs)];
  const hasDiagram = shown === "diagram" || shown === "pending";
  if (hasDiagram || toggle) {
    const key = [
      "mermaid",
      shown,
      editable ? "edit" : "inert",
      toggle ? "toggle" : "",
      hasDiagram && input.result?.state === "rendered" ? input.result.tag : "",
    ].join(":");
    decos.push(
      Decoration.widget(
        pos + 1,
        (view, getPos) => {
          const head = document.createElement("span");
          head.className = "mermaid-head";
          head.contentEditable = "false";
          if (hasDiagram) head.appendChild(diagramFigure(view, getPos, input));
          if (toggle) head.appendChild(toggleButton(view, getPos, shown));
          return head;
        },
        { side: -1, key, ignoreSelection: true, stopEvent: () => true },
      ),
    );
  }
  if (shown === "error" && input.result?.state === "error") {
    const message = input.result.message;
    decos.push(
      Decoration.widget(
        end - 1,
        () => {
          const line = document.createElement("span");
          line.className = "mermaid-error";
          line.contentEditable = "false";
          // EDT-FR-JBZU: the message can quote the source, so it is text.
          line.textContent = `Diagram error: ${message}`;
          return line;
        },
        {
          side: 1,
          key: `mermaid-error:${textTag(message)}`,
          ignoreSelection: true,
          stopEvent: () => true,
        },
      ),
    );
  }
  return decos;
}

/** The current Find match as a position range, if there is one. */
function currentFindMatch(state: EditorState): { from: number; to: number } | null {
  const find = findHighlightKey.getState(state);
  if (!find || find.current < 0) return null;
  return find.ranges[find.current] ?? null;
}

const overlaps = (a: { from: number; to: number }, from: number, to: number) =>
  a.from < to && a.to > from;

/**
 * EDT-FR-MOYS: draws each diagram fence of the WYSIWYG surface as its diagram,
 * with the view toggle of EDT-FR-USWS. Part of the shared extension set, so
 * every surface that reuses the Editor's rich surface carries it (NAW-FR-11,
 * SET-FR-16, DFV-FR-EGFA).
 */
export const MermaidDiagrams = Extension.create({
  name: "mermaidDiagrams",

  addProseMirrorPlugins() {
    const editor = this.editor;
    let editorView: EditorView | null = null;
    // Whether the latest caret move came from the keyboard. Only then can the
    // caret enter text set at zero size, so only then does it reveal the source.
    let keyboardCaret = false;
    // How the fences looked at the last draw, to tell the rail when it changed.
    let drawn = "";
    // The owner of this surface's shown diagrams in the render cache.
    const owner = {};

    const isEditable = () => editor.options.editable !== false;

    const lookOf = (
      state: EditorState,
      node: PMNode,
      pos: number,
      plugin: MermaidState,
      result: DiagramResult | undefined,
    ): FenceLook => {
      const editable = isEditable();
      if (node.textContent.trim() === "") return { view: "source", toggle: false };
      if (result?.state === "error") return { view: "error", toggle: false };
      const settled: DiagramView = result ? "diagram" : "pending";
      if (!editable) return { view: settled, toggle: false };
      const end = pos + node.nodeSize;
      if (plugin.sources.includes(pos)) return { view: "source", toggle: true };
      // EFR-FR-DGBG: the fence shows its source while it holds the current
      // match. The toggle cannot end that, so it is not offered.
      const match = currentFindMatch(state);
      if (match !== null && overlaps(match, pos, end)) {
        return { view: "source", toggle: false };
      }
      const { from, to } = state.selection;
      if (keyboardCaret && editorView?.hasFocus() === true && from > pos && to < end) {
        // "Show diagram" moves the caret out, which ends this peek.
        return { view: "source", toggle: true };
      }
      return { view: settled, toggle: true };
    };

    return [
      new Plugin<MermaidState>({
        key: mermaidKey,
        state: {
          init: () => ({ sources: [], theme: currentTheme() }),
          apply(tr, value, _old, next) {
            const meta = tr.getMeta(mermaidKey) as MermaidMeta | undefined;
            let { sources, theme } = value;
            if (tr.docChanged && sources.length > 0) {
              sources = sources
                .map((pos) => tr.mapping.mapResult(pos, 1))
                .filter((mapped) => !mapped.deleted)
                .map((mapped) => mapped.pos)
                .filter((pos) => {
                  const node = next.doc.nodeAt(pos);
                  return node !== null && isDiagramFence(node);
                });
            }
            if (meta?.type === "show") {
              sources = sources.filter((pos) => pos !== meta.pos);
              if (meta.view === "source") sources = [...sources, meta.pos];
            } else if (meta?.type === "theme") {
              theme = meta.theme;
            }
            return sources === value.sources && theme === value.theme
              ? value
              : { sources, theme };
          },
        },
        props: {
          decorations(state) {
            const plugin = mermaidKey.getState(state);
            if (!plugin) return null;
            const comments = commentHighlightKey.getState(state)?.ranges ?? [];
            const editable = isEditable();
            const decos: Decoration[] = [];
            const looks: string[] = [];
            const onScreen: Array<[string, DiagramTheme]> = [];
            eachFence(state.doc, (node, pos) => {
              onScreen.push([node.textContent, plugin.theme]);
              const result = cachedDiagram(node.textContent, plugin.theme);
              const end = pos + node.nodeSize;
              const look = lookOf(state, node, pos, plugin, result);
              looks.push(
                `${pos}:${look.view}:${result?.state === "rendered" ? result.tag : ""}`,
              );
              decos.push(
                ...fenceDecorations({
                  ...look,
                  node,
                  pos,
                  result,
                  editable,
                  theme: plugin.theme,
                  commented: comments.some((range) => overlaps(range, pos, end)),
                }),
              );
            });
            drawn = looks.join("|");
            showDiagrams(owner, onScreen);
            return decos.length > 0 ? DecorationSet.create(state.doc, decos) : null;
          },
          handleKeyDown() {
            keyboardCaret = true;
            return false;
          },
          handleDOMEvents: {
            mousedown(view, event) {
              keyboardCaret = false;
              // A click into a source that shows as a peek keeps it showing:
              // the author now works in it, and the caret must stay visible.
              let fence: number | null = null;
              try {
                if (event.target instanceof Node) {
                  fence = fenceAround(view.state.doc, view.posAtDOM(event.target, 0));
                }
              } catch (error) {
                // A target the view cannot place is not in a fence.
                logDebug(["frontend"], "a click in the editor could not be placed", {
                  error: error instanceof Error ? error.name : typeof error,
                });
              }
              if (fence === null) return false;
              const dom = view.nodeDOM(fence);
              const plugin = mermaidKey.getState(view.state);
              // An empty fence shows its source because it has nothing else to
              // show, not because the author chose it.
              const empty = view.state.doc.nodeAt(fence)?.textContent.trim() === "";
              if (
                !empty &&
                dom instanceof HTMLElement &&
                dom.getAttribute("data-diagram-view") === "source" &&
                plugin !== undefined &&
                !plugin.sources.includes(fence)
              ) {
                dispatchMeta(view, { type: "show", pos: fence, view: "source" });
              }
              return false;
            },
            blur(view) {
              // A source the keyboard revealed goes back to its diagram once
              // the author leaves the surface.
              if (!keyboardCaret) return false;
              keyboardCaret = false;
              let hasFence = false;
              eachFence(view.state.doc, () => {
                hasFence = true;
              });
              if (hasFence) dispatchMeta(view, { type: "refresh" });
              return false;
            },
          },
        },
        view(view) {
          editorView = view;
          let destroyed = false;
          let announced = drawn;
          const wantedNow = (source: string, theme: DiagramTheme) => () => {
            if (destroyed) return false;
            const state = mermaidKey.getState(view.state);
            if (!state || state.theme !== theme) return false;
            let found = false;
            eachFence(view.state.doc, (node) => {
              if (node.textContent === source) found = true;
            });
            return found;
          };
          // What this surface has asked for and not yet heard back on, so a
          // burst of transactions during one render asks once and redraws once.
          const requested = new Set<string>();
          // Asks for each diagram not yet known. Cheap when all are known.
          const requestRenders = () => {
            const state = mermaidKey.getState(view.state);
            if (!state) return;
            const theme = state.theme;
            eachFence(view.state.doc, (node) => {
              const source = node.textContent;
              const key = `${theme}\u0000${source}`;
              if (source.trim() === "" || requested.has(key)) return;
              if (cachedDiagram(source, theme)) return;
              requested.add(key);
              renderDiagram(source, theme, wantedNow(source, theme))
                .then((result) => {
                  requested.delete(key);
                  if (result !== null && !destroyed) {
                    dispatchMeta(view, { type: "refresh" });
                  }
                })
                .catch((error: unknown) => {
                  requested.delete(key);
                  logDebug(["frontend"], "a diagram render request failed", {
                    error: error instanceof Error ? error.name : typeof error,
                  });
                });
            });
          };
          const stopTheme = subscribeTheme((theme) => {
            if (!destroyed) dispatchMeta(view, { type: "theme", theme });
          });
          requestRenders();
          return {
            update(updated, previous) {
              if (updated.state.doc !== previous.doc) forgetTransientFailures();
              requestRenders();
              if (drawn !== announced) {
                announced = drawn;
                updated.dom.dispatchEvent(new CustomEvent(DIAGRAM_LAYOUT_EVENT));
              }
            },
            destroy() {
              destroyed = true;
              editorView = null;
              releaseDiagrams(owner);
              stopTheme();
            },
          };
        },
      }),
    ];
  },
});
