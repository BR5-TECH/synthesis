import { useEffect, useRef, useState } from "react";

import { Icon } from "./icons";
import { TerminalThemeSubmenu } from "./Terminals/TerminalThemeSubmenu";
import type { TabCloseScope, TerminalThemeId } from "../types";

/**
 * TAB-FR-YUOZ: what the menu of a Terminal tab adds before its five entries.
 */
export interface TerminalTabMenu {
  /** TRS-FR-DOMD: the terminal's theme override, or null. */
  theme: TerminalThemeId | null;
  /** TRM-FR-MOVS: the tab's label turns into the label field. */
  onRename: () => void;
  /** TRM-FR-QXIY: a choice in the Theme submenu. */
  onChooseTheme: (theme: TerminalThemeId | null) => void;
}

/**
 * TAB-FR-32: the open tab context menu — which tab it was opened on, and where
 * it is anchored.
 *
 * The anchor is taken from the tab's own box rather than from wherever the
 * pointer landed, so the menu is anchored to the tab (TAB-FR-32) and a menu
 * opened from the keyboard lands in the same place a right-click would put it.
 */
export interface TabMenuState {
  tabId: string;
  x: number;
  y: number;
}

/**
 * TAB-FR-33: the three mass closes, in the fixed order the menu renders them.
 *
 * The two directional entries share one glyph, mirrored, the upright bar
 * standing on the side the surviving tab is on. **Close other tabs** takes a
 * glyph of its own with a bar on both sides, because its survivor has tabs
 * closing either side of it — and because mirroring would have drawn the
 * right-hand entry twice, leaving two of the three indistinguishable.
 */
const MASS_CLOSES: {
  scope: TabCloseScope;
  label: string;
  flip?: boolean;
  both?: boolean;
}[] = [
  { scope: "left", label: "Close tabs to the left", flip: true },
  { scope: "right", label: "Close tabs to the right" },
  { scope: "others", label: "Close other tabs", both: true },
];

/**
 * TAB-FR-32 / TAB-FR-33: the tab context menu.
 *
 * Five entries in one fixed order whatever it was opened on — the pin action,
 * the three mass closes, and **Close tab** — so the entry acting on one tab is
 * never adjacent to the three acting on many, and an author reaches for the
 * same entry in the same place every time. Every entry acts on the tab the menu
 * was opened on and on no other, whether or not that tab is active.
 *
 * The same menu serves the Home affordance (TAB-FR-40), which is a control
 * rather than a tab: there the pin action and **Close tab** name a tab that
 * does not exist and are greyed in place, which is why enablement arrives as
 * two flags rather than being derived from a `Tab` this component may not have.
 */
