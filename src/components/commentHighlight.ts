/**
 * CMT-FR-28: highlighting comment-thread anchors inside the WYSIWYG body.
 *
 * The rail places a card beside a passage; this is the other half of that
 * pairing — the passage itself is marked in the body, the focused thread's range
 * distinguishably from the rest, and activating a highlight focuses its card.
 *
 * Deliberately a sibling of `findHighlight.ts` rather than an extension of it:
 * the two decorate the same document for unrelated reasons and must be able to
 * coexist (a find match inside a commented paragraph marks both), so they are two
 * plugins with two payloads rather than one with a mode flag.
 *
 * Presentation only — it adds no node and changes no byte, so it occupies no
 * position in the artifact's undo history (EDT-FR-23 / CMT-FR-31).
 */
import { Extension } from "@tiptap/react";
import type { Node as PMNode } from "@tiptap/pm/model";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import { Decoration, DecorationSet } from "@tiptap/pm/view";
import type { PosRange } from "./findHighlight";

/** One anchored thread's range in the body, and which thread it belongs to. */
export interface AnchorRange extends PosRange {
  threadId: string;
}

export interface CommentHighlightPayload {
  ranges: AnchorRange[];
  /** The focused thread, rendered distinctly from the rest. */
  focused: string | null;
}

export const commentHighlightKey = new PluginKey<CommentHighlightPayload>(
  "synthesisCommentHighlight",
);

export const ANCHOR_CLASS = "comment-anchor";
export const FOCUSED_ANCHOR_CLASS = "comment-anchor--focused";
/** Carries the thread id to the click handler through the DOM. */
export const ANCHOR_ATTR = "data-comment-thread";
/** Says in the DOM whether the discussion of the fragment is the focused one. */
export const ANCHOR_FOCUSED_ATTR = "data-comment-focused";

/**
 * CMT-FR-28: the accessible label of a fragment highlight. It names the
 * discussion the fragment belongs to, so the highlight never relies on colour
 * alone. The focused one says so.
 */
export function anchorLabel(threadId: string, focused: boolean): string {
  return focused
    ? `Fragment of discussion ${threadId} (focused)`
    : `Fragment of discussion ${threadId}`;
}

function buildDecorations(
  doc: PMNode,
  payload: CommentHighlightPayload,
): DecorationSet {
  const size = doc.content.size;
  const decos = payload.ranges
    // A range left over from a document that has since changed can point past
    // the end; ProseMirror throws on an out-of-bounds decoration, which would
    // take the whole editing surface down.
    .filter((r) => r.from >= 0 && r.to <= size && r.to > r.from)
    .map((r) => {
      const focused = r.threadId === payload.focused;
      return Decoration.inline(r.from, r.to, {
        class: focused ? `${ANCHOR_CLASS} ${FOCUSED_ANCHOR_CLASS}` : ANCHOR_CLASS,
        [ANCHOR_ATTR]: r.threadId,
        [ANCHOR_FOCUSED_ATTR]: String(focused),
        role: "mark",
        "aria-label": anchorLabel(r.threadId, focused),
      });
    });
  return DecorationSet.create(doc, decos);
}

const EMPTY: CommentHighlightPayload = { ranges: [], focused: null };

export const CommentHighlight = Extension.create({
  name: "commentHighlight",

  addProseMirrorPlugins() {
    return [
      new Plugin<CommentHighlightPayload>({
        key: commentHighlightKey,
        state: {
          init: () => EMPTY,
          apply(tr, value) {
            const pushed = tr.getMeta(commentHighlightKey) as
              | CommentHighlightPayload
              | undefined;
            if (pushed) return pushed;
            if (!tr.docChanged) return value;
            // Map through the change so a keystroke does not leave the
            // highlights visibly lagging before the recompute lands.
            return {
              ranges: value.ranges.map((r) => ({
                threadId: r.threadId,
                from: tr.mapping.map(r.from),
                to: tr.mapping.map(r.to),
              })),
              focused: value.focused,
            };
          },
        },
        props: {
          decorations(state) {
            const payload = commentHighlightKey.getState(state) ?? EMPTY;
            return buildDecorations(state.doc, payload);
          },
        },
      }),
    ];
  },
});

/**
 * The thread a click landed in, or null when it landed outside every anchor.
 *
 * Read off the DOM attribute the decoration carries rather than by re-resolving
 * the click's document position against the range list: the decoration is
 * already the authority on where each anchor renders, and asking it directly
 * cannot drift from what the user actually sees highlighted.
 */
export function threadIdAtEvent(target: EventTarget | null): string | null {
  if (!(target instanceof Element)) return null;
  const el = target.closest(`[${ANCHOR_ATTR}]`);
  return el?.getAttribute(ANCHOR_ATTR) ?? null;
}
