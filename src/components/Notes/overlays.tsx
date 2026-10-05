import type { NoteListItem, NoteScope } from "../../types";
import type { MoveTarget, Overlay } from "./helpers";

// Overlays (NTS-FR-19 … NTS-FR-24)
// ---------------------------------------------------------------------------

interface NoteOverlayProps {
  overlay: Overlay;
  item: NoteListItem | undefined;
  moveTargets: MoveTarget[] | null;
  moveFilter: string;
  setMoveFilter: (text: string) => void;
  reminderDraft: string;
  setReminderDraft: (text: string) => void;
  onClose: () => void;
  onDiscuss: (item: NoteListItem) => void;
  onEdit: (item: NoteListItem) => void;
  onMove: (item: NoteListItem, anchor: DOMRect) => void;
  onPickMoveTarget: (id: string, scope: NoteScope) => void;
  onReminder: (item: NoteListItem, anchor: DOMRect) => void;
  onClearReminder: (item: NoteListItem) => void;
  onCommitReminder: (id: string) => void;
  onAskDelete: (item: NoteListItem, anchor: DOMRect) => void;
  onConfirmDelete: (id: string) => void;
}

/** The folder an entity sits in, or the empty string at the project root. */
function parentFolder(id: string): string {
  const cut = id.lastIndexOf("/");
  return cut === -1 ? "" : id.slice(0, cut);
}

/**
 * CVP-FR-57: what a note conversation is called once it is open — `Chat: <this>`
 * in the conversation tab's header and its entry in the strip.
 *
 * The note's own first line rather than the entity it is filed against, because
 * the conversation is about the note: a header reading `Chat: EDT-editor.md`
 * would name the wrong subject, and would be indistinguishable from a discussion
 * about that file. The excerpt beneath it is the subject on hover titles and
 * accessible names, as it is for every conversation.
 */
export function noteLabel(item: NoteListItem): string {
  const line = plainText(item.note.body.trim().split("\n")[0] ?? "");
  if (!line) return "note";
  return line.length > 32 ? `${line.slice(0, 31)}…` : line;
}

/**
 * A note's first line with its inline Markdown read as text rather than as its
 * source characters.
 *
 * A note's body is Markdown (NTS-FR-25) and the row renders it as rich text, so
 * a label taken raw is the one place in the window where an author's `**` shows
 * up as punctuation — in a tab name, a header, and a spoken announcement,
 * none of which can render marks. Deliberately not a Markdown parse: this needs
 * a short plain string, and the marks that survive being stripped naively are
 * the ones a one-line label would not have shown anyway.
 */
