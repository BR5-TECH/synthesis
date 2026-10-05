import { useId, useLayoutEffect, useMemo, useRef } from "react";
import type { Editor as TiptapEditor } from "@tiptap/react";
import { Icon } from "../icons";
import {
  analyzeFrontmatter,
  DESCRIPTION_LIMIT,
} from "../../state/frontmatterYaml";
import type { FindMatch } from "../../state/findMatches";
import { highlightYaml } from "./highlight";

/**
 * EDT-FR-18/EDT-FR-20: the WYSIWYG-only frontmatter region — a visually distinct,
 * editable box. The inner YAML is editable; an untouched value round-trips
 * byte-for-byte on save (see joinFrontmatter).
 *
 * When `expanded` it never scrolls internally: it auto-grows to fit every line
 * (no vertical scrollbar / clipping) and soft-wraps long lines (no horizontal
 * scrollbar), so the whole block is visible at once (EDT-FR-18), with a minimize
 * button to collapse it. When collapsed (the body is scrolled down) it renders as
 * a single non-editing summary line that expands on click (a transient peek).
 *
 * EDT-FR-21: in the expanded state the block is rendered from a YAML parse of it
 * — keys bold, scalar values normal, indicators and comments receding — via a
 * highlight layer behind the (transparent-text) textarea. EDT-FR-58: a block that
 * does not parse renders unhighlighted under an invalid-YAML indication, as
 * editable as ever. EDT-FR-59/EDT-FR-60: a top-level `description` reports its
 * parsed length against the 1024-character budget on a line along the bottom
 * edge. Both are region chrome — outside the textarea, so neither is editable,
 * matched by the find panels, or an undoable step.
 */
export function FrontmatterRegion({
  value,
  editable,
  expanded,
  inputRef,
  matches = [],
  currentMatch = null,
  onChange,
  onExpand,
  onMinimize,
}: {
  value: string;
  editable: boolean;
  expanded: boolean;
  /** So the Editor can place the caret here when the find panel closes. */
  inputRef?: React.RefObject<HTMLTextAreaElement | null>;
  /** EFR-FR-DSKI: the find panel's matches within this region. */
  matches?: readonly FindMatch[];
  currentMatch?: FindMatch | null;
  onChange: (v: string) => void;
  onExpand: () => void;
  onMinimize: () => void;
}) {
  const ownRef = useRef<HTMLTextAreaElement>(null);
  const ref = inputRef ?? ownRef;
  const countId = useId();

  // EDT-FR-60: recomputed from the buffer on every change — including an edit
  // made in the raw-text surface or a step traversed by undo/redo, both of which
  // reach here as a new `value` — so the region is current whenever it renders.
  const yaml = useMemo(() => analyzeFrontmatter(value), [value]);

  // Grow the textarea to its content height so it never needs a vertical scroll.
  // Runs after layout, on value change, and when (re)expanding; with soft-wrap a
  // wrapped long line increases scrollHeight and the box grows to match.
  useLayoutEffect(() => {
    if (!expanded) return; // textarea isn't mounted while collapsed
    const el = ref.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  }, [value, expanded]);

  if (!expanded) {
    // EDT-FR-20: collapsed single-line summary. The whole bar is the click target
    // to expand; it stays identifiable as frontmatter and is not an editing field.
    const preview = value.split("\n").find((l) => l.trim().length > 0) ?? "";
    return (
      <button
        type="button"
        className="editor__frontmatter editor__frontmatter--collapsed"
        aria-label="Expand frontmatter"
        aria-describedby={yaml.description !== null ? countId : undefined}
        onClick={onExpand}
      >
        <span className="editor__frontmatter-label">frontmatter</span>
        <span className="editor__frontmatter-preview">{preview}</span>
        {/* EDT-FR-59: the budget rides along at the trailing edge, so it stays
            readable while the artifact is scrolled into its body. It is part of
            the bar, so clicking it expands the region like any other spot. The
            bar's aria-label would otherwise swallow it — expanded, the reading
            is ordinary text in the reading order, so it is described here to
            reach assistive tech on both displays alike. */}
        <DescriptionCount description={yaml.description} id={countId} />
      </button>
    );
  }

  return (
    <div className="editor__frontmatter">
      <div className="editor__frontmatter-head">
        <span className="editor__frontmatter-label">frontmatter</span>
        {/* EDT-FR-58: the block does not parse — say so, and leave it alone. The
            indication clears as soon as an edit makes it parse. */}
        {!yaml.valid && (
          <span
            className="editor__frontmatter-invalid"
            role="status"
            title={yaml.error ?? undefined}
          >
            not valid YAML
          </span>
        )}
        <button
          type="button"
          className="btn btn--ghost btn--icon editor__frontmatter-min"
          aria-label="Minimize frontmatter"
          title="Minimize"
          onClick={onMinimize}
        >
          <Icon.Minimize size={14} />
        </button>
      </div>
      {/* EDT-FR-21: the editable textarea sits transparently over an aria-hidden
          highlight layer that bolds YAML keys and leaves values normal. The two
          share identical typography (see kit.css) so the styled text registers
          exactly under the textarea's caret; the textarea remains the sole
          editing surface and the source of the buffer's bytes. */}
      <div className="editor__frontmatter-edit">
        <pre className="editor__frontmatter-hl" aria-hidden="true">
          {highlightYaml(value, yaml.spans, matches, currentMatch)}
        </pre>
        <textarea
          ref={ref}
          className="editor__frontmatter-input"
          aria-label="Frontmatter"
          value={value}
          spellCheck={false}
          readOnly={!editable}
          rows={1}
          // No scrollbars (vertical handled by autosize, horizontal by soft-wrap)
          // and not user-resizable — the box is always exactly content-height.
          style={{ overflow: "hidden", resize: "none" }}
          onChange={(e) => onChange(e.target.value)}
        />
      </div>
      {/* EDT-FR-59: expanded, the same reading sits on a line along the bottom
          edge of the region. */}
      {yaml.description !== null && (
        <div className="editor__frontmatter-foot">
          <DescriptionCount description={yaml.description} />
        </div>
      )}
    </div>
  );
}

