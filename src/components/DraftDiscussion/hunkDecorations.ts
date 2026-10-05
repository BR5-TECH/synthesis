/**
 * DCR-FR-05 / DCR-FR-07: drawing a proposal's changes in the prose they change.
 *
 * A sibling of `findHighlight.ts` and `commentHighlight.ts` rather than an
 * extension of either: the three decorate the same document for unrelated
 * reasons and must coexist — a find match inside a proposed deletion marks both
 * — so each is its own plugin with its own payload.
 *
 * Presentation only. It adds no node and changes no byte, so it occupies no
 * position in the prompt's undo history (EDT-FR-23) and the prompt stays exactly
 * what it was until the author accepts a change (DCR-FR-02).
 */
import { Extension } from "@tiptap/react";
import type { Node as PMNode } from "@tiptap/pm/model";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import { Decoration, DecorationSet, type EditorView } from "@tiptap/pm/view";

import type { HunkKind, HunkState } from "../../types";
import { mountProposedText } from "./proposedText";

/**
 * One change, placed in the document the author is looking at.
 *
 * `from`/`to` name the text the change removes or replaces, and are equal for
 * an insertion, which names no text of its own. `after` is what the change
 * proposes, and is absent for a deletion.
 */
export interface DecoratedHunk {
  id: string;
  kind: HunkKind;
  state: HunkState;
  from: number;
  to: number;
  after?: string | null;
  /** DCP-FR-BMLX: the prompt no longer holds the text this change names. */
  lost: boolean;
  /** Its one-based place in the proposal, for the accessible name. */
  position: number;
  total: number;
  /** Who proposed it, for the accessible name (DCR-FR-10). */
  agent: string;
  /**
   * DCR-FR-24: the proposed text as the author has it, where they have edited
   * it. Absent while it is still the agent's own words.
   */
  draft?: string | null;
  /** DCR-FR-KDSV: a legacy proposal's text is not editable. */
  editable: boolean;
}

export interface HunkDecorationPayload {
  hunks: DecoratedHunk[];
  /** DCR-FR-12: the change the action chip is about. */
  focused: string | null;
  /**
   * DCR-FR-24 / DCR-FR-26: the author rewrote a change's proposed text.
   *
   * A callback in plugin state rather than an event the host listens for,
   * because the text is typed into a widget the plugin owns and nothing else
   * knows when it changed.
   */
  onEdit?: (hunkId: string, after: string) => void;
}

export const hunkDecorationKey = new PluginKey<HunkDecorationPayload>(
  "synthesisHunkDecorations",
);

/** Carries the change's id to the click handler through the DOM. */
export const HUNK_ATTR = "data-hunk";
export const HUNK_CLASS = "hunk";

/**
 * DCR-FR-07 / DCR-FR-28: what a change's state looks like.
 *
 * Every state is carried by a class rather than by colour alone: the rail, the
 * strike-through of a removal, and the marker in the gutter each say the same
 * thing a second way, so a proposal is reviewable without colour
 * discrimination.
 */
function classesFor(hunk: DecoratedHunk, part: "del" | "add"): string {
  const out = [HUNK_CLASS, `${HUNK_CLASS}--${part}`];
  if (hunk.state === "discussing") out.push(`${HUNK_CLASS}--discussing`);
  if (hunk.lost) out.push(`${HUNK_CLASS}--lost`);
  return out.join(" ");
}

/**
 * DCR-FR-10: what a change is called when it is reached by keyboard, where the
 * decoration says nothing.
 */
export function hunkLabel(hunk: DecoratedHunk): string {
  const kind =
    hunk.kind === "add"
      ? "insertion"
      : hunk.kind === "del"
        ? "deletion"
        : "replacement";
  const lost = hunk.lost ? ", no longer in the prompt" : "";
  return `Change ${hunk.position} of ${hunk.total}, ${kind} proposed by ${hunk.agent}${lost}`;
}

/**
 * The teardown of each proposed text's own editing surface, by the widget it is
 * mounted in. ProseMirror calls the widget's `destroy` when it drops the node,
 * and that is the only moment the surface can be freed.
 */
const teardowns = new WeakMap<Node, () => void>();

/**
 * DCR-FR-09: the proposed text, rendered as the Markdown it is.
 *
 * Built as plain DOM rather than through React: a widget is created and
 * destroyed by ProseMirror as the document changes, and a React root mounted
 * inside one would have to be torn down by hand on every redraw. The chip is
 * the part that must be a real focusable control, and it is an overlay rather
 * than a widget for exactly that reason.
 */