export function TabContextMenu({
  menu,
  targetName,
  pinned,
  pinDisabled,
  closeDisabled,
  targetsFor,
  onSetPinned,
  onCloseGroup,
  onCloseTab,
  onDismiss,
  terminal,
}: {
  menu: TabMenuState;
  /** What the menu is about, for its accessible name. */
  targetName: string;
  pinned: boolean;
  pinDisabled: boolean;
  closeDisabled: boolean;
  targetsFor: (scope: TabCloseScope) => string[];
  onSetPinned: (pinned: boolean) => void;
  onCloseGroup: (scope: TabCloseScope) => void;
  onCloseTab: () => void;
  /**
   * `restoreFocus` is true only for Escape, which is the one dismissal that
   * owes the author their place back (TAB-FR-32). Focus leaving the menu is
   * itself a dismissal, so restoring focus on that route would undo the move
   * that triggered it.
   */
  onDismiss: (restoreFocus: boolean) => void;
  /** TAB-FR-YUOZ: present on a Terminal tab alone. */
  terminal?: TerminalTabMenu;
}) {
  const ref = useRef<HTMLDivElement>(null);
  /** TRM-FR-QXIY: the open Theme submenu, anchored to the Theme entry. */
  const [submenu, setSubmenu] = useState<{
    anchor: { left: number; right: number; top: number };
    focusFirst: boolean;
  } | null>(null);
  const submenuRef = useRef(submenu);
  submenuRef.current = submenu;

  // TAB-FR-32: the menu closes on Escape, on the pointer or the focus leaving
  // it, and on any activation outside it. Every one of those routes invokes
  // nothing.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // TRM-FR-VJXC: Escape while the Theme submenu is open closes the
      // submenu alone, also when a pointer opened it.
      if (e.key === "Escape" && submenuRef.current) {
        e.stopPropagation();
        closeSubmenuRef.current();
        return;
      }
      if (e.key === "Escape") {
        // Stops at this menu: an Escape the author meant for the menu must not
        // also dismiss whatever stands behind it.
        e.stopPropagation();
        onDismiss(true);
      }
    };
    const onPointerDown = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) onDismiss(false);
    };
    const onFocusIn = (e: FocusEvent) => {
      if (!ref.current?.contains(e.target as Node)) onDismiss(false);
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

  // TAB-FR-32: operable from the keyboard throughout. Focus lands on the menu
  // when it opens so the arrow keys have somewhere to move from.
  useEffect(() => {
    ref.current?.querySelector<HTMLElement>("[data-menu-item]")?.focus();
  }, []);

  /**
   * TAB-FR-32 (layout note): bounded within the window, so a menu opened on a
   * tab near the trailing edge does not overhang it.
   *
   * Measured after mount rather than computed from a width guessed in advance:
   * the entries are text, and how much room five of them need is a question
   * only the rendered box can answer. The menu is nudged back inside along
   * whichever axis overhangs and never pushed off the opposite edge.
   */
  const [clamp, setClamp] = useState<{ left: number; top: number } | null>(null);
  useEffect(() => {
    const box = ref.current?.getBoundingClientRect();
    if (!box) return;
    const margin = 4;
    const left = Math.max(
      margin,
      Math.min(menu.x, window.innerWidth - box.width - margin),
    );
    const top = Math.max(
      margin,
      Math.min(menu.y, window.innerHeight - box.height - margin),
    );
    setClamp(
      left === menu.x && top === menu.y ? null : { left, top },
    );
  }, [menu.x, menu.y]);

  const themeEntry = () => ref.current?.querySelector<HTMLElement>('[data-testid="tabmenu-theme"]') ?? null;

  const openSubmenu = (focusFirst: boolean) => {
    const box = themeEntry()?.getBoundingClientRect();
    setSubmenu({
      anchor: { left: box?.left ?? menu.x, right: box?.right ?? menu.x, top: box?.top ?? menu.y },
      focusFirst,
    });
  };

  const closeSubmenu = () => {
    setSubmenu(null);
    themeEntry()?.focus();
  };
  const closeSubmenuRef = useRef(closeSubmenu);
  closeSubmenuRef.current = closeSubmenu;

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowRight" && terminal && document.activeElement === themeEntry()) {
      e.preventDefault();
      openSubmenu(true);
      return;
    }
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    setSubmenu(null);
    e.preventDefault();
    const items = Array.from(
      ref.current?.querySelectorAll<HTMLElement>("[data-menu-item]") ?? [],
    );
    if (items.length === 0) return;
    const at = items.indexOf(document.activeElement as HTMLElement);
    const step = e.key === "ArrowDown" ? 1 : -1;
    const next = (at + step + items.length) % items.length;
    items[next]?.focus();
  };

  /**
   * The menu closes after an action, and the action is what it closes on
   * (TAB-FR-32).
   *
   * `keepsTab` says whether the context-clicked tab is still in the strip
   * afterwards, which decides where focus lands. Pin and Unpin close nothing
   * (TAB-FR-34) and a mass close never takes the context-clicked tab
   * (TAB-FR-37), so for four of the five entries the tab is still there and
   * focus returns to it — TAB-FR-34's "focus returns where it was", and the
   * difference between a keyboard author keeping their place and being dropped
   * to the document body. **Close tab** is the one entry that removes the tab
   * it was opened on, and there is nothing left to return focus to.
   */
  const choose = (keepsTab: boolean, run: () => void) => {
    onDismiss(keepsTab);
    run();
  };

  const item = (
    key: string,
    label: string,
    icon: React.ReactNode,
    disabled: boolean,
    keepsTab: boolean,
    run: () => void,
  ) => (
    <div
      key={key}
      className={`menu-item${disabled ? " menu-item--disabled" : ""}`}
      data-menu-item
      data-testid={`tabmenu-${key}`}
      role="menuitem"
      aria-disabled={disabled || undefined}
      tabIndex={-1}
      onMouseEnter={() => setSubmenu(null)}
      onClick={() => {
        if (disabled) return;
        choose(keepsTab, run);
      }}
      onKeyDown={(e) => {
        if (e.key !== "Enter" && e.key !== " ") return;
        e.preventDefault();
        if (disabled) return;
        choose(keepsTab, run);
      }}
    >
      {/* TAB-FR-33: every entry carries a leading icon, as the Project panel's
          entries do (LCM-FR-07), so the window's two context menus read as one
          kind of surface. */}
      {icon}
      <span>{label}</span>
    </div>
  );

  return (
    <div
      ref={ref}
      className="menu menu--tab"
      data-testid="tab-context-menu"
      role="menu"
      aria-label={`Actions for ${targetName}`}
      style={{
        position: "fixed",
        left: clamp?.left ?? menu.x,
        top: clamp?.top ?? menu.y,
      }}
      onKeyDown={onKeyDown}
      onClick={(e) => e.stopPropagation()}
    >
      {/* TAB-FR-YUOZ: a Terminal tab adds Rename terminal and Theme, and a
          separator, before the five entries. */}
      {terminal && (
        <>
          {item("rename-terminal", "Rename terminal", <Icon.Tag size={12} aria-hidden="true" />, false, false, terminal.onRename)}
          <div
            className="menu-item"
            data-menu-item
            data-testid="tabmenu-theme"
            role="menuitem"
            aria-haspopup="menu"
            aria-expanded={submenu !== null}
            tabIndex={-1}
            onMouseEnter={(e) => {
              e.currentTarget.focus();
              if (!submenu) openSubmenu(false);
            }}
            onClick={() => openSubmenu(false)}
            onKeyDown={(e) => {
              if (e.key !== "Enter" && e.key !== " ") return;
              e.preventDefault();
              openSubmenu(true);
            }}
          >
            <Icon.Palette size={12} aria-hidden="true" />
            <span>Theme</span>
            <Icon.CaretRight size={10} className="menu-item__caret" aria-hidden="true" />
          </div>
          {submenu && (
            <TerminalThemeSubmenu
              anchor={submenu.anchor}
              current={terminal.theme}
              focusFirst={submenu.focusFirst}
              onChoose={(theme) => choose(true, () => terminal.onChooseTheme(theme))}
              onClose={closeSubmenu}
            />
          )}
          <div className="menu-sep" role="separator" />
        </>
      )}
      {/* TAB-FR-33: the pin action is **Pin tab** on an unpinned tab and
          **Unpin tab** on a pinned one; exactly one of the two renders and
          never both. TAB-FR-34: it is never disabled on a tab — the Home
          affordance is the one place it is, having no tab to pin (TAB-FR-40). */}
      {item(
        pinned ? "unpin" : "pin",
        pinned ? "Unpin tab" : "Pin tab",
        pinned ? (
          <Icon.Unpin size={12} aria-hidden="true" />
        ) : (
          <Icon.Pin size={12} aria-hidden="true" />
        ),
        pinDisabled,
        true,
        () => onSetPinned(!pinned),
      )}
      <div className="menu-sep" role="separator" />
      {MASS_CLOSES.map(({ scope, label, flip, both }) =>
        // TAB-FR-39: disabled exactly while its own eligible set is empty —
        // a statement about which tabs are pinned, never about the clicked
        // tab's position in the strip.
        item(
          scope,
          label,
          both ? (
            <Icon.CloseOthers size={12} aria-hidden="true" />
          ) : (
            <Icon.CloseSide
              size={12}
              aria-hidden="true"
              style={flip ? { transform: "scaleX(-1)" } : undefined}
            />
          ),
          targetsFor(scope).length === 0,
          true,
          () => onCloseGroup(scope),
        ),
      )}
      <div className="menu-sep" role="separator" />
      {/* TAB-FR-36: closes the context-clicked tab alone, pinned or not, down
          the same path its own close control takes. Disabled in two places: on
          the Dashboard while it is the only open tab (TAB-FR-16), and on the
          Home affordance, which is no tab to close (TAB-FR-40). */}
      {item(
        "close",
        "Close tab",
        <Icon.X size={12} aria-hidden="true" />,
        closeDisabled,
        false,
        onCloseTab,
      )}
    </div>
  );
}
