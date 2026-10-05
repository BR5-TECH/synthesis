import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type { MouseEvent as ReactMouseEvent } from "react";
import type { Editor as TiptapEditor } from "@tiptap/react";
import type { EditorImageHost } from "../hostImages";
import { ANCHOR_ATTR, threadIdAtEvent } from "../commentHighlight";
import { focusDiscussion } from "../../state/discussionFocus";
import { diffEdit, findNearest, shiftAnchor } from "../../state/commentAnchors";
import { useHunkPlacement } from "./useHunkPlacement";
import { useArtifactDocument } from "../../hooks/useArtifactDocument";
import { useExternalChanges } from "../../hooks/useExternalChanges";
import {
  openPromptReview,
  usePromptProposals,
} from "../../state/promptProposals";
import { recordEdit, redoStep, undoStep } from "../../state/editHistory";
import type {
  EditMode,
  EditSurface,
  HistoryMove,
} from "../../state/editHistory";
import { EditSessionStore } from "../../state/editSessions";
import { isMarkdownFile } from "../../state/syntaxHighlight";
import { joinFrontmatter, splitFrontmatter } from "./frontmatter";
import {
  bodyOffsetOfSelection,
  offsetPoint,
  selectionPoint,
  sourceRange,
  type CommentSelection,
  type PendingSelection,
} from "./selection";
import type { LayerAnchor } from "./highlight";
import { REVIEW_SELECTOR, type EditorProps } from "./props";
import { MockEditor } from "./MockEditor";
import { DRAFT_KEY } from "../../hooks/useComments";
import { EditorView } from "./EditorView";
import { useCommentAnchors } from "./useCommentAnchors";
import { useCommentRail } from "./useCommentRail";
import { useEditorFind } from "./useEditorFind";
import { useEditorSurface } from "./useEditorSurface";
import { useFragmentHost } from "./useFragmentHost";

export { AFFORDANCE_WIDTH } from "./selection";

export function Editor(props: EditorProps) {
  const fallback = useState(() => new EditSessionStore())[0];
  // EDT-FR-01/02: a real filesystem artifact loads/saves live via the backend.
  // Surfaces without a backend id (e.g. Dashboard canned rows) keep the mock.
  return props.artifactId ? (
    <LiveEditor
      {...props}
      artifactId={props.artifactId}
      sessions={props.sessions ?? fallback}
    />
  ) : (
    <MockEditor {...props} />
  );
}

/**
 * EDT-FR-01/02/09–15: a live, backend-backed WYSIWYG editor.
 *
 * Tiptap (StarterKit + the Markdown extension) renders the artifact's Markdown
 * as WYSIWYG (EDT-FR-02); the on-disk file stays plain Markdown — `tiptap-markdown`
 * parses it on load and serialises it back on save, so no Synthesis-specific
 * syntax leaks into the file. The `useArtifactDocument` hook owns the lifecycle
 * (load/save/baseline/external-change); this component owns the editing surface
 * and bridges the two. Cut/copy/paste/select-all come from the contenteditable
 * plus the shell's native Edit menu (SNV-FR-14); undo and redo are claimed by
 * this component and served from the artifact's single history spanning both
 * editing modes (EDT-FR-22–EDT-FR-25 / `EXC-editor-external-change.md` EXC-FR-QDMC), so Tiptap's own history is switched off.
 *
 * The component is a *view* onto the artifact's edit session (EDT-FR-28): the
 * buffer, dirty flag, editing mode and history all live in that record, so this
 * component can unmount (a tab switch) and remount without the artifact losing
 * anything or being reloaded (EDT-FR-29 / EDT-FR-30).
 */
