import { useEffect, useState } from "react";

/**
 * The card's overflow menu, for a surface that mounts a single card of its own.
 *
 * The rail holds this state for its whole list, since at most one of its cards
 * may have a menu open at a time (CMT-FR-32). A detached overlay and a
 * conversation tab render exactly one card each and have no list to arbitrate,
 * so they keep the state here instead. Without it the `⋯` renders and opens
 * nothing, which puts Lock and Resolve out of reach in two of the four
 * presentation modes — the surfaces are meant to differ in where a conversation
 * is shown and in nothing about what can be done with it (CVP-FR-33).
 */
export function useCardMenu() {
  const [open, setOpen] = useState(false);

  // CMT-FR-32: Escape or an outside pointer-down dismisses it, and neither
  // invokes anything. `mousedown` rather than `click`, as the rail's does, so
  // the menu is gone before whatever was clicked acts.
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    const onDown = (e: MouseEvent) => {
      const target = e.target;
      if (
        target instanceof Element &&
        target.closest(".comment-card__menu, .comment-card__menu-button")
      ) {
        return;
      }
      setOpen(false);
    };
    document.addEventListener("keydown", onKey);
    document.addEventListener("mousedown", onDown);
    return () => {
      document.removeEventListener("keydown", onKey);
      document.removeEventListener("mousedown", onDown);
    };
  }, [open]);

  return {
    open,
    toggle: () => setOpen((v) => !v),
    close: () => setOpen(false),
  };
}
