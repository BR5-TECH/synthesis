import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { ReactNode } from "react";

export interface BranchMenuEntry {
  id: string;
  label: string;
  /** LCM-FR-07: every entry carries a leading icon. */
  icon: ReactNode;
  danger?: boolean;
  disabled?: boolean;
}

const ENTRY_HEIGHT = 28;
const MENU_WIDTH = 200;

/**
 * GIT-FR-ZEKI: a branch row's context menu.
 *
 * It follows the look and the dismissal rules of the Project panel's menu
 * (LCM-FR-07): the shared `.menu` frame, a leading icon on every entry, and
 * dismissal by Escape or a press outside it. It takes focus when it opens and
 * moves by arrow keys. Closing it, whichever way, is the caller's cue to put
 * focus back on the row.
 */
export function BranchMenu({
  x,
  y,
  label,
  entries,
  onChoose,
  onClose,
}: {
  x: number;
  y: number;
  label: string;
  entries: BranchMenuEntry[];
  onChoose: (id: string) => void;
  onClose: () => void;
}) {
  const frameRef = useRef<HTMLDivElement | null>(null);
  const [index, setIndex] = useState(0);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;

  const enabled = entries.map((e, i) => (e.disabled ? -1 : i)).filter((i) => i >= 0);

  // Focus follows the active entry, which is how the menu takes focus when it
  // opens and how the arrow keys move it.
  useLayoutEffect(() => {
    frameRef.current
      ?.querySelectorAll<HTMLElement>('[role="menuitem"]')
      [index]?.focus();
  }, [index]);

  useEffect(() => {
    const onPointer = (e: MouseEvent) => {
      const target = e.target as Node | null;
      if (!target || !frameRef.current?.contains(target)) closeRef.current();
    };
    // Escape also works when a press on the menu's padding took focus off it.
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") closeRef.current();
    };
    window.addEventListener("mousedown", onPointer);
    window.addEventListener("click", onPointer);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onPointer);
      window.removeEventListener("click", onPointer);
      window.removeEventListener("keydown", onKey);
    };
  }, []);

  const move = (step: 1 | -1) => {
    if (enabled.length === 0) return;
    const at = enabled.indexOf(index);
    const next = enabled[(at + step + enabled.length) % enabled.length];
    setIndex(next);
  };

  const left = Math.max(0, Math.min(x, window.innerWidth - MENU_WIDTH - 8));
  const top = Math.max(
    0,
    Math.min(y, window.innerHeight - entries.length * ENTRY_HEIGHT - 16),
  );

  return (
    <div
      className="menu git-menu"
      role="menu"
      aria-label={label}
      ref={frameRef}
      style={{ position: "fixed", left, top, zIndex: 200 }}
      onKeyDown={(e) => {
        if (e.key === "ArrowDown") {
          e.preventDefault();
          move(1);
        } else if (e.key === "ArrowUp") {
          e.preventDefault();
          move(-1);
        } else if (e.key === "Home") {
          e.preventDefault();
          if (enabled.length) setIndex(enabled[0]);
        } else if (e.key === "End") {
          e.preventDefault();
          if (enabled.length) setIndex(enabled[enabled.length - 1]);
        } else if (e.key === "Escape" || e.key === "Tab") {
          e.preventDefault();
          e.stopPropagation();
          onClose();
        }
      }}
    >
      {entries.map((entry, i) => (
        <button
          key={entry.id}
          type="button"
          role="menuitem"
          className={
            entry.danger
              ? "menu-item menu-item--danger git-menu__item"
              : "menu-item git-menu__item"
          }
          tabIndex={i === index ? 0 : -1}
          disabled={entry.disabled}
          onMouseEnter={() => {
            if (!entry.disabled) setIndex(i);
          }}
          onClick={() => onChoose(entry.id)}
        >
          {entry.icon}
          <span>{entry.label}</span>
        </button>
      ))}
    </div>
  );
}
