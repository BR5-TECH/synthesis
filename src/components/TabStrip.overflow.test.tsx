import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { TabStrip } from "./TabStrip";
import type { TabMenuState } from "./TabStrip";
import type { Tab } from "../types";
import { HOME_TAB_TARGET } from "../types";

afterEach(cleanup);

const noop = () => {};

function strip(tabs: Tab[]) {
  return render(
    <TabStrip
      tabs={tabs}
      activeId={tabs[0]?.id ?? ""}
      onActivate={noop}
      onClose={noop}
      onHome={noop}
    />,
  );
}


// ---------------------------------------------------------------------------
// TAB-FR-27 through TAB-FR-39: overflow, keyboard reach, and the context menu.
// ---------------------------------------------------------------------------

describe("tab strip overflow and keyboard reach (TAB-FR-27 / TAB-FR-30)", () => {
  const TABS: Tab[] = [
    { id: "dashboard", label: "Dashboard" },
    { id: "art:a.md", label: "a.md", kind: "editor", artifactId: "a.md" },
    { id: "art:b.md", label: "b.md", kind: "editor", artifactId: "b.md", dirty: true },
    { id: "art:c.md", label: "c.md", kind: "editor", artifactId: "c.md" },
  ];

  const open = (props: Partial<Parameters<typeof TabStrip>[0]> = {}) =>
    render(
      <TabStrip
        tabs={TABS}
        activeId="art:a.md"
        onActivate={noop}
        onClose={noop}
        onHome={noop}
        {...props}
      />,
    );

  const tabEl = (id: string) =>
    document.querySelector<HTMLElement>(`[data-tab-id="${id}"]`)!;

  // TAB-FR-27 / TAB-FR-29: the tabs live in a scrolling region of their own,
  // and the Home affordance sits outside it so it is reachable at every scroll
  // position rather than being the first thing scrolled out of reach.
  it("puts the tabs in a scrolling region and the Home affordance outside it", () => {
    open({ tabs: TABS.filter((t) => t.id !== "dashboard") });

    const scroll = screen.getByTestId("tabstrip-scroll");
    const home = screen.getByTestId("home-affordance");
    expect(scroll.contains(home)).toBe(false);
    expect(screen.getByTestId("tabstrip").contains(home)).toBe(true);
    expect(scroll.getAttribute("role")).toBe("tablist");
  });

  // TAB-FR-30: one tab stop for the whole strip, on the tab the ring is on.
  it("takes exactly one tab stop, on the active tab", () => {
    open();

    const stops = Array.from(
      document.querySelectorAll<HTMLElement>('[data-tab-id][tabindex="0"]'),
    );
    expect(stops).toHaveLength(1);
    expect(stops[0].dataset.tabId).toBe("art:a.md");
  });

  // TAB-FR-27, TAB-FR-28, TAB-FR-29, TAB-FR-30's keyboard half: every tab takes focus in turn, and none is
  // activated by being focused — a tab is reached and then chosen.
  it("moves focus across the strip with the arrow keys without activating", async () => {
    const onActivate = vi.fn();
    open({ onActivate });

    tabEl("art:a.md").focus();
    await userEvent.keyboard("{ArrowRight}");
    expect(document.activeElement).toBe(tabEl("art:b.md"));
    await userEvent.keyboard("{ArrowRight}");
    expect(document.activeElement).toBe(tabEl("art:c.md"));
    await userEvent.keyboard("{ArrowLeft}");
    expect(document.activeElement).toBe(tabEl("art:b.md"));
    // Reached three tabs, activated none of them.
    expect(onActivate).not.toHaveBeenCalled();

    // Enter is what chooses.
    await userEvent.keyboard("{Enter}");
    expect(onActivate).toHaveBeenCalledWith("art:b.md");
  });

  it("reaches the first and last tab with Home and End", async () => {
    open();

    tabEl("art:b.md").focus();
    await userEvent.keyboard("{End}");
    expect(document.activeElement).toBe(tabEl("art:c.md"));
    await userEvent.keyboard("{Home}");
    expect(document.activeElement).toBe(tabEl("dashboard"));
  });

  // TAB-FR-29 / TAB-FR-31: the accessible name says in full what the strip may
  // have truncated, and carries the states the marks beside the label carry.
  it("names every tab and every close control in full", () => {
    open({ pinned: new Set(["art:c.md"]) });

    expect(tabEl("art:a.md").getAttribute("aria-label")).toBe("a.md");
    expect(tabEl("art:b.md").getAttribute("aria-label")).toBe(
      "b.md, unsaved changes",
    );
    expect(tabEl("art:c.md").getAttribute("aria-label")).toBe("c.md, pinned");
    // Each close control names the tab it closes rather than "close" alone,
    // which is what tells two adjacent ones apart.
    expect(
      screen.getByTestId("close-art:b.md").getAttribute("aria-label"),
    ).toBe("Close b.md");
  });

  // DFV-FR-03: a Diff tab's accessible name carries the comparison its label
  // deliberately omits, so two tabs on one file are told apart.
  it("names a Diff tab by its comparison", () => {
    open({
      tabs: [
        {
          id: "diff:main:a.md",
          label: "Diff: a.md",
          tooltip: "Diff: a.md — against main",
          kind: "diff",
        },
      ],
      activeId: "diff:main:a.md",
    });

    expect(tabEl("diff:main:a.md").getAttribute("aria-label")).toBe(
      "Diff: a.md — against main",
    );
  });

  // TAB-FR-35: the pinned marker's slot is reserved on every tab whether the
  // marker shows or not, so pinning shifts nothing in the strip.
  it("reserves the pinned marker's space on every tab", () => {
    open({ pinned: new Set(["art:c.md"]) });

    expect(document.querySelectorAll(".tabstrip .tab__pin")).toHaveLength(
      TABS.length,
    );
    expect(tabEl("art:c.md").getAttribute("data-pinned")).toBe("true");
    expect(tabEl("art:a.md").getAttribute("data-pinned")).toBeNull();
  });
});

