/**
 * Everything the Editor tab draws: the band above the page, the page itself in
 * whichever of its two surfaces is showing, the comment rail beside it, and the
 * overlays that sit over the lot.
 *
 * A presentational component over the Editor's own state — it owns none of it
 * and holds no hook of its own. Split out so `./index.tsx` reads as the state
 * the tab is, with the markup that renders it in one place beside it.
 *
 * The four groups arrive whole rather than field by field, and are unpacked
 * here under the names the markup already used: the hooks that produce them
 * (`useEditorFind`, `useCommentRail`, `useCommentAnchors`, `useEditorSurface`)
 * are what decide those names, so a field added to one of them reaches the
 * markup without a prop being threaded through for it.
 */
import type { Dispatch, MutableRefObject, RefObject, SetStateAction } from "react";
import { EditorContent } from "@tiptap/react";
import { Icon } from "../icons";
import { FindPanel } from "../FindPanel";
import { CommentRail } from "../CommentRail";
import { GithubTokenPicker } from "../GithubTokenPicker";
import { ReviewOverlay } from "./ReviewOverlay";
import { PromptChangeReview } from "../PromptChangeReview";
import { ActionControl } from "../ActionControl";
import { EditSessionStore } from "../../state/editSessions";
import type { EditSession } from "../../state/editSessions/types";
import type { EditMode, EditSurface } from "../../state/editHistory";
import type { UseArtifactDocument } from "../../hooks/useArtifactDocument";
import type { EditorImageHost } from "../hostImages";
import type { SearchMode } from "../../types";
import { indentUnit } from "../../state/indentation";
import { joinFrontmatter } from "./frontmatter";
import { matchesInWindow, renderSegments } from "./highlight";
import {
  ExternalChangeModal,
  FormattingButtons,
  FrontmatterRegion,
} from "./regions";
import { ActionCluster } from "./toolbar";
import type { CommentSelection, PendingSelection } from "./selection";
import type { LayerAnchor } from "./highlight";
import type { EditorProps } from "./props";
import type { useCommentAnchors } from "./useCommentAnchors";
import type { useCommentRail } from "./useCommentRail";
import type { useEditorFind } from "./useEditorFind";
import type { useEditorSurface } from "./useEditorSurface";
import type { useHunkPlacement } from "./useHunkPlacement";

/**
 * EDT-FR-20: body scrollTop at or under this (px) counts as "at the top" — the
 * frontmatter region shows fully expanded there and collapses once scrolled
 * past it.
 */
const FRONTMATTER_TOP_THRESHOLD = 8;

export interface EditorViewProps {
  findGroup: ReturnType<typeof useEditorFind>;
  railGroup: ReturnType<typeof useCommentRail>;
  anchorGroup: ReturnType<typeof useCommentAnchors>;
  surfaceGroup: ReturnType<typeof useEditorSurface>;
  hunkGroup: ReturnType<typeof useHunkPlacement>;

  artifactId: string;
  artifactName: string | undefined;
  artifactType: string | undefined;
  session: EditSession;
  sessions: EditSessionStore;
  doc: UseArtifactDocument;
  mode: EditMode;
  isMarkdown: boolean;
  blocked: boolean;
  fm: string | null;
  fmExpanded: boolean;
  atTop: boolean;
  setAtTop: Dispatch<SetStateAction<boolean>>;
  setExpandOverride: Dispatch<SetStateAction<boolean | null>>;
  bodyRef: MutableRefObject<string>;
  origFmRef: MutableRefObject<{ fm: string | null; raw: string }>;
  applyFm: (next: string | null) => void;
  applySourceText: (next: string) => void;
  noteEdit: (doc: string, mode: EditMode, surface: EditSurface) => void;
  sourceText: string;
  sourceAnchors: LayerAnchor[];
  imageHost: EditorImageHost | undefined;
  report: EditorProps["report"];
  review: EditorProps["review"];
  showComments: boolean;
  showActions: boolean;
  showResolved: boolean;
  pendingProposal: { id: string } | undefined;
  openPromptReview: (proposalId: string) => void;
  rootRef: RefObject<HTMLDivElement | null>;
  draft: CommentSelection | null;
  setDraft: Dispatch<SetStateAction<CommentSelection | null>>;
  focusedThread: string | null;
  setFocusedThread: Dispatch<SetStateAction<string | null>>;
  pendingSelection: PendingSelection | null;
  setPendingSelection: Dispatch<SetStateAction<PendingSelection | null>>;
  pickerOpen: boolean;
  setPickerOpen: Dispatch<SetStateAction<boolean>>;
  toggleMode: () => void;
  captureSelection: () => void;
  captureSourceSelection: () => void;
  focusThreadFromBody: (e: React.MouseEvent) => void;
  /** DDS-FR-QMBC: Comment on a selection, for a host that renders the discussion. */
  onHostComment?: (selection: { start: number; end: number; quote: string }) => void;
}

