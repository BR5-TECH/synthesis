/**
 * The About panel (`specifications/ui/ABT-about-panel.md`).
 *
 * A centred modal overlay drawn inside the window that opened it
 * (ABT-FR-QZHW), in the shared `.scrim` / `.modal` chrome. It shows one
 * description and one link and holds no setting and no state (ABT-FR-TNRB,
 * ABT-FR-SDFA). The shell mounts it only while it is open, so each opening
 * starts fresh.
 */
import { useEffect, useRef } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";

import { logInfo, logWarn } from "../logging";
import { Icon } from "./icons";

/** ABT-FR-TNRB: the exact description. */
export const ABOUT_DESCRIPTION = "AI-powered IDE for spec-driven development.";

/** ABT-FR-WPLJ: the Synthesis GitHub project. */
export const ABOUT_REPOSITORY_URL = "https://github.com/BR5-TECH/synthesis";

const FOCUSABLE = "a[href], button:not([disabled])";

interface AboutPanelProps {
  onClose: () => void;
}

export function AboutPanel({ onClose }: AboutPanelProps) {
  const dialogRef = useRef<HTMLDivElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);

  // ABT-FR-HYFE: focus moves into the panel on open and returns to the element
  // that held it when the About entry was activated. A native menu item holds
  // no focus in the page, so that element is the nearest place to return to.
  useEffect(() => {
    const opener = document.activeElement;
    closeRef.current?.focus();
    logInfo(["frontend"], "about panel opened");
    return () => {
      logInfo(["frontend"], "about panel closed");
      // Focus returns only if it is not already somewhere new. Another overlay
      // that replaced this panel has taken focus, and it keeps it.
      const resting =
        document.activeElement === null || document.activeElement === document.body;
      if (resting && opener instanceof HTMLElement && opener.isConnected) {
        opener.focus();
      }
    };
  }, []);

  // ABT-FR-DXGC: Escape closes the panel. ABT-FR-HYFE: Tab and Shift+Tab stay
  // inside it. Captured ahead of the surfaces beneath, which must not react to
  // a key the panel answers.
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        onClose();
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
  }, [onClose]);

  // ABT-FR-WPLJ: the address opens in the operating system's default browser,
  // never inside Synthesis, and the panel stays open.
  const openRepository = (e: React.MouseEvent) => {
    e.preventDefault();
    logInfo(["frontend"], "about link activated");
    void openUrl(ABOUT_REPOSITORY_URL).catch((err) =>
      logWarn(["frontend"], "the about link could not be opened", {
        reason: String(err),
      }),
    );
  };

  return (
    <div className="scrim">
      <div
        ref={dialogRef}
        className="modal about-panel"
        role="dialog"
        aria-modal="true"
        aria-label="About Synthesis"
      >
        <div className="modal__head">
          <div className="modal__title">About Synthesis</div>
          <button
            ref={closeRef}
            className="btn btn--ghost btn--icon btn--sm"
            aria-label="Close"
            onClick={onClose}
          >
            <Icon.X size={12} />
          </button>
        </div>
        <div className="modal__body">
          <p className="about-panel__description">{ABOUT_DESCRIPTION}</p>
          <a
            className="about-panel__link"
            href={ABOUT_REPOSITORY_URL}
            aria-label="Synthesis on GitHub"
            onClick={openRepository}
            onAuxClick={(e) => e.preventDefault()}
          >
            Synthesis on GitHub
          </a>
        </div>
      </div>
    </div>
  );
}