describe("tab context menu (TAB-FR-32 / TAB-FR-33 / TAB-FR-39)", () => {
  const TABS: Tab[] = [
    { id: "dashboard", label: "Dashboard" },
    { id: "art:a.md", label: "a.md", kind: "editor", artifactId: "a.md" },
    { id: "art:b.md", label: "b.md", kind: "editor", artifactId: "b.md" },
  ];

  /**
   * Renders the strip with the menu state held above it, as the shell holds it
   * (SNV-FR-56), so opening and dismissing behave as they do in the window.
   */
  function Harness(props: Partial<Parameters<typeof TabStrip>[0]> = {}) {
    const [menu, setMenu] = useState<TabMenuState | null>(null);
    return (
      <TabStrip
        tabs={TABS}
        activeId="art:b.md"
        onActivate={noop}
        onClose={noop}
        onHome={noop}
        menu={menu}
        onMenuChange={setMenu}
        massCloseTargets={() => ["x"]}
        {...props}
      />
    );
  }

  const openMenuOn = async (id: string) => {
    fireEvent.contextMenu(
      document.querySelector<HTMLElement>(`[data-tab-id="${id}"]`)!,
    );
    return screen.findByTestId("tab-context-menu");
  };

  const entries = () =>
    Array.from(
      screen
        .getByTestId("tab-context-menu")
        .querySelectorAll<HTMLElement>("[data-menu-item]"),
    ).map((n) => ({
      label: n.textContent,
      disabled: n.getAttribute("aria-disabled") === "true",
    }));

  // TAB-FR-33: five entries in one fixed order, whatever the tab.
  it("offers five entries in a fixed order, with Pin on an unpinned tab", async () => {
    render(<Harness />);
    await openMenuOn("art:a.md");

    expect(entries().map((e) => e.label)).toEqual([
      "Pin tab",
      "Close tabs to the left",
      "Close tabs to the right",
      "Close other tabs",
      "Close tab",
    ]);
  });

  // TAB-FR-32, TAB-FR-33, TAB-FR-34, TAB-FR-35: exactly one of the two pin entries renders, and it is the one
  // that matches the tab's state.
  it("offers Unpin, and never Pin beside it, on a pinned tab", async () => {
    render(<Harness pinned={new Set(["art:a.md"])} />);
    await openMenuOn("art:a.md");

    const labels = entries().map((e) => e.label);
    expect(labels).toContain("Unpin tab");
    expect(labels).not.toContain("Pin tab");
  });

  // TAB-FR-32: opening the menu neither activates nor closes the tab, so an
  // author can act on a tab they are not reading without leaving the one they
  // are.
  it("neither activates nor closes the tab it is opened on", async () => {
    const onActivate = vi.fn();
    const onClose = vi.fn();
    render(<Harness onActivate={onActivate} onClose={onClose} />);

    await openMenuOn("art:a.md");

    expect(onActivate).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  });

  // TAB-FR-33, TAB-FR-36: every entry acts on the context-clicked tab, even when another
  // tab is the active one.
  it("acts on the context-clicked tab rather than the active one", async () => {
    const onClose = vi.fn();
    const onCloseGroup = vi.fn();
    const onSetPinned = vi.fn();
    render(
      <Harness
        onClose={onClose}
        onCloseGroup={onCloseGroup}
        onSetPinned={onSetPinned}
      />,
    );

    await openMenuOn("art:a.md");
    await userEvent.click(screen.getByTestId("tabmenu-close"));
    expect(onClose).toHaveBeenCalledWith("art:a.md");

    await openMenuOn("art:a.md");
    await userEvent.click(screen.getByTestId("tabmenu-others"));
    expect(onCloseGroup).toHaveBeenCalledWith("others", "art:a.md");

    await openMenuOn("art:a.md");
    await userEvent.click(screen.getByTestId("tabmenu-pin"));
    expect(onSetPinned).toHaveBeenCalledWith("art:a.md", true);
  });

  // TAB-FR-34 / TAB-FR-39: enablement is the eligible set and nothing else —
  // an empty one greys the entry in place rather than removing it.
  it("greys a mass close exactly while its eligible set is empty", async () => {
    render(
      <Harness
        massCloseTargets={(scope) => (scope === "right" ? ["art:b.md"] : [])}
      />,
    );
    await openMenuOn("art:a.md");

    const byLabel = Object.fromEntries(
      entries().map((e) => [e.label, e.disabled]),
    );
    expect(byLabel["Close tabs to the left"]).toBe(true);
    expect(byLabel["Close tabs to the right"]).toBe(false);
    expect(byLabel["Close other tabs"]).toBe(true);
    // Greyed in place: still five entries at the same positions.
    expect(entries()).toHaveLength(5);
  });

  it("does nothing when a greyed mass close is activated", async () => {
    const onCloseGroup = vi.fn();
    render(<Harness massCloseTargets={() => []} onCloseGroup={onCloseGroup} />);
    await openMenuOn("art:a.md");

    await userEvent.click(screen.getByTestId("tabmenu-left"));

    expect(onCloseGroup).not.toHaveBeenCalled();
    // And the menu is still open — a disabled entry is not an action, so it is
    // not one of the routes that closes the menu.
    expect(screen.getByTestId("tab-context-menu")).toBeInTheDocument();
  });

  // TAB-FR-16, TAB-FR-34, TAB-FR-39's menu half / TAB-FR-36: Close tab is disabled on one tab alone —
  // the Dashboard while it is the only open tab (TAB-FR-16) — and the pin
  // action still acts there (TAB-FR-34).
  it("greys Close tab on the sole Dashboard while leaving the pin action live", async () => {
    render(
      <Harness
        tabs={[{ id: "dashboard", label: "Dashboard" }]}
        activeId="dashboard"
        massCloseTargets={() => []}
      />,
    );
    await openMenuOn("dashboard");

    const byLabel = Object.fromEntries(
      entries().map((e) => [e.label, e.disabled]),
    );
    expect(byLabel["Close tab"]).toBe(true);
    expect(byLabel["Pin tab"]).toBe(false);
  });

  // TAB-FR-32, SNV-FR-56: Escape closes the menu, invokes nothing, and returns focus to
  // the tab it was opened on.
  it("closes on Escape, invoking nothing, and returns focus to its tab", async () => {
    const onClose = vi.fn();
    const onCloseGroup = vi.fn();
    render(<Harness onClose={onClose} onCloseGroup={onCloseGroup} />);
    await openMenuOn("art:a.md");

    await userEvent.keyboard("{Escape}");

    expect(screen.queryByTestId("tab-context-menu")).not.toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
    expect(onCloseGroup).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(
      document.querySelector('[data-tab-id="art:a.md"]'),
    );
  });

  // TAB-FR-32: and on a pointer landing outside it.
  it("closes when the pointer lands outside it", async () => {
    render(<Harness />);
    await openMenuOn("art:a.md");

    fireEvent.mouseDown(document.body);

    expect(screen.queryByTestId("tab-context-menu")).not.toBeInTheDocument();
  });

  // TAB-FR-32: focus leaving the menu is itself one of the dismissals, so that
  // route must not pull focus back to the tab — doing so would undo the move
  // the author just made. Escape is the one dismissal that owes them their
  // place back, and the test above covers it.
  it("closes when focus moves outside it, without pulling focus back", async () => {
    const outside = document.createElement("button");
    document.body.appendChild(outside);
    try {
      render(<Harness />);
      await openMenuOn("art:a.md");

      // Wrapped: the dismissal runs from a native `focusin` listener, so the
      // state update it schedules is outside React's own event batching.
      act(() => outside.focus());

      expect(screen.queryByTestId("tab-context-menu")).not.toBeInTheDocument();
      expect(document.activeElement).toBe(outside);
    } finally {
      outside.remove();
    }
  });

  // TAB-FR-32: the menu closes after an action.
  it("closes after an entry is chosen", async () => {
    render(<Harness onCloseGroup={noop} />);
    await openMenuOn("art:a.md");

    await userEvent.click(screen.getByTestId("tabmenu-others"));

    expect(screen.queryByTestId("tab-context-menu")).not.toBeInTheDocument();
  });

  // TAB-FR-32 / SNV-FR-56: opening the menu closes every other floating overlay
  // of the window.
  it("closes the window's other overlays as it opens", async () => {
    const onOverlayOpening = vi.fn();
    render(<Harness onOverlayOpening={onOverlayOpening} />);

    await openMenuOn("art:a.md");

    expect(onOverlayOpening).toHaveBeenCalledTimes(1);
  });

  // TAB-FR-32: reachable from the keyboard, by the dedicated key and by the
  // Shift+F10 that stands in for it on keyboards without one.
  it("opens from the keyboard context-menu gesture", async () => {
    render(<Harness />);
    const tab = document.querySelector<HTMLElement>('[data-tab-id="art:a.md"]')!;

    tab.focus();
    fireEvent.keyDown(tab, { key: "ContextMenu" });
    expect(await screen.findByTestId("tab-context-menu")).toBeInTheDocument();

    await userEvent.keyboard("{Escape}");
    fireEvent.keyDown(tab, { key: "F10", shiftKey: true });
    expect(await screen.findByTestId("tab-context-menu")).toBeInTheDocument();
  });

  // TAB-FR-32: operable from the keyboard throughout — the arrow keys move
  // through the entries and Enter chooses one.
  it("moves through its entries with the arrow keys and chooses with Enter", async () => {
    const onCloseGroup = vi.fn();
    render(<Harness onCloseGroup={onCloseGroup} />);
    await openMenuOn("art:a.md");

    // Focus lands on the first entry when the menu opens.
    expect(document.activeElement).toBe(screen.getByTestId("tabmenu-pin"));
    await userEvent.keyboard("{ArrowDown}{ArrowDown}");
    expect(document.activeElement).toBe(screen.getByTestId("tabmenu-right"));
    await userEvent.keyboard("{Enter}");

    expect(onCloseGroup).toHaveBeenCalledWith("right", "art:a.md");
  });
});

