import { useEffect, useRef, type KeyboardEvent, type ReactNode } from "react";
import { Icon } from "./icons";
import { THEME_CHOICES } from "./GlobalSettings";
import type { ThemePreference } from "../types";

/**
 * The top chrome's theme selector (`SNV-shell-navigation.md` SNV-FR-15,
 * SNV-FR-QXTH): one icon-only trigger that opens a vertical dropdown with one
 * icon-only row per theme.
 */

interface ThemeSelectorProps {
  themePref: ThemePreference;
  onSelectTheme: (pref: ThemePreference) => void;
  /**
   * SNV-FR-56: the dropdown's open state is the shell's, so that opening any
   * other floating overlay closes it, also when that overlay is opened from the
   * keyboard and raises no pointer event.
   */
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** SNV-FR-56: closes every other floating overlay before this one opens. */
  onOverlayOpening: () => void;
}

/** The icon of a theme. The system theme shows a display, never the gear. */
function themeIcon(value: ThemePreference, size: number): ReactNode {
  if (value === "light") return <Icon.Sun size={size} />;
  if (value === "dark") return <Icon.Moon size={size} />;
  return <Icon.Monitor size={size} />;
}

export function ThemeSelector({
  themePref,
  onSelectTheme,
  open,
  onOpenChange,
  onOverlayOpening,
}: ThemeSelectorProps) {
  const rootRef = useRef<HTMLDivElement | null>(null);
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const rowRefs = useRef<Array<HTMLButtonElement | null>>([]);
  // Whether the dropdown was opened and has not been closed since, so that the
  // focus moves to the row only when the dropdown opens, not on each render.
  const focusOnOpen = useRef(false);

  const selectedIndex = Math.max(
    0,
    THEME_CHOICES.findIndex(([value]) => value === themePref),
  );
  const selectedLabel = THEME_CHOICES[selectedIndex][1];

  // SNV-FR-QXTH: an outside pointer-down closes the dropdown with no change,
  // and so does Escape, wherever the focus is. Escape returns the focus to the
  // trigger.
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        onOpenChange(false);
      }
    };
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (e.key !== "Escape") return;
      onOpenChange(false);
      triggerRef.current?.focus();
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open, onOpenChange]);

  // Focus goes to the selected row when the dropdown opens by keyboard. After a
  // pointer open the focus stays where the click put it.
  useEffect(() => {
    if (open && focusOnOpen.current) {
      rowRefs.current[selectedIndex]?.focus();
    }
    focusOnOpen.current = false;
  }, [open, selectedIndex]);

  const openMenu = (focusRow: boolean) => {
    if (open) return;
    // SNV-FR-56: every other overlay closes first, then this one opens.
    onOverlayOpening();
    focusOnOpen.current = focusRow;
    onOpenChange(true);
  };

  const closeMenu = (restoreFocus: boolean) => {
    onOpenChange(false);
    if (restoreFocus) triggerRef.current?.focus();
  };

  const choose = (value: ThemePreference) => {
    onSelectTheme(value);
    closeMenu(true);
  };

  const onTriggerKeyDown = (e: KeyboardEvent<HTMLButtonElement>) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      if (open) rowRefs.current[selectedIndex]?.focus();
      else openMenu(true);
    } else if ((e.key === "Enter" || e.key === " ") && !open) {
      // Enter and Space open the dropdown and put focus on the selected row.
      // The default click is cancelled so it does not toggle the dropdown shut.
      e.preventDefault();
      openMenu(true);
    }
  };

  const onRowKeyDown = (e: KeyboardEvent<HTMLButtonElement>, index: number) => {
    const last = THEME_CHOICES.length - 1;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      rowRefs.current[Math.min(index + 1, last)]?.focus();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      rowRefs.current[Math.max(index - 1, 0)]?.focus();
    } else if (e.key === "Tab") {
      // The focus leaves the selector, so the dropdown closes. The default
      // action of Tab still moves the focus.
      onOpenChange(false);
    }
  };

  return (
    <div
      ref={rootRef}
      className="theme-select"
      data-testid="chrome-theme-select"
      // Tab moving the focus out of the selector closes the dropdown. A null
      // related target is a click on a non-focusable area, which the
      // outside-pointer rule above already decides.
      onBlur={(e) => {
        const next = e.relatedTarget as Node | null;
        if (open && next && !rootRef.current?.contains(next)) onOpenChange(false);
      }}
    >
      <button
        ref={triggerRef}
        type="button"
        className="btn btn--ghost btn--icon btn--sm theme-select__trigger"
        data-testid="chrome-theme-trigger"
        aria-haspopup="true"
        aria-expanded={open}
        aria-label={`Theme: ${selectedLabel}`}
        onClick={() => (open ? closeMenu(false) : openMenu(false))}
        onKeyDown={onTriggerKeyDown}
      >
        {themeIcon(themePref, 13)}
        <span className="theme-select__tip" aria-hidden="true">
          {selectedLabel} theme
        </span>
      </button>

      {open && (
        <div
          role="radiogroup"
          aria-label="Theme"
          className="card theme-select__menu"
          data-testid="chrome-theme-menu"
          data-floating-overlay=""
        >
          {THEME_CHOICES.map(([value, label], index) => (
            <button
              key={value}
              ref={(el) => {
                rowRefs.current[index] = el;
              }}
              type="button"
              role="radio"
              aria-checked={themePref === value}
              aria-label={label}
              // Roving tab stop: only the selected row is in the Tab order, and
              // the arrows reach the others, as in any radio group.
              tabIndex={themePref === value ? 0 : -1}
              className="btn btn--ghost btn--icon btn--sm theme-select__row"
              data-active={themePref === value}
              onClick={() => choose(value)}
              onKeyDown={(e) => onRowKeyDown(e, index)}
            >
              {themeIcon(value, 13)}
              <span className="theme-select__tip" aria-hidden="true">
                {label} theme
              </span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
