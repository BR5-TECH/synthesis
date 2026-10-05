import { useEffect, useRef, useState } from "react";

/**
 * CMT-FR-32: the closers of every surface menu that is mounted. Opening one menu
 * calls the others, so one margin never holds two open menus.
 */
const closers = new Set<() => void>();

/**
 * The overflow menu of one discussion surface (CMT-FR-32).
 *
 * A surface holds at most one open menu. Escape or an outside pointer-down
 * dismisses it, and neither invokes anything. `mousedown` rather than `click`,
 * so the menu is gone before whatever was clicked acts.
 */
export function useThreadMenu() {
  const [open, setOpen] = useState(false);
  const closeSelf = useRef(() => setOpen(false));

  useEffect(() => {
    const close = closeSelf.current;
    closers.add(close);
    return () => {
      closers.delete(close);
    };
  }, []);

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
    toggle: () => {
      if (!open) {
        for (const close of closers) {
          if (close !== closeSelf.current) close();
        }
      }
      setOpen((o) => !o);
    },
    close: () => setOpen(false),
  };
}
