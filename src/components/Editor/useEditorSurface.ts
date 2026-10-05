/**
 * The Tiptap instance the WYSIWYG surface is, together with everything about
 * the images it carries (EDT-FR-85, EDT-FR-86, NAW-FR-50 .. NAW-FR-54).
 *
 * Held together because the image rules are written *into* the instance: the
 * paste, drop and arrow-key handlers ProseMirror captures at creation are the
 * offer to the host, and the decorations that draw what the host answered are
 * pushed into the same view. Splitting them would leave a handler in one file
 * deciding what a decoration in another draws.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { RefObject } from "react";
import { useEditor } from "@tiptap/react";
import type { Editor as TiptapEditor } from "@tiptap/react";
import { getMarkdown, markdownExtensions } from "../markdownFidelity";
import { CommentHighlight } from "../commentHighlight";
import { FindHighlight } from "../findHighlight";
import { HunkDecorations } from "../DraftDiscussion";
import { useHostImageDecorations } from "./useHostImageDecorations";
import { useHostImageResolution } from "./useHostImageResolution";
import {
  HostImages,
  imageFromTransfer,
  selectAdjacentImage,
  selectedImageAt,
  transferHasText,
  type ImageResolution,
  type PastedImage,
  type EditorImageHost,
} from "../hostImages";
import { splitFrontmatter } from "./frontmatter";
import type { EditMode, EditSurface } from "../../state/editHistory";
import type { EditSession } from "../../state/editSessions/types";

export interface EditorSurfaceDeps {
  session: EditSession;
  mode: EditMode;
  imageHost: EditorImageHost | undefined;
  imageHostRef: RefObject<EditorImageHost | undefined>;
  editorRef: RefObject<TiptapEditor | null>;
  announceRef: RefObject<((message: string) => void) | undefined>;
  bodyRef: RefObject<string>;
  loadedBodyRef: RefObject<string>;
  respellPendingRef: RefObject<boolean>;
  replacingRef: RefObject<boolean>;
  origFmRef: RefObject<{ fm: string | null; raw: string }>;
  sourceTextRef: RefObject<string>;
  docVersion: number;
  setDocVersion: (next: (v: number) => number) => void;
  currentDoc: () => string;
  noteEdit: (doc: string, mode: EditMode, surface: EditSurface) => void;
  applyFm: (next: string | null) => void;
  applySourceText: (next: string) => void;
}

export function useEditorSurface(deps: EditorSurfaceDeps) {
  const {
    session,
    mode,
    imageHost,
    imageHostRef,
    editorRef,
    announceRef,
    bodyRef,
    loadedBodyRef,
    respellPendingRef,
    replacingRef,
    origFmRef,
    sourceTextRef,
    docVersion,
    setDocVersion,
    currentDoc,
    noteEdit,
    applyFm,
    applySourceText,
  } = deps;

  /**
   * EDT-FR-85: what each of the document's image destinations resolved to.
   *
   * Held here rather than in the surface's own state because it is a fact about
   * the host's answers rather than about the document: the same destination
   * appearing twice is resolved once, and a destination the author repairs is
   * asked about afresh.
   */
  const [imageResolutions, setImageResolutions] = useState<
    Record<string, ImageResolution>
  >({});
  const imageResolutionsRef = useRef(imageResolutions);
  imageResolutionsRef.current = imageResolutions;
  /** NAW-FR-51: where an insertion is waiting on its store, `null` when none is. */
  const [insertingAt, setInsertingAt] = useState<number | null>(null);

  /**
   * NAW-FR-50 / NAW-FR-51: offer one image to the host and, where it takes it,
   * write what it answers with at the cursor as **one ordinary edit**.
   *
   * The pending state occupies the cursor position while the store is in flight
   * and does not block typing elsewhere in the prompt; a store the host refuses
   * inserts nothing at all, leaving the prompt byte-for-byte what it was and
   * taking no undo step.
   */
  const insertHostImage = useCallback(
    async (image: PastedImage) => {
      const host = imageHostRef.current;
      const surface = editorRef.current;
      if (!host?.accept || !surface) return;
      const at = surface.state.selection.from;
      setInsertingAt(at);
      let markdown: string | null = null;
      try {
        markdown = await host.accept(image);
      } finally {
        setInsertingAt(null);
      }
      // The host refused, or took nothing: no reference is inserted and no undo
      // step was taken. What the author is told is the host's to say.
      if (!markdown) return;
      const live = editorRef.current;
      if (!live) return;
      const { from, to } = live.state.selection;
      // One edit: the selection is replaced exactly as typed text would replace
      // it, and the caret lands directly after the inserted reference with
      // nothing selected (NAW-FR-50).
      live.chain().focus().insertContentAt({ from, to }, markdown).run();
    },
    [],
  );

  /**
   * EDT-FR-86: whether this surface takes the image the transfer carried.
   *
   * Decided **synchronously**, because ProseMirror needs the answer before the
   * event's default is allowed to run: the transfer is inspected for an image
   * here, and the reading and the store happen afterwards. A host that takes no
   * image, and a transfer carrying none, both fall through to what the paste
   * does today.
   */
  const takeImage = useCallback(
    (transfer: DataTransfer | null): boolean => {
      if (!imageHostRef.current?.accept || !transfer) return false;
      const carries =
        Array.from(transfer.files ?? []).some((f) =>
          (f.type ?? "").toLowerCase().startsWith("image/"),
        ) ||
        Array.from(transfer.items ?? []).some(
          (i) =>
            i.kind === "file" &&
            (i.type ?? "").toLowerCase().startsWith("image/"),
        );
      if (!carries) return false;
      // A transfer carrying both an image and a text flavour is a copy of
      // rendered content rather than a picture; the text is what the author
      // meant, and it is what the fallback pastes (EDT-FR-86).
      if (transferHasText(transfer)) return false;
      void imageFromTransfer(transfer).then((image) => {
        if (image) void insertHostImage(image);
      });
      return true;
    },
    [insertHostImage],
  );

  const editor = useEditor({
    // EDT-FR-22: undo/redo are served from the tab-wide history in this
    // component, so Tiptap's own (body-only) history stays off — two competing
    // stacks would let ⌘Z reverse a body edit without the tab knowing.
    // EFR-FR-DOQR: FindHighlight decorates the panel's matches in the rich body.
    // EDT-FR-67/EDT-FR-68: the schema models the Markdown this project is
    // written in, and text is written back as the characters the author typed,
    // so opening a file in the rich surface cannot cost it a construct.
    extensions: markdownExtensions([
      FindHighlight,
      // CMT-FR-28: anchored ranges are marked in the body alongside the find
      // matches; the two decorate independently so a match inside a commented
      // paragraph shows both.
      CommentHighlight,
      // EDT-FR-85: what an image destination resolved to, and the pending state
      // of an insertion. A third independent decoration, for the same reason
      // the two above are independent of each other.
      HostImages,
      // DCR-FR-05 / DCR-FR-07: an agent's proposed changes, drawn in the prose
      // they change. A fourth independent decoration — a find match inside a
      // proposed deletion marks both.
      HunkDecorations,
    ]),
    // The ProseMirror node carries the `doc` class so the existing document
    // typography (.doc h1/p/ul/blockquote/code…) styles the WYSIWYG content.
    editorProps: {
      attributes: { class: "doc editor__prose" },
      // EDT-FR-86: image data is offered to the host **first**. Where the host
      // does not take it — every Editor tab on a project file — this returns
      // false and the paste falls back to what it does today.
      handlePaste: (_view, event) => takeImage(event.clipboardData),
      handleDrop: (_view, event) => {
        const transfer = (event as DragEvent).dataTransfer;
        return takeImage(transfer);
      },
      // NAW-FR-54: extending the selection onto an image selects that image, so
      // the removal action is reachable from the keyboard alone. Offered only
      // where the host takes images at all — an Editor tab on a project file
      // has none to remove, and its arrow keys are left exactly as they were
      // (EDT-FR-86).
      handleKeyDown: (view, event) => {
        if (!imageHostRef.current) return false;
        if (!event.shiftKey || event.metaKey || event.ctrlKey || event.altKey) {
          return false;
        }
        const direction =
          event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
        if (direction === 0) return false;
        const tr = selectAdjacentImage(view.state, direction);
        if (!tr) return false;
        view.dispatch(tr);
        return true;
      },
    },
    // Tiptap v3 does not re-render on transactions by default; the formatting
    // toolbar reads editor.isActive(...) on render, so it must re-render on each
    // transaction (selection moves included) to keep its active state correct.
    shouldRerenderOnTransaction: true,
    // Each user edit syncs the body buffer and marks the tab dirty (EDT-FR-04).
    // setContent below is called with emitUpdate:false, so programmatic
    // (re)loads and mode switches do NOT mark dirty.
    onUpdate: ({ editor }) => {
      // EDT-FR-66: the loaded bytes stand until an edit, and this is that edit
      // — from here the body is the surface's serialisation, which may respell
      // Markdown the author never touched (CMT-FR-66). Asked of the buffer
      // rather than of a latch, so an undo back to the floor genuinely restores
      // the unserialised state instead of only appearing to.
      if (bodyRef.current === loadedBodyRef.current) {
        respellPendingRef.current = true;
      }
      bodyRef.current = getMarkdown(editor);
      // A replacement records one step for the whole operation once every
      // surface it touched has been rewritten (EFR-FR-FYRJ), so the body's own
      // handler stands down for it.
      if (replacingRef.current) return;
      // EDT-FR-22: a user edit to the rich body is a step in the artifact's
      // history. Programmatic writes use setContent(emitUpdate:false) and never
      // land here (EDT-FR-23).
      noteEdit(currentDoc(), "wysiwyg", "body");
    },
    // EFR-FR-ESDZ: any document change — including the programmatic ones
    // `onUpdate` skips — invalidates the match set computed over the body.
    onTransaction: ({ transaction }) => {
      if (transaction.docChanged) setDocVersion((v) => v + 1);
    },
  });

  // EDT-FR-18/EDT-FR-29/EDT-FR-30: seed the editing surfaces from the artifact's
  // buffer — on mount (a first open, or a tab switch resuming a retained
  // buffer), and again whenever a load is adopted (`seedToken`, so a
  // Load-from-filesystem re-applies even when the bytes are unchanged).
  useEffect(() => {
    const parsed = splitFrontmatter(session.buffer);
    origFmRef.current = { fm: parsed.fm, raw: parsed.raw };
    bodyRef.current = parsed.body;
    applyFm(parsed.fm);
    // The text-mode surface is the full file verbatim.
    applySourceText(session.buffer);
  }, [session, session.seedToken, applyFm, applySourceText]);

  // Push the body buffer into the WYSIWYG surface when entering WYSIWYG mode, on
  // mount, or when a load is adopted. Not keyed on `body`, so typing in WYSIWYG
  // (which updates body via onUpdate) never re-sets content. emitUpdate:false
  // keeps the sync clean — toggling modes is lossless and never marks the doc
  // dirty (EDT-FR-17) and is never an undoable step (EDT-FR-23).
  useEffect(() => {
    if (!editor || mode !== "wysiwyg") return;
    editor.commands.setContent(bodyRef.current, { emitUpdate: false });
  }, [editor, mode, session.seedToken]);

  // Kept current for the paste and drop handlers, which ProseMirror captured
  // once at creation.
  useEffect(() => {
    editorRef.current = editor;
  }, [editor]);

  // EDT-FR-85: ask the host about every destination the document now carries
  // that has not been asked about yet, and only when it comes into view.
  useHostImageResolution({
    editor,
    mode,
    docVersion,
    hostRef: imageHostRef,
    resolutionsRef: imageResolutionsRef,
    onResolved: setImageResolutions,
  });

  // EDT-FR-85 / NAW-FR-51: the resolutions and the pending marker, pushed into
  // the surface where they are decorations rather than content.
  useHostImageDecorations(editor, imageHost, imageResolutions, insertingAt);

  /**
   * NAW-FR-53: remove the Markdown reference of the image the surface has
   * selected, and nothing else.
   *
   * One ordinary edit of the prompt — one undo step, one write — and it deletes
   * no file: the asset goes only when housekeeping finds that the saved prompt
   * no longer references it, so removing one of several references to a shared
   * image leaves every other reference drawing.
   */
  /**
   * Both ends of the selection, as one value to depend on.
   *
   * `from` alone is not enough, and the case it misses is exactly the gesture
   * NAW-FR-54 is about: extending forward from the position immediately before
   * an image selects that image while leaving `from` where it already was, so a
   * memo keyed on `from` returns its cached `null` and the removal action never
   * appears. Approaching the same image from the right happens to move `from`
   * and works — which is what makes the omission look like a working feature.
   */
  const selectionRange = editor
    ? `${editor.state.selection.from}:${editor.state.selection.to}`
    : "";
  const selectedImage = useMemo(() => {
    if (!editor || mode !== "wysiwyg") return null;
    const { from, to } = editor.state.selection;
    return selectedImageAt(editor.state.doc, from, to);
  }, [editor, mode, docVersion, selectionRange]);

  const removeSelectedImage = useCallback(() => {
    const live = editorRef.current;
    if (!live || !selectedImage) return;
    live
      .chain()
      .focus()
      .deleteRange({ from: selectedImage.from, to: selectedImage.to })
      .run();
    announceRef.current?.("Image reference removed.");
  }, [selectedImage]);

  /**
   * EDT-FR-86 / NAW-FR-50: the raw surface's half of the offer.
   *
   * The same host, the same one edit, and the same predictable selection — only
   * the surface differs: a `<textarea>` holds the whole file as text, so the
   * reference is spliced at the caret and the caret lands directly after it.
   */
  const takeSourceImage = useCallback(
    (transfer: DataTransfer | null, element: HTMLTextAreaElement): boolean => {
      const host = imageHostRef.current;
      if (!host?.accept || !transfer) return false;
      const carries = Array.from(transfer.files ?? []).some((f) =>
        (f.type ?? "").toLowerCase().startsWith("image/"),
      );
      if (!carries || transferHasText(transfer)) return false;
      const start = element.selectionStart;
      const end = element.selectionEnd;
      void imageFromTransfer(transfer).then(async (image) => {
        if (!image) return;
        const markdown = await host.accept?.(image);
        if (!markdown) return;
        const value = sourceTextRef.current;
        const next = value.slice(0, start) + markdown + value.slice(end);
        applySourceText(next);
        noteEdit(next, "text", "source");
        const caret = start + markdown.length;
        requestAnimationFrame(() => element.setSelectionRange(caret, caret));
      });
      return true;
    },
    [applySourceText, noteEdit],
  );

  return { editor, selectedImage, removeSelectedImage, takeSourceImage };
}
