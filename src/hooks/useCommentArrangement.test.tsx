/**
 * Where the cards go, decided from the tab's width (`CMT-comments.md`
 * CMT-FR-64).
 *
 * The arithmetic is tested as a function, because it is what decides whether a
 * card is laid over the prose. The hook around it is tested with a stubbed
 * `ResizeObserver` and a stubbed `getBoundingClientRect`: jsdom implements
 * neither, so without the stubs the only branch a rendering test could reach is
 * the fallback — and the observer is the branch that actually runs in the app.
 * `style-invariants.test.ts` holds the numbers to the stylesheet's own tokens.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { render, cleanup, act } from "@testing-library/react";
import { useRef } from "react";
import {
  BESIDE_MIN_WIDTH,
  railArrangement,
  useCommentArrangement,
} from "./useCommentArrangement";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  // The geometry stub is a spy on a prototype rather than a global, so it
  // outlives `unstubAllGlobals` — and a leaked width of 1000 makes the
  // unmeasured case below report the stacked arrangement for the wrong reason.
  vi.restoreAllMocks();
});

describe("CMT-FR-64: beside the page, or below it", () => {
  it("keeps the cards beside the page while one still fits there", () => {
    // The page slid as far leading as its inset allows, one card, and both
    // gutters — exactly the width at which the margin still holds a card.
    expect(railArrangement(BESIDE_MIN_WIDTH)).toBe("beside");
    expect(railArrangement(BESIDE_MIN_WIDTH + 400)).toBe("beside");
    // A maximised window on any display the shell supports.
    expect(railArrangement(1999)).toBe("beside");
  });

  it("moves them below the page one pixel before a card would be cramped", () => {
    expect(railArrangement(BESIDE_MIN_WIDTH - 1)).toBe("below");
    // The minimum window size the shell supports, less the activity bar and a
    // Library panel: the case the second screenshot was taken in.
    expect(railArrangement(910)).toBe("below");
  });

  it("decides sub-pixel widths on the same side of the line", () => {
    // `getBoundingClientRect` returns fractions on a scaled display, so the
    // boundary is never actually hit as an integer.
    expect(railArrangement(BESIDE_MIN_WIDTH - 0.5)).toBe("below");
    expect(railArrangement(BESIDE_MIN_WIDTH + 0.4)).toBe("beside");
  });

  it("reads anything that is not a measurement as an ordinary tab", () => {
    // A tab that has not been laid out yet reports zero. Reading that as narrow
    // would flip every rail into the stacked arrangement for a frame on the way
    // in. `NaN` fails every comparison, so without its own guard it would fall
    // through to the stacked arrangement rather than away from it.
    expect(railArrangement(0)).toBe("beside");
    expect(railArrangement(-1)).toBe("beside");
    expect(railArrangement(Number.NaN)).toBe("beside");
  });
});

describe("CMT-FR-64: the tab is watched, not merely measured once", () => {
  /**
   * A `ResizeObserver` that reports what the test says, when the test says.
   * Returns a handle for driving it, so a resize is a deliberate act rather
   * than something the environment might or might not deliver.
   */
  function stubObserver() {
    const callbacks: Array<() => void> = [];
    let disconnects = 0;
    class FakeResizeObserver {
      constructor(cb: () => void) {
        callbacks.push(cb);
      }
      observe() {}
      disconnect() {
        disconnects += 1;
      }
      unobserve() {}
    }
    vi.stubGlobal("ResizeObserver", FakeResizeObserver);
    return {
      fire: () => act(() => callbacks.forEach((cb) => cb())),
      get disconnects() {
        return disconnects;
      },
    };
  }

  function Probe() {
    const ref = useRef<HTMLDivElement>(null);
    const arrangement = useCommentArrangement(ref);
    return (
      <div ref={ref} data-testid="tab" data-comments={arrangement}>
        tab
      </div>
    );
  }

  /** Every element measures `width`, which is all the hook reads. */
  function widthIs(width: number) {
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
      width,
      height: 0,
      top: 0,
      left: 0,
      right: width,
      bottom: 0,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    } as DOMRect);
  }

  it("moves the cards when the tab narrows and back when it widens", () => {
    // The case the hook exists for, and the one a single measurement on mount
    // cannot serve: dragging the Library panel wider narrows the tab without
    // the window resizing at all.
    const observer = stubObserver();
    widthIs(1400);
    const { getByTestId, unmount } = render(<Probe />);
    expect(getByTestId("tab").dataset.comments).toBe("beside");

    widthIs(1000);
    observer.fire();
    expect(getByTestId("tab").dataset.comments).toBe("below");

    widthIs(1400);
    observer.fire();
    expect(getByTestId("tab").dataset.comments).toBe("beside");

    // Nothing keeps observing a tab that has closed.
    unmount();
    expect(observer.disconnects).toBe(1);
  });

  it("falls back to the window where there is no observer to watch with", () => {
    vi.stubGlobal("ResizeObserver", undefined);
    widthIs(1400);
    const { getByTestId } = render(<Probe />);
    expect(getByTestId("tab").dataset.comments).toBe("beside");

    widthIs(1000);
    act(() => {
      window.dispatchEvent(new Event("resize"));
    });
    expect(getByTestId("tab").dataset.comments).toBe("below");
  });

  it("reports the ordinary arrangement where nothing can be measured", () => {
    // jsdom's own geometry: every element is zero-width. The rail has to render
    // as it does in a laid-out tab rather than as it would in a narrow one.
    const { getByTestId } = render(<Probe />);
    expect(getByTestId("tab").dataset.comments).toBe("beside");
  });
});
