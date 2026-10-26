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
  type Transaction,
} from "@tiptap/pm/state";
import { Decoration, DecorationSet, type EditorView } from "@tiptap/pm/view";

import { logDebug } from "../logging";
import { commentHighlightKey } from "./commentHighlight";
import {
  notifyDiagramSurface,
  openDiagramEditor,
  openDiagramViewer,
  takesFocusBack,
  type DiagramOverlayKind,
} from "./DiagramEditor/store";
import {
  CODE_GLYPH,
  DIAGRAM_GLYPH,
  EXPAND_GLYPH,
  PENCIL_GLYPH,
  iconElement,
} from "./domIcons";
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
/** EDT-FR-ADNU: the accessible name and the tooltip of the Edit button. */
export const EDIT_DIAGRAM_LABEL = "Edit diagram";
/** EDT-FR-YVAI: the accessible name and the tooltip of the View button. */
export const VIEW_DIAGRAM_LABEL = "View diagram";
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
  /**
   * EDT-FR-OXQC, EDT-FR-RCAV: start positions of the fences whose source last
   * rendered as a valid diagram. A fence keeps its place here while a changed
   * source renders, so the View button does not flicker while the author types.
   */
  valid: readonly number[];
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
export function fenceAround(doc: PMNode, pos: number): number | null {
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

/** The options of the diagram extension. */
export interface MermaidDiagramsOptions {
  /**
   * EDT-FR-MOQW: the surface offers the Edit button and the View button. False
   * on the candidate of a review modal, which no diagram overlay opens over.
   */
  editButton: boolean;
}

interface FenceDecorationInput extends FenceLook {
  node: PMNode;
  pos: number;
  result: DiagramResult | undefined;
  editable: boolean;
  /** EDT-FR-ADNU, EDT-FR-QTHL: the Edit button is drawn, in every view. */
  edit: boolean;
  /** EDT-FR-YVAI, EDT-FR-OXQC: the View button is drawn. */
  viewer: boolean;
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
  // EDT-FR-UTHM: an icon and no text; the name and the tooltip say the view.
  const label = toSource ? SHOW_SOURCE_LABEL : SHOW_DIAGRAM_LABEL;
  button.appendChild(iconElement(toSource ? CODE_GLYPH : DIAGRAM_GLYPH));
  button.setAttribute("aria-label", label);
  button.title = label;
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

/** The button of each diagram overlay: its class, its glyph, and its name. */
const OVERLAY_BUTTONS: Record<
  DiagramOverlayKind,
  { className: string; glyph: string; label: string }
> = {
  edit: { className: "mermaid-edit", glyph: PENCIL_GLYPH, label: EDIT_DIAGRAM_LABEL },
  view: { className: "mermaid-view", glyph: EXPAND_GLYPH, label: VIEW_DIAGRAM_LABEL },
};

/**
 * EDT-FR-ADNU, EDT-FR-YVAI: the Edit button and the View button. Each opens
 * its diagram overlay on the fence that holds it.
 */
function overlayButton(
  view: EditorView,
  getPos: () => number | undefined,
  kind: DiagramOverlayKind,
): HTMLButtonElement {
  const look = OVERLAY_BUTTONS[kind];
  const button = document.createElement("button");
  button.type = "button";
  button.className = look.className;
  button.appendChild(iconElement(look.glyph));
  button.setAttribute("aria-label", look.label);
  button.title = look.label;
  button.addEventListener("mousedown", (event) => event.preventDefault());
  button.addEventListener("click", (event) => {
    event.preventDefault();
    const fence = fenceOfWidget(view, getPos);
    if (fence === null || !view.editable) {
      logDebug(["frontend"], "a diagram button opened no overlay", {
        kind,
        placed: fence !== null,
        editable: view.editable,
      });
      return;
    }
    if (kind === "edit") openDiagramEditor(view, fence, button);
    else openDiagramViewer(view, fence, button);
  });
  // EDT-FR-HGAZ, EDT-FR-ELUJ: a button drawn again just after its overlay
  // closed takes the focus that the button it replaces held.
  queueMicrotask(() => {
    const fence = button.isConnected ? fenceOfWidget(view, getPos) : null;
    if (fence !== null && takesFocusBack(view, fence, kind)) button.focus();
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
  const { node, pos, view: shown, editable, toggle, edit, viewer } = input;
  const end = pos + node.nodeSize;
  const attrs: Record<string, string> = {
    class: `mermaid-block mermaid-block--${shown}`,
    "data-diagram-view": shown,
  };
  if (input.commented) attrs["data-diagram-commented"] = "true";
  const decos = [Decoration.node(pos, end, attrs)];
  const hasDiagram = shown === "diagram" || shown === "pending";
  if (hasDiagram || toggle || edit || viewer) {
    const key = [
      "mermaid",
      shown,
      editable ? "edit" : "inert",
      toggle ? "toggle" : "",
      edit ? "button" : "",
      viewer ? "viewer" : "",
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
          if (toggle || edit || viewer) {
            // EDT-FR-YVAI, EDT-FR-ADNU: the View button, the Edit button, and
            // the view toggle sit in this order in the top-right corner. A
            // control that the fence does not have leaves no gap.
            const controls = document.createElement("span");
            controls.className = "mermaid-controls";
            if (viewer) controls.appendChild(overlayButton(view, getPos, "view"));
            if (edit) controls.appendChild(overlayButton(view, getPos, "edit"));
            if (toggle) controls.appendChild(toggleButton(view, getPos, shown));
            head.appendChild(controls);
          }
          return head;
        },
        { side: -1, key, ignoreSelection: true, stopEvent: () => true },
      ),
    );
  }
  if (shown === "error" && input.result?.state === "error") {
    const { summary, detail } = input.result;
    decos.push(
      Decoration.widget(
        end - 1,
        () => {
          const line = document.createElement("span");
          line.className = "mermaid-error";
          line.contentEditable = "false";
          // EDT-FR-JBZU: the message can quote the source, so it is text.
          const head = document.createElement("span");
          head.className = "mermaid-error__summary";
          head.textContent = `Diagram error: ${summary}`;
          line.appendChild(head);
          if (detail !== undefined) {
            // EDT-FR-VIOP: the reason, in the source font, line for line.
            const reason = document.createElement("span");
            reason.className = "mermaid-error__detail";
            reason.textContent = detail;
            line.appendChild(reason);
          }
          return line;
        },
        {
          side: 1,
          key: `mermaid-error:${textTag(`${summary}\u0000${detail ?? ""}`)}`,
          ignoreSelection: true,
          stopEvent: () => true,
        },
      ),
    );
  }
  return decos;
}

/** The fence positions of `positions` after `tr`, where a diagram fence still starts. */
function mapFences(
  positions: readonly number[],
  tr: Transaction,
  doc: PMNode,
): number[] {
  return positions
    .map((pos) => tr.mapping.mapResult(pos, 1))
    .filter((mapped) => !mapped.deleted)
    .map((mapped) => mapped.pos)
    .filter((pos) => {
      const node = doc.nodeAt(pos);
      return node !== null && isDiagramFence(node);
    });
}

/**
 * EDT-FR-OXQC, EDT-FR-RCAV: the fences of `doc` whose source renders as a
 * valid diagram. A fence whose render is not known yet keeps its place in
 * `previous`.
 */
function validFences(doc: PMNode, theme: DiagramTheme, previous: readonly number[]): number[] {
  const valid: number[] = [];
  eachFence(doc, (node, pos) => {
    const source = node.textContent;
    if (source.trim() === "") return;
    const result = cachedDiagram(source, theme);
    if (result ? result.state === "rendered" : previous.includes(pos)) valid.push(pos);
  });
  return valid;
}

function samePositions(a: readonly number[], b: readonly number[]): boolean {
  return a.length === b.length && a.every((pos, at) => pos === b[at]);
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
export const MermaidDiagrams = Extension.create<MermaidDiagramsOptions>({
  name: "mermaidDiagrams",

  addOptions() {
    return { editButton: true };
  },

  addProseMirrorPlugins() {
    const editor = this.editor;
    const offersEdit = this.options.editButton;
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
          init: (_config, state) => {
            const theme = currentTheme();
            return { sources: [], valid: validFences(state.doc, theme, []), theme };
          },
          apply(tr, value, _old, next) {
            const meta = tr.getMeta(mermaidKey) as MermaidMeta | undefined;
            let { sources, valid, theme } = value;
            if (tr.docChanged && sources.length > 0) sources = mapFences(sources, tr, next.doc);
            if (tr.docChanged && valid.length > 0) valid = mapFences(valid, tr, next.doc);
            if (meta?.type === "show") {
              sources = sources.filter((pos) => pos !== meta.pos);
              if (meta.view === "source") sources = [...sources, meta.pos];
            } else if (meta?.type === "theme") {
              theme = meta.theme;
            }
            // EDT-FR-OXQC, EDT-FR-RCAV: a settled render decides; a render in
            // progress keeps what the fence had.
            if (tr.docChanged || meta !== undefined) {
              const settled = validFences(next.doc, theme, valid);
              if (!samePositions(settled, valid)) valid = settled;
            }
            return sources === value.sources && valid === value.valid && theme === value.theme
              ? value
              : { sources, valid, theme };
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
                  // EDT-FR-LEUI, EDT-FR-QTHL: every editable fence, in every
                  // view, and no inert one.
                  edit: editable && offersEdit,
                  // EDT-FR-OXQC: only a fence whose source renders as a
                  // valid diagram.
                  viewer: editable && offersEdit && plugin.valid.includes(pos),
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
              // EDT-FR-SHYX, EDT-FR-WPGE: the diagram editor overlay follows
              // its fence through each new state of this surface.
              notifyDiagramSurface(updated, "update");
              requestRenders();
              if (drawn !== announced) {
                announced = drawn;
                updated.dom.dispatchEvent(new CustomEvent(DIAGRAM_LAYOUT_EVENT));
              }
            },
            destroy() {
              destroyed = true;
              notifyDiagramSurface(view, "destroy");
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
