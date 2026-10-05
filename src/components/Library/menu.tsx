import { useEffect, useRef, useState } from "react";
import { logError } from "../../logging";
import { ARTIFACT_TYPES } from "../../artifactTypes";
import { Icon } from "../icons";
import type { ArtifactType, TreeNode } from "../../types";

// LCM-FR-01: the single declarative action-applicability mapping. Each context-
// menu action declares, in one place, the node kinds and artifact types it
// applies to; the menu render below only consults this map and never re-derives
// availability, so retargeting an action is a one-line edit here. Notes is
// offered for any artifact file (a file node with a resolved type, Flows
// included); Delete/Copy/Rename and the
// Artifact Type submenu are universal; Paste and the three creation entries — New
// File, New Artifact and New Folder — are folder-only (LCM-FR-08, LCM-FR-09,
// LCM-FR-10).
type MenuAction =
  | "newFile"
  | "newArtifact"
  | "newFolder"
  | "notes"
  | "delete"
  | "copy"
  | "paste"
  | "rename"
  | "artifactType";

const ACTION_APPLIES: Record<MenuAction, (node: TreeNode) => boolean> = {
  newFile: (n) => n.nodeKind === "folder",
  newArtifact: (n) => n.nodeKind === "folder",
  newFolder: (n) => n.nodeKind === "folder",
  notes: (n) => n.nodeKind === "file" && n.artifactType != null,
  delete: () => true,
  copy: () => true,
  paste: (n) => n.nodeKind === "folder",
  rename: () => true,
  artifactType: () => true };

export interface ContextMenuState {
  x: number;
  y: number;
  node: TreeNode;
}

interface ContextMenuProps extends ContextMenuState {
  // The path currently on the panel's copy clipboard, or null when empty.
  // Drives whether folder **Paste** is enabled (LCM-FR-03).
  clipboard: string | null;
  onClose: () => void;
  onNewFile: () => void;
  onNewArtifact: () => void;
  onNewFolder: () => void;
  onNotes: () => void;
  onAssign: (type: ArtifactType) => void;
  onClear: () => void;
  onDelete: () => void;
  onCopy: () => void;
  onPaste: () => void;
  onRename: (newName: string) => void;
}

// Why a rename was rejected client-side, or null when it is valid (LCM-FR-02,
// LCM-FR-12). A reason rather than a boolean because the three cases are not
// interchangeable: "unchanged" is the one that produces an ERROR record, and the
// user needs to be told which mistake they made.
export type RenameRejection = "empty" | "separator" | "unchanged";

export function rejectRename(
  name: string,
  currentName: string,
): RenameRejection | null {
  const trimmed = name.trim();
  if (trimmed === "") return "empty";
  if (trimmed.includes("/") || trimmed.includes("\\")) return "separator";
  // LCM-FR-12: a rename to the name the node already has changes nothing, so it
  // is refused here and never reaches the backend. Compared against the node's
  // own basename, which is what `"rename path"` would replace.
  if (trimmed === currentName) return "unchanged";
  return null;
}

/**
 * Whether a press landed outside the context menu, and so dismisses it.
 *
 * A target the press itself took out of the document is **not** an outside
 * press. **Rename** replaces the menu's entry list with its rename input, and a
 * browser re-renders between the entry's own handler and the window listener
 * that this test serves — so by the time the press reaches the window, the entry
 * it came from is detached and a containment test against the frame answers
 * "outside". Dismissing there closed the menu on its way into rename mode, which
 * is why **Rename** did nothing at all in the shipped window while every jsdom
 * test stayed green: jsdom flushes the re-render later than a browser does, so
 * the unit tests never met the detached target.
 *
 * A press with no target at all is outside: nothing places it in the menu.
 */
export function pressLandsOutside(
  target: Node | null,
  frame: HTMLElement | null,
): boolean {
  if (!target) return true;
  if (!target.isConnected) return false;
  return !frame?.contains(target);
}

