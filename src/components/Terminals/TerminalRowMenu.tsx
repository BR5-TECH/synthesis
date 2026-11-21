/**
 * The context menu of a Terminals panel row (`TRM-terminals.md` TRM-FR-EUMZ,
 * TRM-FR-KFCS).
 *
 * Four entries in a fixed order: the pin action, a separator, Close terminal,
 * Close other terminals, Close all terminals. It takes focus when it opens,
 * moves by the arrow keys, chooses on Enter, and closes on a choice, on Escape,
 * and on any activation outside it. Closing it, whichever way, is the caller's
 * cue to put focus back on the row.
 */
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Icon } from "../icons";
import type { CloseScope } from "../../state/terminalGroups";

export type TerminalMenuChoice = "pin" | CloseScope;

interface Entry {
  id: TerminalMenuChoice;
  label: string;
  icon: React.ReactNode;
  disabled: boolean;
  separatorAfter?: boolean;
}

const MENU_WIDTH = 220;
const ENTRY_HEIGHT = 28;

export function TerminalRowMenu({
  x,
  y,
  terminalLabel,
  pinned,
  othersDisabled,
  allDisabled,
  onChoose,
  onClose,
}: {
  x: number;
  y: number;
  terminalLabel: string;
  pinned: boolean;
  othersDisabled: boolean;
  allDisabled: boolean;
  onChoose: (choice: TerminalMenuChoice) => void;
  /** `restoreFocus` is true for Escape, which puts focus back on the row. */
  onClose: (restoreFocus: boolean) => void;
}) {
  const entries: Entry[] = [
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
  const enabled = entries.map((e, i) => (e.disabled ? -1 : i)).filter((i) => i >= 0);
  const [index, setIndex] = useState(0);
  const frameRef = useRef<HTMLDivElement | null>(null);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;

  useLayoutEffect(() => {
    frameRef.current?.querySelectorAll<HTMLElement>('[role="menuitem"]')[index]?.focus();
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
      closeRef.current(true);
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

  const move = (step: 1 | -1) => {
    const at = enabled.indexOf(index);
    setIndex(enabled[(at + step + enabled.length) % enabled.length]);
  };

  const left = Math.max(0, Math.min(x, window.innerWidth - MENU_WIDTH - 8));
  const top = Math.max(0, Math.min(y, window.innerHeight - entries.length * ENTRY_HEIGHT - 24));

  return (
    <div
      className="menu terminal-menu"
      role="menu"
      aria-label={`${terminalLabel} actions`}
      ref={frameRef}
      style={{ position: "fixed", left, top, zIndex: 200, minWidth: MENU_WIDTH }}
      onKeyDown={(e) => {
        if (e.key === "ArrowDown") {
          e.preventDefault();
          move(1);
        } else if (e.key === "ArrowUp") {
          e.preventDefault();
          move(-1);
        } else if (e.key === "Home") {
          e.preventDefault();
          setIndex(enabled[0]);
        } else if (e.key === "End") {
          e.preventDefault();
          setIndex(enabled[enabled.length - 1]);
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
            disabled={entry.disabled}
            tabIndex={i === index ? 0 : -1}
            onMouseEnter={() => {
              if (!entry.disabled) setIndex(i);
            }}
            onClick={() => {
              if (!entry.disabled) onChoose(entry.id);
            }}
          >
            {entry.icon}
            <span>{entry.label}</span>
          </button>
          {entry.separatorAfter && <div className="menu-sep" role="separator" />}
        </div>
      ))}
    </div>
  );
}
