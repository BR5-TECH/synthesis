/**
 * The context menu of a Terminals panel row (`TRM-terminals.md` TRM-FR-EUMZ,
 * TRM-FR-KFCS, TRM-FR-QXIY).
 *
 * Six entries in a fixed order: Rename terminal, Theme, the pin action, a
 * separator, Close terminal, Close other terminals, Close all terminals. Theme
 * opens the Theme submenu beside it. The menu takes focus when it opens, moves
 * by the arrow keys, chooses on Enter, and closes on a choice, on Escape, and
 * on any activation outside it. Closing it, whichever way, is the caller's cue
 * to put focus back on the row.
 */
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Icon } from "../icons";
import type { CloseScope } from "../../state/terminalGroups";
import type { TerminalThemeId } from "../../types";
import { TerminalThemeSubmenu } from "./TerminalThemeSubmenu";

export type TerminalMenuChoice = "rename" | "pin" | CloseScope;

type EntryId = TerminalMenuChoice | "theme";

interface Entry {
  id: EntryId;
  label: string;
  icon: React.ReactNode;
  disabled: boolean;
  separatorAfter?: boolean;
}

const MENU_WIDTH = 220;
const MARGIN = 8;

interface SubmenuState {
  anchor: { left: number; right: number; top: number };
  focusFirst: boolean;
}

