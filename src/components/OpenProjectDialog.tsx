/**
 * The open project dialog
 * (`../specifications/ui/PJT-project-tabs.md` PJT-FR-GASB, PJT-FR-DLGT,
 * PJT-FR-RMCB, PJT-FR-YCNT).
 *
 * It asks where a project opens: in this project tab, in a new one, or
 * nowhere. Each choice is a card that says what it does to the project of this
 * window. It owns the choice and the Remember my choice flag, and nothing that
 * follows from them; the caller stores the flag, writes pending changes, and
 * calls the backend.
 */
import { useEffect, useId, useRef, useState } from "react";

export type OpenProjectChoice = "this-tab" | "new-tab";

interface OpenProjectDialogProps {
  /** The project that opens. */
  projectName: string;
  /** The project of this window, which This tab closes. */
  currentName: string;
  onChoose: (choice: OpenProjectChoice, remember: boolean) => void;
  onCancel: () => void;
}

const FOCUSABLE = "button:not([disabled]), input:not([disabled])";

export function OpenProjectDialog({
  projectName,
  currentName,
  onChoose,
  onCancel,
}: OpenProjectDialogProps) {
  const dialogRef = useRef<HTMLDivElement>(null);
  const newTabRef = useRef<HTMLButtonElement>(null);
  // PJT-FR-RMCB: off at each show.
  const [remember, setRemember] = useState(false);
  const id = useId();

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
        aria-describedby={`${id}-lead`}
      >
        <div className="modal__head">
          <div className="modal__title">Open {projectName}</div>
        </div>
        <div className="modal__body">
          <p id={`${id}-lead`} className="open-project-dialog__lead">
            {currentName} is open in this window. Where do you want to open {projectName}?
          </p>
          <div className="open-project-dialog__choices">
            <button
              type="button"
              className="open-project-dialog__choice"
              aria-labelledby={`${id}-this`}
              aria-describedby={`${id}-this-desc`}
              onClick={() => onChoose("this-tab", remember)}
            >
              <span id={`${id}-this`} className="open-project-dialog__label">
                This tab
              </span>
              <span id={`${id}-this-desc`} className="open-project-dialog__desc">
                {currentName} closes in this window. Its changes are saved first.
              </span>
            </button>
            <button
              ref={newTabRef}
              type="button"
              className="open-project-dialog__choice"
              aria-labelledby={`${id}-new`}
              aria-describedby={`${id}-new-desc`}
              onClick={() => onChoose("new-tab", remember)}
            >
              <span id={`${id}-new`} className="open-project-dialog__label">
                New tab
              </span>
              <span id={`${id}-new-desc`} className="open-project-dialog__desc">
                {projectName} opens in a new tab. {currentName} stays open with its files and
                runs.
              </span>
            </button>
          </div>
          <label className="open-project-dialog__remember">
            <input
              type="checkbox"
              checked={remember}
              aria-labelledby={`${id}-remember`}
              aria-describedby={`${id}-remember-hint`}
              onChange={(e) => setRemember(e.target.checked)}
            />
            <span>
              <span id={`${id}-remember`} className="open-project-dialog__remember-label">
                Remember my choice
              </span>
              <span id={`${id}-remember-hint`} className="open-project-dialog__desc">
                From now on, projects open this way with no question. You can change this in
                Settings → Navigation.
              </span>
            </span>
          </label>
        </div>
        <div className="modal__actions">
          <button type="button" className="btn btn--ghost btn--sm" onClick={onCancel}>
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}
