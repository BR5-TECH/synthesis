/**
 * MDL-FR-MIMF / MDL-FR-BWEF: the status filter of the Delivery lens, in the
 * toolbar after the progress track. It is one **Status** button that opens a
 * menu, so the toolbar keeps its width. A status toggle is checked while the
 * board shows that status. **Blocked only** keeps the blocked items alone.
 */
import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";

import type { DeliveryBoardStatus, DeliveryView } from "../../types";
import { Icon } from "../icons";

/** DBR-FR-QZPA: the board statuses, in the order of the board. */
export const BOARD_STATUSES: readonly DeliveryBoardStatus[] = [
  "backlog",
  "ready",
  "in progress",
  "in review",
  "done",
];

export interface StatusFilterProps {
  view: DeliveryView;
  /** MDL-FR-BWEF: the count of the cards and groups the filter hides. */
  hiddenCount: number;
  /** MDL layout notes: below a tab width of 900px the button shows its count
   * and chevron alone. */
  compact?: boolean;
  onChange: (next: DeliveryView) => void;
}

const ITEM_SELECTOR = '[role="menuitemcheckbox"], [role="menuitem"]';

/** The width of the menu, for its place. */
const MENU_WIDTH = 190;
const EDGE = 8;

/** The place of the menu: below the button, and never past the right edge of
 * the window. The toolbar clips what overflows its height, so the menu is
 * placed in the viewport. */
function placeBelow(
  button: HTMLElement | null,
): { top: number; left: number } | null {
  if (!button) return null;
  const rect = button.getBoundingClientRect();
  const left = Math.max(
    EDGE,
    Math.min(rect.left, window.innerWidth - MENU_WIDTH - EDGE),
  );
  return { top: rect.bottom + 4, left };
}

export function StatusFilter({
  view,
  hiddenCount,
  compact = false,
  onChange,
}: StatusFilterProps) {
  const hidden = new Set(view.hidden);
  const [open, setOpen] = useState(false);
  const [place, setPlace] = useState<{ top: number; left: number } | null>(
    null,
  );
  const root = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);

  const close = (refocus: boolean) => {
    setOpen(false);
    if (refocus) button.current?.focus();
  };

  useLayoutEffect(() => {
    if (open) setPlace(placeBelow(button.current));
  }, [open]);

  useEffect(() => {
    if (!open) return;
    root.current?.querySelector<HTMLElement>(ITEM_SELECTOR)?.focus();
    const onDown = (e: PointerEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false);
    };
    // The menu stays where the button was; a resize closes it.
    const onResize = () => setOpen(false);
    window.addEventListener("pointerdown", onDown);
    window.addEventListener("resize", onResize);
    return () => {
      window.removeEventListener("pointerdown", onDown);
      window.removeEventListener("resize", onResize);
    };
  }, [open]);

  const toggle = (status: DeliveryBoardStatus) => {
    const next = new Set(hidden);
    if (next.has(status)) next.delete(status);
    else next.add(status);
    onChange({ ...view, hidden: BOARD_STATUSES.filter((s) => next.has(s)) });
  };

  const onMenuKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      close(true);
      return;
    }
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    const items = [
      ...(root.current?.querySelectorAll<HTMLElement>(ITEM_SELECTOR) ?? []),
    ];
    const at = items.indexOf(document.activeElement as HTMLElement);
    const step = e.key === "ArrowDown" ? 1 : -1;
    items[(at + step + items.length) % items.length]?.focus();
  };

  return (
    <div
      ref={root}
      className="dlens-status"
      data-testid="delivery-status-filter"
    >
      <button
        ref={button}
        type="button"
        className="btn btn--sm dlens-status__toggle"
        aria-haspopup="menu"
        aria-expanded={open}
        title="Filter the board by status"
        onClick={() => setOpen((o) => !o)}
      >
        <span className={compact ? "sr-only" : undefined}>Status</span>
        {hiddenCount > 0 && (
          <span
            className="dlens-status__count"
            data-testid="delivery-hidden-count"
          >
            {hiddenCount} hidden
          </span>
        )}
        <Icon.Caret size={12} aria-hidden="true" />
      </button>
      {open && (
        <div
          className="menu dlens-status__menu"
          role="menu"
          aria-label="Status filter"
          style={place ? { top: place.top, left: place.left } : undefined}
          onKeyDown={onMenuKey}
        >
          {BOARD_STATUSES.map((status) => {
            const shown = !hidden.has(status);
            return (
              <button
                key={status}
                type="button"
                role="menuitemcheckbox"
                aria-checked={shown}
                className="menu-item dlens-status__item"
                data-status={status}
                onClick={() => toggle(status)}
              >
                <span className="dlens-status__check" aria-hidden="true">
                  {shown && <Icon.Check size={12} />}
                </span>
                <i className="dm-dot" data-s={status} aria-hidden="true" />
                <span>{status}</span>
              </button>
            );
          })}
          <div className="menu-sep" role="separator" />
          <button
            type="button"
            role="menuitemcheckbox"
            aria-checked={view.blockedOnly}
            className="menu-item dlens-status__item"
            data-filter="blocked"
            onClick={() =>
              onChange({ ...view, blockedOnly: !view.blockedOnly })
            }
          >
            <span className="dlens-status__check" aria-hidden="true">
              {view.blockedOnly && <Icon.Check size={12} />}
            </span>
            <span>Blocked only</span>
          </button>
          {hiddenCount > 0 && (
            <button
              type="button"
              role="menuitem"
              className="menu-item dlens-status__item"
              onClick={() => {
                onChange({ hidden: [], blockedOnly: false });
                close(true);
              }}
            >
              <span className="dlens-status__check" aria-hidden="true" />
              <span>Show all</span>
            </button>
          )}
        </div>
      )}
    </div>
  );
}