/**
 * TAB-FR-28: the wheel is how a pointer reaches the far end of a strip that
 * renders no scrollbar, so it is the one part of the overflow contract that is
 * behaviour rather than layout — and the one part jsdom can be made to answer.
 *
 * jsdom reports every box as zero-sized, so `scrollWidth`/`clientWidth` are
 * defined onto the region here. Without them the handler's own
 * "nothing to scroll" guard short-circuits and every branch past it is
 * unreachable, which would leave this whole requirement untested.
 */
describe("scrolling the strip with the wheel (TAB-FR-28)", () => {
  const TABS: Tab[] = [
    { id: "art:a.md", label: "a.md", kind: "editor" },
    { id: "art:b.md", label: "b.md", kind: "editor" },
  ];

  /** Render, and make the region report itself as overflowing by `overflow`. */
  const overflowing = (overflow = 800) => {
    render(
      <TabStrip
        tabs={TABS}
        activeId="art:a.md"
        onActivate={noop}
        onClose={noop}
        onHome={noop}
      />,
    );
    const region = screen.getByTestId("tabstrip-scroll");
    Object.defineProperty(region, "clientWidth", {
      value: 200,
      configurable: true,
    });
    Object.defineProperty(region, "scrollWidth", {
      value: 200 + overflow,
      configurable: true,
    });
    region.scrollLeft = 0;
    return region;
  };

  // The FR's central claim: a device that reports only vertical movement still
  // reaches the far end of the strip.
  it("scrolls horizontally on a vertical wheel", () => {
    const region = overflowing();

    const e = new WheelEvent("wheel", {
      deltaY: 120,
      deltaX: 0,
      bubbles: true,
      cancelable: true,
    });
    region.dispatchEvent(e);

    expect(region.scrollLeft).toBe(120);
    // Prevented, or the gesture scrolls the strip AND whatever lies behind it.
    expect(e.defaultPrevented).toBe(true);
  });

  // A trackpad's horizontal gesture arrives as deltaX; the larger axis wins
  // rather than one being hard-coded.
  it("honours a horizontal gesture when it is the larger axis", () => {
    const region = overflowing();

    region.dispatchEvent(
      new WheelEvent("wheel", {
        deltaX: 90,
        deltaY: 10,
        bubbles: true,
        cancelable: true,
      }),
    );

    expect(region.scrollLeft).toBe(90);
  });

  it("scrolls back on a negative delta", () => {
    const region = overflowing();
    region.scrollLeft = 300;

    region.dispatchEvent(
      new WheelEvent("wheel", {
        deltaY: -120,
        bubbles: true,
        cancelable: true,
      }),
    );

    expect(region.scrollLeft).toBe(180);
  });

  // A pinch-zoom gesture, not a scroll: left entirely alone.
  it("ignores a ctrl-modified wheel", () => {
    const region = overflowing();

    const e = new WheelEvent("wheel", {
      deltaY: 120,
      ctrlKey: true,
      bubbles: true,
      cancelable: true,
    });
    region.dispatchEvent(e);

    expect(region.scrollLeft).toBe(0);
    expect(e.defaultPrevented).toBe(false);
  });

  // Nothing to scroll: the event is not prevented, so the gesture reaches
  // whatever else would have handled it.
  it("leaves the event alone when the strip does not overflow", () => {
    const region = overflowing(0);

    const e = new WheelEvent("wheel", {
      deltaY: 120,
      bubbles: true,
      cancelable: true,
    });
    region.dispatchEvent(e);

    expect(region.scrollLeft).toBe(0);
    expect(e.defaultPrevented).toBe(false);
  });
});