export function EditorView(props: EditorViewProps) {
  const {
    find,
    sourceTokens,
    sourceLayer,
    sourceRef,
    sourceHlRef,
    sourceWrapRef,
    fmInputRef,
    findFocus,
    engine,
    patchFind,
    replaceCurrent,
    replaceAll,
  } = props.findGroup;
  const {
    comments,
    control,
    dismissControl,
    unresolvedInTab,
    railOpen,
    railOnSource,
    railHere,
    toggleRail,
    discussionsInRail,
    railReply,
    railSetLock,
    railSetResolved,
    railPendingTurns,
    railTurnFailures,
    railErrors,
    railFailedTurns,
    railImageNotices,
    railRetryingTurnIds,
    railRetryTurn,
    railCancelTurn,
  } = props.railGroup;
  const {
    bodyScrollRef,
    arrangement,
    scrollTop,
    setScrollTop,
    anchorTops,
    identityBlock,
    scrollFragmentIntoView,
  } = props.anchorGroup;
  const { editor, selectedImage, removeSelectedImage, takeSourceImage } =
    props.surfaceGroup;
  const { placedHunks, focusedPlaced, focusHunkFromBody } = props.hunkGroup;
  const {
    artifactId,
    artifactName,
    artifactType,
    session,
    sessions,
    doc,
    mode,
    isMarkdown,
    blocked,
    fm,
    fmExpanded,
    atTop,
    setAtTop,
    setExpandOverride,
    bodyRef,
    origFmRef,
    applyFm,
    applySourceText,
    noteEdit,
    sourceText,
    sourceAnchors,
    imageHost,
    report,
    review,
    showComments,
    showActions,
    showResolved,
    pendingProposal,
    openPromptReview,
    rootRef,
    draft,
    setDraft,
    focusedThread,
    setFocusedThread,
    pendingSelection,
    setPendingSelection,
    pickerOpen,
    setPickerOpen,
    toggleMode,
    captureSelection,
    captureSourceSelection,
    focusThreadFromBody,
    onHostComment,
  } = props;

  /** The name the chrome of a modal or a picker calls this artifact by. */
  const title = artifactName ?? artifactId;

  /**
   * CMT-FR-05: a non-empty selection reveals the affordance that starts a thread
   * over it. A zero-length selection reveals nothing.
   *
   * One element for both surfaces (CMT-FR-02), placed by whichever of them made
   * the selection: its point is already in that surface's own coordinates, so
   * the button sits with the passage rather than at a fixed corner of the tab.
   */
  const affordance =
    (showComments || onHostComment) && pendingSelection ? (
      <button
        className="editor__comment-affordance"
        aria-label="Comment on selection"
        title="Comment on selection"
        style={{ top: pendingSelection.top, left: pendingSelection.left }}
        onMouseDown={(e) => e.preventDefault()}
        onClick={() => {
          const { start, end, quote } = pendingSelection;
          if (onHostComment) {
            onHostComment({ start, end, quote });
            setPendingSelection(null);
            return;
          }
          setDraft({ start, end, quote });
          setPendingSelection(null);
          // The composer lives in the rail, so starting a thread has to make the
          // rail visible — otherwise activating this button would appear to do
          // nothing at all.
          if (!railOpen) sessions.update(artifactId, { railOpen: true });
        }}
      >
        <Icon.Comment size={14} />
      </button>
    ) : null;

  /**
   * EDT-FR-17 / ESH-FR-SSDV: the source surface itself — the whole file as one
   * editable document, and the sole target of every event on this surface
   * (ESH-FR-VNPW). What is drawn behind it decorates; this is what is edited.
   */
  const sourceTextarea = (
    <textarea
      ref={sourceRef}
      className="editor__source"
      /* ESH-FR-SSDV: what the surface is called follows the shape of the file on
         it — a Markdown file's source is Markdown, and a source file's is
         whatever language it is written in. */
      aria-label={isMarkdown ? "Markdown source" : "Source"}
      value={sourceText}
      spellCheck={false}
      disabled={blocked}
      // Keep the highlight layer registered with the text it sits behind.
      onScroll={(e) => {
        const hl = sourceHlRef.current;
        // CMT-FR-27: the cards are placed in the surface's own coordinates, so
        // the rail is told how far it has scrolled — that is what keeps each
        // card level with its passage.
        if (railOnSource) setScrollTop(e.currentTarget.scrollTop);
        if (!hl) return;
        hl.scrollTop = e.currentTarget.scrollTop;
        hl.scrollLeft = e.currentTarget.scrollLeft;
      }}
      onMouseUp={captureSourceSelection}
      onKeyUp={captureSourceSelection}
      onSelect={captureSourceSelection}
      // CMT-FR-28: the layer behind this surface takes no pointer event, so a
      // click on a marked passage arrives here. Which passage it was is read
      // from the caret rather than from the target, there being no element under
      // the pointer to name a thread.
      onClick={(e) => {
        if (!railOnSource) return;
        const at = e.currentTarget.selectionStart;
        const hit = sourceAnchors.find(
          (a) => a.marked && at >= a.start && at <= a.end,
        );
        if (hit) setFocusedThread(hit.threadId);
      }}
      // EDT-FR-38: Tab inserts the artifact's indentation convention
      // (STB-FR-21/STB-FR-23) rather than moving focus out of the source
      // surface. The insertion is an ordinary user edit — one step in the
      // artifact's single history (EDT-FR-22), so one undo reverses exactly
      // the inserted whitespace. Choosing the convention is not: it rewrites
      // no line and occupies no position in the history (EDT-FR-23).
      onKeyDown={(e) => {
        if (e.key !== "Tab" || e.metaKey || e.ctrlKey || e.altKey) return;
        // Shift+Tab is left to the browser: it is a backwards focus move, and
        // de-indentation is not something EDT-FR-38 asks for.
        if (e.shiftKey) return;
        e.preventDefault();
        const el = e.currentTarget;
        const unit = indentUnit(session.indentation);
        const { selectionStart, selectionEnd, value } = el;
        const next =
          value.slice(0, selectionStart) + unit + value.slice(selectionEnd);
        applySourceText(next);
        noteEdit(next, "text", "source");
        // React re-renders from `sourceText`, which resets the caret to the
        // end; put it back just after the inserted unit so typing continues
        // where the user was.
        const caret = selectionStart + unit.length;
        requestAnimationFrame(() => el.setSelectionRange(caret, caret));
      }}
      // EDT-FR-86 / NAW-FR-50: both surfaces accept a pasted or dropped image,
      // the rich one and the raw one, on identical terms — the host is offered
      // it first, and where the host takes it the reference is written at the
      // caret, replacing the selection where there is one.
      onPaste={(e) => {
        if (takeSourceImage(e.clipboardData, e.currentTarget)) e.preventDefault();
      }}
      onDrop={(e) => {
        if (takeSourceImage(e.dataTransfer, e.currentTarget)) e.preventDefault();
      }}
      onChange={(e) => {
        applySourceText(e.target.value);
        // EDT-FR-22: source edits share the artifact's single history.
        noteEdit(e.target.value, "text", "source");
      }}
    />
  );

  return (
    <div
      ref={rootRef}
      /* EDT-FR-63: the tab is one band above one page, both set on the field.
         The field is declared here, on the one element that is present in both
         editing modes, so the page's edge has the same thing behind it whether
         the WYSIWYG surface or the raw-text surface is the one showing. */
      className="editor-tab"
      /* EDT-FR-63 / EDT-FR-18: the frontmatter region is part of the editing
         surface, so it takes the head of the page rather than floating on the
         field above it. The sheet gives up its own top inset and top corners
         to it, which is what this says. */
      data-frontmatter={mode === "wysiwyg" && fm !== null ? "on" : "off"}
      /* EDT-FR-61 / CMT-FR-64: whether the margin is showing, and where its
         cards are. Declared here rather than on the wrapper below because the
         page slides as one with the frontmatter region above it — the region is
         a sibling of that wrapper, so this is the nearest element from which
         both can be reached. */
      data-rail={railHere && railOpen ? "open" : "closed"}
      data-comments={arrangement}
      style={{
        display: "flex",
        flexDirection: "column",
        height: "100%",
        position: "relative",
      }}
    >
      {/* EFR-FR-ABHF: the band holds exactly one of three surfaces — the
          formatting toolbar, the Find panel, or the Find & Replace panel. The
          formatting toolbar occupies it whenever neither panel is open and
          returns the moment one closes. The band is part of the tab's own
          chrome rather than a floating overlay, so an open panel neither
          closes nor is closed by the main window's overlays (STB-FR-12). */}
      <div className="editor__toolbar" data-find={find.form ?? "off"}>
        {find.form !== null ? (
          <FindPanel
            form={find.form}
            query={find.query}
            replacement={find.replacement}
            mode={find.mode}
            ordinal={engine.current + 1}
            total={engine.matches.length}
            valid={engine.valid}
            focus={findFocus}
            onQueryChange={(query) => patchFind({ query })}
            onReplacementChange={(replacement) => patchFind({ replacement })}
            onModeChange={(next: SearchMode) => patchFind({ mode: next })}
            onStep={engine.step}
            onReplace={replaceCurrent}
            onReplaceAll={replaceAll}
            onClose={() => patchFind({ form: null })}
          />
        ) : (
          /* EFR-FR-ACMM: the formatting affordances are the rich surface's. Over a
             source file the band holds the find panels alone and is otherwise
             empty — those controls format a rich Markdown document, and a source
             file has none to format. */
          mode === "wysiwyg" && <FormattingButtons editor={editor} />
        )}
        {/* NAW-FR-53 / NAW-FR-54: the removal action an image the surface has
            selected carries. Reachable from the keyboard on the selected image
            and named in words rather than by an icon alone, and offered only
            where the host takes images at all — an Editor tab on a project file
            has none to remove (EDT-FR-86). */}
        {find.form === null && imageHost && selectedImage && (
          <button
            type="button"
            className="editor__image-remove"
            onClick={removeSelectedImage}
          >
            Remove image
          </button>
        )}
        <div className="spacer" />
        <ActionCluster
          dirty={doc.dirty}
          report={report}
          /* PCR-FR-16 / EDT-FR-84: an artifact carrying a proposal in state
             `pending` renders an indication in this cluster saying so, whichever
             editing mode the tab is in. */
          proposal={
            pendingProposal
              ? { onOpen: () => openPromptReview(pendingProposal.id) }
              : undefined
          }
          /* EDT-FR-16 / ESH-FR-LKNM: the toggle is a Markdown file's. Over a
             source file the cluster holds the comments control alone — that
             file has one editing surface, so there is nothing for a toggle to
             move between. */
          mode={isMarkdown ? mode : undefined}
          onToggleMode={isMarkdown ? toggleMode : undefined}
          // CMT-FR-29: present in both editing modes, so switching to raw text
          // never hides the fact that threads exist.
          comments={
            showComments
              ? {
                  // CMT-FR-56: an unresolved discussion is counted here like any
                  // other thread — a conversation waiting on the author is
                  // waiting on them whether it names a passage or not.
                  count: unresolvedInTab,
                  open: railOpen,
                  onToggle: toggleRail,
                }
              : undefined
          }
        />
      </div>

      {/* EDT-FR-18: in WYSIWYG the frontmatter is carved into a visually distinct,
          editable region above the rich body (never as body content). In text
          mode it is edited inline in the source surface, so no region here.
          EDT-FR-20: it collapses to a single line as the body scrolls down. */}
      {mode === "wysiwyg" && fm !== null && (
        <FrontmatterRegion
          value={fm}
          editable={!blocked}
          expanded={fmExpanded}
          inputRef={fmInputRef}
          // EFR-FR-DSKI: the panel's matches inside the region are marked on the
          // same highlight layer that bolds the YAML keys.
          matches={engine.matchesIn("frontmatter")}
          currentMatch={
            engine.currentMatch?.surface === "frontmatter"
              ? { start: engine.currentMatch.start, end: engine.currentMatch.end }
              : null
          }
          onExpand={() => setExpandOverride(true)}
          onMinimize={() => setExpandOverride(false)}
          onChange={(v) => {
            applyFm(v);
            // EDT-FR-22: frontmatter edits share the artifact's single history.
            noteEdit(
              joinFrontmatter(v, bodyRef.current, origFmRef.current),
              "wysiwyg",
              "frontmatter",
            );
          }}
        />
      )}

      {/* EDT-FR-17: WYSIWYG vs raw Markdown source. The Tiptap surface stays
          mounted (kept in sync) and is hidden in text mode so its instance and
          history survive a round-trip; the source surface holds the whole file.
          EDT-FR-20: this is the scroll container that drives the collapse. */}
      {/* CMT-FR-01: the cards occupy the empty margin this page already leaves
          beside itself rather than a column taken from it, so the artifact keeps
          its width and its position whether or not they are showing. They are a
          region of the tab, not a floating overlay of the main window, and so
          take no part in their mutual exclusion (STB-FR-12).

          Both pages live inside this one wrapper, because the rail is beside
          whichever of them is showing (CMT-FR-02): a Markdown file's rich page,
          and a source file's own (ESH-FR-SSDV). The page that is not showing is
          `display: none` rather than merely empty — it is a `flex: 1` sibling of
          the other, so leaving it displayed would let it claim half the tab's
          height and push the visible one into the bottom of it. */}
      <div
        className="editor__with-rail"
        data-rail={railHere && railOpen ? "open" : "closed"}
        data-testid="editor-with-rail"
      >
      {/* ESH-FR-SSDV: a source file has no rich page at all — not a hidden one.
          Rendering one would put a rich editing surface in the tab of a file
          that is never read on it. */}
      {isMarkdown && (
      <div
        className="editor"
        ref={bodyScrollRef}
        style={{
          flex: 1,
          overflow: "auto",
          display: mode === "wysiwyg" ? undefined : "none",
        }}
        onMouseUp={captureSelection}
        onKeyUp={captureSelection}
        onClick={(event) => {
          focusThreadFromBody(event);
          focusHunkFromBody(event);
        }}
        /* DCR-FR-30: the caret entered the prose, so the accelerators that
           accept and reject stand down — the editing surface binds both keys,
           and a change decided by typing a new paragraph is the one failure the
           guard exists to prevent. */
        onFocusCapture={() => review?.onCaretInProse()}
        onScroll={(e) => {
          // CMT-FR-27: the cards are placed in the body's own coordinates, so
          // the layer holding them is offset by however far the body has
          // scrolled — that is what keeps each card level with its passage.
          setScrollTop(e.currentTarget.scrollTop);
          const nowAtTop =
            e.currentTarget.scrollTop <= FRONTMATTER_TOP_THRESHOLD;
          setAtTop(nowAtTop);
          // Scroll is authoritative — dismiss a manual peek/minimize, EXCEPT a
          // scroll that merely stays within the top zone (so a minimize-at-top
          // is not reverted by scroll jitter). Arriving at the top from below
          // clears it too, so scrolling back up re-expands (scroll wins). `atTop`
          // here is the pre-scroll value.
          if (!nowAtTop || !atTop) setExpandOverride(null);
        }}
      >
        <EditorContent editor={editor} aria-label="artifact body" />
        {mode === "wysiwyg" && affordance}
        {/* DCR-FR-12 / DCR-FR-BQNL: the change under review carries its three
            actions, and every change of the proposal carries a mark on the
            scroller's outer edge. Both are inside the scroller so they travel
            with the text they are about. */}
        <ReviewOverlay
          review={review}
          placed={placedHunks}
          focused={focusedPlaced}
          hostRef={bodyScrollRef}
          extent={editor?.state.doc.content.size ?? 0}
        />
      </div>
      )}
      {mode === "text" && (
        <div
          className="editor__source-wrap"
          ref={sourceWrapRef}
          /* ESH-FR-YTNN: while the layer is showing, the textarea's own glyphs
             give way to it and only the caret and the selection remain visible
             — so this says whether there is a layer, not which of the reasons
             put one there. */
          data-layer={sourceLayer ? "on" : "off"}
        >
          {/* EFR-FR-DOQR / ESH-FR-YTNN / CMT-FR-28: a textarea cannot carry inline
              marks, so the find matches, the syntax tokens, and the anchored
              passages are drawn on an aria-hidden layer behind it whose text is
              transparent while it is showing — the same arrangement the
              frontmatter region uses for its YAML weighting. Presentation-only:
              the layer reproduces the buffer character for character, takes no
              event of any kind, and the textarea remains the sole editing
              surface. ESH-FR-LXQR: it is out of the accessibility tree, so what a
              screen reader reads and what a copy carries out is the file's own
              plain text. */}
          {sourceLayer && (
            <pre
              ref={sourceHlRef}
              className="editor__source-hl"
              aria-hidden="true"
              data-testid="source-highlight"
            >
              {/* EDT-FR-21 is WYSIWYG-only: this surface carries the find
                  matches, the syntax tokens, and the comment anchors, with no
                  YAML role treatment. */}
              {renderSegments(
                sourceText,
                [],
                matchesInWindow(
                  engine.matchesIn("source"),
                  engine.currentMatch?.surface === "source"
                    ? { start: engine.currentMatch.start, end: engine.currentMatch.end }
                    : null,
                  0,
                  sourceText.length,
                ),
                { tokens: sourceTokens ?? [], anchors: sourceAnchors },
              )}
              {/* The two boxes have to be the same height, and a `<textarea>`
                  and a `<pre>` disagree about the last line: a textarea reserves
                  a line box for the file's trailing newline and a pre generates
                  none after a final forced break. Without this the layer is one
                  line shorter, its scroll clamps that much sooner, and at the
                  foot of the file every glyph on it sits a line above the
                  character it is decorating — so a click lands on the line below
                  the one the author pointed at.

                  Unconditional, and correct either way for the same reason it is
                  needed at all: because a pre's *final* newline produces no line
                  box, appending one to a newline-terminated buffer yields
                  exactly one blank line — matching the textarea — and appending
                  one to a buffer that ends without a newline yields none, which
                  also matches. It is added after every decoration, so it shifts
                  no offset. */}
              {"\n"}
            </pre>
          )}
          {railOnSource && affordance}
          {sourceTextarea}
        </div>
      )}
      {railHere && railOpen && (
        <CommentRail
          threads={comments.threads}
          anchorTops={anchorTops}
          scrollTop={scrollTop}
          arrangement={arrangement}
          /* CMT-FR-RPLC: what the unavailable state names. */
          ownerLabel={artifactName ?? artifactId}
          ownerUnavailable={doc.error !== null && session.buffer === ""}
          /* CMT-FR-53 / CMT-FR-61: the Discussion section, pinned at the rail's
             head above the aligned cards. An Editor tab renders one wherever the
             rail renders at all — the artifact's conversations and the remarks
             pinned inside it are read in the one place. */
          discussions={showActions ? control.discussions.discussions : []}
          identity={comments.identity}
          identityBlock={
            identityBlock
              ? {
                  message: identityBlock.message,
                  route: identityBlock.route,
                  // CMT-FR-25: the one refusal the author can resolve from here.
                  action: identityBlock.opensPicker
                    ? { label: "Choose a token…", onActivate: () => setPickerOpen(true) }
                    : undefined,
                }
              : null
          }
          blocked={blocked}
          focusedThreadId={focusedThread}
          onFocusThread={setFocusedThread}
          // CTA-FR-VQFJ / CTA-FR-ZOLW / CTA-FR-LCVQ: the agents a composer can
          // address, what is still coming, and why a turn stopped coming. All
          // three are the hook's — the rail renders them and dispatches nothing
          // (AGT-FR-32).
          agents={comments.agents}
          pendingTurns={railPendingTurns}
          turnFailures={railTurnFailures}
          failedTurns={railFailedTurns}
          imageNotices={railImageNotices}
          retryingTurnIds={railRetryingTurnIds}
          onRetryTurn={railRetryTurn}
          onCancelTurn={railCancelTurn}
          draft={
            draft
              ? {
                  target: { kind: "artifact", artifactId },
                  fragmentTarget: {
                    owner: { kind: "artifact", artifactId },
                    path: artifactId,
                    start: draft.start,
                    end: draft.end,
                    quote: draft.quote,
                  },
                }
              : null
          }
          onOpenDraft={(request) =>
            comments.openThread(
              // The draft moves with the buffer, so the request reads it as it
              // stands now rather than as it stood when the card opened.
              request.fragmentTarget ?? {
                owner: request.target,
                path: artifactId,
                start: draft?.start ?? 0,
                end: draft?.end ?? 0,
                quote: draft?.quote ?? "",
              },
              request.body,
              request.attachments,
            )
          }
          onCancelDraft={() => setDraft(null)}
          onFocusFragment={(_fragment, discussion) => {
            setFocusedThread(discussion.id);
            scrollFragmentIntoView(discussion.id);
          }}
          onReply={railReply}
          onSetLock={railSetLock}
          onSetResolved={railSetResolved}
          errors={railErrors}
          showResolved={showResolved}
          onToggleResolved={() =>
            sessions.update(artifactId, { resolvedOpen: !showResolved })
          }
        />
      )}
      </div>

      {/* DCR-FR-11: the review bar, at the **foot** of the document column
          below the scroller. It holds its place there while the document
          scrolls, so the counter and every control stay reachable — and it
          sits where the hand already is rather than standing between the
          formatting toolbar and the first line of the prompt. */}
      {review?.bar}

      {/* EDT-FR-71 / EDT-FR-32, EDT-FR-34: a write that failed is attached to the tab, and
          it arrives without the author having asked for anything — nobody is
          watching for it. `role="alert"` is what makes it announced rather than
          merely rendered, on the same footing as the Flow tab's own. */}
      {doc.error && (
        <div
          role="alert"
          style={{ padding: "6px 20px", fontSize: "var(--fs-ui-sm)", color: "var(--fg-3)" }}
        >
          {doc.error}
        </div>
      )}
      {/* CMT-FR-25 / GHA-FR-16: the picker resolves a selection-required
          identity. On confirm the identity is re-resolved, which is what turns
          the composer back on; on cancel nothing is bound and the rail stays
          disabled with its reason (GHA-FR-17). */}
      {pickerOpen && (
        <GithubTokenPicker
          projectName={title}
          onCancel={() => setPickerOpen(false)}
          onConfirm={() => {
            setPickerOpen(false);
            void comments.retryIdentity();
          }}
        />
      )}
      {doc.conflict && (
        <ExternalChangeModal
          name={title}
          onLoad={() => void doc.loadFromFilesystem()}
          onKeep={doc.keepMine}
        />
      )}
      {/* EDT-FR-84 / PCR-FR-01: the tab lends its own bounds to the review
          modal. It is anchored here rather than in the window, so opening it
          closes no floating overlay and no overlay closes it — and every
          operation behind it belongs to `PCR-prompt-change-review.md` and
          `PCP-prompt-change-proposals.md` rather than to this spec. */}
      <PromptChangeReview artifactId={artifactId} sessions={sessions} />
      {/* EDT-FR-64 / EDT-FR-65: the one control every tab carries, standing apart
          from the action cluster in the chrome — the cluster carries what the
          Editor does to the file, and this carries what is done with the file as
          a whole, which is the same wherever that file is open. */}
      {showActions && (
      <ActionControl
        discussions={control.discussions}
        composer={control.composer}
        onComposerChange={control.setComposer}
        attachments={control.attachments}
        /* ACT-FR-04: Discuss on everything the Editor opens, and Implement in
           addition on an artifact whose type is `spec`. */
        artifactType={artifactType === "spec" ? "spec" : undefined}
        discussionsInRail={discussionsInRail}
        /* CVP-FR-08 / CVP-FR-64: the same owner the rail's own cards name, so a
           discussion created here and one detached from the rail read the same
           label wherever either is presented. */
        ownerLabel={artifactName ?? artifactId}
        ownerSurface="editor"
        /* ACT-FR-07: the file has gone from the project, so there is nothing
           left to discuss or to hand over — every entry renders disabled rather
           than posting into a refusal. */
        missing={doc.error !== null}
        blocked={blocked}
        dismissSignal={dismissControl}
        onDiscussionOpened={(threadId: string) => {
          // ACT-FR-16: the conversation is where it will be read from the moment
          // it exists — which, in WYSIWYG mode, means showing the margin.
          if (mode !== "wysiwyg") return;
          if (!railOpen) sessions.update(artifactId, { railOpen: true });
          setFocusedThread(threadId);
        }}
      />
      )}
    </div>
  );
}
