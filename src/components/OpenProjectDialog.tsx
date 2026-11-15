/**
 * The open project dialog
 * (`../specifications/ui/PJT-project-tabs.md` PJT-FR-GASB, PJT-FR-YCNT).
 *
 * It asks where a project opens: in this project tab, in a new one, or
 * nowhere. It owns the choice and nothing that follows from it; the caller
 * writes pending changes and calls the backend.
 */
import { useEffect, useRef } from "react";

export type OpenProjectChoice = "this-tab" | "new-tab";

interface OpenProjectDialogProps {
  projectName: string;
  onChoose: (choice: OpenProjectChoice) => void;
  onCancel: () => void;
}

const FOCUSABLE = "button:not([disabled])";

export function OpenProjectDialog({
  projectName,
  onChoose,
  onCancel,
}: OpenProjectDialogProps) {
  const dialogRef = useRef<HTMLDivElement>(null);
  const newTabRef = useRef<HTMLButtonElement>(null);

  // PJT-FR-GASB: the first focus is on New tab. PJT-FR-YCNT: focus returns to
  // the control that opened the dialog when it closes.
  useEffect(() => {
    const opener = document.activeElement;
    newTabRef.current?.focus();
    return () => {
      if (opener instanceof HTMLElement && opener.isConnected) opener.focus();
    };
  }, []);

  // Escape cancels. Tab and Shift+Tab stay inside the dialog.
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        onCancel();
        return;
      }
      if (e.key !== "Tab") return;
      const dialog = dialogRef.current;
      if (!dialog) return;
      const items = Array.from(dialog.querySelectorAll<HTMLElement>(FOCUSABLE));
      if (items.length === 0) return;
      const first = items[0];
      const last = items[items.length - 1];
      const active = document.activeElement;
      if (!(active instanceof Node) || !dialog.contains(active)) {
        e.preventDefault();
        (e.shiftKey ? last : first).focus();
      } else if (e.shiftKey && active === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && active === last) {
        e.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [onCancel]);

  return (
    <div className="scrim">
      <div
        ref={dialogRef}
        className="modal open-project-dialog"
        role="dialog"
        aria-modal="true"
        aria-label={`Open ${projectName}`}
      >
        <div className="modal__head">
          <div className="modal__title">Open {projectName}</div>
        </div>
        <div className="modal__actions">
          <button
            type="button"
            className="btn btn--sm"
            onClick={() => onChoose("this-tab")}
          >
            This tab
          </button>
          <button
            ref={newTabRef}
            type="button"
            className="btn btn--primary btn--sm"
            onClick={() => onChoose("new-tab")}
          >
            New tab
          </button>
          <button type="button" className="btn btn--ghost btn--sm" onClick={onCancel}>
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}
