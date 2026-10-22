import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useRef } from "react";

import { TabScrollScope, useRememberedScroll } from "./useRememberedScroll";
import { resetLogBufferForTest } from "../logging";
import {
  holdTabScroll,
  resetTabScrollForTest,
  tabScrollOf,
} from "../state/tabScroll";
import {
  activeResizeObservers,
  installFakeResizeObserver,
  installFakeScroll,
  resizeAll,
} from "../test/fakeScroll";
import { otherCommand } from "../test/invokeMocks";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string) => otherCommand(cmd),
}));

interface SurfaceProps {
  surface?: string;
  content?: unknown;
  /** How far the scroller can scroll, or undefined for no limit. */
  maxScroll?: number;
  onRestored?: (top: number) => void;
}

function Surface({ surface = "doc", content = 0, maxScroll, onRestored }: SurfaceProps) {
  const ref = useRef<HTMLDivElement>(null);
  const onScroll = useRememberedScroll({
    scroller: () => ref.current,
    surface,
    content,
    onRestored,
  });
  return (
    <div
      data-testid="scroller"
      ref={ref}
      onScroll={onScroll}
      data-max-scroll={maxScroll}
    >
      <p>content</p>
    </div>
  );
}

function inTab(tabId: string, props: SurfaceProps = {}) {
  return (
    <TabScrollScope tabId={tabId}>
      <Surface {...props} />
    </TabScrollScope>
  );
}

const scroller = () => screen.getByTestId("scroller");

/** The author scrolls the scroller to `top`. */
function scrollTo(top: number) {
  const el = scroller();
  el.scrollTop = top;
  fireEvent.scroll(el);
}

let restoreScroll: () => void;
let restoreObserver: () => void;

beforeEach(() => {
  resetTabScrollForTest();
  resetLogBufferForTest();
  restoreScroll = installFakeScroll();
  restoreObserver = installFakeResizeObserver();
});

afterEach(() => {
  cleanup();
  restoreObserver();
  restoreScroll();
  resetLogBufferForTest();
});