const RENAME_MESSAGE: Record<RenameRejection, string> = {
  empty: "Enter a name.",
  separator: "A name cannot contain a path separator.",
  unchanged: "That is already the name." };

/**
 * LCM-FR-11: the recursive-delete confirmation.
 *
 * A centered modal overlay rather than a menu region, because it is the one
 * action here that cannot be undone and it outlives the menu that started it.
 * As a floating overlay it is mutually exclusive with the window's others
 * (SNV-FR-56) — it is mounted by this panel and nothing else is open beside it.
 *
 * **Cancel** takes focus rather than **Delete**, so a stray Enter on a dialog
 * the user did not expect removes nothing.
 */
export function RecursiveDeleteConfirm({
  node,
  onCancel,
  onConfirm }: {
  node: TreeNode;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const cancelRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    cancelRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onCancel]);

  return (
    <div
      className="scrim"
      onClick={(e) => {
        // LCM-FR-11: an outside click dismisses, invoking nothing further.
        if (e.target === e.currentTarget) onCancel();
      }}
    >
      <div
        className="modal"
        role="dialog"
        aria-labelledby="recursive-delete-title"
      >
        <div className="modal__head">
          {/* The subject of the dialog, not a second close control — every other
              modal leads its head with what it is about (a folder, here). */}
          <Icon.Folder size={14} />
          <div className="modal__title" id="recursive-delete-title">
            Delete folder
          </div>
          <button
            className="btn btn--ghost btn--icon btn--sm"
            aria-label="Close"
            onClick={onCancel}
          >
            <Icon.X size={12} />
          </button>
        </div>
        <div className="modal__body">
          {/* `overflow-wrap: anywhere` because the folder's name is the one
              thing this dialog must convey, and `.modal` clips its overflow: a
              long unbroken name would otherwise be cut mid-word with no
              ellipsis, leaving the user consenting to destroy a folder they
              cannot fully see. */}
          <div style={{ overflowWrap: "anywhere" }}>
            “{node.name}” is not empty. Delete it and everything inside it?
          </div>
          <div className="t-muted">This cannot be undone.</div>
        </div>
        <div className="modal__actions">
          <button ref={cancelRef} className="btn btn--sm" onClick={onCancel}>
            Cancel
          </button>
          <button className="btn btn--sm btn--danger" onClick={onConfirm}>
            Delete
          </button>
        </div>
      </div>
    </div>
  );
}

