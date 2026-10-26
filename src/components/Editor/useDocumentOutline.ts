import { useEffect, useRef, useState, type RefObject } from "react";
import type { Editor as TiptapEditor } from "@tiptap/react";
import type { EditMode } from "../../state/editHistory";
import { logDebug, logWarn } from "../../logging";
import {
  richHeadings,
  sourceHeadings,
  type OutlineEntry,
} from "./outline";

/**
 * EDT-FR-KJLQ / EDT-FR-YSJX: the headings of the document a surface shows, and
 * the way to move to one of them.
 *
 * A store for `useSyncExternalStore`, so a table of contents control that is
 * not inside the Editor (the chrome row of a New Artifact tab, NAW-FR-TFFX)
 * reads the same list as one that is. The list is computed only when a
 * subscriber asks for it, and again only when the document changes, so a
 * closed panel costs nothing.
 */
export interface DocumentOutline {
  subscribe: (listener: () => void) => () => void;
  getSnapshot: () => readonly OutlineEntry[];
  reveal: (entry: OutlineEntry) => void;
}

interface OutlineInput {
  editor: TiptapEditor | null;
  mode: EditMode;
  sourceText: string;
  sourceRef: RefObject<HTMLTextAreaElement | null>;
}

const NO_ENTRIES: readonly OutlineEntry[] = [];

export function useDocumentOutline({
  editor,
  mode,
  sourceText,
  sourceRef,
}: OutlineInput): DocumentOutline {
  const input = useRef({ editor, mode, sourceText, sourceRef });
  input.current = { editor, mode, sourceText, sourceRef };
  const listeners = useRef(new Set<() => void>());
  const cache = useRef<{ key: unknown; entries: readonly OutlineEntry[] }>({
    key: null,
    entries: NO_ENTRIES,
  });

  const notify = () => {
    for (const listener of listeners.current) listener();
  };

  // EDT-FR-KJLQ: each change of the rich document — an edit, an undo, a redo,
  // or the sync that a switch into WYSIWYG makes — gives a new list.
  useEffect(() => {
    if (!editor) return;
    const onTransaction = ({ transaction }: { transaction: { docChanged: boolean } }) => {
      if (transaction.docChanged) notify();
    };
    editor.on("transaction", onTransaction);
    return () => {
      editor.off("transaction", onTransaction);
    };
  }, [editor]);

  // EDT-FR-KJLQ: an edit on the source surface, and a switch between modes.
  useEffect(() => {
    notify();
  }, [mode, sourceText, editor]);

  const [outline] = useState<DocumentOutline>(() => ({
    subscribe: (listener) => {
      listeners.current.add(listener);
      return () => {
        listeners.current.delete(listener);
      };
    },
    getSnapshot: () => {
      const { editor: ed, mode: m, sourceText: text } = input.current;
      // The rich document is immutable, so its identity says whether it
      // changed; the source text is a string, compared by value.
      const key = m === "text" ? text : (ed?.state.doc ?? null);
      if (cache.current.key === key) return cache.current.entries;
      const entries =
        m === "text" ? sourceHeadings(text) : ed ? richHeadings(ed.state.doc) : NO_ENTRIES;
      cache.current = { key, entries };
      return entries;
    },
    reveal: (entry) => {
      const { editor: ed, sourceRef: ref } = input.current;
      const el = ref.current;
      const moved =
        entry.target.kind === "rich"
          ? ed !== null && revealRich(ed, entry.target.pos)
          : el !== null && revealSource(el, entry.target.offset);
      const fields = { surface: entry.target.kind, level: entry.level };
      if (moved) {
        logDebug(["frontend"], "table of contents entry selected", fields);
      } else {
        // The surface went away, or an edit moved the heading between the
        // render of the list and the selection.
        logWarn(["frontend"], "table of contents target not found", fields);
      }
    },
  }));
  return outline;
}

/**
 * EDT-FR-YSJX / EDT-FR-NHDM: put the caret at the start of the heading at
 * `pos` and scroll it to the top of the view. A change of selection only, so
 * the buffer, the dirty state, and the undo history stay as they are.
 */
function revealRich(editor: TiptapEditor, pos: number): boolean {
  const node = editor.state.doc.nodeAt(pos);
  if (!node || node.type.name !== "heading") return false;
  editor
    .chain()
    .setTextSelection(pos + 1)
    .focus(undefined, { scrollIntoView: false })
    .run();
  const dom = editor.view.nodeDOM(pos);
  if (dom instanceof HTMLElement && typeof dom.scrollIntoView === "function") {
    dom.scrollIntoView({ behavior: "smooth", block: "start" });
  }
  return true;
}

/**
 * EDT-FR-YSJX: put the caret at the start of the source line at `offset` and
 * scroll that line to the top of the textarea.
 */
function revealSource(el: HTMLTextAreaElement, offset: number): boolean {
  const at = Math.min(offset, el.value.length);
  el.focus({ preventScroll: true });
  el.setSelectionRange(at, at);
  const top = lineTop(el, at);
  if (typeof el.scrollTo === "function") {
    el.scrollTo({ top, behavior: "smooth" });
  } else {
    el.scrollTop = top;
  }
  return true;
}

/** The style properties that decide where the text of a textarea wraps. */
const MIRRORED = [
  "boxSizing",
  "paddingTop",
  "paddingRight",
  "paddingBottom",
  "paddingLeft",
  "borderTopWidth",
  "borderRightWidth",
  "borderBottomWidth",
  "borderLeftWidth",
  "fontFamily",
  "fontSize",
  "fontWeight",
  "fontStyle",
  "lineHeight",
  "letterSpacing",
  "wordSpacing",
  "tabSize",
  "textIndent",
  "textTransform",
] as const;

/**
 * The scroll offset that puts the line holding `offset` at the top of the
 * textarea. A textarea cannot tell where a character is, so the text before it
 * is laid out in a hidden copy with the same metrics, and wrapped lines count.
 */
function lineTop(el: HTMLTextAreaElement, offset: number): number {
  const doc = el.ownerDocument;
  const style = doc.defaultView?.getComputedStyle(el);
  if (!style) return 0;
  const mirror = doc.createElement("div");
  for (const key of MIRRORED) mirror.style[key] = style[key];
  // The client width leaves out a scrollbar that the textarea shows and the
  // copy does not, so both wrap their text at the same width.
  const borders = (parseFloat(style.borderLeftWidth) || 0) + (parseFloat(style.borderRightWidth) || 0);
  mirror.style.width = `${el.clientWidth + borders}px`;
  mirror.style.position = "absolute";
  mirror.style.visibility = "hidden";
  mirror.style.top = "0";
  mirror.style.left = "-9999px";
  mirror.style.whiteSpace = "pre-wrap";
  mirror.style.overflowWrap = "break-word";
  mirror.style.height = "auto";
  mirror.textContent = el.value.slice(0, offset);
  const marker = doc.createElement("span");
  marker.textContent = "​";
  mirror.appendChild(marker);
  doc.body.appendChild(mirror);
  try {
    return Math.max(0, marker.offsetTop - (parseFloat(style.paddingTop) || 0));
  } finally {
    mirror.remove();
  }
}