describe("useRememberedScroll", () => {
  it("EDT-FR-UERT, DFV-FR-YMJA: a scroll is held for the tab and restored when the surface mounts again", () => {
    const { unmount } = render(inTab("tab-a"));
    scrollTo(420);
    expect(tabScrollOf("tab-a", "doc")).toBe(420);

    unmount();
    const onRestored = vi.fn();
    render(inTab("tab-a", { onRestored }));

    expect(scroller().scrollTop).toBe(420);
    expect(onRestored).toHaveBeenCalledWith(420);
  });

  it("EDT-FR-UERT: a surface that the tab never scrolled mounts at the top and reports no restore", () => {
    const onRestored = vi.fn();
    render(inTab("tab-a", { onRestored }));

    expect(scroller().scrollTop).toBe(0);
    expect(onRestored).not.toHaveBeenCalled();
  });

  it("EDT-FR-UERT: each tab gets its own offset back", () => {
    holdTabScroll("tab-a", "doc", 100);
    holdTabScroll("tab-b", "doc", 300);

    const first = render(inTab("tab-a"));
    expect(scroller().scrollTop).toBe(100);
    first.unmount();

    render(inTab("tab-b"));
    expect(scroller().scrollTop).toBe(300);
  });

  it("EDT-FR-UERT: outside a tab the hook neither holds nor restores", () => {
    holdTabScroll("tab-a", "doc", 100);
    render(<Surface />);

    expect(scroller().scrollTop).toBe(0);
    scrollTo(50);
    expect(tabScrollOf("tab-a", "doc")).toBe(100);
  });

  it("EDT-FR-UERT: a surface that replaces another in the same mount starts at the top, and so does its next restore", () => {
    holdTabScroll("tab-a", "doc:text", 900);
    const view = render(inTab("tab-a", { surface: "doc:wysiwyg" }));
    scrollTo(200);

    view.rerender(inTab("tab-a", { surface: "doc:text" }));

    expect(scroller().scrollTop).toBe(0);
    expect(tabScrollOf("tab-a", "doc:text")).toBe(0);
    expect(tabScrollOf("tab-a", "doc:wysiwyg")).toBe(200);
  });

  it("EDT-FR-UERT: after a surface change, a scroll is held for the new surface only", () => {
    const view = render(inTab("tab-a", { surface: "s1" }));
    scrollTo(200);
    view.rerender(inTab("tab-a", { surface: "s2" }));

    scrollTo(350);

    expect(tabScrollOf("tab-a", "s2")).toBe(350);
    expect(tabScrollOf("tab-a", "s1")).toBe(200);
  });

  it("EDT-FR-UERT, EDT-FR-NYCP: a surface change ends a restore that waits", () => {
    holdTabScroll("tab-a", "s1", 500);
    const view = render(inTab("tab-a", { surface: "s1", maxScroll: 100 }));
    expect(scroller().scrollTop).toBe(100);

    view.rerender(inTab("tab-a", { surface: "s2", maxScroll: 1000, content: 1 }));
    act(() => resizeAll());

    expect(scroller().scrollTop).toBe(0);
    expect(tabScrollOf("tab-a", "s1")).toBe(500);
  });

  it("EDT-FR-UERT: another tab in the same mount gets its own offset, or the top when it holds none", () => {
    holdTabScroll("tab-a", "doc", 300);
    holdTabScroll("tab-b", "doc", 120);
    const view = render(inTab("tab-a"));
    expect(scroller().scrollTop).toBe(300);

    view.rerender(inTab("tab-b"));
    expect(scroller().scrollTop).toBe(120);

    view.rerender(inTab("tab-c"));
    expect(scroller().scrollTop).toBe(0);
  });

  it("EDT-FR-NYCP: another tab in the same mount ends the restore the first tab waited for", () => {
    holdTabScroll("tab-a", "doc", 500);
    const view = render(inTab("tab-a", { maxScroll: 100 }));
    expect(scroller().scrollTop).toBe(100);

    view.rerender(inTab("tab-b", { maxScroll: 1000, content: 1 }));
    act(() => resizeAll());

    expect(scroller().scrollTop).toBe(0);
  });

  it("EDT-FR-UERT: a held offset of 0 reports no restore", () => {
    holdTabScroll("tab-a", "doc", 0);
    const onRestored = vi.fn();
    render(inTab("tab-a", { onRestored }));
    expect(onRestored).not.toHaveBeenCalled();
  });

  it("EDT-FR-NYCP: the scroll event of a restore step within a pixel of it does not end the wait, and one further away does", () => {
    holdTabScroll("tab-a", "doc", 500);
    const view = render(inTab("tab-a", { maxScroll: 100 }));

    scrollTo(99);
    expect(tabScrollOf("tab-a", "doc")).toBe(500);

    scrollTo(98);
    expect(tabScrollOf("tab-a", "doc")).toBe(98);
    view.rerender(inTab("tab-a", { maxScroll: 1000, content: 1 }));
    expect(scroller().scrollTop).toBe(98);
  });

  it("EDT-FR-NYCP: a restore that comes to rest within a pixel of its offset is done", () => {
    holdTabScroll("tab-a", "doc", 500);
    const view = render(inTab("tab-a", { maxScroll: 499 }));
    expect(scroller().scrollTop).toBe(499);

    view.rerender(inTab("tab-a", { maxScroll: 1000, content: 1 }));
    expect(scroller().scrollTop).toBe(499);
  });

  it("EDT-FR-NYCP, DFV-FR-YMJA: a restore waits for content that is too short and finishes when the content changes", () => {
    holdTabScroll("tab-a", "doc", 500);
    const onRestored = vi.fn();
    const view = render(inTab("tab-a", { maxScroll: 100, onRestored }));

    expect(scroller().scrollTop).toBe(100);
    expect(onRestored).toHaveBeenLastCalledWith(100);
    // The clamped step does not overwrite the offset the tab waits for.
    fireEvent.scroll(scroller());
    expect(tabScrollOf("tab-a", "doc")).toBe(500);

    view.rerender(inTab("tab-a", { maxScroll: 1000, content: 1, onRestored }));

    expect(scroller().scrollTop).toBe(500);
    expect(onRestored).toHaveBeenLastCalledWith(500);
  });

  it("EDT-FR-NYCP: a restore that waits finishes when the content resizes without a render", () => {
    holdTabScroll("tab-a", "doc", 500);
    render(inTab("tab-a", { maxScroll: 100 }));
    expect(scroller().scrollTop).toBe(100);
    expect(activeResizeObservers()).toBe(1);

    scroller().setAttribute("data-max-scroll", "1000");
    act(() => resizeAll());

    expect(scroller().scrollTop).toBe(500);
  });

  it("EDT-FR-NYCP: a restore that reached its offset stops following the content", () => {
    holdTabScroll("tab-a", "doc", 300);
    const view = render(inTab("tab-a"));
    expect(scroller().scrollTop).toBe(300);
    scrollTo(40);

    view.rerender(inTab("tab-a", { content: 1 }));
    act(() => resizeAll());

    expect(scroller().scrollTop).toBe(40);
  });

  it.each(["wheel", "pointerDown", "keyDown", "touchStart"] as const)(
    "EDT-FR-NYCP, DFV-FR-YMJA: author input (%s) ends a restore that waits",
    (input) => {
      holdTabScroll("tab-a", "doc", 500);
      const view = render(inTab("tab-a", { maxScroll: 100 }));
      expect(scroller().scrollTop).toBe(100);

      fireEvent[input](scroller());
      scroller().setAttribute("data-max-scroll", "1000");
      view.rerender(inTab("tab-a", { maxScroll: 1000, content: 1 }));
      act(() => resizeAll());

      expect(scroller().scrollTop).toBe(100);
    },
  );

  it("EDT-FR-NYCP: a scroll by the author ends a restore that waits and holds the author's offset", () => {
    holdTabScroll("tab-a", "doc", 500);
    const view = render(inTab("tab-a", { maxScroll: 100 }));

    scrollTo(30);
    expect(tabScrollOf("tab-a", "doc")).toBe(30);

    view.rerender(inTab("tab-a", { maxScroll: 1000, content: 1 }));
    expect(scroller().scrollTop).toBe(30);
  });

  it("EDT-FR-NYCP: the observer is released when the surface unmounts", () => {
    holdTabScroll("tab-a", "doc", 500);
    const view = render(inTab("tab-a", { maxScroll: 100 }));
    expect(activeResizeObservers()).toBe(1);

    view.unmount();
    expect(activeResizeObservers()).toBe(0);
  });

  it("EDT-FR-UERT, EDT-FR-NYCP: a reached restore holds its offset against content that arrives later, until the author takes over", () => {
    holdTabScroll("tab-a", "doc", 480);
    render(inTab("tab-a"));
    expect(scroller().scrollTop).toBe(480);

    // Content arrives above the offset and the browser moves the scroller
    // without any input from the author (scroll anchoring).
    scroller().scrollTop = 824;
    act(() => resizeAll());
    expect(scroller().scrollTop).toBe(480);
    fireEvent.scroll(scroller());
    expect(tabScrollOf("tab-a", "doc")).toBe(480);

    fireEvent.wheel(scroller());
    scroller().scrollTop = 824;
    act(() => resizeAll());
    expect(scroller().scrollTop).toBe(824);
  });

  it("EDT-FR-UERT: scroll anchoring is off while a restore holds, and back on once the author takes over", () => {
    holdTabScroll("tab-a", "doc", 480);
    const view = render(inTab("tab-a"));
    expect(scroller().style.overflowAnchor).toBe("none");

    scrollTo(100);
    expect(scroller().style.overflowAnchor).toBe("");
    expect(tabScrollOf("tab-a", "doc")).toBe(100);

    view.unmount();
    render(inTab("tab-a"));
    expect(scroller().style.overflowAnchor).toBe("none");
    fireEvent.keyDown(scroller());
    expect(scroller().style.overflowAnchor).toBe("");
  });

  it("EDT-FR-UERT: a surface with nothing to restore keeps scroll anchoring on", () => {
    render(inTab("tab-a"));
    expect(scroller().style.overflowAnchor).toBe("");
  });
});