// The Project context menu (LCM-FR-01). Entries vary by node kind and artifact
// type, driven entirely by ACTION_APPLIES: file ops are universal; Notes appears
// for artifact files and Flows; Paste only
// for folders (greyed when the clipboard is empty); and artifact-type curation
// lives under a nested **Artifact Type** submenu (LCM-FR-04) that holds the eight
// types and, below a divider, **Clear Type** at the very bottom. Every entry and
// submenu carries a leading icon (LCM-FR-07). Rename resolves inline in a rename
// input; Delete acts directly and needs no native dialog.
export function ContextMenu({
  x,
  y,
  node,
  clipboard,
  onClose,
  onNewFile,
  onNewArtifact,
  onNewFolder,
  onNotes,
  onAssign,
  onClear,
  onDelete,
  onCopy,
  onPaste,
  onRename }: ContextMenuProps) {
  // "menu" is the root list; "rename" swaps in a rename input. Delete has no
  // mode of its own: it acts directly (LCM-FR-11), and the only prompt in the
  // flow is the recursive-delete window the *backend* triggers by reporting a
  // folder non-empty.
  const [mode, setMode] = useState<"menu" | "rename">("menu");
  // Whether the nested **Artifact Type** submenu is expanded (LCM-FR-04). It
  // opens on mouse-over of its parent entry, without a click.
  const [assignOpen, setAssignOpen] = useState(false);
  const [nameDraft, setNameDraft] = useState(node.name);
  // Why the last rename attempt was rejected client-side, or null. The input
  // stays open carrying the offending value so the user can correct it rather
  // than retype it (LCM-FR-12).
  const [renameError, setRenameError] = useState<RenameRejection | null>(null);

  /** The menu's own frame, so a press inside it is not an outside one. */
  const frameRef = useRef<HTMLDivElement | null>(null);

  // The menu dismisses on an outside click or on Escape, in any mode and without
  // invoking an action (LCM non-functional requirement).
  //
  // `mousedown` as well as `click`, because SNV-FR-56 names this menu among the
  // window's mutually-exclusive floating overlays alongside the tab strip's (per
  // `TAB-tabs.md` TAB-FR-32) — and a right-click, which is how that other menu
  // is opened, raises `mousedown` but never `click`. Listening on `click` alone
  // left the two context menus mounted side by side.
  //
  // **Containment is what makes the `mousedown` listener safe.** A press that
  // begins inside the menu must not dismiss it: unmounting between `mousedown`
  // and `mouseup` means the browser dispatches no `click` at all, so every entry
  // here — the whole creation cluster included — would be dead to a real mouse
  // while still working under a synthetic `click`. Stopping propagation on the
  // frame's `onClick` does not help, because it is the `mousedown` that arrives
  // first. The Drafts panel's menus and the tab strip's guard the same way.
  useEffect(() => {
    const onPointer = (e: MouseEvent) => {
      if (pressLandsOutside(e.target as Node | null, frameRef.current))
        onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("click", onPointer);
    window.addEventListener("mousedown", onPointer);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("click", onPointer);
      window.removeEventListener("mousedown", onPointer);
      window.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  // Availability is read from the single ACTION_APPLIES mapping (LCM-FR-01), so
  // this component decides nothing about which node kinds/types an action serves.
  const can = (action: MenuAction) => ACTION_APPLIES[action](node);
  // Paste is shown on folders and additionally enabled only while the clipboard
  // holds a path (LCM-FR-03).
  const canPaste = clipboard != null;

  const submitRename = () => {
    const rejection = rejectRename(nameDraft, node.name);
    if (rejection) {
      setRenameError(rejection);
      // LCM-FR-12: an unchanged name is the one rejection that is logged. The
      // other two are malformed input a user corrects in place; this one is a
      // request the application refused to act on, which is what an ERROR record
      // exists to account for. The path is safe to log; no file content is read.
      if (rejection === "unchanged") {
        logError(["frontend"], "rename refused: name unchanged", {
          path: node.path,
          name: nameDraft.trim() });
      }
      return;
    }
    onRename(nameDraft.trim());
  };

  const wrapperStyle: React.CSSProperties = {
    position: "fixed",
    left: x,
    top: y,
    zIndex: 200 };

  if (mode === "rename") {
    return (
      <div className="menu" style={wrapperStyle} ref={frameRef}>
        <div className="menu-rename">
          <input
            autoFocus
            aria-label="New name"
            className="input"
            value={nameDraft}
            onChange={(e) => {
              setNameDraft(e.target.value);
              setRenameError(null);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") submitRename();
              if (e.key === "Escape") onClose();
            }}
          />
          {renameError && (
            <div className="menu-rename__error" role="alert">
              {RENAME_MESSAGE[renameError]}
            </div>
          )}
          <div className="menu-rename__actions">
            <button className="btn btn--sm" onClick={submitRename}>
              Rename
            </button>
            <button className="btn btn--ghost btn--sm" onClick={onClose}>
              Cancel
            </button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div
      className="menu"
      style={wrapperStyle}
      // The dismissal listener above tests containment against this frame, so a
      // press that begins inside the menu is not an outside one. Each actionable
      // entry closes the menu itself.
      ref={frameRef}
    >
      {/* LCM-FR-08 / LCM-FR-09 / LCM-FR-10: folder nodes lead with the three-entry
          creation cluster — **New File**, then **New Artifact**, then **New
          Folder** — above the file operations and separated from them by one
          divider. The File menu presents the same three in the same order
          (SNV-FR-23), so the two entry points into creation read identically. New
          File opens its modal with this folder as the *starting* location, still
          editable (NFI-FR-07); New Artifact opens its modal with this folder as the
          *starting* location, still editable, and the artifact type unchosen
          whatever type this folder carries (NTA-FR-08); New Folder opens its modal with
          this folder as the *starting* parent, still editable, and the artifact
          type unset (NFW-FR-07). Each carries its own leading icon (LCM-FR-07). */}
      {can("newFile") && (
        <div className="menu-item" onClick={onNewFile}>
          <Icon.File size={13} /> New File
        </div>
      )}
      {can("newArtifact") && (
        <div className="menu-item" onClick={onNewArtifact}>
          <Icon.Plus size={13} /> New Artifact
        </div>
      )}
      {can("newFolder") && (
        <div className="menu-item" onClick={onNewFolder}>
          <Icon.Folder size={13} /> New Folder
        </div>
      )}
      {(can("newFile") || can("newArtifact") || can("newFolder")) && (
        <div className="menu-sep"></div>
      )}
      {can("notes") && (
        <div className="menu-item" onClick={onNotes}>
          <Icon.Notes size={13} /> Notes
        </div>
      )}
      {can("notes") && <div className="menu-sep"></div>}

      {/* LCM-FR-11: Delete acts directly. A file and an empty folder are removed
          by the first call with no confirmation at all; the only thing that ever
          asks is the recursive-delete window, and it asks solely because the
          backend reported the folder non-empty. Marked destructive here rather
          than relying on a prompt to carry that signal. */}
      <div className="menu-item menu-item--danger" onClick={onDelete}>
        <Icon.X size={13} /> Delete
      </div>
      <div className="menu-item" onClick={onCopy}>
        <Icon.File size={13} /> Copy
      </div>
      {can("paste") && (
        <div
          className={canPaste ? "menu-item" : "menu-item menu-item--disabled"}
          aria-disabled={!canPaste}
          onClick={() => {
            if (canPaste) onPaste();
          }}
        >
          <Icon.FolderOpen size={13} /> Paste
        </div>
      )}
      <div
        className="menu-item"
        onClick={() => {
          setNameDraft(node.name);
          setRenameError(null);
          setMode("rename");
        }}
      >
        <Icon.Doc size={13} /> Rename
      </div>

      <div className="menu-sep"></div>

      {/* LCM-FR-04: the nested **Artifact Type** submenu. It opens on mouse-over
          and carries its own leading icon (LCM-FR-04 / LCM-FR-07). It lists the
          eight types and, after a divider, **Clear Type** at the very bottom
          (LCM-FR-04). */}
      <div
        className="menu-item--parent"
        onMouseEnter={() => setAssignOpen(true)}
        onMouseLeave={() => setAssignOpen(false)}
      >
        <div
          className="menu-item"
          aria-expanded={assignOpen}
          // Hover is the primary opener; a click also opens (never closes) so
          // the submenu is reachable without a pointer that can hover.
          onClick={() => setAssignOpen(true)}
        >
          <Icon.Tag size={13} /> Artifact Type
          <span className="menu-item__kbd">
            <Icon.CaretRight size={12} />
          </span>
        </div>
        {assignOpen && (
          <div className="menu menu--sub">
            {ARTIFACT_TYPES.map((t) => {
              const TypeIcon = t.icon;
              return (
                <div
                  key={t.value}
                  className="menu-item"
                  onClick={() => onAssign(t.value)}
                >
                  <TypeIcon size={13} />
                  <span className="chip-type" data-type={t.value}>
                    {t.chip}
                  </span>{" "}
                  {t.label}
                </div>
              );
            })}
            <div className="menu-sep"></div>
            <div className="menu-item" onClick={onClear}>
              <Icon.X size={13} /> Clear Type
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
