/**
 * The document column of the New Artifact tab (`NAW-new-artifact.md`
 * NAW-FR-09, NAW-FR-11, NAW-FR-27): the field the page is set on, the reading
 * of a past version, and the Editor over the live prompt. Split out of
 * `index.tsx` to keep that file within the size rule.
 */
import { useRef, type ComponentProps } from "react";

import type { DraftHistoryEntry } from "../../types";
import type { DraftSessionStore } from "../../state/draftSessions";
import type { EditMode } from "../../state/editHistory";
import { useCommentArrangement } from "../../hooks/useCommentArrangement";
import { Icon } from "../icons";
import { Editor } from "../Editor";
import { DraftVersionReading } from "../DraftVersionReading";
import { sourceLabelOf, standingOf } from "./labels";

type EditorProps = ComponentProps<typeof Editor>;

export function DraftDocumentColumn({
  drafts,
  docKey,
  selected,
  overlayOpen,
  imageError,
  imageStatus,
  imageHost,
  announceImage,
  viewing,
  viewText,
  viewError,
  entries,
  readingMode,
  onReadingMode,
  onReturnToLive,
  onRead,
  writeReport,
  dirty,
  readOnly,
  onEdit,
  fragmentHost,
  review,
}: {
  drafts: DraftSessionStore;
  /** The edit-session key of the live prompt, or null before the record lands. */
  docKey: string | null;
  selected: string | null;
  overlayOpen: boolean;
  imageError: string | null;
  imageStatus: string;
  imageHost: EditorProps["imageHost"];
  announceImage: (message: string) => void;
  viewing: DraftHistoryEntry | null;
  viewText: string | null;
  viewError: string | null;
  entries: DraftHistoryEntry[];
  readingMode: EditMode;
  onReadingMode: (mode: EditMode) => void;
  onReturnToLive: () => void;
  onRead: (entry: DraftHistoryEntry) => void;
  writeReport: string;
  dirty: boolean;
  readOnly: boolean;
  onEdit: () => void;
  fragmentHost: EditorProps["fragmentHost"];
  review: EditorProps["review"];
}) {
  /**
   * CMT-FR-64: the field the page and the margin share, measured so the cards
   * know which arrangement they are in. The History rail is inside the tab and
   * outside this element, so showing it narrows what is measured here — which is
   * right, because it narrows what the cards actually have.
   */
  const draftEditorRef = useRef<HTMLDivElement | null>(null);
  const arrangement = useCommentArrangement(draftEditorRef);

  /**
   * NAW-FR-14 / DDS-FR-KPSZ: the threads anchored to a passage of the prompt.
   *
   * They render below the page on CMT-FR-64's arrangement rather than in a
   * margin beside it. This tab does not serve them yet — `showComments` is off
   * on the embedded Editor for the reason NAW-FR-14 records — so the list is
   * empty and the field opens no margin.
   */
  const anchoredThreads: readonly unknown[] = [];

  return (
    /* NAW-FR-27: the field the page is set on, and the corner of it the
       action control floats in. The collapsed control has a strip of that
       field held open for it below the page, so the chrome the tab always
       carries covers nothing; what the control opens floats over the page
       rather than pushing it.

       `data-overlay` gives the page more to SCROLL while one is open —
       enough that its last line comes out from under whichever surface is
       covering it. It changes the scroll range and not the sheet, so the
       page's box stays exactly where it is. */
    <div
      className="draft-editor"
      ref={draftEditorRef}
      data-overlay={overlayOpen ? "on" : "off"}
      /* NAW-FR-14 / CMT-FR-64: the field's margin state and its
         arrangement, on the element that owns the field.

         The draft's **discussions** are no longer here — they are the
         column beside this one (DDS-FR-KTVW), and they lend the page's
         field nothing. What can still open a margin is a thread anchored to
         a passage of the prompt, which this tab does not yet serve; until it
         does, the field keeps its whole measure for the writing. */
      data-rail={anchoredThreads.length > 0 ? "open" : "closed"}
      data-comments={arrangement}
    >
      {/* NAW-FR-51 / NAW-FR-54: the typed refusal of an insertion, rendered
          inline where the author is looking, and the live region that
          announces every image outcome — an insertion, a refusal, and a
          removal alike.

          **Once**, here in the band around the page rather than inside the
          surface: they stand whichever of the two renderings is showing and
          whether or not a version is being read, so a second copy in the
          reading's own strip would put two `alert`s on the page for one
          refusal and read it out twice. The refusal is words rather than a
          tone, so it is legible without colour. */}
      {imageError !== null && (
        <p className="draft-workspace__image-error" role="alert">
          {imageError}
        </p>
      )}
      {/* A polite live region rather than a `status` role: the action row
          already carries one for what the control has to say, and a second
          would make "the tab's status" ambiguous to a reader traversing it.
          `aria-live` announces the change on its own. */}
      <span
        className="sr-only"
        aria-live="polite"
        data-testid="draft-image-status"
      >
        {imageStatus}
      </span>
      {/* NAW-FR-09 / NAW-FR-39: while a version is showing, a strip above
          the page states that the reading is read-only and carries the
          return to the live prompt. Read-only is conveyed in words and in
          accessible semantics rather than by colour or fill alone. */}
      {viewing !== null && (
        <div className="draft-reading" role="status">
          {/* NAW-FR-10: the way back leads the strip, ahead of what the
              strip says — it is the one control here, and a control the
              author reaches for at the end of every reading belongs where
              the eye starts rather than at the far edge of the tab. */}
          <button className="btn btn--sm" onClick={onReturnToLive}>
            ← Back to live prompt
          </button>
          <span className="t-ui-xs draft-reading__state">
            Reading a past version · read-only
          </span>
          <div className="spacer" />
          {/* NAW-FR-13: the live prompt's own write report, which an
              autosave can land while a version is being read. In the strip's
              flow, ahead of the toggle. */}
          {writeReport !== "" && (
            <span className="editor__report t-ui-xs" data-dirty={dirty}>
              {writeReport}
            </span>
          )}
          {/* NAW-FR-09 / EDT-FR-17: a version is read in the same two modes
              the prompt is edited in, behind the same toggle in the same
              position. It changes only the reading — the live prompt's own
              mode is exactly as it was left (NAW-FR-10). */}
          {viewError === null && (
            <button
              className="btn btn--ghost btn--icon"
              aria-label={
                readingMode === "wysiwyg"
                  ? "View as Markdown source"
                  : "View as rich text"
              }
              title={
                readingMode === "wysiwyg"
                  ? "View as Markdown source"
                  : "View as rich text"
              }
              data-active={readingMode === "text"}
              onClick={() =>
                onReadingMode(readingMode === "wysiwyg" ? "text" : "wysiwyg")
              }
            >
              {readingMode === "wysiwyg" ? (
                <Icon.Code size={15} />
              ) : (
                <Icon.Doc size={15} />
              )}
            </button>
          )}
        </div>
      )}

      {/* ACT-FR-09: while the review bar stands, the column's foot ends in
          the bar rather than in a strip of field held open for the action
          control. The control keeps its corner and floats over the bar. */}
      <div
        className="draft-editor__surface"
        data-foot={review !== undefined ? "bar" : "field"}
      >
        {viewing !== null ? (
          viewError !== null ? (
            /* NAW-FR-41: an inline statement in place of the reading,
               naming what went wrong and offering a retry and the return.
               The live prompt's buffer and dirty state are untouched. */
            <div className="draft-editor__empty" role="alert">
              <p className="t-ui-sm">⚠ This version could not be read.</p>
              <p className="t-ui-xs">{viewError}</p>
              <div className="draft-editor__empty-actions">
                <button
                  className="btn btn--sm"
                  onClick={() => onRead(viewing)}
                >
                  Try again
                </button>
                <button className="btn btn--sm" onClick={onReturnToLive}>
                  Live prompt
                </button>
              </div>
            </div>
          ) : (
            /* NAW-FR-09: that version's prompt, read-only, **on the same
               page in the same measure** and in the same two modes — the
               sheet, the inset, the frontmatter region and the typographic
               roles the Editor sets the live prompt on (`EDT-editor.md`
               EDT-FR-17, EDT-FR-63), so the only thing that changes between
               reading a version and editing the prompt is whether the
               surface takes a keystroke. */
            <DraftVersionReading
              text={viewText ?? ""}
              mode={readingMode}
              label={`Read-only past version of ${viewing.path} — ${sourceLabelOf(viewing)}, ${standingOf(viewing, entries)}`}
            />
          )
        ) : docKey !== null ? (
          /* NAW-FR-11: the Editor's own two-mode Markdown surface — the same
             WYSIWYG default, the same raw-Markdown toggle, the same
             frontmatter region, the same formatting toolbar and the same
             Find panels an artifact is edited with. Keyed on the document so
             a rename gives a fresh Tiptap instance; that costs nothing,
             because the buffer, dirty flag, mode and history live in the
             shell's store (NAW-FR-12). */
          <Editor
            key={docKey}
            artifactId={docKey}
            artifactName={selected ?? undefined}
            sessions={drafts.docs}
            // NAW-FR-14: a draft's threads are served in the draft scope of
            // the thread operations, which this surface does not yet
            // distinguish — so the rail stays off rather than reading and
            // writing artifact-scoped threads against a draft's key.
            showComments={false}
            // DDS-FR-QMBC: the prompt marks its fragment discussions and
            // offers Comment on a selection; the discussions themselves
            // render in the column.
            fragmentHost={fragmentHost}
            // NAW-FR-27 / ACT-FR-01: the tab carries exactly one action
            // control, and it governs the DRAFT — so the embedded Editor
            // mounts none of its own.
            showActions={false}
            // NAW-FR-13: the prompt writes itself, so the action row carries
            // a report of whether the pending edit has landed in place of
            // the unsaved badge an artifact's Save-owning row carries.
            report={writeReport}
            // NAW-FR-44: read-only while a run holds the prompt, and for
            // good once the draft is `graduated`. The surface takes no
            // edits at all rather than accepting them and refusing the
            // write, which would lose whatever was typed.
            readOnly={readOnly}
            onEdit={onEdit}
            /* NAW-FR-50 / NAW-FR-52: what this tab lends the surface about
               images — the store a pasted or dropped picture goes through,
               and the resolution that draws one the prompt references. An
               Editor tab on a project file lends neither, which is what
               keeps a paste there unchanged (EDT-FR-86). */
            imageHost={imageHost}
            /* NAW-FR-54: an insertion, a refusal, and a removal are each
               announced through the tab's existing accessibility feedback,
               in the same way its other outcomes are. */
            announce={announceImage}
            /* DCR-FR-01 / DDS-FR-ZRPT: the proposal is reviewed IN this
               document — the hunks in the prose they change, the chip on
               the one under review, and the gutter map on the scroller's
               edge. There is no modal and no second surface. */
            review={review}
            /* NAW-FR-TFFX: the table of contents control of the live
               prompt, directly before its mode toggle. A past version reads
               through `DraftVersionReading` above and has none. */
            toc
          />
        ) : (
          /* The moment before the record has landed. A draft always holds
             its one prompt (NAW-FR-06), so this is never an empty state the
             author can act in — and there is nothing here to act with. */
          <div className="draft-editor__empty" aria-busy="true" />
        )}
      </div>
    </div>
  );
}
