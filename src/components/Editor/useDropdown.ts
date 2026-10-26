import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";

/**
 * EDT-FR-OWNT / EDT-FR-AQCR / EDT-FR-KOUB: the open state, the close rules,
 * and the keyboard model that the dropdowns of the Editor's chrome share — the
 * diagram type menu of the formatting toolbar and the table of contents panel.
 *
 * The dropdown closes on a selection, on a pointer-down outside it, and on
 * Escape, which returns focus to the trigger. It is tab chrome, not a floating
 * overlay of the window (EFR-FR-AVML), so it registers with nothing.
 */
export function useDropdown() {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const itemRefs = useRef<Array<HTMLElement | null>>([]);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  // Focus goes to the first entry when the dropdown opens, so the arrow keys
  // reach the entries at once. With no entry, focus stays on the trigger.
  useEffect(() => {
    if (open) itemRefs.current.find((el) => el !== null)?.focus();
  }, [open]);

  const close = useCallback((restoreFocus: boolean) => {
    setOpen(false);
    if (restoreFocus) triggerRef.current?.focus();
  }, []);

  const toggle = useCallback(() => setOpen((was) => !was), []);

  /** Escape anywhere in the dropdown closes it and returns focus. */
  const onRootKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (e.key !== "Escape" || !open) return;
      e.preventDefault();
      // The dropdown answers Escape alone: the band, the comment rail, and the
      // surfaces of the tab around it stay as they are.
      e.stopPropagation();
      close(true);
    },
    [open, close],
  );

  /** The arrow keys, Home, and End move between entries; Tab leaves. */
  const onItemKeyDown = useCallback((e: KeyboardEvent, index: number) => {
    const items = itemRefs.current.filter((el): el is HTMLElement => el !== null);
    const last = items.length - 1;
    const move = (to: number) => {
      e.preventDefault();
      items[Math.max(0, Math.min(last, to))]?.focus();
    };
    if (e.key === "ArrowDown") move(index + 1);
    else if (e.key === "ArrowUp") move(index - 1);
    else if (e.key === "Home") move(0);
    else if (e.key === "End") move(last);
    else if (e.key === "Tab") setOpen(false);
  }, []);

  /** Opens the dropdown from the keyboard on the trigger. */
  const onTriggerKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (e.key === "ArrowDown" && !open) {
        e.preventDefault();
        setOpen(true);
      } else if (e.key === "Tab" && open) {
        // Focus leaves the dropdown, also from a panel with no entry to focus.
        setOpen(false);
      }
    },
    [open],
  );

  return {
    open,
    setOpen,
    rootRef,
    triggerRef,
    itemRefs,
    close,
    toggle,
    onRootKeyDown,
    onItemKeyDown,
    onTriggerKeyDown,
  };
}
