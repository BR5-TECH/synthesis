import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

import { Icon } from "./icons";

/** Space kept between the menu and a window edge (PPK-FR-VEID). */
const EDGE_MARGIN = 4;

/**
 * PPK-FR-SDXU: the open recent entry menu — the entry it acts on, and the point
 * it opens at. A pointer opening gives the pointer; a keyboard opening gives a
 * point on the focused entry (PPK-FR-VEID).
 */
export interface RecentMenuState {
  path: string;
  x: number;
  y: number;
  /**
   * The vertical point the menu flips to when it does not fit below `y`. A
   * keyboard opening gives the top of the entry, so that a flipped menu does
   * not cover the entry it acts on. Without it, the menu flips at `y`.
   */
  flipY?: number;
}

/** The action a menu entry starts (PPK-FR-MHJD). */
export type RecentMenuAction = "pin" | "unpin" | "hide";

/**
 * PPK-FR-VEID: the position that keeps a box of `size` fully inside `limit`.
 *
 * The menu opens at `at`. When it overhangs the far edge, it flips to the other
 * side of `flipAt` (by default `at`). When it still overhangs, it shifts back
 * inside, and it never goes past the near edge.
 */
export function placeOnAxis(
  at: number,
  size: number,
  limit: number,
  flipAt: number = at,
): number {
  let start = at;
  if (start + size > limit - EDGE_MARGIN) start = flipAt - size;
  const max = limit - size - EDGE_MARGIN;
  return Math.max(EDGE_MARGIN, Math.min(start, max));
}

/**
 * PPK-FR-MHJD / PPK-FR-FINT / PPK-FR-VFDK / PPK-FR-VEID: the context menu of
 * one recent entry in the Project picker.
 *
 * Two entries in a fixed order: the pin action (**Pin** or **Unpin**), then
 * **Hide**. Each has a leading icon, as every other context menu in the
 * application has (`LCM-library-context-menu.md` LCM-FR-07).
 *
 * The menu goes into a portal on `document.body` with a fixed position, so the
 * scroll region of the recent list and the `overflow: hidden` of the picker
 * frame do not clip it.
 */
export function RecentProjectMenu({
  menu,
  name,
  pinned,
  onChoose,
  onDismiss,
}: {
  menu: RecentMenuState;
  /** The project name, for the accessible name of the menu. */
  name: string;
  pinned: boolean;
  onChoose: (action: RecentMenuAction) => void;
  /**
   * `restoreFocus` is true for Escape only. A press outside the menu or focus
   * that leaves it already puts the user somewhere else (PPK-FR-VFDK).
   */
  onDismiss: (restoreFocus: boolean) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [place, setPlace] = useState<{ left: number; top: number } | null>(
    null,
  );

  // PPK-FR-VFDK: Escape, a press outside the menu, and focus that leaves the
  // menu close it and start no action.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopPropagation();
      onDismiss(true);
    };
    const outside = (target: EventTarget | null) =>
      !ref.current?.contains(target as Node | null);
    const onPointerDown = (e: MouseEvent) => {
      if (outside(e.target)) onDismiss(false);
    };
    const onFocusIn = (e: FocusEvent) => {
      if (outside(e.target)) onDismiss(false);
    };
    document.addEventListener("keydown", onKey, true);
    document.addEventListener("mousedown", onPointerDown, true);
    document.addEventListener("focusin", onFocusIn, true);
    return () => {
      document.removeEventListener("keydown", onKey, true);
      document.removeEventListener("mousedown", onPointerDown, true);
      document.removeEventListener("focusin", onFocusIn, true);
    };
  }, [onDismiss]);

  // PPK-FR-VEID: measure the rendered box, then flip or shift it inside the
  // window. A layout effect runs before paint, so the menu never paints at the
  // unplaced position. The entries are text, so only the rendered box knows its
  // size.
  useLayoutEffect(() => {
    const box = ref.current?.getBoundingClientRect();
    if (!box) return;
    setPlace({
      left: placeOnAxis(menu.x, box.width, window.innerWidth),
      top: placeOnAxis(menu.y, box.height, window.innerHeight, menu.flipY),
    });
  }, [menu.x, menu.y, menu.flipY]);

  // PPK-FR-FINT: focus moves to the first entry when the menu opens.
  useEffect(() => {
    ref.current?.querySelector<HTMLElement>("[data-menu-item]")?.focus();
  }, [menu.path]);

  // PPK-FR-FINT: the Up and Down arrow keys move between the entries and wrap.
  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    const items = Array.from(
      ref.current?.querySelectorAll<HTMLElement>("[data-menu-item]") ?? [],
    );
    if (items.length === 0) return;
    const at = items.indexOf(document.activeElement as HTMLElement);
    const step = e.key === "ArrowDown" ? 1 : -1;
    items[(at + step + items.length) % items.length]?.focus();
  };

  const item = (
    action: RecentMenuAction,
    label: string,
    icon: React.ReactNode,
  ) => (
    <div
      key={action}
      className="menu-item"
      data-menu-item
      data-testid={`recent-menu-${action}`}
      role="menuitem"
      tabIndex={-1}
      onClick={() => onChoose(action)}
      onKeyDown={(e) => {
        if (e.key !== "Enter" && e.key !== " ") return;
        e.preventDefault();
        onChoose(action);
      }}
    >
      {icon}
      <span>{label}</span>
    </div>
  );

  return createPortal(
    <div
      ref={ref}
      className="menu menu--recent"
      data-testid="recent-context-menu"
      role="menu"
      aria-label={`Actions for ${name}`}
      style={{
        position: "fixed",
        left: place?.left ?? menu.x,
        top: place?.top ?? menu.y,
      }}
      onKeyDown={onKeyDown}
      onContextMenu={(e) => e.preventDefault()}
    >
      {/* PPK-FR-MHJD: exactly one of Pin and Unpin renders, then Hide. */}
      {pinned
        ? item("unpin", "Unpin", <Icon.Unpin size={12} aria-hidden="true" />)
        : item("pin", "Pin", <Icon.Pin size={12} aria-hidden="true" />)}
      {item("hide", "Hide", <Icon.Hide size={12} aria-hidden="true" />)}
    </div>,
    document.body,
  );
}
