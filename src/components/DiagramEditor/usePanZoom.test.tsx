import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useRef } from "react";

vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

import { usePanZoom, type PanZoom } from "./usePanZoom";

const NATURAL = { width: 400, height: 300 };
let view: PanZoom;

function Area() {
  const ref = useRef<HTMLDivElement>(null);
  view = usePanZoom(ref, NATURAL, "render");
  return (
    <div
      ref={(el) => {
        ref.current = el;
        if (el) {
          Object.defineProperty(el, "clientWidth", { configurable: true, value: 800 });
          Object.defineProperty(el, "clientHeight", { configurable: true, value: 600 });
        }
      }}
      data-testid="area"
      data-fitted={view.fitted ? "true" : undefined}
      onPointerDown={view.onPointerDown}
    >
      <button type="button" data-testid="item">
        item
      </button>
    </div>
  );
}

const pointer = (type: string, init: Record<string, number>) => {
  const event = new MouseEvent(type, { bubbles: true, ...init }) as MouseEvent & {
    pointerId: number;
  };
  Object.defineProperty(event, "pointerId", { value: 1 });
  return event;
};

afterEach(() => cleanup());

describe("the pan and zoom of a drawing (EDT-FR-PDBC, CAD-FR-MBML)", () => {
  it("CAD-FR-KUBU, EDT-FR-PDBC: a press and release with no move pans nothing and clicks the item", () => {
    render(<Area />);
    const before = view.transform;
    const clicked = vi.fn();
    screen.getByTestId("item").addEventListener("click", clicked);
    fireEvent.pointerDown(screen.getByTestId("item"), { button: 0, clientX: 10, clientY: 10 });
    const area = screen.getByTestId("area");
    act(() => {
      area.dispatchEvent(pointer("pointermove", { clientX: 11, clientY: 11, buttons: 1 }));
      area.dispatchEvent(pointer("pointerup", { clientX: 11, clientY: 11 }));
    });
    fireEvent.click(screen.getByTestId("item"));
    expect(view.transform).toEqual(before);
    expect(clicked).toHaveBeenCalledTimes(1);
  });

  it("EDT-FR-PDBC: a drag pans the drawing and ends the fit", () => {
    render(<Area />);
    const before = view.transform;
    fireEvent.pointerDown(screen.getByTestId("area"), { button: 0, clientX: 10, clientY: 10 });
    act(() => {
      screen.getByTestId("area").dispatchEvent(pointer("pointermove", { clientX: 40, clientY: 30, buttons: 1 }));
    });
    expect(view.transform.x).toBe(before.x + 30);
    expect(view.transform.y).toBe(before.y + 20);
    expect(screen.getByTestId("area")).not.toHaveAttribute("data-fitted");
  });

  it("EDT-FR-PDBC: a release outside the area before a drag began leaves no pan behind", () => {
    render(<Area />);
    const before = view.transform;
    fireEvent.pointerDown(screen.getByTestId("area"), { button: 0, clientX: 10, clientY: 10 });
    act(() => {
      // The button comes up outside the area, then the pointer moves over it.
      window.dispatchEvent(pointer("pointerup", { clientX: 900, clientY: 900 }));
      screen.getByTestId("area").dispatchEvent(pointer("pointermove", { clientX: 200, clientY: 200 }));
      screen.getByTestId("area").dispatchEvent(pointer("pointermove", { clientX: 300, clientY: 300 }));
    });
    expect(view.transform).toEqual(before);
  });

  it("CAD-FR-MXHO: reveal keeps a box that shows and centres one that does not", () => {
    render(<Area />);
    const before = view.transform;
    act(() => view.reveal({ x: 10, y: 10, width: 50, height: 50 }));
    expect(view.transform).toEqual(before);
    expect(screen.getByTestId("area")).toHaveAttribute("data-fitted", "true");
    act(() => view.reveal({ x: 5000, y: 4000, width: 100, height: 100 }));
    expect(view.transform.x).toBeCloseTo(400 - 5050 * view.transform.scale);
    expect(view.transform.y).toBeCloseTo(300 - 4050 * view.transform.scale);
    expect(screen.getByTestId("area")).not.toHaveAttribute("data-fitted");
  });
});
