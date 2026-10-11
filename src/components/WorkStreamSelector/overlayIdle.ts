/**
 * Whether any floating overlay of the main window is open
 * (`../../../specifications/ui/WSS-work-stream-selector.md` WSS-FR-YHCT).
 *
 * The overlays keep their open state in many places, so this reads the
 * document. A modal window renders a `.scrim`, a dropdown or a context menu has
 * the `menu` role, and the overlays that have neither carry
 * `data-floating-overlay`. An element inside `data-merge-prompt` is the merge
 * prompt itself and is not another overlay.
 */

import { useEffect, useRef, useState } from "react";

const OVERLAY_SELECTOR = '.scrim, [role="menu"], [data-floating-overlay]';

/** True while an overlay other than the merge prompt is open. */
export function otherOverlayIsOpen(): boolean {
  return Array.from(document.querySelectorAll(OVERLAY_SELECTOR)).some(
    (element) => !element.closest("[data-merge-prompt]"),
  );
}

/**
 * Follow `otherOverlayIsOpen` while `watching` is true. The answer is read from
 * the document on each render, so a caller never acts on a stale `false` for one
 * commit. A document that changes is read again; nothing polls and no timer
 * runs.
 */
export function useOtherOverlayOpen(watching: boolean): boolean {
  const [, setTick] = useState(0);
  const open = watching ? otherOverlayIsOpen() : false;
  const last = useRef(open);
  last.current = open;
  useEffect(() => {
    if (!watching) return;
    const observer = new MutationObserver(() => {
      if (otherOverlayIsOpen() !== last.current) setTick((tick) => tick + 1);
    });
    observer.observe(document.body, { childList: true, subtree: true });
    return () => observer.disconnect();
  }, [watching]);
  return open;
}