export function TerminalRowMenu({
  x,
  y,
  terminalLabel,
  pinned,
  theme,
  othersDisabled,
  allDisabled,
  onChoose,
  onChooseTheme,
  onClose,
}: {
  x: number;
  y: number;
  terminalLabel: string;
  pinned: boolean;
  /** TRM-FR-QXIY: the terminal's theme override, or null. */
  theme: TerminalThemeId | null;
  othersDisabled: boolean;
  allDisabled: boolean;
  onChoose: (choice: TerminalMenuChoice) => void;
  onChooseTheme: (theme: TerminalThemeId | null) => void;
  /** `restoreFocus` is true for Escape, which puts focus back on the row. */
  onClose: (restoreFocus: boolean) => void;
}) {
  const entries: Entry[] = [
    { id: "rename", label: "Rename terminal", icon: <Icon.Tag size={13} />, disabled: false },
    { id: "theme", label: "Theme", icon: <Icon.Palette size={13} />, disabled: false },
    // TRM-FR-KFCS: the pin action is never disabled, and only one form shows.
    {
      id: "pin",
      label: pinned ? "Unpin terminal" : "Pin terminal",
      icon: pinned ? <Icon.Unpin size={13} /> : <Icon.Pin size={13} />,
      disabled: false,
      separatorAfter: true,
    },
    { id: "one", label: "Close terminal", icon: <Icon.X size={13} />, disabled: false },
    {
      id: "others",
      label: "Close other terminals",
      icon: <Icon.CloseMany size={13} />,
      disabled: othersDisabled,
    },
    {
      id: "all",
      label: "Close all terminals",
      icon: <Icon.Trash size={13} />,
      disabled: allDisabled,
    },
  ];
  const themeIndex = entries.findIndex((e) => e.id === "theme");
  const enabled = entries.map((e, i) => (e.disabled ? -1 : i)).filter((i) => i >= 0);
  const [index, setIndex] = useState(0);
  const [submenu, setSubmenu] = useState<SubmenuState | null>(null);
  const submenuRef = useRef(submenu);
  submenuRef.current = submenu;
  const frameRef = useRef<HTMLDivElement | null>(null);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;

  const entryElement = (i: number) => frameRef.current?.querySelectorAll<HTMLElement>(':scope > div > [role="menuitem"]')[i];

  useLayoutEffect(() => {
    if (!submenuRef.current?.focusFirst) entryElement(index)?.focus();
  }, [index]);

  useEffect(() => {
    const onPointer = (e: Event) => {
      const target = e.target as Node | null;
      if (!target || !frameRef.current?.contains(target)) closeRef.current(false);
    };
    const onFocus = (e: FocusEvent) => {
      const target = e.target as Node | null;
      if (target && !frameRef.current?.contains(target)) closeRef.current(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      // TRM-FR-VJXC: Escape while the submenu is open closes the submenu alone,
      // also when a pointer opened it and the focus is still on Theme.
      if (submenuRef.current) closeSubmenuRef.current();
      else closeRef.current(true);
    };
    document.addEventListener("mousedown", onPointer, true);
    document.addEventListener("contextmenu", onPointer, true);
    document.addEventListener("focusin", onFocus, true);
    window.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("mousedown", onPointer, true);
      document.removeEventListener("contextmenu", onPointer, true);
      document.removeEventListener("focusin", onFocus, true);
      window.removeEventListener("keydown", onKey, true);
    };
  }, []);

  const openSubmenu = (focusFirst: boolean) => {
    const box = entryElement(themeIndex)?.getBoundingClientRect();
    setIndex(themeIndex);
    setSubmenu({ anchor: { left: box?.left ?? x, right: box?.right ?? x + MENU_WIDTH, top: box?.top ?? y }, focusFirst });
  };

  const closeSubmenu = () => {
    setSubmenu(null);
    entryElement(themeIndex)?.focus();
  };
  const closeSubmenuRef = useRef(closeSubmenu);
  closeSubmenuRef.current = closeSubmenu;

  const move = (step: 1 | -1) => {
    setSubmenu(null);
    const at = enabled.indexOf(index);
    setIndex(enabled[(at + step + enabled.length) % enabled.length]);
  };

  // TRM-FR-EUMZ: the menu stays inside the window. Its rendered size decides.
  const [place, setPlace] = useState({ left: x, top: y });
  useLayoutEffect(() => {
    const box = frameRef.current?.getBoundingClientRect();
    const width = box?.width || MENU_WIDTH;
    const height = box?.height ?? 0;
    const left = Math.max(0, Math.min(x, window.innerWidth - width - MARGIN));
    const top = Math.max(0, Math.min(y, window.innerHeight - height - MARGIN));
    setPlace((p) => (p.left === left && p.top === top ? p : { left, top }));
  }, [x, y]);

  return (
    <div
      className="menu terminal-menu"
      role="menu"
      aria-label={`${terminalLabel} actions`}
      ref={frameRef}
      style={{ position: "fixed", left: place.left, top: place.top, zIndex: 200, minWidth: MENU_WIDTH }}
      onKeyDown={(e) => {
        if (e.key === "ArrowDown") {
          e.preventDefault();
          move(1);
        } else if (e.key === "ArrowUp") {
          e.preventDefault();
          move(-1);
        } else if (e.key === "Home") {
          e.preventDefault();
          setSubmenu(null);
          setIndex(enabled[0]);
        } else if (e.key === "End") {
          e.preventDefault();
          setSubmenu(null);
          setIndex(enabled[enabled.length - 1]);
        } else if (e.key === "ArrowRight" && index === themeIndex) {
          e.preventDefault();
          openSubmenu(true);
        } else if (e.key === "Tab") {
          e.preventDefault();
          onClose(true);
        }
      }}
    >
      {entries.map((entry, i) => (
        <div key={entry.id} role="none">
          <button
            type="button"
            role="menuitem"
            className={entry.disabled ? "menu-item menu-item--disabled" : "menu-item"}
            aria-label={entry.label}
            aria-disabled={entry.disabled || undefined}
            aria-haspopup={entry.id === "theme" ? "menu" : undefined}
            aria-expanded={entry.id === "theme" ? submenu !== null : undefined}
            disabled={entry.disabled}
            tabIndex={i === index ? 0 : -1}
            onMouseEnter={() => {
              if (entry.disabled) return;
              setIndex(i);
              // TRM-FR-VJXC: a pointer over the entry opens its submenu.
              if (entry.id === "theme") {
                if (!submenu) openSubmenu(false);
              } else setSubmenu(null);
            }}
            onKeyDown={(e) => {
              if (entry.id === "theme" && e.key === "Enter") {
                e.preventDefault();
                openSubmenu(true);
              }
            }}
            onClick={(e) => {
              if (entry.disabled) return;
              if (entry.id === "theme") {
                // A click by the keyboard (detail 0) is handled on keydown.
                if (e.detail !== 0) openSubmenu(false);
                return;
              }
              onChoose(entry.id);
            }}
          >
            {entry.icon}
            <span>{entry.label}</span>
            {entry.id === "theme" && <Icon.CaretRight size={11} className="menu-item__caret" aria-hidden="true" />}
          </button>
          {entry.separatorAfter && <div className="menu-sep" role="separator" />}
        </div>
      ))}
      {submenu && (
        <TerminalThemeSubmenu
          anchor={submenu.anchor}
          current={theme}
          focusFirst={submenu.focusFirst}
          onChoose={onChooseTheme}
          onClose={closeSubmenu}
        />
      )}
    </div>
  );
}
