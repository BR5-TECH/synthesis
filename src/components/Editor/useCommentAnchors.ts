/**
 * Where each thread's passage sits on the editing surface, and which discussion
 * has the focus (CMT-FR-28, CMT-FR-36).
 *
 * Split out of the Editor whole because the pieces are one lookup: an anchor is
 * a range in the Markdown source, the layout effect finds that text in the
 * rendered body, and the decoration plugin marks the range the same lookup
 * produced.
 */
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { Editor as TiptapEditor } from "@tiptap/react";
import { docText, rangeToPositions } from "../findHighlight";
import { commentHighlightKey, type AnchorRange } from "../commentHighlight";
import { DIAGRAM_LAYOUT_EVENT } from "../markdownMermaid";
import { findNearest } from "../../state/commentAnchors";
import { identityBlockFor } from "../../hooks/useComments";
import type { EditMode } from "../../state/editHistory";
import type { EditSession } from "../../state/editSessions/types";

export interface CommentAnchorDeps {
  editor: TiptapEditor | null;
  session: EditSession;
  mode: EditMode;
  /** CMT-FR-28: whether the body marks the fragments at all. */
  marksShown: boolean;
  docVersion: number;
  /** The anchored-thread hook's state, which the placement is computed over. */
  comments: {
    threads: { thread: { id: string; resolved: boolean }; anchor: { quote: string; start: number } | null }[];
    identityError: Parameters<typeof identityBlockFor>[0];
  };
  focusedThread: string | null;
  setFocusedThread: (id: string | null) => void;
  focusThreadId: string | null | undefined;
  onThreadFocused: (() => void) | undefined;
}

export function useCommentAnchors(deps: CommentAnchorDeps) {
  const {
    editor,
    session,
    mode,
    marksShown,
    docVersion,
    comments,
    focusedThread,
    setFocusedThread,
    focusThreadId,
    onThreadFocused,
  } = deps;

  const bodyScrollRef = useRef<HTMLDivElement>(null);
  const [anchorRanges, setAnchorRanges] = useState<AnchorRange[]>([]);
  // EDT-FR-UYEN: a diagram fence changes its text without a document change — a
  // change of view, or a render that finished — so the marks are placed again.
  const [diagramLayout, setDiagramLayout] = useState(0);
  useEffect(() => {
    if (!editor || editor.isDestroyed) return;
    const dom = editor.view?.dom;
    if (!dom) return;
    const bump = () => setDiagramLayout((n) => n + 1);
    dom.addEventListener(DIAGRAM_LAYOUT_EVENT, bump);
    return () => dom.removeEventListener(DIAGRAM_LAYOUT_EVENT, bump);
  }, [editor]);

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
   * the column's shown state are set by the shell before the tab is focused, so all
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
   * CMT-FR-28: focusing a discussion scrolls its anchor into view — once per thread,
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
   * CMT-FR-28: the fragment quote of a discussion was activated in the column.
   * The fragment is scrolled into view again, also when its discussion already
   * has the focus. On the raw-text surface the caller puts the tab on WYSIWYG
   * first, and the effect above scrolls once the marks are painted there.
   */
  const revealFragment = useCallback(
    (threadId: string) => {
      scrolledForRef.current = null;
      setFocusedThread(threadId);
      if (mode === "wysiwyg" && scrollFragmentIntoView(threadId)) {
        scrolledForRef.current = threadId;
      }
    },
    [mode, scrollFragmentIntoView, setFocusedThread],
  );

  /**
   * CMT-FR-28: the range of each thread's anchor within the rendered body, which
   * the decoration plugin marks.
   *
   * The anchor itself is a range in the Markdown **source** — that is what is
   * persisted and what means the same thing in either mode (CMT-FR-06). Marking
   * it means finding the same text in the *rendered* body. For ordinary prose the
   * two are the same characters; where they are not (a quote that includes
   * Markdown syntax the WYSIWYG surface hides), the lookup fails and nothing is
   * marked rather than something wrong. The thread is never lost either way.
   */
  useLayoutEffect(() => {
    if (mode !== "wysiwyg" || !marksShown || !editor || editor.isDestroyed) return;
    const index = docText(editor.state.doc);
    const ranges: AnchorRange[] = [];
    for (const entry of comments.threads) {
      // A resolved thread's passage is no longer under discussion, so it is not
      // marked in the body — the disclosure is where it lives now (CMT-FR-17).
      if (entry.anchor === null || entry.thread.resolved) continue;
      const { quote, start } = entry.anchor;
      const at = findNearest(index.text, quote, start);
      if (at === -1) continue;
      const pos = rangeToPositions(index, at, at + quote.length);
      if (pos) ranges.push({ threadId: entry.thread.id, from: pos.from, to: pos.to });
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
    // `docVersion` stands in for the mutable document, and `diagramLayout` for
    // the height of its diagram fences.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    mode,
    marksShown,
    editor,
    docVersion,
    diagramLayout,
    comments.threads,
    session.seedToken,
  ]);

  return {
    bodyScrollRef,
    anchorRanges,
    identityBlock,
    scrollFragmentIntoView,
    revealFragment,
  };
}