/**
 * The parts of the menu that a stubbed callback cannot vouch for: where focus
 * lands after a choice, what the entries are made of, and the reveal that
 * pairs with a keyboard move.
 */
describe("tab menu focus, structure, and reveal (TAB-FR-30 / TAB-FR-33 / TAB-FR-34)", () => {
  const TABS: Tab[] = [
    { id: "art:a.md", label: "a.md", kind: "editor" },
    { id: "art:b.md", label: "b.md", kind: "editor" },
  ];

  function Harness(props: Partial<Parameters<typeof TabStrip>[0]> = {}) {
    const [menu, setMenu] = useState<TabMenuState | null>(null);
    return (
      <TabStrip
        tabs={TABS}
        activeId="art:b.md"
        onActivate={noop}
        onClose={noop}
        onHome={noop}
        menu={menu}
        onMenuChange={setMenu}
        massCloseTargets={() => ["x"]}
        {...props}
      />
    );
  }

  const openMenuOn = async (id: string) => {
    fireEvent.contextMenu(
      document.querySelector<HTMLElement>(`[data-tab-id="${id}"]`)!,
    );
    return screen.findByTestId("tab-context-menu");
  };

  // TAB-FR-34: "focus returns where it was". Pin closes nothing, so the tab is
  // still there to return it to — and a keyboard author who pins a tab must not
  // be dropped to the document body.
  it("returns focus to the tab after pinning it", async () => {
    render(<Harness onSetPinned={noop} />);
    await openMenuOn("art:a.md");

    await userEvent.click(screen.getByTestId("tabmenu-pin"));

    expect(document.activeElement).toBe(
      document.querySelector('[data-tab-id="art:a.md"]'),
    );
  });

  // TAB-FR-37: a mass close never takes the context-clicked tab either, so the
  // same holds.
  it("returns focus to the tab after a mass close", async () => {
    render(<Harness onCloseGroup={noop} />);
    await openMenuOn("art:a.md");

    await userEvent.click(screen.getByTestId("tabmenu-others"));

    expect(document.activeElement).toBe(
      document.querySelector('[data-tab-id="art:a.md"]'),
    );
  });

  // The one entry that removes the tab the menu was opened on: there is nothing
  // left to return focus to, so it must not try.
  it("does not chase the tab it just closed", async () => {
    render(<Harness onClose={noop} />);
    await openMenuOn("art:a.md");

    await userEvent.click(screen.getByTestId("tabmenu-close"));

    expect(document.activeElement).not.toBe(
      document.querySelector('[data-tab-id="art:a.md"]'),
    );
  });

  // TAB-FR-33: "a separator after the pin action and another before Close tab",
  // and "every entry carries a leading icon". Both are invisible to an
  // assertion that reads only text, so they are read structurally here.
  it("carries two separators and a leading icon on every entry", async () => {
    render(<Harness />);
    const menu = await openMenuOn("art:a.md");

    const seps = menu.querySelectorAll('[role="separator"]');
    expect(seps).toHaveLength(2);

    const items = Array.from(
      menu.querySelectorAll<HTMLElement>("[data-menu-item]"),
    );
    expect(items).toHaveLength(5);
    expect(items.every((n) => !!n.querySelector("svg"))).toBe(true);

    // The separators sit after the pin action and before Close tab, which is
    // what keeps the entry acting on one tab away from the three acting on many.
    const children = Array.from(menu.children);
    expect(children.indexOf(seps[0])).toBe(1);
    expect(children.indexOf(seps[1])).toBe(children.length - 2);
  });

  // TAB-FR-32: a menu whose tab is taken out from under it by an automatic
  // closure (TAB-FR-19 / TAB-FR-22) is dropped rather than left standing to
  // reappear when that path is reopened.
  it("drops itself when its tab leaves the strip", async () => {
    const onMenuChange = vi.fn();
    const menu: TabMenuState = { tabId: "art:gone.md", x: 10, y: 10 };
    render(
      <TabStrip
        tabs={TABS}
        activeId="art:a.md"
        onActivate={noop}
        onClose={noop}
        onHome={noop}
        menu={menu}
        onMenuChange={onMenuChange}
      />,
    );

    expect(screen.queryByTestId("tab-context-menu")).not.toBeInTheDocument();
    expect(onMenuChange).toHaveBeenCalledWith(null);
  });
});

