/**
 * TAB-FR-ERUM .. TAB-FR-SFGR: the author moves tabs along the strip by drag and
 * by keyboard. jsdom has no layout, so each tab's box is set by hand.
 */
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, createEvent, fireEvent, render, screen } from "@testing-library/react";

import { TabStrip } from "./TabStrip";
import { moveTabInStrip } from "../hooks/shell/tabStripActions";
import type { Tab } from "../types";

afterEach(cleanup);

const noop = () => {};
const DASHBOARD: Tab = { id: "dashboard", label: "Dashboard" };
const A: Tab = { id: "a.md", label: "a.md", kind: "editor" };
const B: Tab = { id: "b.md", label: "b.md", kind: "editor" };
const C: Tab = { id: "c.md", label: "c.md", kind: "editor" };
const TAB_WIDTH = 100;

function strip(tabs: Tab[], props: Partial<React.ComponentProps<typeof TabStrip>> = {}) {
  const onMoveTab = vi.fn();
  const onActivate = vi.fn();
  const onClose = vi.fn();
  const view = render(
    <TabStrip
      tabs={tabs}
      activeId={tabs[0].id}
      onActivate={onActivate}
      onClose={onClose}
      onHome={noop}
      onMoveTab={onMoveTab}
      {...props}
    />,
  );
  layOut(tabs);
  return { onMoveTab, onActivate, onClose, view };
}

/** Each tab is TAB_WIDTH wide, side by side from x = 0; the region is 300 wide. */
function layOut(tabs: Tab[]) {
  tabs.forEach((t, i) => {
    const el = tabEl(t.id);
    el.getBoundingClientRect = () =>
      ({ left: i * TAB_WIDTH, right: (i + 1) * TAB_WIDTH, width: TAB_WIDTH, top: 0, bottom: 30, height: 30 }) as DOMRect;
  });
  region().getBoundingClientRect = () =>
    ({ left: 0, right: 300, width: 300, top: 0, bottom: 30, height: 30 }) as DOMRect;
}

const region = () => screen.getByTestId("tabstrip-scroll");
const tabEl = (id: string) =>
  Array.from(document.querySelectorAll<HTMLElement>("[data-tab-id]")).find(
    (n) => n.dataset.tabId === id,
  )!;

/** testing-library drops `clientX` from a drag event in jsdom, so it is set here. */
function dragAt(type: "dragOver" | "drop" | "dragLeave", el: HTMLElement, clientX: number, clientY = 15) {
  const event = createEvent[type](el);
  Object.defineProperty(event, "clientX", { value: clientX });
  Object.defineProperty(event, "clientY", { value: clientY });
  fireEvent(el, event);
  return event;
}
const dragOver = (el: HTMLElement, clientX: number) => dragAt("dragOver", el, clientX);
const dropAt = (el: HTMLElement, clientX: number) => dragAt("drop", el, clientX);

/** Drag `id` and drop it on the leading or the trailing half of `targetId`. */
function dragTo(id: string, targetId: string, half: "lead" | "trail", tabs: Tab[]) {
  const i = tabs.findIndex((t) => t.id === targetId);
  const x = i * TAB_WIDTH + (half === "lead" ? 10 : TAB_WIDTH - 10);
  fireEvent.dragStart(tabEl(id));
  dragOver(tabEl(targetId), x);
  dropAt(tabEl(targetId), x);
  fireEvent.dragEnd(tabEl(id));
}

