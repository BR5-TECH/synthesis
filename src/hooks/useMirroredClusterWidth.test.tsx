import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";

import { useMirroredClusterWidth } from "./useMirroredClusterWidth";

// jsdom does no layout, so each cluster's width is stubbed on the element and
// the ResizeObserver is a test double that the test fires by hand.
let observerCallbacks: Array<() => void> = [];
let observed: Element[] = [];
let disconnects = 0;

beforeEach(() => {
  observerCallbacks = [];
  observed = [];
  disconnects = 0;
  widths = new WeakMap();
  vi.stubGlobal(
    "ResizeObserver",
    class {
      constructor(cb: () => void) {
        observerCallbacks.push(cb);
      }
      observe(el: Element) {
        observed.push(el);
      }
      disconnect() {
        disconnects += 1;
      }
    },
  );
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

// The width each element reports. A re-render does not write it, so a width a
// test sets stays until the test sets another.
let widths = new WeakMap<Element, number>();

function setWidth(el: HTMLElement, width: number) {
  widths.set(el, width);
  el.getBoundingClientRect = () => {
    const w = widths.get(el) ?? 0;
    return { width: w, height: 0, top: 0, left: 0, right: w, bottom: 0, x: 0, y: 0, toJSON: () => ({}) } as DOMRect;
  };
}

/** A chrome whose clusters report `left` and `right` from their first measure. */
function Chrome({ left, right }: { left: number; right: number }) {
  const { leftRef, rightRef, style } = useMirroredClusterWidth<HTMLDivElement, HTMLDivElement>();
  // Only the first attachment sets the width, so a re-render keeps the width
  // that a test set later.
  const stub = (width: number) => (el: HTMLDivElement | null) => {
    if (el && !widths.has(el)) setWidth(el, width);
  };
  return (
    <div data-testid="chrome" style={style}>
      <div
        data-testid="left"
        ref={(el) => {
          stub(left)(el);
          leftRef.current = el;
        }}
      />
      <div
        data-testid="right"
        ref={(el) => {
          stub(right)(el);
          rightRef.current = el;
        }}
      />
    </div>
  );
}

const side = () => screen.getByTestId("chrome").style.getPropertyValue("--top-chrome-side");

describe("useMirroredClusterWidth (SNV-FR-HMCZ)", () => {
  it("SNV-FR-HMCZ: sets both side tracks to the leading cluster when it is wider", () => {
    render(<Chrome left={503} right={84} />);
    expect(side()).toBe("503px");
  });

  it("SNV-FR-HMCZ: sets both side tracks to the trailing cluster when it is wider", () => {
    render(<Chrome left={120} right={300} />);
    expect(side()).toBe("300px");
  });

  it("SNV-FR-HMCZ: rounds a fractional width up so the wider cluster always fits", () => {
    render(<Chrome left={502.2} right={84} />);
    expect(side()).toBe("503px");
  });

  it("SNV-FR-HMCZ: follows a cluster that grows or shrinks after mount", () => {
    render(<Chrome left={503} right={84} />);
    expect(observed).toEqual([screen.getByTestId("left"), screen.getByTestId("right")]);

    setWidth(screen.getByTestId("left"), 610);
    act(() => observerCallbacks.forEach((cb) => cb()));
    expect(side()).toBe("610px");

    setWidth(screen.getByTestId("left"), 60);
    act(() => observerCallbacks.forEach((cb) => cb()));
    expect(side()).toBe("84px");
  });

  it("SNV-FR-HMCZ: leaves the property unset before the clusters have a width", () => {
    // Each side track then falls back to its own cluster's width.
    render(<Chrome left={0} right={0} />);
    expect(side()).toBe("");
  });

  it("SNV-FR-HMCZ: keeps one subscription across re-renders", () => {
    const { rerender } = render(<Chrome left={503} right={84} />);
    setWidth(screen.getByTestId("right"), 700);
    rerender(<Chrome left={503} right={84} />);
    expect(observerCallbacks).toHaveLength(1);
    expect(observed).toHaveLength(2);
    // The re-render left the width the test set.
    act(() => observerCallbacks.forEach((cb) => cb()));
    expect(side()).toBe("700px");
  });

  it("SNV-FR-HMCZ: stops observing when the chrome unmounts", () => {
    const { unmount } = render(<Chrome left={503} right={84} />);
    unmount();
    expect(disconnects).toBe(1);
  });

  it("SNV-FR-HMCZ: measures once without a ResizeObserver", () => {
    vi.stubGlobal("ResizeObserver", undefined);
    render(<Chrome left={503} right={84} />);
    expect(side()).toBe("503px");
  });
});