function plainText(markdown: string): string {
  return markdown
    .replace(/`([^`]*)`/g, "$1")
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/(\*\*|__)(.*?)\1/g, "$2")
    .replace(/(\*|_)(.*?)\1/g, "$2")
    .replace(/^#{1,6}\s+/, "")
    .replace(/^>\s*/, "")
    .trim();
}

/** A short, single-line rendering of a note, for the delete confirmation. */
export function excerpt(body: string): string {
  const line = plainText(body.trim().split("\n")[0] ?? "");
  // A note with no body still has to be nameable in the confirmation.
  if (!line) return "this empty note";
  return line.length > 48 ? `${line.slice(0, 47)}…` : line;
}

export function NoteOverlay({
  overlay,
  item,
  moveTargets,
  moveFilter,
  setMoveFilter,
  reminderDraft,
  setReminderDraft,
  onClose,
  onDiscuss,
  onEdit,
  onMove,
  onPickMoveTarget,
  onReminder,
  onClearReminder,
  onCommitReminder,
  onAskDelete,
  onConfirmDelete,
}: NoteOverlayProps) {
  // The note an overlay was opened on can leave the list under it — a reload
  // that no longer carries it, or a scope change that moved it out of view.
  if (!item) return null;

  const style: React.CSSProperties = {
    position: "fixed",
    left: overlay.x,
    top: overlay.y,
    zIndex: 200,
  };
  // Clicks inside an overlay must not reach the window-level dismissal; each
  // actionable entry closes the overlay itself.
  const stop = (e: React.MouseEvent) => e.stopPropagation();

  if (overlay.kind === "menu") {
    // NTS-FR-19: exactly five entries, in this order.
    return (
      <div className="menu" style={style} onClick={stop}>
        {/* NTS-FR-26: offered on every note the panel renders, in all three
            scope positions and on the Unresolved group's rows too — a note
            whose entity has gone is still a note with something written in it,
            and the conversation is about what is written. */}
        <div className="menu-item" onClick={() => onDiscuss(item)}>
          Discuss
        </div>
        <div className="menu-item" onClick={() => onEdit(item)}>
          Edit
        </div>
        <div
          className="menu-item"
          onClick={(e) => onMove(item, e.currentTarget.getBoundingClientRect())}
        >
          Move…
        </div>
        {/* NTS-FR-20: the entry reads Set Reminder… on a note carrying none and
            opens the picker; Clear Reminder on one that has it, and clears it
            without opening the picker. */}
        {item.note.reminder ? (
          <div className="menu-item" onClick={() => onClearReminder(item)}>
            Clear Reminder
          </div>
        ) : (
          <div
            className="menu-item"
            onClick={(e) =>
              onReminder(item, e.currentTarget.getBoundingClientRect())
            }
          >
            Set Reminder…
          </div>
        )}
        <div
          className="menu-item"
          onClick={(e) =>
            onAskDelete(item, e.currentTarget.getBoundingClientRect())
          }
        >
          Delete
        </div>
      </div>
    );
  }

  if (overlay.kind === "confirmDelete") {
    // NTS-FR-21: a confirmation naming the note; dismissing it invokes nothing.
    return (
      <div className="menu" style={style} onClick={stop}>
        <div className="menu-confirm">
          <div className="menu-confirm__text">
            Delete “{excerpt(item.note.body)}”?
          </div>
          <div className="menu-confirm__actions">
            <button
              className="btn btn--sm btn--danger"
              onClick={() => onConfirmDelete(item.note.id)}
            >
              Delete
            </button>
            <button className="btn btn--ghost btn--sm" onClick={onClose}>
              Cancel
            </button>
          </div>
        </div>
      </div>
    );
  }

  if (overlay.kind === "reminder") {
    return (
      <div className="menu" style={style} onClick={stop}>
        <div className="menu-rename">
          <input
            autoFocus
            type="datetime-local"
            className="input"
            aria-label="Reminder"
            value={reminderDraft}
            onChange={(e) => setReminderDraft(e.target.value)}
          />
          <div className="menu-rename__actions">
            <button
              className="btn btn--sm"
              onClick={() => onCommitReminder(item.note.id)}
            >
              Set
            </button>
            <button className="btn btn--ghost btn--sm" onClick={onClose}>
              Cancel
            </button>
          </div>
        </div>
      </div>
    );
  }

  // NTS-FR-22: the Move picker — a type-to-filter list of the project's
  // artifacts and Flows, with a **Project level** entry at its head.
  // The filter matches the whole project-relative path, not just the basename,
  // so "specs/a" narrows as readily as "a.md" and two artifacts sharing a
  // basename are separable.
  const needle = moveFilter.trim().toLowerCase();
  const matches = (moveTargets ?? []).filter(
    (t) => !needle || t.id.toLowerCase().includes(needle),
  );
  return (
    <div className="menu notes-move" style={style} onClick={stop}>
      <div className="wt-select__filter">
        <input
          autoFocus
          className="input"
          aria-label="Filter artifacts"
          placeholder="Filter…"
          value={moveFilter}
          onChange={(e) => setMoveFilter(e.target.value)}
        />
      </div>
      <div
        className="menu-item"
        onClick={() => onPickMoveTarget(item.note.id, { kind: "project" })}
      >
        Project level
      </div>
      <div className="menu-sep"></div>
      <div className="notes-move__list">
        {moveTargets === null && <div className="menu-item menu-item--disabled">Loading…</div>}
        {moveTargets !== null && matches.length === 0 && (
          <div className="menu-item menu-item--disabled">No artifact matches.</div>
        )}
        {matches.map((target) => (
          <div
            key={target.id}
            className="menu-item notes-move__item"
            title={target.id}
            onClick={() =>
              onPickMoveTarget(item.note.id, {
                kind: "entity",
                entityId: target.id,
                entityPath: target.id,
              })
            }
          >
            <span className="notes-move__name">{target.name}</span>
            {/* Two artifacts can share a basename; the folder is what tells
                them apart in the list. */}
            {target.id !== target.name && (
              <span className="notes-move__path">{parentFolder(target.id)}</span>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
