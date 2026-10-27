/**
 * Everything the Editor tab draws: the band above the page, the page itself in
 * whichever of its two surfaces is showing, the discussion column beside it, and
 * the overlays that sit over the lot.
 *
 * A presentational component over the Editor's own state — it owns none of it.
 * The one state it holds is the request to focus the column's composer, and
 * the ref of the grid the splitter is measured against. Split out so `./index.tsx` reads as the state
 * the tab is, with the markup that renders it in one place beside it.
 *
 * The four groups arrive whole rather than field by field, and are unpacked
 * here under the names the markup already used: the hooks that produce them
 * (`useEditorFind`, `useEditorDiscussions`, `useCommentAnchors`, `useEditorSurface`)
 * are what decide those names, so a field added to one of them reaches the
 * markup without a prop being threaded through for it.
 */
import { useRef, useState, type Dispatch, type MutableRefObject, type RefObject, type SetStateAction } from "react";
import { EditorContent } from "@tiptap/react";
import { Icon } from "../icons";
import { FindPanel } from "../FindPanel";
import { gridTemplate } from "../DraftDiscussion";
import { EditorDiscussionColumn } from "./EditorDiscussionColumn";
import { GithubTokenPicker } from "../GithubTokenPicker";
import { ReviewOverlay } from "./ReviewOverlay";
import { PromptChangeReview } from "../PromptChangeReview";
import { ActionControl } from "../ActionControl";
import { EditSessionStore } from "../../state/editSessions";
import type { EditSession } from "../../state/editSessions/types";
import { sealBurst, type EditMode, type EditSurface } from "../../state/editHistory";
import type { UseArtifactDocument } from "../../hooks/useArtifactDocument";
import type { EditorImageHost } from "../hostImages";
import type { LiteralSearchMode } from "../../types";
import { indentUnit } from "../../state/indentation";
import { FRONTMATTER_TOP_THRESHOLD, joinFrontmatter } from "./frontmatter";
import { matchesInWindow, renderSegments } from "./highlight";
import {
  ExternalChangeModal,
  FormattingButtons,
  FrontmatterRegion,
} from "./regions";
import { ActionCluster } from "./toolbar";
import { insertDiagramFence, type DiagramTemplate } from "./diagramTemplates";
import type { DocumentOutline } from "./useDocumentOutline";
import { logDebug, logWarn } from "../../logging";
import type { CommentSelection, PendingSelection } from "./selection";
import type { LayerAnchor } from "./highlight";
import type { EditorProps } from "./props";
import type { useCommentAnchors } from "./useCommentAnchors";
import type { useEditorDiscussions } from "./useEditorDiscussions";
import type { useEditorFind } from "./useEditorFind";
import type { useEditorSurface } from "./useEditorSurface";
import type { useHunkPlacement } from "./useHunkPlacement";

export interface EditorViewProps {
  findGroup: ReturnType<typeof useEditorFind>;
  railGroup: ReturnType<typeof useEditorDiscussions>;
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
  /** EDT-FR-UERT: called from each scroller's scroll handler. */
  rememberScroll: () => void;
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
  /** EDT-FR-LGQA: the outline the table of contents control lists. */
  outline: DocumentOutline | undefined;
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
  // ACT-FR-BRZK / DDS-FR-JWNC: each Discuss advances it, which puts the caret
  // in the column's composer.
  const [discussFocus, setDiscussFocus] = useState(0);
  // DDS-FR-PNXR: the grid the splitter's drag is measured against.
  const splitRef = useRef<HTMLDivElement | null>(null);
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
  const { comments, control, dismissControl, split, showColumn } = props.railGroup;
  // CMT-FR-02 / ESH-FR-SSDV: a source file marks its fragments on its source.
  const railOnSource = !props.isMarkdown;
  const { bodyScrollRef, identityBlock, revealFragment } = props.anchorGroup;
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
    rememberScroll,
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
    outline,
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
   * EDT-FR-IPMX / EDT-FR-HAGB: the insert is one step of the history of its
   * own. The burst is sealed on both sides, so neither the typing before it
   * nor the typing after it joins that step.
   */
  const insertDiagram = (template: DiagramTemplate) => {
    if (!editor) {
      logWarn(["frontend"], "diagram fence not inserted: no editing surface", {
        chartType: template.key,
      });
      return;
    }
    sealBurst(session.history);
    const inserted = insertDiagramFence(editor, template.example);
    sealBurst(session.history);
    logDebug(["frontend"], "diagram fence inserted", {
      chartType: template.key,
      inserted,
    });
  };

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
          // CMT-FR-05: the composer lives in the column, so starting a
          // discussion has to make the column visible — otherwise activating
          // this button would appear to do nothing at all.
          showColumn();
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
        // EDT-FR-UERT: the tab keeps the offset for its next activation.
        rememberScroll();
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
        if (!hit) return;
        showColumn();
        setFocusedThread(hit.threadId);
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

