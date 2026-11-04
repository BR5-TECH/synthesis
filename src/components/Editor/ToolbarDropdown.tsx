import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { ReactNode } from "react";

/** One toggle button of a toolbar dropdown (EDT-FR-UPNB). */
export interface ToolbarDropdownItem {
  key: string;
  /** The accessible name and the tooltip of the button. */
  label: string;
  /** What the button shows: a level or an icon. */
  content: ReactNode;
  /** EDT-FR-GELS: true for the active choice of the caret block. */
  pressed: boolean;
  onSelect: () => void;
}

/**
 * EDT-FR-UPNB / EDT-FR-VSUO / EDT-FR-IRNA: a dropdown of the block group of
 * the formatting toolbar. Its trigger opens a popup with one row of toggle
 * buttons under it.
 *
 * A pointer press on the trigger or on an item does not take the focus, so the
 * editor keeps its selection, and the choice applies to it. A keyboard opening
 * moves the focus into the popup. The popup is fixed to the window, because
 * the band clips what does not fit in it (EDT-FR-16).
 *
 * The caller holds the open state, so that at most one dropdown of the toolbar
 * is open at a time.
 */
export function ToolbarDropdown({
  label,
  trigger,
  items,
  disabled,
  open,
  onOpenChange,
  testId,
  choice,
}: {
  /** The accessible name and the tooltip of the trigger. */
  label: string;
  /** What the trigger shows: its own label or the active choice. */
  trigger: ReactNode;
  items: ToolbarDropdownItem[];
  /** EDT-FR-IRNA: true while the surface accepts no input. */
  disabled: boolean;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  testId: string;
  /** EDT-FR-GELS: the key of the active choice the trigger shows, if any. */
  choice?: string;
}) {
  const rootRef = useRef<HTMLSpanElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const itemRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const [place, setPlace] = useState<{ top: number; left: number } | null>(null);
  /** The item to focus when the popup opens from the keyboard, or null. */
  const focusOnOpen = useRef<number | null>(null);

  const close = (focusTrigger: boolean) => {
    onOpenChange(false);
    if (focusTrigger) triggerRef.current?.focus();
  };

  // A control that stops accepting input closes the popup it can no longer act on.
  useEffect(() => {
    if (disabled && open) onOpenChange(false);
  }, [disabled, open, onOpenChange]);

  useLayoutEffect(() => {
    if (!open) {
      setPlace(null);
      return;
    }
    const measure = () => {
      const rect = triggerRef.current?.getBoundingClientRect();
      if (rect) setPlace({ top: Math.round(rect.bottom + 4), left: Math.round(rect.left) });
    };
    measure();
    window.addEventListener("resize", measure);
    window.addEventListener("scroll", measure, true);
    return () => {
      window.removeEventListener("resize", measure);
      window.removeEventListener("scroll", measure, true);
    };
  }, [open]);

  // The popup is hidden until it is placed, and a hidden button takes no
  // focus, so the keyboard focus moves in only once the popup has its place.
  useEffect(() => {
    if (!open || !place || focusOnOpen.current === null) return;
    itemRefs.current[focusOnOpen.current]?.focus();
    focusOnOpen.current = null;
  }, [open, place]);

  // EDT-FR-VSUO: a click outside closes the popup, and Escape closes it with
  // the focus on the trigger, also while the focus stays in the editor.
  useEffect(() => {
    if (!open) return;
    const onDown = (event: MouseEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) onOpenChange(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopPropagation();
      onOpenChange(false);
      triggerRef.current?.focus();
    };
    // EDT-FR-IRNA: focus that moves to another control, such as the diagram
    // type menu opened from the keyboard, closes the popup. The editor is not
    // another control: a pointer opening leaves the focus there.
    const onFocus = (event: FocusEvent) => {
      const target = event.target as Element | null;
      if (rootRef.current?.contains(target) || target?.closest?.(".ProseMirror")) return;
      onOpenChange(false);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("focusin", onFocus);
    window.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("focusin", onFocus);
      window.removeEventListener("keydown", onKey, true);
    };
  }, [open, onOpenChange]);

  const openFromKeyboard = () => {
    const active = items.findIndex((item) => item.pressed);
    focusOnOpen.current = active === -1 ? 0 : active;
    if (open) {
      itemRefs.current[focusOnOpen.current]?.focus();
      focusOnOpen.current = null;
    } else {
      // EDT-FR-IRNA: a menu of the toolbar that closes on an outside press,
      // such as the diagram type menu, closes as it does for a pointer press.
      document.body.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
      onOpenChange(true);
    }
  };

  const select = (item: ToolbarDropdownItem) => {
    onOpenChange(false);
    item.onSelect();
  };

  const onItemKey = (event: React.KeyboardEvent, index: number) => {
    const step =
      event.key === "ArrowRight" || event.key === "ArrowDown"
        ? 1
        : event.key === "ArrowLeft" || event.key === "ArrowUp"
          ? -1
          : 0;
    if (step !== 0) {
      event.preventDefault();
      const next = (index + step + items.length) % items.length;
      itemRefs.current[next]?.focus();
    } else if (event.key === "Home" || event.key === "End") {
      event.preventDefault();
      itemRefs.current[event.key === "Home" ? 0 : items.length - 1]?.focus();
    } else if (event.key === "Tab") {
      close(false);
    }
  };

  return (
    <span className="block-menu" ref={rootRef} data-testid={testId}>
      <button
        ref={triggerRef}
        type="button"
        className="btn btn--ghost btn--sm block-menu__trigger"
        title={label}
        aria-label={label}
        aria-haspopup="true"
        aria-expanded={open}
        disabled={disabled}
        onMouseDown={(event) => event.preventDefault()}
        onClick={() => onOpenChange(!open)}
        onKeyDown={(event) => {
          if (event.key === "Enter" || event.key === " " || event.key === "ArrowDown") {
            event.preventDefault();
            openFromKeyboard();
          }
        }}
      >
        <span className="block-menu__value" data-choice={choice ?? ""}>
          {trigger}
        </span>
        <span className="block-menu__caret" aria-hidden="true">
          ▾
        </span>
      </button>
      {open && (
        <span
          className="block-menu__popup"
          role="group"
          aria-label={label}
          style={place ? { top: place.top, left: place.left } : { visibility: "hidden" }}
        >
          {items.map((item, index) => (
            <button
              key={item.key}
              ref={(el) => {
                itemRefs.current[index] = el;
              }}
              type="button"
              className="btn btn--ghost btn--sm block-menu__item"
              title={item.label}
              aria-label={item.label}
              aria-pressed={item.pressed}
              data-active={item.pressed}
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => select(item)}
              onKeyDown={(event) => onItemKey(event, index)}
            >
              {item.content}
            </button>
          ))}
        </span>
      )}
    </span>
  );
}