function proposedNode(
  hunk: DecoratedHunk,
  focused: boolean,
  onEdit: ((hunkId: string, after: string) => void) | undefined,
  view: EditorView | null,
): HTMLElement {
  const box = document.createElement("div");
  box.className = classesFor(hunk, "add");
  box.setAttribute(HUNK_ATTR, hunk.id);
  box.setAttribute("role", "group");
  box.setAttribute("aria-label", hunkLabel(hunk));
  box.dataset.focused = String(focused);
  const gutter = document.createElement("span");
  gutter.className = `${HUNK_CLASS}__gutter`;
  gutter.setAttribute("aria-hidden", "true");
  gutter.textContent = "+";
  const body = document.createElement("div");
  body.className = `${HUNK_CLASS}__body`;
  box.append(gutter, body);
  /**
   * DCR-FR-09: rendered with the Editor's own Markdown, and never as raw HTML.
   *
   * DCR-FR-24: a change that is right but for one word is corrected in place
   * and taken, rather than declined and asked for again. The widget is not part
   * of the prompt's document — ProseMirror renders it beside the text rather
   * than in it — so typing here changes no byte of the draft. That is exactly
   * the promise DCR-FR-26 makes about this write: it reaches proposal storage
   * and never the draft.
   */
  const teardown = mountProposedText(body, hunk.draft ?? hunk.after ?? "", {
    editable: hunk.editable,
    label: `Proposed text of ${hunkLabel(hunk)}`,
    onChange: onEdit
      ? (after) => {
          // The callback the review holds now, rather than the one it held when
          // this widget was built: the widget lives on across payloads, and an
          // older callback reads a reading of the proposal that has moved on.
          const current = view ? hunkDecorationKey.getState(view.state)?.onEdit : undefined;
          (current ?? onEdit)(hunk.id, after);
        }
      : undefined,
  });
  teardowns.set(box, teardown);
  return box;
}

/**
 * DCR-FR-12: the space a deletion holds open for its action chip.
 *
 * A deletion is drawn over the document's own text, and an inline box reserves
 * no vertical space however much padding it declares — so without this the chip
 * lies over the line below the struck passage, which is a line of the author's
 * prompt. A block of its own, placed after the whole passage, is space the chip
 * can be put in. It carries the change's id, so the chip finds it and the press
 * that puts a change under review still reaches it.
 */
function removalFootNode(hunk: DecoratedHunk): HTMLElement {
  const foot = document.createElement("div");
  // The state classes too, so the band under a change held for discussion
  // agrees with the struck text above it rather than reading as a deletion.
  const classes = [HUNK_CLASS, `${HUNK_CLASS}--del-foot`];
  if (hunk.state === "discussing") classes.push(`${HUNK_CLASS}--discussing`);
  if (hunk.lost) classes.push(`${HUNK_CLASS}--lost`);
  foot.className = classes.join(" ");
  foot.setAttribute(HUNK_ATTR, hunk.id);
  foot.setAttribute("aria-hidden", "true");
  return foot;
}

/**
 * The position a change's own block takes, which is never inside a block of the
 * document (DCR-FR-09).
 *
 * ProseMirror mounts an inline widget inside the DOM of the textblock holding
 * its position. A widget placed at the end of a heading therefore becomes a
 * child of that heading and reads at the heading's size and weight, which is
 * neither the document's body typography nor valid markup. Taken to the
 * boundary after that textblock, the block is a sibling of it and carries only
 * the typography its own rule gives it.
 */
function blockBoundary(doc: PMNode, pos: number): number {
  const at = Math.min(Math.max(pos, 0), doc.content.size);
  const $at = doc.resolve(at);
  for (let depth = $at.depth; depth > 0; depth -= 1) {
    if (!$at.node(depth).isTextblock) continue;
    // Which side of the block. An insertion anchored at the very head of one —
    // the head of the prompt, or the head of a block the change before it ends
    // at — is written **above** that block, so drawing it below would show the
    // author the change landing a paragraph further down than it does.
    return $at.parentOffset === 0 ? $at.before(depth) : $at.after(depth);
  }
  return at;
}

/**
 * DCR-FR-20 / DCR-FR-17 / DCR-FR-21: which of a proposal's changes are still
 * drawn in the document.
 *
 * A change the author has decided — here, or in another window — is no longer
 * part of the review and leaves the prose at once. Every other change is
 * untouched by that decision, which is the whole of "acceptance is per hunk":
 * accepting one must leave the rest exactly where the author is looking at
 * them. A change held for discussion is undecided, so it stays.
 */
export function undecoratedByDecision(hunk: DecoratedHunk): boolean {
  return hunk.state === "accepted" || hunk.state === "rejected";
}

/**
 * The whole of what a proposal draws, as a set over the document.
 *
 * Exported because it is the pure half of this module: given a document and
 * the changes placed in it, what is drawn is settled here and nowhere else.
 */
