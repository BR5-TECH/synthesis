import { useEffect, useRef, useState } from "react";
import { Icon } from "./icons";
import {
  dismissToast,
  INFO_TOAST_MS,
  useToasts,
  type NotificationLevel,
  type ToastEntry,
} from "../state/toasts";

/**
 * The shell's top-chrome row height, from `src/styles/kit.css`'s
 * `grid-template-rows: 44px 1fr auto`. Duplicated here rather than measured
 * because the stack must be positioned before its first paint; pinned by a test
 * so the two cannot drift apart silently.
 */
export const TOP_CHROME_HEIGHT_PX = 44;

const LEVEL_ICON = {
  Info: Icon.Info,
  Warn: Icon.Warning,
  Error: Icon.ErrorCircle,
} as const satisfies Record<NotificationLevel, unknown>;

/**
 * One toast (`NTF-notifications.md` NTF-FR-DGLS through NTF-FR-OPCD).
 *
 * The click target and the dismissal are two buttons, so each is in the tab
 * order and operable with the keyboard. Nothing here takes focus when the toast
 * appears (NTF-FR-BVCG).
 */
function ToastItem({
  toast,
  onActivate,
}: {
  toast: ToastEntry;
  onActivate: (address: string, key: string) => void;
}) {
  const [hovered, setHovered] = useState(false);
  const [focusedWithin, setFocusedWithin] = useState(false);
  const paused = hovered || focusedWithin;

  // NTF-FR-YJAE: the time an Info toast has left. A pause keeps what remained;
  // a replacement (a new revision) starts the full time again.
  const remaining = useRef(INFO_TOAST_MS);
  const revision = useRef(toast.revision);

  useEffect(() => {
    if (revision.current !== toast.revision) {
      revision.current = toast.revision;
      remaining.current = INFO_TOAST_MS;
    }
    if (toast.level !== "Info" || paused) return;
    const startedAt = Date.now();
    const timer = window.setTimeout(
      () => dismissToast(toast.key),
      remaining.current,
    );
    return () => {
      window.clearTimeout(timer);
      remaining.current = Math.max(0, remaining.current - (Date.now() - startedAt));
    };
  }, [toast.key, toast.level, toast.revision, paused]);

  const LevelIcon = LEVEL_ICON[toast.level];

  const rootRef = useRef<HTMLDivElement>(null);

  // The activation runs first. An unreachable address then shows its toast in
  // the slot this click frees (NTF-FR-20).
  const onClick = () => {
    if (toast.address) onActivate(toast.address, toast.key);
    dismissToast(toast.key);
  };

  // NTF-FR-BVCG: a keyboard author who dismisses a toast keeps a place to
  // work from. Focus moves to a neighbouring toast when one exists.
  const onDismissClick = () => {
    const root = rootRef.current;
    if (root?.contains(document.activeElement)) {
      const neighbour = root.nextElementSibling ?? root.previousElementSibling;
      neighbour?.querySelector<HTMLElement>("button")?.focus();
    }
    dismissToast(toast.key);
  };

  return (
    <div
      // NTF-FR-OPCD: Info is polite, Warn and Error are assertive.
      ref={rootRef}
      role={toast.level === "Info" ? "status" : "alert"}
      className="ntf-toast card"
      data-testid="notification-toast"
      data-level={toast.level}
      data-key={toast.key}
      onPointerEnter={() => setHovered(true)}
      onPointerLeave={() => setHovered(false)}
      onFocus={() => setFocusedWithin(true)}
      onBlur={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) {
          setFocusedWithin(false);
        }
      }}
      style={{
        pointerEvents: "auto",
        display: "flex",
        alignItems: "flex-start",
        gap: 8,
        padding: "10px 12px",
        boxShadow: "var(--shadow-3)",
        // Opaque over whatever it covers, so no two sets of text read through
        // one another.
        background: "var(--bg-elevated, var(--bg-panel))",
      }}
    >
      <button
        type="button"
        className="ntf-toast__target"
        data-testid="notification-toast-target"
        onClick={onClick}
        style={{
          flex: 1,
          minWidth: 0,
          display: "flex",
          flexDirection: "column",
          gap: 2,
          padding: 0,
          border: 0,
          background: "none",
          color: "inherit",
          font: "inherit",
          textAlign: "start",
          cursor: "pointer",
        }}
      >
        <span
          style={{
            display: "flex",
            alignItems: "center",
            gap: 6,
            fontSize: "var(--fs-ui-xs)",
            fontWeight: 600,
            color: "var(--fg-2)",
          }}
        >
          <LevelIcon size={12} className="icon" />
          <span data-testid="notification-toast-level">{toast.level}</span>
        </span>
        <span
          style={{
            fontSize: "var(--fs-ui-sm)",
            fontWeight: 600,
            lineHeight: 1.4,
            overflowWrap: "anywhere",
          }}
        >
          {toast.title}
        </span>
        {toast.body !== "" && (
          <span
            style={{
              fontSize: "var(--fs-ui-sm)",
              lineHeight: 1.45,
              overflowWrap: "anywhere",
            }}
          >
            {toast.body}
          </span>
        )}
      </button>
      <button
        type="button"
        className="icon-btn"
        aria-label="Dismiss"
        onClick={onDismissClick}
        style={{ flex: "0 0 auto" }}
      >
        <Icon.X size={12} className="icon" />
      </button>
    </div>
  );
}

/**
 * The toast stack of the notification facility (`NTF-notifications.md`
 * NTF-FR-HZNF, NTF-FR-LAWP, NTF-FR-BVCG).
 *
 * Anchored at the top-trailing corner of the shell, below the top chrome, so it
 * is clear of the tab strip's controls, of the band above an Editor's editing
 * surface (EFR-FR-AVML), and of the action control every tab carries in its
 * opposite corner (ACT-FR-01).
 *
 * It is **not** one of the main window's mutually-exclusive floating overlays
 * (SNV-FR-56): it takes no focus and may show while a modal or a dropdown is
 * open. `pointerEvents` is `none` on the region and `auto` on each toast, which
 * makes "intercepts no pointer event outside the toasts" a property of the
 * layout rather than a promise. `fixed` resolves against the viewport, which is
 * what "below the top chrome" means at the App root.
 */
export function NotificationToasts({
  onActivate,
}: {
  onActivate: (address: string, key: string) => void;
}) {
  const toasts = useToasts();
  if (toasts.length === 0) return null;
  return (
    <div
      data-testid="notification-toast-stack"
      style={{
        position: "fixed",
        top: TOP_CHROME_HEIGHT_PX + 8,
        right: 12,
        zIndex: 250,
        pointerEvents: "none",
        width: 360,
        maxWidth: "40vw",
        display: "flex",
        flexDirection: "column",
        gap: 8,
      }}
    >
      {toasts.map((toast) => (
        <ToastItem key={toast.key} toast={toast} onActivate={onActivate} />
      ))}
    </div>
  );
}
