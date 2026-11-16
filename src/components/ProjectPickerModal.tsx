/**
 * The Project picker as a modal over a project window
 * (`../specifications/ui/PPK-project-picker.md` PPK-FR-GMYH, PPK-FR-IHWN).
 *
 * It wraps the one picker implementation; it adds the dialog frame, the close
 * control, and Escape.
 */
import { useEffect } from "react";

import type { ProjectHandle } from "../types";
import { Icon } from "./icons";
import { ProjectPicker } from "./ProjectPicker";

interface ProjectPickerModalProps {
  onChoose: (handle: ProjectHandle) => void;
  onDismiss: () => void;
  /** The last failure of an open, shown inline (PPK-FR-KRPD). */
  error?: string;
  /** PJT-FR-PNDG: the name of the project that opens in a new tab now. */
  opening?: string;
}

export function ProjectPickerModal({
  onChoose,
  onDismiss,
  error,
  opening = "",
}: ProjectPickerModalProps) {
  // PPK-FR-IHWN: Escape dismisses the modal and changes nothing.
  useEffect(() => {
    const opener = document.activeElement;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      // The open project dialog above the modal takes Escape first.
      if (document.querySelector(".open-project-dialog")) return;
      // PJT-FR-PNDG: an open in progress finishes first.
      if (document.querySelector(".project-picker-modal__opening")) return;
      e.preventDefault();
      onDismiss();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      if (opener instanceof HTMLElement && opener.isConnected) opener.focus();
    };
  }, [onDismiss]);

  return (
    <div className="scrim project-picker-modal__scrim">
      <div
        className="project-picker-modal"
        role="dialog"
        aria-modal="true"
        aria-label="Project picker"
        aria-busy={opening ? true : undefined}
      >
        <button
          type="button"
          className="project-picker-modal__close btn btn--ghost btn--sm"
          aria-label="Close project picker"
          onClick={onDismiss}
          disabled={!!opening}
        >
          <Icon.X size={12} />
        </button>
        <div inert={opening ? true : undefined}>
          <ProjectPicker onOpen={onChoose} resolveOnly presentation="modal" />
        </div>
        {opening && (
          <div className="project-picker-modal__opening" role="status">
            Opening {opening} in a new tab…
          </div>
        )}
        {error && (
          <p className="project-picker-modal__error" role="alert">
            {error}
          </p>
        )}
      </div>
    </div>
  );
}
