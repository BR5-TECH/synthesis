import { useEffect, useId, useRef, useState } from "react";

/**
 * The activity bar's tooltip (`SNV-shell-navigation.md` SNV-FR-49).
 *
 * An application-rendered bubble in the current theme rather than the native
 * `title` attribute, because the strip's controls are icon-only: their label is
 * the only thing that names them, and a native tooltip cannot be styled,
 * positioned, or shown on keyboard focus.
 *
 * The bubble is deliberately **not** a floating overlay in the sense the rest of
 * the shell uses the word (the single-overlay invariant that makes the search
 * overlay, the project switcher, and the modals mutually exclusive). It takes no
 * focus and intercepts no pointer events, so it can coexist with any of them and
 * never swallows a click meant for the control underneath.
 */

/** How long the pointer must rest on a control before its tooltip appears. */
export const TOOLTIP_DELAY_MS = 400;

interface TooltipProps {
  /** The text of the bubble, and the control's accessible name. */
  label: string;
  /**
   * Which edge of the window the activity bar sits on (SNV-FR-06). The bubble
   * is placed on the strip's *inner* side — right of a left-hand strip, left of
   * a right-hand one — so it never overhangs the window edge (SNV-FR-49, SNV-FR-06).
   *
   * `below` opens the bubble under the control, aligned to its trailing edge,
   * for a control at the trailing end of a header (DDS-FR-WRNB).
   */
  side: "left" | "right" | "below";
  /**
   * What the bubble shows, where it says more than the accessible name. Absent,
   * the bubble shows `label`.
   */
  content?: React.ReactNode;
  /** Overridable so tests need not wait out the real delay. */
  delayMs?: number;
  /**
   * Asked each time the bubble would open, with the anchor. While it answers
   * true the bubble stays closed, so a bubble never covers a menu that is
   * open beside its control (DDS-FR-WRNB). Absent, the bubble always opens.
   */
  blocked?: (anchor: HTMLElement) => boolean;
  /**
   * The control. Receives the props that bind it to the bubble: the accessible
   * name, and `aria-describedby` while the bubble is up.
   */
  children: (props: {
    "aria-label": string;
    "aria-describedby"?: string;
  }) => React.ReactNode;
}

export function Tooltip({
  label,
  side,
  content,
  delayMs = TOOLTIP_DELAY_MS,
  blocked,
  children,
}: TooltipProps) {
  const [open, setOpen] = useState(false);
  const id = useId();
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const anchor = useRef<HTMLSpanElement | null>(null);

  const show = () => {
    if (blocked && anchor.current && blocked(anchor.current)) return;
    setOpen(true);
  };

  const cancel = () => {
    if (timer.current !== null) {
      clearTimeout(timer.current);
      timer.current = null;
    }
  };
  const hide = () => {
    cancel();
    setOpen(false);
  };

  // A pending timer must not fire into an unmounted tree — the panel side can
  // flip and the strip re-render while the pointer rests on a control.
  useEffect(() => cancel, []);

  // Escape dismisses the bubble. Bound only while it is up, so the strip adds
  // no keyboard listener in its resting state.
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") hide();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open]);

  return (
    <span
      ref={anchor}
      className="tooltip-anchor"
      // Hover shows after a delay so sweeping the pointer across the strip does
      // not flash six bubbles in a row.
      onPointerEnter={() => {
        cancel();
        timer.current = setTimeout(show, delayMs);
      }}
      onPointerLeave={hide}
      // Keyboard focus shows immediately: there is no sweep to debounce, and a
      // delay would make the label feel unreachable from the keyboard.
      onFocus={() => {
        cancel();
        show();
      }}
      onBlur={hide}
      // Activating the control dismisses the bubble — the panel it describes
      // has just moved, so the label is stale the moment the click lands.
      onClick={hide}
    >
      {children({
        "aria-label": label,
        ...(open ? { "aria-describedby": id } : {}),
      })}
      {open && (
        <span
          id={id}
          role="tooltip"
          className="tooltip"
          data-side={side}
          data-testid={`tooltip-${label}`}
        >
          {content ?? label}
        </span>
      )}
    </span>
  );
}