  /**
   * EDT-FR-18 / EDT-FR-17 / DCR-FR-11: what the document column holds — the
   * frontmatter region, the page in whichever surface is showing, and the
   * review bar at its foot.
   */
  const documentColumn = (
    <>
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
      {/* Both pages live inside this one wrapper: a Markdown file's rich page,
          and a source file's own (ESH-FR-SSDV). The page that is not showing is
          `display: none` rather than merely empty — it is a `flex: 1` sibling of
          the other, so leaving it displayed would let it claim half the tab's
          height and push the visible one into the bottom of it. */}
      <div className="editor__document" data-testid="editor-document">
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
        onScroll={(e) => {
          const nowAtTop =
            e.currentTarget.scrollTop <= FRONTMATTER_TOP_THRESHOLD;
          setAtTop(nowAtTop);
          // Scroll is authoritative — dismiss a manual peek/minimize, EXCEPT a
          // scroll that merely stays within the top zone (so a minimize-at-top
          // is not reverted by scroll jitter). Arriving at the top from below
          // clears it too, so scrolling back up re-expands (scroll wins). `atTop`
          // here is the pre-scroll value.
          if (!nowAtTop || !atTop) setExpandOverride(null);
          // EDT-FR-UERT: the tab keeps the offset for its next activation.
          rememberScroll();
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
      </div>


      {/* DCR-FR-11: the review bar, at the **foot** of the document column
          below the scroller. It holds its place there while the document
          scrolls, so the counter and every control stay reachable — and it
          sits where the hand already is rather than standing between the
          formatting toolbar and the first line of the prompt. */}
      {review?.bar}
    </>
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
        {/* EDT-FR-16: the surface of the band takes the width that the action
            cluster leaves, and clips at its trailing end the controls that do
            not fit. So at a narrow width no control paints over the cluster
            or outside the band. */}
        <div className="editor__band-surface">
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
              onModeChange={(next: LiteralSearchMode) => patchFind({ mode: next })}
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
            mode === "wysiwyg" && (
              <FormattingButtons
                editor={editor}
                editable={!blocked}
                onInsertDiagram={insertDiagram}
              />
            )
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
        </div>
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
             source file the cluster holds the column's controls alone — that
             file has one editing surface, so there is nothing for a toggle to
             move between. */
          mode={isMarkdown ? mode : undefined}
          onToggleMode={isMarkdown ? toggleMode : undefined}
          // EDT-FR-LGQA: a Markdown file's, directly before the mode toggle.
          outline={outline}
          // CMT-FR-29 / DDS-FR-XQMF: the column's split and its hide control,
          // at the trailing end in every editing mode, so switching to raw
          // text never hides the way to the discussions.
          split={
            showComments
              ? {
                  ratio: split.view.ratio,
                  hidden: split.view.hidden,
                  onRatioChange: split.setRatio,
                  onToggle: split.toggleDiscussion,
                }
              : undefined
          }
        />
      </div>

      {showComments ? (
        /* CMT-FR-01 / DDS-FR-KTVW: the document column, the splitter, and the
           discussion column, as one grid below the toolbar. DDS-FR-XQMF: one
           column takes the whole tab while the other is hidden. */
        <div
          className="dds-split editor__split"
          ref={splitRef}
          style={{
            gridTemplateColumns: split.view.hidden
              ? "1fr"
              : gridTemplate(split.view.ratio),
          }}
          data-discussion={split.view.hidden ? "hidden" : "shown"}
          data-testid="editor-discussion-split"
        >
          <div className="editor__column">{documentColumn}</div>
          <EditorDiscussionColumn
            group={props.railGroup}
            splitRef={splitRef}
            artifactId={artifactId}
            artifactName={artifactName}
            artifactType={artifactType}
            identityBlock={identityBlock}
            onChooseToken={() => setPickerOpen(true)}
            blocked={blocked}
            draft={draft}
            setDraft={setDraft}
            focusedThread={focusedThread}
            setFocusedThread={setFocusedThread}
            onFocusFragment={(threadId) => {
              // CMT-FR-28: the raw-text surface marks no fragment, so the tab
              // goes to WYSIWYG first and the passage is scrolled to there.
              if (isMarkdown && mode !== "wysiwyg") toggleMode();
              revealFragment(threadId);
            }}
            showResolved={showResolved}
            onShowResolved={(shown) =>
              sessions.update(artifactId, { resolvedOpen: shown })
            }
            focusSignal={discussFocus}
          />
        </div>
      ) : (
        documentColumn
      )}

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
        /* ACT-FR-19: the discussions are read in the column, so no floating
           panel is rendered. */
        discussionsInRail={showComments}
        /* ACT-FR-BRZK / ACT-FR-23: with the column shown the conversation is
           already beside the file, so the action has nothing left to do and
           says so rather than reading as a control that does nothing. */
        discussUnavailable={
          showComments && !split.view.hidden
            ? `The discussion is already beside the ${
                artifactType === "spec" ? "specification" : "file"
              }`
            : undefined
        }
        /* ACT-FR-BRZK / DDS-FR-JWNC: Discuss opens no composer of its own. It
           shows the column by the route of the toolbar's own control, which
           persists the state (DDS-FR-XQMF), and puts the caret in it. */
        onDiscuss={
          showComments
            ? () => {
                showColumn();
                setDiscussFocus((n) => n + 1);
                logDebug(["frontend"], "Discuss showed the discussion column", {
                  artifactId,
                });
              }
            : undefined
        }
        /* CVP-FR-08 / CVP-FR-64: the same owner the column names, so a
           discussion created here and one read in the column read the same
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
          // it exists, which is the column.
          showColumn();
          setFocusedThread(threadId);
        }}
      />
      )}
    </div>
  );
}