describe("TabStrip drag to reorder", () => {
  const tabs = [DASHBOARD, A, B, C];

  it("TAB-FR-ERUM: every tab but the Dashboard is draggable", () => {
    strip(tabs);
    expect(tabEl("dashboard")).toHaveAttribute("draggable", "false");
    for (const t of [A, B, C]) expect(tabEl(t.id)).toHaveAttribute("draggable", "true");
  });

  it("TAB-FR-ERUM: pinned, terminal and marked tabs are draggable too", () => {
    const term: Tab = { id: "terminal:t1", label: "zsh", kind: "terminal" };
    strip([DASHBOARD, A, B, term], {
      activeId: "dashboard",
      pinned: new Set([A.id]),
      attention: new Map([[B.id, "on" as const]]),
    });
    expect(tabEl(A.id)).toHaveAttribute("draggable", "true");
    expect(tabEl(B.id)).toHaveAttribute("draggable", "true");
    expect(tabEl(term.id)).toHaveAttribute("draggable", "true");
  });

  it("TAB-FR-ERUM: a drop on the trailing half of a later tab puts the tab after it", () => {
    const { onMoveTab } = strip(tabs);
    dragTo(A.id, C.id, "trail", tabs);
    expect(onMoveTab).toHaveBeenCalledWith(A.id, 3);
  });

  it("TAB-FR-ERUM: a drop on the leading half of a later tab puts the tab before it", () => {
    const { onMoveTab } = strip(tabs);
    dragTo(A.id, C.id, "lead", tabs);
    expect(onMoveTab).toHaveBeenCalledWith(A.id, 2);
  });

  it("TAB-FR-ERUM: a drop on the leading half of an earlier tab puts the tab before it", () => {
    const { onMoveTab } = strip(tabs);
    dragTo(C.id, A.id, "lead", tabs);
    expect(onMoveTab).toHaveBeenCalledWith(C.id, 1);
  });

  it("TAB-FR-ZMZS: a drop where the tab already stands moves nothing", () => {
    const { onMoveTab } = strip(tabs);
    dragTo(B.id, A.id, "trail", tabs);
    dragTo(B.id, C.id, "lead", tabs);
    dragTo(B.id, B.id, "lead", tabs);
    expect(onMoveTab).not.toHaveBeenCalled();
  });

  it("TAB-FR-XNID: a drop on the Dashboard's leading half puts the tab directly after it", () => {
    const { onMoveTab } = strip(tabs);
    fireEvent.dragStart(tabEl(C.id));
    dragOver(tabEl("dashboard"), 5);
    expect(tabEl("dashboard")).toHaveAttribute("data-drop", "after");
    dropAt(tabEl("dashboard"), 5);
    expect(onMoveTab).toHaveBeenCalledWith(C.id, 1);
  });

  it("TAB-FR-XNID: a drag on the Dashboard does not start", () => {
    const { onMoveTab } = strip(tabs);
    const start = createEvent.dragStart(tabEl("dashboard"));
    fireEvent(tabEl("dashboard"), start);
    expect(start.defaultPrevented).toBe(true);
    dragOver(tabEl(C.id), 290);
    fireEvent.drop(tabEl(C.id));
    expect(onMoveTab).not.toHaveBeenCalled();
    expect(tabEl("dashboard")).not.toHaveAttribute("data-dragging");
  });

  it("TAB-FR-XNID: the Home affordance is not a drop target", () => {
    strip([A, B, C]);
    const home = screen.getByTestId("home-affordance");
    fireEvent.dragStart(tabEl(C.id));
    const over = dragOver(home, 0);
    expect(over.defaultPrevented).toBe(false);
  });

  it("TAB-FR-ERUM: the empty space after the last tab drops the tab at the end", () => {
    const { onMoveTab } = strip(tabs);
    fireEvent.dragStart(tabEl(A.id));
    dragOver(region(), 200);
    expect(tabEl(C.id)).toHaveAttribute("data-drop", "after");
    dropAt(region(), 200);
    expect(onMoveTab).toHaveBeenCalledWith(A.id, 3);
  });

  it("TAB-FR-PHHH: the dragged tab recedes and one indicator marks the drop edge", () => {
    strip(tabs);
    fireEvent.dragStart(tabEl(A.id));
    // Not yet: the engine takes the drag image after dragstart.
    expect(tabEl(A.id)).not.toHaveAttribute("data-dragging");
    fireEvent.drag(tabEl(A.id));
    expect(tabEl(A.id)).toHaveAttribute("data-dragging", "true");
    dragOver(tabEl(B.id), 110);
    expect(tabEl(B.id)).toHaveAttribute("data-drop", "before");
    dragOver(tabEl(C.id), 390);
    expect(tabEl(C.id)).toHaveAttribute("data-drop", "after");
    expect(document.querySelectorAll("[data-drop]")).toHaveLength(1);
  });

  it("TAB-FR-PHHH: the dragged tab shows no indicator of its own", () => {
    strip(tabs);
    fireEvent.dragStart(tabEl(B.id));
    dragOver(tabEl(B.id), 150);
    expect(tabEl(B.id)).not.toHaveAttribute("data-drop");
  });

  it("TAB-FR-PHHH, TAB-FR-ZMZS: a drag that ends with no drop moves nothing and clears the marks", () => {
    const { onMoveTab, onActivate } = strip(tabs);
    fireEvent.dragStart(tabEl(A.id));
    fireEvent.drag(tabEl(A.id));
    dragOver(tabEl(C.id), 290);
    fireEvent.dragEnd(tabEl(A.id));
    expect(onMoveTab).not.toHaveBeenCalled();
    expect(onActivate).not.toHaveBeenCalled();
    expect(document.querySelectorAll("[data-drop], [data-dragging]")).toHaveLength(0);
  });

  it("TAB-FR-PHHH: the indicator goes when the drag leaves the strip", () => {
    strip(tabs);
    fireEvent.dragStart(tabEl(A.id));
    dragOver(tabEl(C.id), 290);
    dragAt("dragLeave", region(), 350);
    expect(document.querySelectorAll("[data-drop]")).toHaveLength(0);
  });

  it("TAB-FR-ZMZS: a drop neither activates nor closes a tab", () => {
    const { onActivate, onClose } = strip(tabs);
    dragTo(A.id, C.id, "trail", tabs);
    expect(onActivate).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  });

  it("TAB-FR-ZMZS: a drag that did not start in this strip is ignored", () => {
    const { onMoveTab } = strip(tabs);
    const over = dragOver(tabEl(C.id), 290);
    expect(over.defaultPrevented).toBe(false);
    fireEvent.drop(tabEl(C.id));
    expect(onMoveTab).not.toHaveBeenCalled();
    expect(document.querySelectorAll("[data-drop]")).toHaveLength(0);
  });

  it("TAB-FR-PHHH: a dragleave with the pointer still in the region keeps the indicator", () => {
    strip(tabs);
    fireEvent.dragStart(tabEl(A.id));
    dragOver(tabEl(C.id), 390);
    // WebKit sends `dragleave` with no related target between child elements.
    dragAt("dragLeave", region(), 250);
    expect(tabEl(C.id)).toHaveAttribute("data-drop", "after");
  });

  it("TAB-FR-ERUM: a drop is placed by the drop event, also after the indicator went", () => {
    const { onMoveTab } = strip(tabs);
    fireEvent.dragStart(tabEl(A.id));
    dragOver(tabEl(C.id), 390);
    dragAt("dragLeave", region(), 250, 60);
    expect(document.querySelectorAll("[data-drop]")).toHaveLength(0);
    dropAt(tabEl(C.id), 390);
    expect(onMoveTab).toHaveBeenCalledWith(A.id, 3);
  });

  it("TAB-FR-XNID: space in front of the first tab places the tab directly after the Dashboard", () => {
    const { onMoveTab } = strip(tabs);
    tabs.forEach((t, i) => {
      tabEl(t.id).getBoundingClientRect = () =>
        ({ left: 20 + i * TAB_WIDTH, width: TAB_WIDTH }) as DOMRect;
    });
    fireEvent.dragStart(tabEl(C.id));
    dropAt(region(), 10);
    expect(onMoveTab).toHaveBeenCalledWith(C.id, 1);
  });

  it("TAB-FR-ERUM: a drag start fills the strip's own drag type", () => {
    strip(tabs);
    const setData = vi.fn();
    const dataTransfer = { setData, effectAllowed: "", dropEffect: "" };
    fireEvent.dragStart(tabEl(A.id), { dataTransfer });
    expect(setData).toHaveBeenCalledWith("application/x-synthesis-tab", A.id);
    expect(dataTransfer.effectAllowed).toBe("move");
  });

  it("TAB-FR-ZMZS: a dragged tab that leaves the strip ends the drag, also when it comes back", () => {
    const { onMoveTab, view } = strip(tabs);
    const props = { activeId: "dashboard", onActivate: noop, onClose: noop, onHome: noop, onMoveTab };
    fireEvent.dragStart(tabEl(A.id));
    fireEvent.drag(tabEl(A.id));
    dragOver(tabEl(C.id), 390);
    view.rerender(<TabStrip tabs={[DASHBOARD, B, C]} {...props} />);
    layOut([DASHBOARD, B, C]);
    expect(dragOver(tabEl(C.id), 290).defaultPrevented).toBe(false);
    expect(document.querySelectorAll("[data-drop]")).toHaveLength(0);
    // The same file opens again under the same id.
    view.rerender(<TabStrip tabs={tabs} {...props} />);
    layOut(tabs);
    expect(tabEl(A.id)).not.toHaveAttribute("data-dragging");
    expect(dragOver(tabEl(C.id), 390).defaultPrevented).toBe(false);
    expect(document.querySelectorAll("[data-drop]")).toHaveLength(0);
  });

  it("TAB-FR-ERUM: with no move handler, no tab is draggable", () => {
    strip(tabs, { onMoveTab: undefined });
    for (const t of tabs) expect(tabEl(t.id)).toHaveAttribute("draggable", "false");
  });
});

