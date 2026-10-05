import { useCallback, useEffect, useRef, useState } from "react";

import { Icon } from "./icons";
import type { Tab, TabCloseScope } from "../types";
import { HOME_TAB_TARGET } from "../types";

/**
 * NTF-FR-35: how a tab's needs-attention emphasis is drawn right now. `"pulse"`
 * is the bounded arrival pulse; `"on"` is the static emphasis it settles into
 * and that stands until the indication is cleared.
 */
export type TabAttention = "pulse" | "on";

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

interface TabStripProps {
  tabs: Tab[];
  activeId: string;
  onActivate: (id: string) => void;
  onClose: (id: string) => void;
  onHome: () => void;
  /**
   * TAB-FR-26 / NTF-FR-26: the tabs needing attention, by tab id.
   *
   * The state itself belongs to the notifications facility and is keyed by
   * address there (NTF-FR-30); the strip is handed the mapping onto the tabs it
   * is currently rendering and draws it, deciding none of it. The emphasis is
   * presentation and nothing more: it changes no closure, single-tab, focus, or
   * never-empty rule, and no tab is opened, closed, moved, or focused on account
   * of one appearing or going.
   */
  attention?: ReadonlyMap<string, TabAttention>;
  /**
   * TAB-FR-35: which tabs are pinned, by tab id. Session state owned by the
   * shell; the strip draws it and the menu below changes it, and neither
   * persists any of it.
   */
  pinned?: ReadonlySet<string>;
  /** TAB-FR-34: pin or unpin the context-clicked tab. */
  onSetPinned?: (id: string, pinned: boolean) => void;
  /** TAB-FR-38: run one of the three mass closes on the context-clicked tab. */
  onCloseGroup?: (scope: TabCloseScope, targetId: string) => void;
  /**
   * TAB-FR-39: the tabs a given mass close would take. The menu asks rather
   * than re-deriving, so an entry is enabled by exactly the rule that will run
   * if the author chooses it.
   */
  massCloseTargets?: (scope: TabCloseScope, targetId: string) => string[];
  /**
   * TAB-FR-32 / SNV-FR-56: the menu's open state, owned by the shell.
   *
   * Held above this component for the reason the agents roster's is: mutual
   * exclusion is the *window's* rule, and a menu that closed only on its own
   * outside-pointer-down would stay mounted beside a chrome dropdown opened
   * from the keyboard, which raises no pointer event at all.
   */
  menu?: TabMenuState | null;
  onMenuChange?: (menu: TabMenuState | null) => void;
  /** SNV-FR-56: opening the menu closes every other floating overlay. */
  onOverlayOpening?: () => void;
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

export function TabStrip({
  tabs,
  activeId,
  onActivate,
  onClose,
  onHome,
  attention,
  pinned,
  onSetPinned,
  onCloseGroup,
  massCloseTargets,
  menu,
  onMenuChange,
  onOverlayOpening,
}: TabStripProps) {
  // SNV-FR-09: the Home affordance stands in the Dashboard tab's own place at
  // the head of the strip, and only while that tab is closed — so the Dashboard
  // is presented exactly once, as the affordance or as the tab, never as both.
  // It is a control of the strip rather than a tab: no label, no close control.
  const dashboardOpen = tabs.some((t) => t.id === "dashboard");

  /**
   * TAB-FR-27 / TAB-FR-28: the scrolling region.
   *
   * The tabs alone live in it. The Home affordance sits outside it at the
   * strip's head, so it stays reachable at every scroll position (TAB-FR-29)
   * rather than being the first thing scrolled out of reach — and so the
   * `tablist` holds tabs and nothing else.
   */
  const scrollRef = useRef<HTMLDivElement>(null);
  /**
   * The whole strip, scrolling region and Home affordance together.
   *
   * Focus restoration (TAB-FR-32) has to reach either, and the affordance sits
   * outside the region by TAB-FR-29 — so a lookup scoped to the region alone
   * would silently drop focus on Escape from the affordance's own menu.
   */
  const stripRef = useRef<HTMLDivElement>(null);

  /**
   * TAB-FR-30: bring a tab wholly into the region.
   *
   * `scrollIntoView` on the element rather than on the container so the browser
   * picks the smallest scroll that reveals it: a tab already fully visible is
   * left exactly where it is, which is the common case and must not jump.
   */
  const reveal = useCallback((el: HTMLElement | null | undefined) => {
    // jsdom implements neither scrollIntoView options nor layout, so guard the
    // call rather than let a test environment throw on it.
    el?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
  }, []);

  /**
   * A tab the user has just opened being off-screen with nothing pointing at it
   * is indistinguishable from it not having opened, so the active tab is
   * revealed whenever it changes or the set of tabs does.
   */
  useEffect(() => {
    const active = scrollRef.current?.querySelector<HTMLElement>(
      '[data-active="true"]',
    );
    reveal(active);
  }, [activeId, tabs.length, reveal]);

  /**
   * TAB-FR-28: the wheel scrolls the strip along its one axis whichever axis it
   * reports.
   *
   * The strip renders no scrollbar, so the wheel is how a pointer reaches the
   * far end of it — and a strip that answered only `deltaX` would be
   * unreachable on every mouse that reports vertical movement alone. A
   * trackpad's horizontal gesture arrives as `deltaX` and is honoured too, so
   * the larger of the two wins rather than one being hard-coded.
   *
   * Registered here rather than as an `onWheel` prop because React attaches its
   * wheel listeners passively, and a passive listener cannot call
   * `preventDefault` — without which a vertical wheel over the strip scrolls
   * the strip *and* whatever lies behind it.
   */
  useEffect(() => {
    const region = scrollRef.current;
    if (!region) return;
    const onWheel = (e: WheelEvent) => {
      if (e.ctrlKey) return; // a pinch-zoom gesture, not a scroll
      const delta =
        Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
      if (delta === 0) return;
      // Nothing to scroll: leave the event alone so the gesture reaches
      // whatever else would have handled it.
      if (region.scrollWidth <= region.clientWidth) return;
      e.preventDefault();
      region.scrollLeft += delta;
    };
    region.addEventListener("wheel", onWheel, { passive: false });
    return () => region.removeEventListener("wheel", onWheel);
  }, []);

  /**
   * TAB-FR-30: the strip takes one tab stop, and the arrow keys move a focus
   * ring across it from there.
   *
   * `roving` names the tab currently holding that stop. It follows the active
   * tab until the author moves it, which is what makes Tab land on the tab they
   * are reading rather than on wherever the ring was left in a previous visit.
   */
  const [roving, setRoving] = useState<string | null>(null);
  const rovingId = roving && tabs.some((t) => t.id === roving) ? roving : activeId;
  useEffect(() => {
    setRoving(null);
  }, [activeId]);

  const focusTab = (id: string) => {
    setRoving(id);
    // Matched against the dataset rather than through an attribute selector: a
    // tab id is a project-relative path or a `kind:comparison:path` triple
    // (TAB-FR-04, DFV-FR-04), so it carries `/` and `:` freely and would have
    // to be escaped into a selector before it could be one.
    const el = Array.from(
      stripRef.current?.querySelectorAll<HTMLElement>("[data-tab-id]") ?? [],
    ).find((n) => n.dataset.tabId === id);
    el?.focus();
    reveal(el);
  };

  const openMenuFor = (id: string, anchor: HTMLElement | null) => {
    if (!onMenuChange) return;
    // SNV-FR-56: every other floating overlay closes first, then this one opens.
    onOverlayOpening?.();
    const box = anchor?.getBoundingClientRect();
    onMenuChange({
      tabId: id,
      x: box?.left ?? 0,
      y: box?.bottom ?? 0,
    });
  };

  const onTabKeyDown = (e: React.KeyboardEvent<HTMLDivElement>, id: string) => {
    const index = tabs.findIndex((t) => t.id === id);
    if (index < 0) return;
    // TAB-FR-32: the platform's keyboard route to a context menu — the
    // dedicated key, and the Shift+F10 that stands in for it on keyboards
    // without one.
    if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) {
      e.preventDefault();
      openMenuFor(id, e.currentTarget);
      return;
    }
    // Moving focus does NOT activate (TAB-FR-30): a tab is reached and then
    // chosen, so traversing a strip of twenty does not activate nineteen of
    // them on the way.
    if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
      e.preventDefault();
      const step = e.key === "ArrowRight" ? 1 : -1;
      const next = tabs[index + step];
      if (next) focusTab(next.id);
      return;
    }
    if (e.key === "Home" || e.key === "End") {
      e.preventDefault();
      const next = e.key === "Home" ? tabs[0] : tabs[tabs.length - 1];
      if (next) focusTab(next.id);
      return;
    }
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onActivate(id);
    }
  };

  /**
   * NTF-FR-36: taking the needs-attention state and losing it are each announced
   * once per change.
   *
   * Announced from the *set* of marked tabs rather than from each raise, which
   * is what makes it once per change rather than once per key — several raises
   * against one tab render one emphasis (NTF-FR-31) and say so once. A tab that
   * left the strip entirely is not announced as no longer needing attention:
   * the author closed it, and a tab that is gone is not news.
   */
  const [announcement, setAnnouncement] = useState("");
  const announcedRef = useRef<ReadonlySet<string>>(new Set<string>());
  const attentionKey = attention
    ? [...attention.keys()].sort().join("\u0000")
    : "";
  useEffect(() => {
    const now = new Set(attention ? [...attention.keys()] : []);
    const previous = announcedRef.current;
    announcedRef.current = now;

    const open = new Map(tabs.map((t) => [t.id, t.label]));
    const gained = [...now]
      .filter((id) => !previous.has(id) && open.has(id))
      .map((id) => open.get(id)!);
    const lost = [...previous]
      .filter((id) => !now.has(id) && open.has(id))
      .map((id) => open.get(id)!);

    const parts: string[] = [];
    if (gained.length > 0) {
      parts.push(
        `${gained.join(", ")} ${gained.length === 1 ? "needs" : "need"} attention`,
      );
    }
    if (lost.length > 0) {
      parts.push(
        `${lost.join(", ")} no longer ${lost.length === 1 ? "needs" : "need"} attention`,
      );
    }
    if (parts.length > 0) setAnnouncement(parts.join("; "));
    // Keyed on the marked set alone: a re-render that changed no tab's
    // attention must not re-announce what the author has already been told.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [attentionKey]);

  /**
   * TAB-FR-40: the menu is opened on a tab, or on the Home affordance.
   *
   * The affordance is a control rather than a tab (per `SNV-shell-navigation.md`
   * SNV-FR-09), so it has no `Tab` record to be found — `onHome` stands in for
   * one, carrying only what the menu needs: a name for its accessible label.
   */
  const menuOnHome = menu?.tabId === HOME_TAB_TARGET;
  const menuTab = menu && !menuOnHome
    ? tabs.find((t) => t.id === menu.tabId)
    : undefined;
  // The affordance is rendered only while the Dashboard tab is closed, so a
  // menu open on it is dropped if the Dashboard reappears beneath it.
  const menuTarget = menuOnHome && !dashboardOpen ? "Home" : menuTab?.label;
  /**
   * A menu whose tab has left the strip is dropped rather than merely hidden.
   *
   * The context-clicked tab can be taken out from under an open menu by an
   * automatic closure — a removed path (TAB-FR-19) or a commit (TAB-FR-22).
   * Rendering already stops at `menuTab`, but state left standing would put the
   * menu back at its old coordinates the moment that same path was reopened
   * under the same id, unbidden and anchored to nothing the author pointed at.
   */
  useEffect(() => {
    if (menu && menuTarget === undefined) onMenuChange?.(null);
  }, [menu, menuTarget, onMenuChange]);

  return (
    <>
      {/* Outside the strip rather than within it: the region below is a flex
          row of tabs, and every reader of it — the scroll-into-view query above
          included — is entitled to assume its children are tabs. */}
      <div className="sr-only" role="status" aria-live="polite">
        {announcement}
      </div>
      <div className="tabstrip" data-testid="tabstrip" ref={stripRef}>
        {!dashboardOpen && (
          <div
            className="tab tab--home"
            data-testid="home-affordance"
            data-tab-id={HOME_TAB_TARGET}
            role="button"
            tabIndex={0}
            onClick={onHome}
            // TAB-FR-40: the head of the strip answers a right-click too, with
            // the same menu the tabs carry — so the author does not have to
            // aim around the one thing standing where a tab used to be. It
            // answers the keyboard route on the same terms (TAB-FR-32), which
            // is why it takes a tab stop of its own: it is a control beside the
            // tablist rather than a member of it, so this is not a second stop
            // inside the one TAB-FR-30 gives the strip.
            onContextMenu={(e) => {
              e.preventDefault();
              openMenuFor(HOME_TAB_TARGET, e.currentTarget);
            }}
            onKeyDown={(e) => {
              if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) {
                e.preventDefault();
                openMenuFor(HOME_TAB_TARGET, e.currentTarget);
                return;
              }
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                onHome();
              }
            }}
            aria-label="Home — open Dashboard"
            title="Home — open Dashboard"
          >
            <Icon.Home size={12} />
          </div>
        )}
        {/* TAB-FR-27 / TAB-FR-28: one row, scrolled rather than wrapped, and
            clipped rather than given a scrollbar. */}
        <div
          className="tabstrip__scroll"
          data-testid="tabstrip-scroll"
          role="tablist"
          aria-label="Open tabs"
          ref={scrollRef}
        >
          {tabs.map((t) => {
            // TAB-FR-16 / DSH-FR-02: the Dashboard's close control is active
            // only while at least one other tab is open. Alone in the strip it
            // renders greyed-out and closes nothing, so the strip is never
            // emptied by closing the Dashboard itself — the one case the
            // fallback of TAB-FR-15 would otherwise answer by reopening what
            // was just closed. Every other tab's close control is
            // unconditional, a pinned tab's included (TAB-FR-36): closing the
            // last of *those* is what brings the Dashboard back.
            const closeInert = t.id === "dashboard" && tabs.length === 1;
            // NTF-FR-26: the active tab is never marked. Enforced at the raise
            // (NTF-FR-29) and again here, because the strip is where a tab
            // becoming active and its indication clearing are one frame apart.
            const mark = t.id === activeId ? undefined : attention?.get(t.id);
            const isPinned = !!pinned?.has(t.id);
            return (
              <div
                key={t.id}
                className="tab"
                data-tab-id={t.id}
                data-active={t.id === activeId}
                data-attention={mark}
                data-pinned={isPinned || undefined}
                role="tab"
                aria-selected={t.id === activeId}
                // TAB-FR-30: one tab stop for the whole strip, on the tab the
                // ring is currently on.
                tabIndex={t.id === rovingId ? 0 : -1}
                onKeyDown={(e) => onTabKeyDown(e, t.id)}
                onClick={() => onActivate(t.id)}
                // TAB-FR-32: opening the menu neither activates nor closes the
                // tab, so the author can act on a tab they are not reading
                // without first being taken away from the one they are.
                onContextMenu={(e) => {
                  e.preventDefault();
                  openMenuFor(t.id, e.currentTarget);
                }}
                // TAB-FR-31: the tab's accessible name says in full what the
                // strip may have truncated to a few characters, and carries the
                // states a sighted reader takes from the marks beside the label.
                aria-label={[
                  t.tooltip ?? t.label,
                  isPinned ? "pinned" : null,
                  t.dirty ? "unsaved changes" : null,
                ]
                  .filter(Boolean)
                  .join(", ")}
                // DFV-FR-03: a Diff tab's label names only the file, so two tabs
                // on one file under different comparisons read identically in
                // the strip. The tooltip is what tells them apart.
                title={t.tooltip ?? t.label}
              >
                {/* NTF-FR-34: the needs-attention mark, at the tab's leading
                    edge ahead of everything the tab already renders. Always in
                    the tree so the strip reserves its space: a tab taking or
                    losing an indication must shift nothing, itself or its
                    neighbours. It is a diamond rather than a dot, and the tab's
                    surface carries a tone of its own, so shape and tone each say
                    it and neither colour alone nor motion alone is what the
                    author reads. */}
                <span className="tab__attention" aria-hidden="true" />
                {/* TAB-FR-35: the pinned marker, in its own slot beside the
                    attention mark's and reserved on every tab whether it shows
                    or not — so pinning and unpinning shift nothing in the strip.
                    It covers neither the label nor the close control. */}
                <span className="tab__pin" aria-hidden="true">
                  <Icon.Pin size={9} />
                </span>
                {t.chip ? (
                  <span className="chip-type" data-type={t.type}>
                    {t.chip}
                  </span>
                ) : t.id === "dashboard" ? (
                  <Icon.Home size={12} />
                ) : t.kind === "diff" ? (
                  // A Diff tab carries no artifact chip (it is a view, not the
                  // artifact itself), so it gets its own glyph rather than the
                  // settings gear every chip-less tab would otherwise fall back
                  // to.
                  <Icon.Branch size={12} />
                ) : t.kind === "map" ? (
                  // SMP-FR-KQTD: the layers icon in front of the Map tab's label.
                  <Icon.Layers size={12} data-icon="layers" />
                ) : t.kind === "document" ? (
                  // TAB-FR-LKCT: a Document tab is a viewer of a reference file,
                  // not an Editor, so it carries a document glyph of its own.
                  <Icon.Doc size={12} data-icon="document" />
                ) : t.kind === "pdf" ? (
                  <Icon.Pdf size={12} data-icon="pdf" />
                ) : (
                  <Icon.Settings size={12} />
                )}
                <span className="tab__label">{t.label}</span>
                {/* NTF-FR-36: the state itself, for a reader who cannot see the
                    treatment. On the tab rather than on a separate element they
                    would have to go and find. */}
                {mark && <span className="sr-only"> — needs attention</span>}
                {t.dirty && <span className="tab__dirty" />}
                {/* TAB-FR-08: every tab carries a close control, the Dashboard's
                    included; the Home affordance above brings it back. */}
                <span
                  className={`tab__close${closeInert ? " tab__close--disabled" : ""}`}
                  data-testid={`close-${t.id}`}
                  // TAB-FR-31: `aria-label` names an element only where the
                  // role allows a name from the author, which a bare `span`
                  // does not — so the name below would reach nothing without
                  // this. No `tabIndex`: the strip takes exactly one tab stop
                  // (TAB-FR-30), and the keyboard closes a tab through the
                  // context menu's **Close tab** rather than by stepping onto
                  // every close control in the strip.
                  role="button"
                  aria-disabled={closeInert || undefined}
                  // TAB-FR-31: names the tab it closes rather than the word
                  // "close" alone, which is what tells two adjacent close
                  // controls apart.
                  aria-label={
                    closeInert
                      ? "Dashboard stays open while it is the only tab"
                      : `Close ${t.label}`
                  }
                  title={
                    closeInert
                      ? "Dashboard stays open while it is the only tab"
                      : `Close ${t.label}`
                  }
                  onClick={(e) => {
                    e.stopPropagation();
                    if (closeInert) return;
                    onClose(t.id);
                  }}
                >
                  <Icon.X size={10} />
                </span>
              </div>
            );
          })}
        </div>
      </div>
      {menu && menuTarget !== undefined && (
        <TabContextMenu
          menu={menu}
          targetName={menuTarget}
          pinned={!!menuTab && !!pinned?.has(menuTab.id)}
          // TAB-FR-40: on the Home affordance both of these name a tab that
          // does not exist, so both render greyed rather than being dropped —
          // the menu keeps every entry at the position it occupies everywhere
          // else, whatever it was opened on.
          pinDisabled={menuOnHome}
          closeDisabled={
            menuOnHome || (menuTab?.id === "dashboard" && tabs.length === 1)
          }
          targetsFor={(scope) => massCloseTargets?.(scope, menu.tabId) ?? []}
          onSetPinned={(next) => menuTab && onSetPinned?.(menuTab.id, next)}
          onCloseGroup={(scope) => onCloseGroup?.(scope, menu.tabId)}
          onCloseTab={() => menuTab && onClose(menuTab.id)}
          onDismiss={(restoreFocus) => {
            onMenuChange?.(null);
            // TAB-FR-32: Escape returns focus to the tab the menu was opened
            // on, rather than dropping it to the body. The other dismissals do
            // not: focus leaving the menu is itself one of them, and pulling it
            // back to the tab would undo the move the author just made.
            if (restoreFocus) focusTab(menu.tabId);
          }}
        />
      )}
    </>
  );
}

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
function TabContextMenu({
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
}) {
  const ref = useRef<HTMLDivElement>(null);

  // TAB-FR-32: the menu closes on Escape, on the pointer or the focus leaving
  // it, and on any activation outside it. Every one of those routes invokes
  // nothing.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
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

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
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