function LiveEditor({
  artifactId,
  artifactName,
  artifactType,
  sessions,
  focusThreadId,
  onThreadFocused,
  onEdit,
  showComments = true,
  showActions = true,
  readOnly,
  report,
  imageHost,
  announce,
  review,
  fragmentHost,
}: EditorProps & { artifactId: string; sessions: EditSessionStore }) {
  // EXC-FR-LKHZ: keep the store's divergence watch alive while an Editor is up, so
  // the surface works on its own (embedded, or without the shell) rather than
  // relying on an ambient watcher. Ref-counted — the shell holds one too.
  useExternalChanges(sessions);
  const doc = useArtifactDocument(artifactId, sessions);
  /**
   * PCR-FR-16 / PCR-FR-27: the artifact's proposals, from the one reading per
   * artifact every surface here renders from. Subscribing is what takes the
   * reading and what makes this tab somewhere an arriving proposal can arrive.
   */
  const promptProposals = usePromptProposals(artifactId);
  const pendingProposal = promptProposals.find((p) => p.state === "pending");
  const session = doc.session;

  /**
   * ESH-FR-BLTT: which of the two shapes of file this tab has open.
   *
   * Read from the file's name alone — `.md` in any letter case is a Markdown
   * file, everything else is a source file — and from the same string the
   * session record derived its starting mode from, so the two can never disagree
   * about which surface the tab is on.
   */
  const isMarkdown = isMarkdownFile(artifactId);
  // EDT-FR-17/EDT-FR-29: WYSIWYG is the default the first time a Markdown file
  // opens in a session, and a reopened tab resumes the mode its record carries.
  // ESH-FR-LKNM: a source file has one surface, so its record starts on `text` and
  // nothing offers to move it — the guard here is belt-and-braces against a
  // record built before the file's shape was known.
  const mode: EditMode = isMarkdown ? session.mode : "text";
  // EDT-FR-18: in WYSIWYG, frontmatter is carved out and edited independently of
  // the rich body; `fm` is its inner text (null when the file has none).
  const [fm, setFm] = useState<string | null>(null);
  // The canonical body buffer (frontmatter stripped, WYSIWYG side), held in a ref
  // so the WYSIWYG sync effect, the mode toggle, and save read the latest without
  // re-rendering on every keystroke. origFmRef holds the current frontmatter
  // baseline for byte-preserving round-trips.
  const bodyRef = useRef("");
  const origFmRef = useRef<{ fm: string | null; raw: string }>({
    fm: null,
    raw: "",
  });
  /**
   * EDT-FR-66 / CMT-FR-66: set on the edit that first replaces the loaded bytes
   * with the rich surface's serialisation.
   *
   * That step can respell Markdown anywhere in the file, not only where the
   * author typed, so the anchors cannot be tracked through it as a single edit
   * — they are re-resolved from scratch instead (CMT-FR-66). Cleared once that
   * has happened; every later edit is an ordinary one the diff can follow.
   */
  const respellPendingRef = useRef(false);
  /**
   * EDT-FR-66: the body exactly as the artifact loaded with it.
   *
   * `bodyRef` equals this for as long as the loaded bytes stand, which is what
   * makes "the next edit is the one that first serialises them" a question the
   * buffer answers rather than a flag to keep in step with it. Undo reaches the
   * floor by restoring these very bytes (EDT-FR-24), so an artifact undone all
   * the way back is once again unserialised — and the edit after that respells
   * the file and re-resolves the anchors, exactly as the first one did.
   */
  const loadedBodyRef = useRef("");
  // EDT-FR-17/EDT-FR-18: in raw-text mode the source surface holds the WHOLE
  // file (frontmatter + body) as one editable document — no separate region.
  const [sourceText, setSourceText] = useState("");
  // Mirrors of the two pieces of surface state the history path reads from
  // outside React's render flow: Tiptap's onUpdate is captured once at editor
  // creation, and the undo/redo listeners are native. Written next to their
  // setState so a reader is never a commit behind. (The mode needs no mirror —
  // it lives on the session record, which is itself a stable mutable object.)
  const fmRef = useRef<string | null>(null);
  const sourceTextRef = useRef("");
  const applyMode = useCallback(
    (next: EditMode) => {
      // ESH-FR-SSDV: a source file has one surface. Nothing in the tab offers to
      // leave it, but a history step or an arriving activation can still name a
      // mode, and honouring one would put a Rust file on the rich surface.
      if (!isMarkdown && next !== "text") return;
      sessions.update(artifactId, { mode: next });
    },
    [artifactId, isMarkdown, sessions],
  );
  const applyFm = useCallback((next: string | null) => {
    fmRef.current = next;
    setFm(next);
  }, []);
  const applySourceText = useCallback((next: string) => {
    sourceTextRef.current = next;
    setSourceText(next);
  }, []);
  // The whole Markdown the artifact currently holds, in either mode — the unit
  // the history records and restores (EDT-FR-22), and the bytes a save writes.
  /**
   * The bytes a save writes right now. Text mode is the whole file verbatim
   * (byte-exact); WYSIWYG rejoins the carved-out frontmatter with the body
   * (EDT-FR-18).
   *
   * EDT-FR-66: the body is `bodyRef`, which holds the bytes the artifact loaded
   * with until a user edit replaces them with the rich surface's serialisation
   * — it is not re-derived from the editor here. Re-serialising on every save
   * would rewrite a file the author had only read, spelling its Markdown the
   * serialiser's way and producing a diff nobody authored. `bodyRef` is kept
   * current by the surface's own `onUpdate` and by the replacement path, both of
   * which are user edits, so what a save writes is still the document on screen.
   */
  const currentDoc = useCallback(
    () =>
      session.mode === "text"
        ? sourceTextRef.current
        : joinFrontmatter(fmRef.current, bodyRef.current, origFmRef.current),
    [session],
  );
  /**
   * EDT-FR-22/EDT-FR-28: a user edit — the artifact's buffer moves to `next`, the
   * step joins its history, and the artifact is dirty. Every editing surface
   * funnels through here, so the buffer a flush writes (EDT-FR-31) is always the
   * document the user is looking at, whichever surface produced it.
   */
  const noteEdit = useCallback(
    (next: string, editMode: EditMode, surface: EditSurface) => {
      session.buffer = next;
      recordEdit(session.history, next, editMode, surface);
      doc.markDirty();
      // NAW-FR-13: every edit, not only the one that turned the document dirty
      // — a debounce is only a debounce if the last keystroke restarts it.
      onEdit?.();
    },
    [session, doc, onEdit],
  );
  // EDT-FR-20: the WYSIWYG frontmatter region collapses as the artifact scrolls
  // down. `atTop` tracks the body scroll position (authoritative); `expandOverride`
  // is a manual peek (true) / minimize (false) that the next scroll dismisses.
  // Displayed expanded = expandOverride ?? atTop.
  const [atTop, setAtTop] = useState(true);
  const [expandOverride, setExpandOverride] = useState<boolean | null>(null);
  const fmExpanded = expandOverride ?? atTop;

  // Bumped on every document change the WYSIWYG surface makes, user or
  // programmatic, so the find panel's projection of the body (`bodyIndex`)
  // recomputes. Not derived from `onUpdate`: that deliberately does not fire for
  // programmatic writes, which still change what there is to match against.
  const [docVersion, setDocVersion] = useState(0);
  // Set while a replacement is being applied, so the surfaces' own change
  // handlers do not each record a history step — the replacement records
  // exactly one for the whole operation (EFR-FR-FYRJ).
  const replacingRef = useRef(false);

  /**
   * The live surface, for the handlers ProseMirror captures at creation — they
   * are installed before `editor` exists and must still reach it.
   */
  const editorRef = useRef<TiptapEditor | null>(null);
  const imageHostRef = useRef<EditorImageHost | undefined>(imageHost);
  imageHostRef.current = imageHost;
  const announceRef = useRef<((message: string) => void) | undefined>(announce);
  announceRef.current = announce;
  /**
   * EDT-FR-85 / EDT-FR-86 / NAW-FR-50 .. NAW-FR-54: the rich editing surface and
   * everything about the images it carries.
   */
  const surfaceGroup = useEditorSurface({
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
  });
  const { editor } = surfaceGroup;


  // EXC-FR-VTUH/EXC-FR-QDMC: the external-change modal blocks the tab outright —
  // every edit operation is inert while it is unresolved, and the write the
  // author had scheduled is held rather than performed (EDT-FR-70).
  const blocked = doc.conflict || readOnly === true;

  // EXC-FR-VTUH: the modal is blocking — editing is disabled while it is up.
  // `false` suppresses Tiptap's update event so toggling editability never
  // marks the doc dirty (it would otherwise fire on mount and on resolution).
  useEffect(() => {
    editor?.setEditable(!blocked, false);
  }, [editor, blocked]);

  // EDT-FR-17: switch modes, carrying edits across without loss and without
  // marking the doc dirty (no markDirty here; setContent uses emitUpdate:false).
  // A toggle is not an edit: it records nothing, so undo never reverses one
  // (EDT-FR-25) — it is byte-neutral, so both modes describe the same document.
  const enterMode = useCallback(
    (next: EditMode) => {
      // EDT-FR-20: the body scroll container is display:none in text mode and the
      // webview resets its scrollTop, so re-baseline the frontmatter collapse state
      // to the top (expanded) on any switch rather than trusting stale scroll state.
      setAtTop(true);
      setExpandOverride(null);
      if (next === "text") {
        // Compose the full file from the live WYSIWYG state for inline editing.
        applySourceText(
          joinFrontmatter(fmRef.current, bodyRef.current, origFmRef.current),
        );
      } else {
        // Re-split the (possibly edited) source; it becomes the new split baseline,
        // so the WYSIWYG region reflects inline frontmatter edits made in text mode.
        const parsed = splitFrontmatter(sourceTextRef.current);
        origFmRef.current = { fm: parsed.fm, raw: parsed.raw };
        bodyRef.current = parsed.body;
        applyFm(parsed.fm);
      }
      applyMode(next);
    },
    [applyFm, applyMode, applySourceText],
  );

  // ESH-FR-LKNM: offered over a Markdown file alone — a source file's cluster has
  // no toggle at all, so this is never reachable there.
  const toggleMode = () =>
    enterMode(session.mode === "wysiwyg" ? "text" : "wysiwyg");

  /**
   * PCR-FR-01 / EDT-FR-25: the tab's own root. Declared before the find surface,
   * which scrolls the current match into view through it.
   */
  const rootRef = useRef<HTMLDivElement>(null);

  // EFR-editor-find-replace.md: the two panels, their match engine, and the
  // decorations and replace actions built on it.
  const findGroup = useEditorFind({
    artifactId,
    sessions,
    session,
    blocked,
    mode,
    isMarkdown,
    editor,
    sourceText,
    sourceTextRef,
    fm,
    fmRef,
    fmExpanded,
    bodyRef,
    replacingRef,
    rootRef,
    docVersion,
    currentDoc,
    noteEdit,
    applyFm,
    applySourceText,
    setExpandOverride,
  });
  const { sourceTokens, sourceRef, sourceHlRef, sourceWrapRef } = findGroup;


  /**
   * EDT-FR-25: put the tab back into the state a history move describes. The
   * move carries the mode its step was authored in, so the Editor switches there
   * first and the reversal is visible where the user made it; a frontmatter step
   * additionally peeks the region open if it sits collapsed (EDT-FR-20).
   */
  const restore = useCallback(
    (move: HistoryMove) => {
      const target = move.mode ?? session.mode;
      if (target !== session.mode) {
        // Not enterMode(): that derives one surface from the other, which would
        // overwrite the restored document with the live buffer.
        setAtTop(true);
        setExpandOverride(null);
        applyMode(target);
      }
      const parsed = splitFrontmatter(move.doc);
      origFmRef.current = { fm: parsed.fm, raw: parsed.raw };
      bodyRef.current = parsed.body;
      // CMT-FR-66: a traversal replaces the buffer wholesale rather than
      // editing it in place, so — like the step that first serialises it —
      // there is no single edit for the diff to follow. Re-resolve the anchors
      // instead: undo is not a passage-destroying edit, and a thread must not
      // orphan for a step that put its passage back (CMT-FR-20).
      respellPendingRef.current = true;
      applyFm(parsed.fm);
      applySourceText(move.doc);
      if (target === "wysiwyg") {
        // emitUpdate:false — restoring is not itself an edit (EDT-FR-23).
        editor?.commands.setContent(parsed.body, { emitUpdate: false });
        if (move.surface === "frontmatter") setExpandOverride(true);
      }
      // A traversal changes the buffer, so the artifact is dirty. Undoing all the
      // way back to the loaded document also leaves it flagged: the indicator is
      // conservative here rather than tracking byte equality with the last write.
      session.buffer = move.doc;
      doc.markDirty();
    },
    [applyFm, applyMode, applySourceText, doc, editor, session],
  );

  // EXC-FR-QDMC: while a modal blocks the tab, undo and redo are inert like every
  // other edit operation.
  const undo = useCallback(() => {
    if (blocked) return;
    const move = undoStep(session.history);
    if (move) restore(move);
  }, [blocked, session, restore]);

  const redo = useCallback(() => {
    if (blocked) return;
    const move = redoStep(session.history);
    if (move) restore(move);
  }, [blocked, session, restore]);

  /**
   * EDT-FR-15/EDT-FR-22: the Editor claims undo/redo for the whole tab. Both
   * routes are intercepted natively (React's synthetic beforeInput does not
   * carry `inputType`) and in the capture phase, so neither the contenteditable
   * nor a textarea gets to run its own surface-local history first:
   *  - the accelerators (⌘Z / ⇧⌘Z / ⌘Y) as keydown, and
   *  - the native Edit menu's Undo/Redo roles (SNV-FR-14), which reach the
   *    webview as a beforeinput with inputType historyUndo/historyRedo.
   * The default is always prevented — including while blocked (EXC-FR-QDMC) — so a
   * surface-local stack can never diverge from the tab history.
   *
   * EDT-FR-84 / PCR-FR-20: the one thing inside this tab the claim does **not**
   * reach is the prompt change review's candidate. That candidate is not this
   * artifact's editing session — it holds its own buffer and its own undo
   * history — so one undo issued in the tab and one issued in the review have to
   * reverse two different edits. The claim is in the capture phase on the tab's
   * own root, which is an ancestor of the modal (PCR-FR-01 anchors it here), so
   * without this it would run first, prevent the default, and undo the
   * **artifact** while the author was looking at the candidate.
   */
  useEffect(() => {
    const el = rootRef.current;
    if (!el) return;
    const inTheReview = (target: EventTarget | null): boolean =>
      target instanceof Node &&
      (target instanceof Element
        ? target.closest(REVIEW_SELECTOR) !== null
        : target.parentElement?.closest(REVIEW_SELECTOR) != null);
    const onKeyDown = (e: KeyboardEvent) => {
      if (!e.metaKey && !e.ctrlKey) return;
      const key = e.key.toLowerCase();
      const isRedo = key === "y" || (key === "z" && e.shiftKey);
      const isUndo = key === "z" && !e.shiftKey;
      if (!isUndo && !isRedo) return;
      if (inTheReview(e.target)) return;
      e.preventDefault();
      e.stopPropagation();
      if (isRedo) redo();
      else undo();
    };
    const onBeforeInput = (e: Event) => {
      const type = (e as InputEvent).inputType;
      if (type !== "historyUndo" && type !== "historyRedo") return;
      if (inTheReview(e.target)) return;
      e.preventDefault();
      e.stopPropagation();
      if (type === "historyRedo") redo();
      else undo();
    };
    el.addEventListener("keydown", onKeyDown, true);
    el.addEventListener("beforeinput", onBeforeInput, true);
    return () => {
      el.removeEventListener("keydown", onKeyDown, true);
      el.removeEventListener("beforeinput", onBeforeInput, true);
    };
  }, [undo, redo]);

  /**
   * CMT-FR-05 / CMT-FR-25: the composer's draft, declared before the rail so the
   * rail's own toggle can abandon one in flight rather than leave it armed where
   * it cannot be seen or cancelled.
   */
  const [draft, setDraft] = useState<CommentSelection | null>(null);

  // ---- Comment rail (CMT-comments.md, hosted per EDT-FR-61) ---------------
  const railGroup = useCommentRail({
    artifactId,
    sessions,
    session,
    doc,
    mode,
    isMarkdown,
    showComments,
    showActions,
    currentDoc,
    setDraft,
  });
  const { comments, railOpen, railOnSource } = railGroup;


  /**
   * CMT-FR-20: keep every anchor pointed at its passage as the buffer moves.
   *
   * Derived from the buffer rather than reported by each editing surface: the
   * surfaces are three (rich body, frontmatter region, raw source) and a
   * replacement rewrites several at once, so watching the composed document is
   * the one place that sees every change exactly once.
   */
  const lastSourceRef = useRef<string | null>(null);
  useEffect(() => {
    if (!doc.loaded) return;
    const now = currentDoc();
    const before = lastSourceRef.current;
    lastSourceRef.current = now;
    if (before === null || before === now) return;
    // CMT-FR-66: the step that replaced the loaded bytes with the surface's
    // serialisation can respell Markdown anywhere in the file, so there is no
    // single edit for the diff to follow. Re-resolve every anchor against the
    // new buffer instead, on the terms a load resolves them (CMT-FR-18), so a
    // thread whose passage survives in a different spelling keeps its anchor
    // rather than orphaning on a rewriting the author did not perform.
    if (respellPendingRef.current) {
      respellPendingRef.current = false;
      comments.reanchorAll(now);
      return;
    }
    comments.noteBufferChange(before, now);
    // CMT-FR-20 applies to the draft's passage too. Nothing else tracks it —
    // it belongs to no thread yet — and posting an anchor the buffer has moved
    // out from under would persist a thread pointing at the wrong text.
    const edit = diffEdit(before, now);
    if (edit) {
      setDraft((prev) => {
        if (!prev) return prev;
        const moved = shiftAnchor(prev, edit);
        // An edit that lands *inside* the passage leaves nothing to comment on:
        // the same case that orphans a thread (CMT-FR-19), except this one has
        // no thread to orphan.
        return moved === null ? null : { ...moved };
      });
      // The affordance describes a selection the edit has just invalidated,
      // pixels included. Any real selection gesture re-creates it.
      setPendingSelection(null);
    }
  });

  /**
   * CMT-FR-18 / CMT-FR-22: a load the Editor adopts replaces the buffer wholesale
   * — a first open, or Load-from-filesystem — so anchors are re-resolved from
   * scratch rather than tracked through an edit that has nothing to track.
   */
  useEffect(() => {
    if (!doc.loaded) return;
    const content = currentDoc();
    lastSourceRef.current = content;
    // EDT-FR-66: a load adopts the file's own bytes, so the next edit is once
    // again the one that first serialises them.
    loadedBodyRef.current = bodyRef.current;
    respellPendingRef.current = false;
    comments.reanchorAll(content);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [session.seedToken, doc.loaded]);

  /**
   * CMT-FR-28: activating a highlight focuses that thread's card. The rail's own
   * `onMouseDown` closes the loop in the other direction.
   */
  const focusThreadFromBody = useCallback(
    (e: ReactMouseEvent) => {
      const threadId = threadIdAtEvent(e.target);
      if (!threadId) return;
      setFocusedThread(threadId);
      // CMT-FR-28 / CVP-FR-06: the card that already shows the discussion takes
      // focus. The text is not touched.
      focusDiscussion(threadId);
      // DDS-FR-QMBC: activating a fragment focuses its discussion in the host.
      fragmentHost?.onActivate(threadId);
    },
    [fragmentHost],
  );

  // CMT-FR-05: the selection a Comment affordance would open a thread over, and
  // where on the editing surface that affordance sits.
  const [pendingSelection, setPendingSelection] = useState<PendingSelection | null>(
    null,
  );
  /**
   * The passage the open draft composer belongs to. Held separately from
   * `pendingSelection` so the composer survives the browser selection being
   * collapsed — a click anywhere in the body does that, and it must not silently
   * discard what has been typed.
   */
  const [focusedThread, setFocusedThread] = useState<string | null>(null);
  // CMT-FR-17 / CMT-FR-30: per artifact and part of its retained edit state, so
  // it survives both the rail unmounting on a mode toggle and the tab closing
  // and reopening within the session.
  const showResolved = session.resolvedOpen;
  // CMT-FR-25: the picker is open because the rail asked for it. Cancelling
  // leaves the rail disabled with the reason still stated; confirming
  // re-resolves the identity, which is what enables commenting.
  const [pickerOpen, setPickerOpen] = useState(false);

  // CMT-FR-27 / CMT-FR-28 / CMT-FR-36 / CMT-FR-64: where each thread's passage
  // sits on the surface, and which card has the focus.
  const hosted = useFragmentHost(fragmentHost, setFocusedThread, rootRef);
  const anchorComments = useMemo(
    () => ({
      threads: [...comments.threads, ...hosted.threads],
      identityError: comments.identityError,
    }),
    [comments.threads, comments.identityError, hosted.threads],
  );
  const anchorGroup = useCommentAnchors({
    rootRef,
    editor,
    session,
    mode,
    railOpen: railOpen || hosted.active,
    docVersion,
    comments: anchorComments,
    draft,
    focusedThread,
    setFocusedThread,
    focusThreadId,
    onThreadFocused,
  });
  const { bodyScrollRef, setScrollTop, setAnchorTops, identityBlock } =
    anchorGroup;


  // DCR-FR-05 / DCR-FR-06: the proposal's changes, placed in the document as it
  // stands and pushed into the decoration plugin. A hook rather than a block
  // here, because placing a change is a lookup in the text and has nothing to
  // do with the rest of this surface.
  const hunkGroup = useHunkPlacement(
    editor,
    review,
    docVersion,
    mode,
  );

  /**
   * CMT-FR-28 on the source surface: the anchored passages, as ranges of the
   * text the textarea holds.
   *
   * Simpler than the rich body's, and for one reason: an anchor **is** a range
   * of the source (CMT-FR-06), so there is no projection to invert here — the
   * quote is looked up in the buffer by the same nearest-occurrence rule
   * re-anchoring uses, and that is the answer. A quote the buffer no longer
   * holds places nothing, and its thread is still read in the rail with its card
   * at the top; alignment is best-effort, the thread never is.
   */
  const sourceAnchors: LayerAnchor[] = useMemo(() => {
    if (mode !== "text" || !railOnSource || !railOpen) return [];
    const out: LayerAnchor[] = [];
    const place = (threadId: string, quote: string, hint: number, mark: boolean) => {
      if (quote === "") return;
      const at = findNearest(sourceText, quote, hint);
      if (at === -1) return;
      out.push({
        threadId,
        start: at,
        end: at + quote.length,
        // A resolved thread's passage is no longer under discussion, so it is
        // not marked — but it is still placed, because its card is aligned to it
        // while the disclosure holding it is open (CMT-FR-17).
        focused: mark && threadId === focusedThread,
        marked: mark,
      });
    };
    for (const entry of comments.threads) {
      if (entry.anchor === null) continue;
      place(
        entry.thread.id,
        entry.anchor.quote,
        entry.anchor.start,
        !entry.thread.resolved,
      );
    }
    if (draft) place(DRAFT_KEY, draft.quote, draft.start, false);
    // Overlapping anchors would nest on the layer, and the outer of the two
    // would swallow the inner's measurement. Ordering by start and dropping a
    // span that reaches back into its predecessor keeps them disjoint, the way
    // the tokens and the matches already are.
    out.sort((a, b) => a.start - b.start || a.end - b.end);
    const disjoint: LayerAnchor[] = [];
    for (const a of out) {
      const last = disjoint[disjoint.length - 1];
      if (last && a.start < last.end) continue;
      disjoint.push(a);
    }
    return disjoint;
  }, [mode, railOnSource, railOpen, sourceText, comments.threads, draft, focusedThread]);

  /**
   * CMT-FR-27 on the source surface: each card's alignment, measured off the
   * layer that marked the passages.
   *
   * The layer is the only place on this surface where a passage has a box at
   * all — the textarea renders one run of text with no elements inside it — so
   * it is what the offsets are read from. They are in the layer's own content
   * coordinates, which is the space the rail's cards are placed in once the
   * scroll offset is subtracted, exactly as for the rich body.
   */
  useLayoutEffect(() => {
    if (mode !== "text" || !railOnSource || !railOpen) return;
    const layer = sourceHlRef.current;
    if (!layer) return;
    const base = layer.getBoundingClientRect().top - layer.scrollTop;
    const tops: Record<string, number> = {};
    layer
      .querySelectorAll<HTMLElement>(`[${ANCHOR_ATTR}]`)
      .forEach((el) => {
        const id = el.getAttribute(ANCHOR_ATTR);
        if (!id || id in tops) return;
        tops[id] = Math.max(0, el.getBoundingClientRect().top - base);
      });
    setAnchorTops((prev) => {
      const same =
        Object.keys(prev).length === Object.keys(tops).length &&
        Object.keys(tops).every((k) => prev[k] === tops[k]);
      return same ? prev : tops;
    });
  }, [mode, railOnSource, railOpen, sourceAnchors, sourceText, sourceTokens]);

  /**
   * CMT-FR-05 on the source surface: a non-empty selection in the textarea
   * reveals the affordance that starts a thread over it.
   *
   * The selection is already a range of the source (CMT-FR-06), so unlike the
   * rich body's there is nothing to map: the offsets the textarea reports are
   * the anchor, and the quote is the buffer between them.
   */
  const captureSourceSelection = useCallback(() => {
    if (mode !== "text" || !railOnSource || blocked || identityBlock !== null) {
      return;
    }
    if (draft) return;
    const el = sourceRef.current;
    if (!el) return;
    const start = el.selectionStart;
    const end = el.selectionEnd;
    const quote = sourceText.slice(start, end);
    if (start === end || quote.trim() === "") {
      setPendingSelection(null);
      return;
    }
    setPendingSelection({
      start,
      end,
      quote,
      // Measured on the layer and placed in the page's box: the layer is what
      // has the characters laid out, and the page is the non-scrolling parent
      // the button is a child of.
      ...offsetPoint(sourceHlRef.current, end, sourceWrapRef.current),
    });
  }, [mode, railOnSource, blocked, identityBlock, draft, sourceText]);

  /**
   * CMT-FR-06: map the rich body's selection onto a range of the Markdown source.
   *
   * The selected text is located in the source near where the body's own offset
   * puts it — the same nearest-occurrence rule re-anchoring uses — because the
   * rendered body and the source differ by exactly the Markdown syntax the
   * WYSIWYG surface hides. `sourceRange` handles both the single-block case and
   * the cross-block one; a selection it cannot place at all opens no thread
   * rather than anchoring to the wrong passage.
   */
  const captureSelection = useCallback(() => {
    if (mode !== "wysiwyg" || blocked || identityBlock !== null) return;
    // An open composer owns the passage until it is submitted or cancelled.
    // Re-reading the selection here would collapse it out from under a draft the
    // author is still writing.
    if (draft) return;
    const sel = typeof window !== "undefined" ? window.getSelection() : null;
    const text = sel?.toString() ?? "";
    if (text.trim() === "") {
      setPendingSelection(null);
      return;
    }
    const source = currentDoc();
    // Nearest to where the *rendered* body puts the selection, not the first
    // occurrence: a spec that repeats a heading would otherwise anchor every
    // thread on the later ones to the first, and CMT-FR-06's promise that the
    // anchor means the same passage in either mode would fail immediately.
    const hint = bodyOffsetOfSelection(editor, sel);
    const range = sourceRange(source, text, hint);
    if (range === null) {
      setPendingSelection(null);
      return;
    }
    setPendingSelection({
      ...range,
      // The quote is read back out of the source, so it is the passage as the
      // file spells it — which is what re-anchoring searches for later.
      quote: source.slice(range.start, range.end),
      ...selectionPoint(sel, bodyScrollRef.current),
    });
  }, [mode, blocked, identityBlock, currentDoc, editor, draft]);

  /**
   * A Markdown file toggled to raw text takes both the affordance and the
   * composer off screen (CMT-FR-02), so neither may stay armed behind the source
   * surface — a draft the author can no longer see or cancel would leave the
   * Comment affordance suppressed on return. A source file never leaves the one
   * surface it has, so nothing here ever fires for it.
   */
  useEffect(() => {
    if (mode === "wysiwyg") {
      // Hiding the surface resets its scroll position, and no scroll event
      // fires on the way back — so the offset is re-read rather than trusted,
      // or the cards return shifted by however far the body used to be scrolled.
      setScrollTop(bodyScrollRef.current?.scrollTop ?? 0);
      return;
    }
    setDraft(null);
    setPendingSelection(null);
  }, [mode]);

  /**
   * CMT-FR-21: a write is when a drifted anchor is recorded, so the next load
   * starts its search from where the passage now is.
   *
   * Keyed on the artifact going clean rather than hung off a Save control,
   * because with EDT-FR-70 there is no longer an interaction to hang it off:
   * the write that lands may be the rest elapsing, a Save, or a close-time
   * flush, and every one of them ends the same way — dirty falling to false.
   */
  const wasDirty = useRef(false);
  useEffect(() => {
    if (wasDirty.current && !doc.dirty) void comments.persistDrift();
    wasDirty.current = doc.dirty;
  }, [doc.dirty, comments]);

  return (
    <EditorView
      findGroup={findGroup}
      railGroup={railGroup}
      anchorGroup={anchorGroup}
      surfaceGroup={surfaceGroup}
      hunkGroup={hunkGroup}
      artifactId={artifactId}
      artifactName={artifactName}
      artifactType={artifactType}
      session={session}
      sessions={sessions}
      doc={doc}
      mode={mode}
      isMarkdown={isMarkdown}
      blocked={blocked}
      fm={fm}
      fmExpanded={fmExpanded}
      atTop={atTop}
      setAtTop={setAtTop}
      setExpandOverride={setExpandOverride}
      bodyRef={bodyRef}
      origFmRef={origFmRef}
      applyFm={applyFm}
      applySourceText={applySourceText}
      noteEdit={noteEdit}
      sourceText={sourceText}
      sourceAnchors={sourceAnchors}
      imageHost={imageHost}
      report={report}
      review={review}
      showComments={showComments}
      showActions={showActions}
      showResolved={showResolved}
      pendingProposal={pendingProposal}
      openPromptReview={openPromptReview}
      rootRef={rootRef}
      draft={draft}
      setDraft={setDraft}
      focusedThread={focusedThread}
      setFocusedThread={setFocusedThread}
      pendingSelection={pendingSelection}
      setPendingSelection={setPendingSelection}
      pickerOpen={pickerOpen}
      setPickerOpen={setPickerOpen}
      toggleMode={toggleMode}
      captureSelection={captureSelection}
      captureSourceSelection={captureSourceSelection}
      focusThreadFromBody={focusThreadFromBody}
      onHostComment={fragmentHost?.onComment}
    />
  );

}
