/**
 * Where each thread's passage sits on the editing surface, and which card has
 * the focus (CMT-FR-27, CMT-FR-28, CMT-FR-36, CMT-FR-64).
 *
 * Split out of the Editor whole because the three pieces are one measurement:
 * an anchor is a range in the Markdown source, the layout effect finds that
 * text in the rendered body to get a pixel offset, and the decoration plugin
 * marks the range the same lookup produced. A card aligned by one of them and
 * marked by another would drift apart.
 */
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { RefObject } from "react";
import type { Editor as TiptapEditor } from "@tiptap/react";
import { docText, rangeToPositions } from "../findHighlight";
import { commentHighlightKey, type AnchorRange } from "../commentHighlight";
import { findNearest } from "../../state/commentAnchors";
import { identityBlockFor, DRAFT_KEY } from "../../hooks/useComments";
import { useCommentArrangement } from "../../hooks/useCommentArrangement";
import type { EditMode } from "../../state/editHistory";
import type { EditSession } from "../../state/editSessions/types";
import type { CommentSelection } from "./selection";

export interface CommentAnchorDeps {
  rootRef: RefObject<HTMLDivElement | null>;
  editor: TiptapEditor | null;
  session: EditSession;
  mode: EditMode;
  railOpen: boolean;
  docVersion: number;
  /** The anchored-thread hook's state, which the placement is computed over. */
  comments: {
    threads: { thread: { id: string; resolved: boolean }; anchor: { quote: string; start: number } | null }[];
    identityError: Parameters<typeof identityBlockFor>[0];
  };
  draft: CommentSelection | null;
  focusedThread: string | null;
  setFocusedThread: (id: string | null) => void;
  focusThreadId: string | null | undefined;
  onThreadFocused: (() => void) | undefined;
}

