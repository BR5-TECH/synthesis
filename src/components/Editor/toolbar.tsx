import { Icon } from "../icons";
import type { EditMode } from "../../state/editHistory";

/**
 * EDT-FR-16: the Editor's primary actions rendered as graphical icon controls in
 * a single right-aligned cluster — the WYSIWYG⇄source mode toggle (EDT-FR-17)
 * and the comments control. `mode`/`onToggleMode` are omitted on surfaces
 * without dual-mode editing (the mock).
 *
 * The cluster carries no Save control: an artifact writes itself (EDT-FR-70), so
 * a control to write it would either do nothing or duplicate what has already
 * happened. The unsaved badge stays, because whether the buffer has reached disk
 * is still worth reading (EDT-FR-04) even once nobody has to act on it.
 */
interface ActionClusterProps {
  dirty: boolean;
  mode?: EditMode;
  onToggleMode?: () => void;
  /**
   * CMT-FR-29: the comments control and the artifact's unresolved-thread count.
   * Omitted on surfaces with no rail (the mock). Present in **both** editing
   * modes, so switching to raw text never hides the fact that threads exist.
   */
  comments?: { count: number; open: boolean; onToggle: () => void };
  /**
   * PCR-FR-16 / EDT-FR-84: the pending indication an artifact carrying an
   * undecided proposed change renders, which opens the review on it.
   *
   * In this cluster rather than in a chrome of its own (EDT-FR-84), present
   * whichever editing mode the tab is in, and absent altogether for an artifact
   * carrying no pending proposal — there being no third kind of indication.
   */
  proposal?: { onOpen: () => void };
  /**
   * NAW-FR-13: a surface whose writes are its own to report — a draft, whose
   * prompt writes itself and which therefore reports whether the pending edit
   * has landed rather than offering a Save (`NAW-new-artifact.md`).
   *
   * It takes the place of the unsaved badge rather than standing beside it: the
   * two say the same thing about the same buffer, and it belongs **in** this row
   * rather than floating over it — placed over the row it would sit exactly
   * where the trailing control does.
   */
  report?: string;
}

export function ActionCluster({
  dirty,
  mode,
  onToggleMode,
  comments,
  proposal,
  report,
}: ActionClusterProps) {
  return (
    <div className="editor__actions">
      {report ? (
        <span className="editor__report t-ui-xs" data-dirty={dirty === true}>
          {report}
        </span>
      ) : (
        dirty && (
          <span className="badge badge--warn" style={{ marginRight: 4 }}>
            ● unsaved
          </span>
        )
      )}
      {comments && (
        <button
          className="btn btn--ghost btn--icon editor__comments-toggle"
          aria-label={
            comments.open
              ? `Hide comments (${comments.count} unresolved)`
              : `Show comments (${comments.count} unresolved)`
          }
          title={`${comments.count} unresolved ${
            comments.count === 1 ? "thread" : "threads"
          }`}
          aria-expanded={comments.open}
          data-active={comments.open}
          onClick={comments.onToggle}
        >
          <Icon.Comment size={15} />
          <span className="editor__comments-count">{comments.count}</span>
        </button>
      )}
      {/* PCR-FR-16: the way back to a review that was dismissed, and the way in
          to one that arrived while the tab was closed. It clears the moment the
          proposal is decided, because it renders from the held reading of
          PCR-FR-27 rather than from anything this component keeps. */}
      {proposal && (
        <button
          className="btn btn--ghost btn--icon editor__proposal-indication"
          aria-label="Review the proposed change to this prompt"
          title="A change has been proposed to this prompt"
          data-testid="editor-prompt-proposal-indication"
          data-active
          onClick={proposal.onOpen}
        >
          <Icon.Diff size={15} />
        </button>
      )}
      {mode && onToggleMode && (
        <button
          className="btn btn--ghost btn--icon"
          aria-label={
            mode === "wysiwyg" ? "Edit as Markdown source" : "Edit as rich text"
          }
          title={
            mode === "wysiwyg" ? "Edit as Markdown source" : "Edit as rich text"
          }
          data-active={mode === "text"}
          onClick={onToggleMode}
        >
          {mode === "wysiwyg" ? <Icon.Code size={15} /> : <Icon.Doc size={15} />}
        </button>
      )}
    </div>
  );
}

/** Mock toolbar: the static formatting affordances plus the shared cluster. */
export function EditorToolbar({ dirty }: { dirty: boolean }) {
  return (
    <div className="editor__toolbar">
      <button className="btn btn--ghost btn--sm" title="Bold">
        <b style={{ fontFamily: "serif" }}>B</b>
      </button>
      <button className="btn btn--ghost btn--sm" title="Italic">
        <i style={{ fontFamily: "serif" }}>I</i>
      </button>
      <button className="btn btn--ghost btn--sm" title="Code">
        <span style={{ fontFamily: "var(--font-mono)" }}>{"<>"}</span>
      </button>
      <span className="sep--v" style={{ height: 18, margin: "0 6px" }} />
      <button className="btn btn--ghost btn--sm">H1</button>
      <button className="btn btn--ghost btn--sm">H2</button>
      <button className="btn btn--ghost btn--sm">H3</button>
      <span className="sep--v" style={{ height: 18, margin: "0 6px" }} />
      <button className="btn btn--ghost btn--sm" title="Bulleted list">
        •
      </button>
      <button className="btn btn--ghost btn--sm" title="Numbered">
        1.
      </button>
      <button className="btn btn--ghost btn--sm" title="Quote">
        ”
      </button>
      <div className="spacer" />
      <ActionCluster dirty={dirty} />
    </div>
  );
}
