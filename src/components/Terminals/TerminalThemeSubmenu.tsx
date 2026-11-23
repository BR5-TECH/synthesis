/**
 * The Theme submenu of a terminal (`../../../specifications/ui/TRM-terminals.md`
 * TRM-FR-QXIY, TRM-FR-MOVS): **Use default** and the five themes, the current
 * one checked. The row menu of the Terminals panel and the tab menu of a
 * terminal tab open it beside their **Theme** entry.
 *
 * The caller renders it inside its own menu element, so a click or a focus in
 * the submenu is no activation outside the menu.
 */
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { TERMINAL_THEMES } from "../../state/terminalThemes";
import type { TerminalThemeId } from "../../types";
import { Icon } from "../icons";

const SUBMENU_WIDTH = 180;
const MARGIN = 8;

interface Item {
  value: TerminalThemeId | null;
  label: string;
  swatch?: { background: string; foreground: string };
}

const ITEMS: Item[] = [
  { value: null, label: "Use default" },
  ...TERMINAL_THEMES.map((t) => ({
    value: t.id,
    label: t.name,
    swatch: { background: t.palette.background, foreground: t.palette.foreground },
  })),
];

export function TerminalThemeSubmenu({
  anchor,
  current,
  focusFirst,
  onChoose,
  onClose,
}: {
  /** The box of the **Theme** entry that opened the submenu. */
  anchor: { left: number; right: number; top: number };
  /** The terminal's override, or null for none. */
  current: TerminalThemeId | null;
  /** The keyboard opened the submenu, so the focus goes to its first entry. */
  focusFirst: boolean;
  onChoose: (theme: TerminalThemeId | null) => void;
  /** Left or Escape: the focus goes back to the **Theme** entry. */
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement | null>(null);
  const [index, setIndex] = useState<number | null>(focusFirst ? 0 : null);
  const [place, setPlace] = useState<{ left: number; top: number }>({ left: anchor.right, top: anchor.top - 4 });

  useLayoutEffect(() => {
    if (index === null) return;
    ref.current?.querySelectorAll<HTMLElement>('[role="menuitemradio"]')[index]?.focus();
  }, [index]);

  // TRM-FR-VJXC: a key that opens a submenu that a pointer opened already puts
  // the focus on its first entry.
  useEffect(() => {
    if (focusFirst) setIndex((i) => i ?? 0);
  }, [focusFirst]);

  // TRM-FR-VJXC: the submenu opens on the trailing side, or on the leading side
  // where the window has no space, and stays inside the window. Its rendered
  // size decides, not an estimate.
  useLayoutEffect(() => {
    const box = ref.current?.getBoundingClientRect();
    const width = box?.width || SUBMENU_WIDTH;
    const height = box?.height ?? 0;
    const fitsRight = anchor.right + width + MARGIN <= window.innerWidth;
    const left = fitsRight ? anchor.right : Math.max(0, anchor.left - width);
    const top = Math.max(0, Math.min(anchor.top - 4, window.innerHeight - height - MARGIN));
    setPlace((p) => (p.left === left && p.top === top ? p : { left, top }));
  }, [anchor.left, anchor.right, anchor.top]);

  const move = (step: number) => setIndex((i) => ((i ?? -1) + step + ITEMS.length) % ITEMS.length);

  return (
    <div
      ref={ref}
      className="menu terminal-menu terminal-theme-menu"
      role="menu"
      aria-label="Terminal theme"
      data-terminal-theme-menu
      style={{ position: "fixed", left: place.left, top: place.top, zIndex: 210, minWidth: SUBMENU_WIDTH }}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "ArrowDown") {
          e.preventDefault();
          move(1);
        } else if (e.key === "ArrowUp") {
          e.preventDefault();
          move(-1);
        } else if (e.key === "Home") {
          e.preventDefault();
          setIndex(0);
        } else if (e.key === "End") {
          e.preventDefault();
          setIndex(ITEMS.length - 1);
        } else if (e.key === "ArrowLeft" || e.key === "Escape") {
          e.preventDefault();
          onClose();
        }
      }}
    >
      {ITEMS.map((item, i) => {
        const checked = item.value === current;
        return (
          <button
            key={item.value ?? "default"}
            type="button"
            role="menuitemradio"
            aria-checked={checked}
            className="menu-item terminal-theme-menu__item"
            tabIndex={i === (index ?? 0) ? 0 : -1}
            onMouseEnter={() => setIndex(i)}
            onClick={() => onChoose(item.value)}
          >
            <span className="terminal-theme-menu__check" aria-hidden="true">
              {checked && <Icon.Check size={12} />}
            </span>
            {item.swatch ? (
              <span
                className="terminal-theme-swatch"
                aria-hidden="true"
                style={{ background: item.swatch.background, color: item.swatch.foreground }}
              >
                Aa
              </span>
            ) : (
              <span className="terminal-theme-swatch terminal-theme-swatch--default" aria-hidden="true" />
            )}
            <span>{item.label}</span>
          </button>
        );
      })}
    </div>
  );
}