describe("TabStrip edge scrolling during a drag (TAB-FR-ILJY)", () => {
  const tabs = [DASHBOARD, A, B, C];

  it("TAB-FR-ILJY: scrolls toward the trailing edge, then toward the leading edge", () => {
    strip(tabs);
    fireEvent.dragStart(tabEl(A.id));
    dragOver(region(), 295);
    const forward = region().scrollLeft;
    expect(forward).toBeGreaterThan(0);
    dragOver(region(), 5);
    expect(region().scrollLeft).toBeLessThan(forward);
  });

  it("TAB-FR-ILJY: does not scroll away from the edge zones, or with no drag", () => {
    strip(tabs);
    dragOver(region(), 295);
    expect(region().scrollLeft).toBe(0);
    fireEvent.dragStart(tabEl(A.id));
    dragOver(region(), 150);
    expect(region().scrollLeft).toBe(0);
  });
});

describe("TabStrip keyboard move (TAB-FR-SFGR)", () => {
  const tabs = [DASHBOARD, A, B, C];
  const press = (id: string, key: "ArrowLeft" | "ArrowRight") =>
    fireEvent.keyDown(tabEl(id), { key, altKey: true, shiftKey: true });

  it("TAB-FR-SFGR: Alt+Shift+Arrow moves the tab one place", () => {
    const { onMoveTab } = strip(tabs);
    press(A.id, "ArrowRight");
    expect(onMoveTab).toHaveBeenLastCalledWith(A.id, 2);
    press(C.id, "ArrowLeft");
    expect(onMoveTab).toHaveBeenLastCalledWith(C.id, 2);
  });

  it("TAB-FR-SFGR: moves nothing at the end, directly after the Dashboard, or on the Dashboard", () => {
    const { onMoveTab } = strip(tabs);
    press(C.id, "ArrowRight");
    press(A.id, "ArrowLeft");
    press("dashboard", "ArrowRight");
    expect(onMoveTab).not.toHaveBeenCalled();
  });

  it("TAB-FR-SFGR: with no Dashboard open, the first tab is the end", () => {
    const noDashboard = [A, B, C];
    const { onMoveTab } = strip(noDashboard);
    press(A.id, "ArrowLeft");
    expect(onMoveTab).not.toHaveBeenCalled();
    press(B.id, "ArrowLeft");
    expect(onMoveTab).toHaveBeenCalledWith(B.id, 0);
  });

  it("TAB-FR-SFGR, TAB-FR-ZMZS: focus stays on the moved tab, and the move activates nothing", () => {
    const onActivate = vi.fn();
    // The strip with the shell's own order rule behind it, so the move and
    // the re-render arrive in one update as they do in the app.
    function Harness() {
      const [order, setOrder] = useState(tabs);
      return (
        <TabStrip
          tabs={order}
          activeId="dashboard"
          onActivate={onActivate}
          onClose={noop}
          onHome={noop}
          onMoveTab={(id, to) => setOrder((ts) => moveTabInStrip(ts, id, to))}
        />
      );
    }
    render(<Harness />);
    act(() => tabEl(A.id).focus());
    press(A.id, "ArrowRight");
    press(A.id, "ArrowRight");
    const order = Array.from(document.querySelectorAll<HTMLElement>("[data-tab-id]")).map(
      (n) => n.dataset.tabId,
    );
    expect(order).toEqual(["dashboard", B.id, C.id, A.id]);
    expect(document.activeElement).toBe(tabEl(A.id));
    expect(tabEl(A.id)).toHaveAttribute("tabindex", "0");
    expect(onActivate).not.toHaveBeenCalled();
  });

  it("TAB-FR-SFGR: the moved tab is revealed", () => {
    const reveal = vi.fn();
    const original = Element.prototype.scrollIntoView;
    Element.prototype.scrollIntoView = reveal;
    try {
      function Harness() {
        const [order, setOrder] = useState(tabs);
        return (
          <TabStrip
            tabs={order}
            activeId="dashboard"
            onActivate={noop}
            onClose={noop}
            onHome={noop}
            onMoveTab={(id, to) => setOrder((ts) => moveTabInStrip(ts, id, to))}
          />
        );
      }
      render(<Harness />);
      reveal.mockClear();
      press(A.id, "ArrowRight");
      expect(reveal.mock.contexts).toContain(tabEl(A.id));
      expect(reveal).toHaveBeenCalledWith({ block: "nearest", inline: "nearest" });
    } finally {
      Element.prototype.scrollIntoView = original;
    }
  });

  it("TAB-FR-30: a plain arrow still moves focus and moves no tab", () => {
    const { onMoveTab } = strip(tabs);
    fireEvent.keyDown(tabEl(A.id), { key: "ArrowRight" });
    expect(onMoveTab).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(tabEl(B.id));
  });
});