export function useCommentAnchors(deps: CommentAnchorDeps) {
  const {
    rootRef,
    editor,
    session,
    mode,
    railOpen,
    docVersion,
    comments,
    draft,
    focusedThread,
    setFocusedThread,
    focusThreadId,
    onThreadFocused,
  } = deps;

  const bodyScrollRef = useRef<HTMLDivElement>(null);
  // CMT-FR-64: measured on the tab rather than on the window, because the tab is
  // what the cards have to fit in — dragging the Library panel wider narrows it
  // without the window changing size at all.
  const arrangement = useCommentArrangement(rootRef);
  // How far the editing surface has scrolled, which the comment layer subtracts
  // from each card's placement so the two move together (CMT-FR-27).
  const [scrollTop, setScrollTop] = useState(0);
  const [anchorTops, setAnchorTops] = useState<Record<string, number>>({});
  const [anchorRanges, setAnchorRanges] = useState<AnchorRange[]>([]);

  /**
   * CMT-FR-28: push the anchor ranges and the focused thread into the decoration
   * plugin. A meta-only transaction changes no document, so this adds no history
   * step (EDT-FR-23) and does not re-enter `onUpdate`.
   */
  useEffect(() => {
    if (!editor || editor.isDestroyed) return;
    const tr = editor.state.tr.setMeta(commentHighlightKey, {
      ranges: anchorRanges,
      focused: focusedThread,
    });
    tr.setMeta("addToHistory", false);
    editor.view.dispatch(tr);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editor, anchorRanges, focusedThread]);

  const identityBlock = identityBlockFor(comments.identityError);

  /**
   * CMT-FR-36: land on the thread the Comments panel sent us to. The mode and
   * the rail's open state are set by the shell before the tab is focused, so all
   * that is left here is the focus itself — which drives the scroll-into-view
   * effect below and the distinct highlight of CMT-FR-28.
   *
   * The callback goes through a ref so a caller that re-creates it every render
   * cannot turn this into a loop.
   */
  const onThreadFocusedRef = useRef(onThreadFocused);
  onThreadFocusedRef.current = onThreadFocused;
  useEffect(() => {
    if (!focusThreadId) return;
    setFocusedThread(focusThreadId);
    onThreadFocusedRef.current?.();
  }, [focusThreadId]);

  /**
   * CMT-FR-28: focusing a card scrolls its anchor into view — once per thread,
   * so re-rendering for any other reason does not yank the body around under the
   * author.
   *
   * The decorations that carry the anchors are painted by a later dispatch than
   * the one that sets the focus, so on arrival from the Comments panel
   * (CMT-FR-36) the marked passage does not exist yet when the focus lands.
   * Hence the dependency on `anchorRanges`: the effect re-runs when they appear
   * and scrolls then, guarded by `scrolledForRef` so the *subsequent* changes an
   * edit makes to those ranges do not scroll again.
   */
  const scrolledForRef = useRef<string | null>(null);
  /**
   * CMT-FR-28: bring the fragment of one discussion into view. Returns whether a
   * marked fragment was found. An orphaned discussion has none and is left as it
   * is. Nothing here changes the document.
   */
  const scrollFragmentIntoView = useCallback((threadId: string): boolean => {
    const root = bodyScrollRef.current;
    if (!root) return false;
    const el = root.querySelector<HTMLElement>(
      `[data-comment-thread="${CSS.escape(threadId)}"]`,
    );
    if (!el) return false;
    el.scrollIntoView?.({ block: "nearest" });
    return true;
  }, []);
  useEffect(() => {
    if (!focusedThread) {
      scrolledForRef.current = null;
      return;
    }
    if (mode !== "wysiwyg") return;
    if (scrolledForRef.current === focusedThread) return;
    if (scrollFragmentIntoView(focusedThread)) scrolledForRef.current = focusedThread;
  }, [focusedThread, mode, anchorRanges, scrollFragmentIntoView]);

  /**
   * CMT-FR-27: the pixel offset of each thread's anchor within the editing
   * surface, which is what the rail aligns its cards to.
   *
   * The anchor itself is a range in the Markdown **source** — that is what is
   * persisted and what means the same thing in either mode (CMT-FR-06). Turning
   * it into a screen position means finding the same text in the *rendered* body,
   * because that is where the pixels are. For ordinary prose the two are the same
   * characters; where they are not (a quote that includes Markdown syntax the
   * WYSIWYG surface hides), the lookup fails and the card falls back to the top of
   * the rail rather than being placed somewhere wrong. The thread is never lost
   * either way — only its alignment is best-effort.
   */
  useLayoutEffect(() => {
    if (mode !== "wysiwyg" || !railOpen || !editor || editor.isDestroyed) return;
    const container = bodyScrollRef.current;
    if (!container) return;
    const index = docText(editor.state.doc);
    const containerTop = container.getBoundingClientRect().top - container.scrollTop;
    const tops: Record<string, number> = {};
    const ranges: AnchorRange[] = [];
    const place = (key: string, quote: string, hint: number, mark: boolean) => {
      const at = findNearest(index.text, quote, hint);
      if (at === -1) return;
      const pos = rangeToPositions(index, at, at + quote.length);
      if (!pos) return;
      if (mark) ranges.push({ threadId: key, from: pos.from, to: pos.to });
      try {
        tops[key] = Math.max(0, editor.view.coordsAtPos(pos.from).top - containerTop);
      } catch {
        // A position the view cannot resolve (mid-relayout) simply has no
        // alignment this pass; the next one picks it up.
      }
    };
    for (const entry of comments.threads) {
      if (entry.anchor === null) continue;
      // A resolved thread's passage is no longer under discussion, so it is not
      // marked in the body — the disclosure is where it lives now (CMT-FR-17).
      place(
        entry.thread.id,
        entry.anchor.quote,
        entry.anchor.start,
        !entry.thread.resolved,
      );
    }
    if (draft) {
      place(DRAFT_KEY, draft.quote, draft.start, false);
    }
    setAnchorRanges((prev) =>
      prev.length === ranges.length &&
      prev.every(
        (r, i) =>
          r.threadId === ranges[i].threadId &&
          r.from === ranges[i].from &&
          r.to === ranges[i].to,
      )
        ? prev
        : ranges,
    );
    setAnchorTops((prev) => {
      const sameKeys =
        Object.keys(prev).length === Object.keys(tops).length &&
        Object.keys(tops).every((k) => prev[k] === tops[k]);
      return sameKeys ? prev : tops;
    });
    // `docVersion` stands in for the mutable document.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode, railOpen, editor, docVersion, comments.threads, draft, session.seedToken]);

  return {
    bodyScrollRef,
    arrangement,
    scrollTop,
    setScrollTop,
    anchorTops,
    setAnchorTops,
    anchorRanges,
    identityBlock,
    scrollFragmentIntoView,
  };
}