/**
 * EDT-FR-59/EDT-FR-60: the description's parsed length against its fixed budget,
 * rendered identically in both of the region's displays. Over the limit it takes
 * a warning treatment and keeps reading the true count — it blocks no edit and
 * gates no save. Renders nothing when there is no scalar `description` to count.
 */
function DescriptionCount({
  description,
  id,
}: {
  description: string | null;
  /** Set on the collapsed bar, which describes itself by this reading. */
  id?: string;
}) {
  if (description === null) return null;
  return (
    <span
      id={id}
      className="editor__frontmatter-count"
      data-over={description.length > DESCRIPTION_LIMIT ? "true" : "false"}
      title="Characters used by the frontmatter description"
    >
      {description.length}/{DESCRIPTION_LIMIT}
    </span>
  );
}

/**
 * Serialise the editor's current document back to Markdown via the
 * `tiptap-markdown` storage. Typed locally because the extension does not ship
 * a module augmentation for `editor.storage`.
 */
/**
 * Formatting controls wired to Tiptap commands, with active-state highlighting.
 * Rendered only in WYSIWYG mode (EDT-FR-17); the primary actions live in the
 * right-aligned ActionCluster (EDT-FR-16).
 */
export function FormattingButtons({ editor }: { editor: TiptapEditor | null }) {
  // `onMouseDown` preventDefault keeps the selection in the editor when a
  // toolbar button is clicked (otherwise the button steals focus first).
  const cmd = (run: () => void) => (e: React.MouseEvent) => {
    e.preventDefault();
    run();
  };
  const active = (name: string, attrs?: Record<string, unknown>) =>
    editor?.isActive(name, attrs) ?? false;

  return (
    <>
      <button
        className="btn btn--ghost btn--sm"
        title="Bold"
        data-active={active("bold")}
        onMouseDown={cmd(() => editor?.chain().focus().toggleBold().run())}
      >
        <b style={{ fontFamily: "serif" }}>B</b>
      </button>
      <button
        className="btn btn--ghost btn--sm"
        title="Italic"
        data-active={active("italic")}
        onMouseDown={cmd(() => editor?.chain().focus().toggleItalic().run())}
      >
        <i style={{ fontFamily: "serif" }}>I</i>
      </button>
      <button
        className="btn btn--ghost btn--sm"
        title="Inline code"
        data-active={active("code")}
        onMouseDown={cmd(() => editor?.chain().focus().toggleCode().run())}
      >
        <span style={{ fontFamily: "var(--font-mono)" }}>{"<>"}</span>
      </button>
      <span className="sep--v" style={{ height: 18, margin: "0 6px" }} />
      {([1, 2, 3] as const).map((level) => (
        <button
          key={level}
          className="btn btn--ghost btn--sm"
          title={`Heading ${level}`}
          data-active={active("heading", { level })}
          onMouseDown={cmd(() =>
            editor?.chain().focus().toggleHeading({ level }).run(),
          )}
        >
          H{level}
        </button>
      ))}
      <span className="sep--v" style={{ height: 18, margin: "0 6px" }} />
      <button
        className="btn btn--ghost btn--sm"
        title="Bulleted list"
        data-active={active("bulletList")}
        onMouseDown={cmd(() => editor?.chain().focus().toggleBulletList().run())}
      >
        •
      </button>
      <button
        className="btn btn--ghost btn--sm"
        title="Numbered list"
        data-active={active("orderedList")}
        onMouseDown={cmd(() => editor?.chain().focus().toggleOrderedList().run())}
      >
        1.
      </button>
      <button
        className="btn btn--ghost btn--sm"
        title="Quote"
        data-active={active("blockquote")}
        onMouseDown={cmd(() => editor?.chain().focus().toggleBlockquote().run())}
      >
        ”
      </button>
    </>
  );
}

interface ExternalChangeModalProps {
  name: string;
  onLoad: () => void;
  onKeep: () => void;
}

/**
 * EXC-FR-TYKX: the blocking external-change modal. Exactly two resolutions —
 * "Load from filesystem" (EXC-FR-WDAV) and "Keep my version" (EXC-FR-WDEJ). No merge.
 */
export function ExternalChangeModal({ name, onLoad, onKeep }: ExternalChangeModalProps) {
  return (
    <div className="scrim">
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="external-change-title"
      >
        <div className="modal__head">
          <span className="modal__title" id="external-change-title">
            “{name}” changed on disk
          </span>
        </div>
        <div className="modal__body">
          This file was modified outside the editor. Load the version from disk,
          or keep your in-memory copy? Keeping it will overwrite the file on disk
          when you next save.
        </div>
        <div className="modal__actions">
          <button className="btn btn--default btn--sm" onClick={onLoad}>
            Load from filesystem
          </button>
          <button className="btn btn--primary btn--sm" onClick={onKeep}>
            Keep my version
          </button>
        </div>
      </div>
    </div>
  );
}
