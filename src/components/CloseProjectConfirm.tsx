/**
 * The confirmation before a project tab with a run in flight closes
 * (`../specifications/ui/PJT-project-tabs.md` PJT-FR-OJZG).
 */
import { useEffect, useRef } from "react";

interface CloseProjectConfirmProps {
  projectName: string;
  onConfirm: () => void;
  onCancel: () => void;
}

export function CloseProjectConfirm({
  projectName,
  onConfirm,
  onCancel,
}: CloseProjectConfirmProps) {
  const cancelRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    cancelRef.current?.focus();
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onCancel();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onCancel]);

  return (
    <div className="scrim">
      <div
        className="modal close-project-confirm"
        role="alertdialog"
        aria-modal="true"
        aria-label={`Close ${projectName}`}
      >
        <div className="modal__head">
          <div className="modal__title">Close {projectName}?</div>
        </div>
        <p className="modal__body">
          A run is still working in this project. Closing the tab stops it.
        </p>
        <div className="modal__actions">
          <button
            ref={cancelRef}
            type="button"
            className="btn btn--sm"
            onClick={onCancel}
          >
            Keep open
          </button>
          <button type="button" className="btn btn--primary btn--sm" onClick={onConfirm}>
            Stop and close
          </button>
        </div>
      </div>
    </div>
  );
}