export function buildDecorations(
  doc: PMNode,
  payload: HunkDecorationPayload,
): DecorationSet {
  const size = doc.content.size;
  const decos: Decoration[] = [];
  for (const hunk of payload.hunks) {
    if (undecoratedByDecision(hunk)) continue;
    const focused = hunk.id === payload.focused;
    // A range left over from a document that has since changed can point past
    // the end; ProseMirror throws on an out-of-bounds decoration, which would
    // take the whole editing surface down.
    if (hunk.from < 0 || hunk.to > size) continue;

    // The change's own block sits between the document's blocks rather than
    // inside one of them (DCR-FR-09).
    const boundary = blockBoundary(doc, hunk.to);

    // DCR-FR-07: the text a change removes or replaces, struck through where it
    // stands rather than moved anywhere. An insertion removes nothing, so it
    // marks no text however the change was recorded (DCR-FR-05).
    if (hunk.kind !== "add" && hunk.to > hunk.from) {
      decos.push(
        Decoration.inline(hunk.from, hunk.to, {
          class: classesFor(hunk, "del"),
          [HUNK_ATTR]: hunk.id,
          "data-focused": String(focused),
        }),
      );
      // DCR-FR-12: a deletion has no proposed text to hold the chip's space
      // open, so it holds it open itself.
      if (hunk.kind === "del") {
        decos.push(
          Decoration.widget(boundary, () => removalFootNode(hunk), {
            side: 1,
            ignoreSelection: true,
            stopEvent: () => true,
            key: `${hunk.id}:foot:${focused}`,
          }),
        );
      }
    }
    // DCR-FR-09: what the change proposes, immediately after what it replaces,
    // so a replacement reads as the removal above its insertion.
    if (hunk.kind !== "del") {
      decos.push(
        Decoration.widget(
          boundary,
          (view) => proposedNode(hunk, focused, payload.onEdit, view ?? null),
          {
            side: 1,
            // The widget is beside the document rather than in it, so the
            // prompt's own selection never runs through it.
            ignoreSelection: true,
            // Every event inside the widget is the widget's. Without this,
            // typing into the proposed text would reach the editing surface and
            // be applied to the prompt — the one thing that must not happen
            // while a proposal stands (DCR-FR-02).
            stopEvent: () => true,
            // Deliberately NOT keyed on the text the author is typing: a key
            // that changed per keystroke would rebuild the node under the caret
            // and put it back at the start of the line on every character.
            // Nor on which change is under review: a press in the text puts its
            // change under review, and a rebuild then would destroy the surface
            // the press just put the caret in. `syncFocused` marks it instead.
            key: [
              hunk.id,
              hunk.state,
              hunk.lost,
              hunk.editable,
              `${hunk.position}/${hunk.total}`,
              hunk.after ?? "",
            ].join(":"),
            // The proposed text's own editing surface goes with its widget.
            destroy: (node) => {
              teardowns.get(node)?.();
              teardowns.delete(node);
            },
          },
        ),
      );
    }
  }
  return DecorationSet.create(doc, decos);
}

const EMPTY: HunkDecorationPayload = { hunks: [], focused: null };

/**
 * DCR-FR-12: mark which proposed text is under review, in place.
 *
 * A proposed text's widget is not rebuilt when the change under review moves
 * (see its key), so the mark its node was built with is brought up to date
 * here after each update.
 */
export function syncFocused(dom: ParentNode, focused: string | null): void {
  const boxes = dom.querySelectorAll<HTMLElement>(`.${HUNK_CLASS}--add[${HUNK_ATTR}]`);
  for (const box of boxes) {
    const next = String(box.getAttribute(HUNK_ATTR) === focused);
    if (box.dataset.focused !== next) box.dataset.focused = next;
  }
}

export const HunkDecorations = Extension.create({
  name: "hunkDecorations",

  addProseMirrorPlugins() {
    return [
      new Plugin<HunkDecorationPayload>({
        key: hunkDecorationKey,
        state: {
          init: () => EMPTY,
          apply(tr, value) {
            const pushed = tr.getMeta(hunkDecorationKey) as
              | HunkDecorationPayload
              | undefined;
            if (pushed) return pushed;
            // The ranges are recomputed from the text on every document change
            // rather than mapped through the transaction: a change names the
            // text it alters rather than a position (DCP-FR-HRQN), so the
            // answer after an edit is a fresh lookup and not a shifted offset.
            if (!tr.docChanged) return value;
            return value;
          },
        },
        view: (view) => {
          syncFocused(view.dom, (hunkDecorationKey.getState(view.state) ?? EMPTY).focused);
          return {
            update: (next) => {
              syncFocused(next.dom, (hunkDecorationKey.getState(next.state) ?? EMPTY).focused);
            },
          };
        },
        props: {
          decorations(state) {
            const payload = hunkDecorationKey.getState(state) ?? EMPTY;
            return buildDecorations(state.doc, payload);
          },
        },
      }),
    ];
  },
});
