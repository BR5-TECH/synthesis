import { useEffect, useRef } from "react";
import type { ReactNode } from "react";
import { Icon } from "../icons";

const FOCUSABLE =
  'button:not([disabled]), input:not([disabled]), [href], select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/**
 * GIT-FR-VCDG: a modal window of the Git panel.
 *
 * It takes focus when it opens and keeps it inside itself: Tab and Shift+Tab
 * wrap at the ends. When it closes, focus goes back to the control that opened
 * it, through `restoreFocus`. Escape, the close control and the backdrop close
 * it, unless `dismissible` is false — a running deletion must not be
 * abandoned, because it would still finish unseen.
 *
 * Only one of these is mounted at a time: the panel holds one overlay state, so
 * opening one replaces any other (NFI-FR-01).
 */
export function Overlay({
  title,
  titleId,
  onClose,
  dismissible = true,
  initialFocus,
  restoreFocus,
  actions,
  children,
  testId,
}: {
  title: string;
  titleId: string;
  onClose: () => void;
  dismissible?: boolean;
  /** The control that takes focus first. The window itself, when absent. */
  initialFocus?: () => HTMLElement | null;
  /** The control that opened the window. */
  restoreFocus: () => HTMLElement | null;
  actions?: ReactNode;
  children: ReactNode;
  testId?: string;
}) {
  const dialogRef = useRef<HTMLDivElement | null>(null);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  const dismissibleRef = useRef(dismissible);
  dismissibleRef.current = dismissible;
  const restoreRef = useRef(restoreFocus);
  restoreRef.current = restoreFocus;

  useEffect(() => {
    (initialFocus?.() ?? dialogRef.current)?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        if (dismissibleRef.current) closeRef.current();
        return;
      }
      if (e.key !== "Tab") return;
      const dialog = dialogRef.current;
      if (!dialog) return;
      const items = Array.from(dialog.querySelectorAll<HTMLElement>(FOCUSABLE));
      if (items.length === 0) {
        e.preventDefault();
        dialog.focus();
        return;
      }
      const first = items[0];
      const last = items[items.length - 1];
      const active = document.activeElement as HTMLElement | null;
      if (!active || !dialog.contains(active)) {
        e.preventDefault();
        first.focus();
      } else if (e.shiftKey && (active === first || active === dialog)) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && active === last) {
        e.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("keydown", onKey, true);
      restoreRef.current()?.focus();
    };
    // Focus is taken once, when the window opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div
      className="scrim"
      data-testid="git-overlay-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget && dismissible) onClose();
      }}
    >
      <div
        className="modal git-overlay"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        ref={dialogRef}
        data-testid={testId}
      >
        <div className="modal__head">
          <div className="modal__title" id={titleId}>
            {title}
          </div>
          <button
            type="button"
            className="btn btn--ghost btn--icon btn--sm"
            aria-label="Close"
            disabled={!dismissible}
            onClick={onClose}
          >
            <Icon.X size={12} />
          </button>
        </div>
        <div className="modal__body git-overlay__body">{children}</div>
        {actions && <div className="modal__actions">{actions}</div>}
      </div>
    </div>
  );
}