describe("revealing a tab reached from the keyboard (TAB-FR-30)", () => {
  let scrolled: HTMLElement[];
  beforeEach(() => {
    scrolled = [];
    (HTMLElement.prototype as Partial<HTMLElement>).scrollIntoView =
      function (this: HTMLElement) {
        scrolled.push(this);
      };
  });
  afterEach(() => {
    delete (HTMLElement.prototype as Partial<HTMLElement>).scrollIntoView;
  });

  // TAB-FR-27, TAB-FR-28, TAB-FR-29, TAB-FR-30's final clause pairs focus with the reveal: a tab the strip is
  // not currently showing is how the keyboard reaches the far end of it.
  it("scrolls a tab into view as focus reaches it", async () => {
    render(
      <TabStrip
        tabs={[
          { id: "a", label: "a", kind: "editor" },
          { id: "b", label: "b", kind: "editor" },
          { id: "c", label: "c", kind: "editor" },
        ]}
        activeId="a"
        onActivate={noop}
        onClose={noop}
        onHome={noop}
      />,
    );
    document.querySelector<HTMLElement>('[data-tab-id="a"]')!.focus();
    scrolled = [];

    await userEvent.keyboard("{ArrowRight}");
    expect(scrolled[scrolled.length - 1]).toHaveAttribute("data-tab-id", "b");

    await userEvent.keyboard("{End}");
    expect(scrolled[scrolled.length - 1]).toHaveAttribute("data-tab-id", "c");
  });
});

