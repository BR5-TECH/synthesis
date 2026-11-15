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
}

export function ProjectPickerModal({ onChoose, onDismiss, error }: ProjectPickerModalProps) {
  // PPK-FR-IHWN: Escape dismisses the modal and changes nothing.
  useEffect(() => {
    const opener = document.activeElement;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      // The open project dialog above the modal takes Escape first.
      if (document.querySelector(".open-project-dialog")) return;
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
      >
        <button
          type="button"
          className="project-picker-modal__close btn btn--ghost btn--sm"
          aria-label="Close project picker"
          onClick={onDismiss}
        >
          <Icon.X size={12} />
        </button>
        <ProjectPicker onOpen={onChoose} resolveOnly presentation="modal" />
        {error && (
          <p className="project-picker-modal__error" role="alert">
            {error}
          </p>
        )}
      </div>
    </div>
  );
}
