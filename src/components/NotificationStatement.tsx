import { useEffect } from "react";
import { Icon } from "./icons";

/**
 * The statement an activation leaves when its address cannot be reached
 * (`NTF-notifications.md` NTF-FR-20 / NTF-FR-21).
 *
 * Anchored at the top-trailing corner of the shell, below the top chrome, so it
 * is clear of the tab strip's controls, of the band above an Editor's editing
 * surface (EFR-FR-AVML), and of the action control every tab carries in its
 * opposite corner (ACT-FR-01).
 *
 * It is **not** one of the main window's mutually-exclusive floating overlays
 * (SNV-FR-56): it takes no focus and intercepts no pointer event, so it may show
 * while a modal or a dropdown is open without either dismissing the other, and
 * a click passes through to whatever lies beneath it. `pointerEvents` is
 * therefore `none` on the container and re-enabled on the dismissal alone —
 * which is what makes "never blocks a click on what lies beneath" a property of
 * the layout rather than a promise.
 */
/**
 * The shell's top-chrome row height, from `src/styles/kit.css`'s
 * `grid-template-rows: 44px 1fr auto`. Duplicated here rather than measured
 * because the statement must be positioned before its first paint; pinned by a
 * test so the two cannot drift apart silently.
 */
export const TOP_CHROME_HEIGHT_PX = 44;

export function NotificationStatement({
  message,
  onDismiss,
}: {
  message: string | null;
  onDismiss: () => void;
}) {
  // NTF-FR-20: dismissed by its own control, by Escape, and on its own after a
  // short interval. The timer is keyed to the message, so a second statement
  // replacing the first (rather than stacking beside it) restarts the clock
  // instead of inheriting the remainder of the first one's.
  useEffect(() => {
    if (!message) return;
    const timer = window.setTimeout(onDismiss, 6000);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onDismiss();
    };
    // Capture phase: the statement takes no focus, so a plain bubble listener
    // would be beaten by any surface that stops propagation on Escape.
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.clearTimeout(timer);
      window.removeEventListener("keydown", onKey, true);
    };
  }, [message, onDismiss]);

  if (!message) return null;

  return (
    <div
      // Not `role="alert"`: an alert interrupts a screen reader mid-utterance,
      // and this is a consequence of the author's own click rather than an
      // emergency. `status` announces it politely and takes no focus.
      role="status"
      aria-live="polite"
      data-testid="notification-statement"
      style={{
        // `fixed`, not `absolute`. The statement is rendered at the App root,
        // which has no positioned ancestor, so `absolute` resolves against the
        // BODY and lands the box ON TOP of the top chrome — opaquely covering
        // the theme selector and the agents control. `fixed` resolves against
        // the viewport, which is what "the shell's top-trailing corner, below
        // the top chrome" actually means here, and it matches how `Toast`
        // anchors itself.
        position: "fixed",
        // Clears the 44px top-chrome row (`src/styles/kit.css` grid-template-rows).
        top: TOP_CHROME_HEIGHT_PX + 8,
        right: 12,
        zIndex: 250,
        // The whole point of NTF-FR-21: the container never eats a click.
        pointerEvents: "none",
        maxWidth: "min(360px, 40vw)",
      }}
    >
      <div
        className="card"
        style={{
          padding: "10px 12px",
          display: "flex",
          alignItems: "flex-start",
          gap: 8,
          boxShadow: "var(--shadow-3)",
          // Opaque over whatever it covers, so no two sets of text read
          // through one another.
          background: "var(--bg-elevated, var(--bg-panel))",
        }}
      >
        <span
          style={{
            fontSize: "var(--fs-ui-sm)",
            lineHeight: 1.45,
            // The sentence wraps on a measure narrower than the viewport
            // rather than running its full width.
            flex: 1,
          }}
        >
          {message}
        </span>
        <button
          type="button"
          className="icon-btn"
          aria-label="Dismiss"
          onClick={onDismiss}
          // Re-enabled only here: the dismissal is the one thing in the
          // statement that is meant to take a click.
          style={{ pointerEvents: "auto", flex: "0 0 auto" }}
        >
          <Icon.X size={12} className="icon" />
        </button>
      </div>
    </div>
  );
}