/**
 * TAB-FR-40: the Home affordance carries the same menu the tabs carry.
 *
 * It is a control rather than a tab (SNV-FR-09), which is what decides its
 * entries: the pin action and **Close tab** name a tab that does not exist, and
 * standing at the head of the strip leaves nothing to its left — so three of
 * the five are greyed in place and the two that remain take the whole strip.
 */
describe("the Home affordance's context menu (TAB-FR-40)", () => {
  // No Dashboard tab, so the affordance stands in its place at the head.
  const TABS: Tab[] = [
    { id: "art:a.md", label: "a.md", kind: "editor" },
    { id: "art:b.md", label: "b.md", kind: "editor" },
  ];

  function Harness(props: Partial<Parameters<typeof TabStrip>[0]> = {}) {
    const [menu, setMenu] = useState<TabMenuState | null>(null);
    return (
      <TabStrip
        tabs={TABS}
        activeId="art:a.md"
        onActivate={noop}
        onClose={noop}
        onHome={noop}
        menu={menu}
        onMenuChange={setMenu}
        massCloseTargets={(scope) =>
          scope === "left" ? [] : ["art:a.md", "art:b.md"]
        }
        {...props}
      />
    );
  }

  const home = () => screen.getByTestId("home-affordance");
  const openMenu = async () => {
    fireEvent.contextMenu(home());
    return screen.findByTestId("tab-context-menu");
  };
  const entries = () =>
    Array.from(
      screen
        .getByTestId("tab-context-menu")
        .querySelectorAll<HTMLElement>("[data-menu-item]"),
    ).map((n) => ({
      label: n.textContent,
      disabled: n.getAttribute("aria-disabled") === "true",
    }));

  it("holds the same five entries in the same order, with three greyed", async () => {
    render(<Harness />);
    await openMenu();

    expect(entries()).toEqual([
      { label: "Pin tab", disabled: true },
      { label: "Close tabs to the left", disabled: true },
      { label: "Close tabs to the right", disabled: false },
      { label: "Close other tabs", disabled: false },
      { label: "Close tab", disabled: true },
    ]);
  });

  it("does nothing when one of the three greyed entries is activated", async () => {
    const onSetPinned = vi.fn();
    const onClose = vi.fn();
    const onCloseGroup = vi.fn();
    render(
      <Harness
        onSetPinned={onSetPinned}
        onClose={onClose}
        onCloseGroup={onCloseGroup}
      />,
    );
    await openMenu();

    await userEvent.click(screen.getByTestId("tabmenu-pin"));
    await userEvent.click(screen.getByTestId("tabmenu-close"));
    await userEvent.click(screen.getByTestId("tabmenu-left"));

    expect(onSetPinned).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
    expect(onCloseGroup).not.toHaveBeenCalled();
    // None of the three is an action, so none of them closed the menu either.
    expect(screen.getByTestId("tab-context-menu")).toBeInTheDocument();
  });

  it("runs the two live entries against the head of the strip", async () => {
    const onCloseGroup = vi.fn();
    render(<Harness onCloseGroup={onCloseGroup} />);

    await openMenu();
    await userEvent.click(screen.getByTestId("tabmenu-right"));
    expect(onCloseGroup).toHaveBeenCalledWith("right", HOME_TAB_TARGET);

    await openMenu();
    await userEvent.click(screen.getByTestId("tabmenu-others"));
    expect(onCloseGroup).toHaveBeenCalledWith("others", HOME_TAB_TARGET);
  });

  // TAB-FR-40 / TAB-FR-39: greyed on the eligible set, not on the affordance —
  // with every tab pinned there is nothing for either live entry to take.
  it("greys both live entries when every tab is pinned", async () => {
    render(<Harness massCloseTargets={() => []} />);
    await openMenu();

    expect(entries().every((e) => e.disabled)).toBe(true);
  });

  // TAB-FR-40 opens and dismisses on TAB-FR-32's terms, the keyboard route
  // included — which is why the affordance takes a tab stop of its own.
  it("opens from the keyboard and returns focus to the affordance on Escape", async () => {
    render(<Harness />);
    home().focus();

    fireEvent.keyDown(home(), { key: "F10", shiftKey: true });
    expect(await screen.findByTestId("tab-context-menu")).toBeInTheDocument();

    await userEvent.keyboard("{Escape}");
    expect(screen.queryByTestId("tab-context-menu")).not.toBeInTheDocument();
    expect(document.activeElement).toBe(home());
  });

  // The affordance is rendered only while the Dashboard tab is closed, so a
  // menu open on it is dropped if the Dashboard reappears beneath it.
  it("drops the menu when the Dashboard tab reappears", async () => {
    const onMenuChange = vi.fn();
    render(
      <TabStrip
        tabs={[{ id: "dashboard", label: "Dashboard" }, ...TABS]}
        activeId="dashboard"
        onActivate={noop}
        onClose={noop}
        onHome={noop}
        menu={{ tabId: HOME_TAB_TARGET, x: 0, y: 0 }}
        onMenuChange={onMenuChange}
      />,
    );

    expect(screen.queryByTestId("home-affordance")).not.toBeInTheDocument();
    expect(screen.queryByTestId("tab-context-menu")).not.toBeInTheDocument();
    expect(onMenuChange).toHaveBeenCalledWith(null);
  });
});
