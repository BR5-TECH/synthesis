import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";

import { StatusBar, type StatusBarProps } from "./StatusBar";
import { HIDE_DELAY_MS, SHOW_DELAY_MS } from "../hooks/useSteadyProgress";
import type { Operation } from "../types";

// STB-status-bar.md — the center region with its real timing. The other
// StatusBar suites replace the timing so they can check the rendering alone.
// Fake timers only: no result here depends on real time.

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

function operation(id: string, overrides: Partial<Operation> = {}): Operation {
  return {
    id,
    kind: "index",
    label: `Operation ${id}`,
    state: "running",
    sequence: Number(id.replace(/\D/g, "")) || 1,
    ...overrides,
  };
}

function props(overrides: Partial<StatusBarProps> = {}): StatusBarProps {
  return {
    onSettings: vi.fn(),
    operations: [],
    overlayOpen: false,
    onOpenOverlay: vi.fn(),
    onCloseOverlay: vi.fn(),
    onActivateOperation: vi.fn(),
    lineEndings: "lf",
    onSelectLineEndings: vi.fn(),
    indentation: null,
    onSelectIndentation: vi.fn(),
    diffTotals: null,
    ...overrides,
  };
}

function advance(ms: number) {
  act(() => {
    vi.advanceTimersByTime(ms);
  });
}

describe("centre region timing (STB-FR-NAEX, STB-FR-JWLN, STB-FR-KPRU)", () => {
  it("STB-FR-NAEX, STB-FR-07: a short operation leaves the region empty for its whole life", () => {
    const view = render(<StatusBar {...props({ operations: [operation("op-1")] })} />);
    expect(screen.getByTestId("status-bar-center")).toBeEmptyDOMElement();
    advance(SHOW_DELAY_MS - 1);
    expect(screen.getByTestId("status-bar-center")).toBeEmptyDOMElement();
    view.rerender(<StatusBar {...props()} />);
    advance(SHOW_DELAY_MS * 5);
    expect(screen.getByTestId("status-bar-center")).toBeEmptyDOMElement();
  });

  it("STB-FR-JWLN: a held bar with nothing in flight is not a click target", () => {
    const p = props({ operations: [operation("op-1")] });
    const view = render(<StatusBar {...p} />);
    advance(SHOW_DELAY_MS);
    const bar = screen.getByTestId("status-bar-progress");
    expect(bar).toBeEnabled();

    view.rerender(<StatusBar {...p} operations={[]} />);
    expect(screen.getByText("Operation op-1")).toBeInTheDocument();
    expect(bar).toBeDisabled();
    fireEvent.click(bar);
    expect(p.onOpenOverlay).not.toHaveBeenCalled();

    advance(HIDE_DELAY_MS * 2);
    expect(screen.queryByTestId("status-bar-progress")).not.toBeInTheDocument();
  });

  it("STB-FR-KPRU: an operation that starts during the hold replaces the held one on the same node", () => {
    const p = props({ operations: [operation("op-1")] });
    const view = render(<StatusBar {...p} />);
    advance(SHOW_DELAY_MS);
    advance(2000);
    const bar = screen.getByTestId("status-bar-progress");
    const track = screen.getByTestId("progress-track");

    view.rerender(<StatusBar {...p} operations={[]} />);
    advance(HIDE_DELAY_MS - 100);
    view.rerender(<StatusBar {...p} operations={[operation("op-2")]} />);

    expect(screen.getByTestId("status-bar-progress")).toBe(bar);
    expect(screen.getByTestId("progress-track")).toBe(track);
    expect(bar).toBeEnabled();
    expect(screen.getByText("Operation op-2")).toBeInTheDocument();
    advance(HIDE_DELAY_MS * 2);
    expect(screen.getByTestId("status-bar-progress")).toBe(bar);
    // STB-FR-10: the replacing operation is in flight, so the bar opens the
    // overlay again.
    fireEvent.click(bar);
    expect(p.onOpenOverlay).toHaveBeenCalledTimes(1);
  });
});

describe("overlay closing (STB-FR-14)", () => {
  it("STB-FR-14: the overlay closes as soon as the last in-flight operation terminates, not after the hold", () => {
    const p = props({ operations: [operation("op-1")], overlayOpen: true });
    const view = render(<StatusBar {...p} />);
    advance(SHOW_DELAY_MS);
    expect(p.onCloseOverlay).not.toHaveBeenCalled();

    view.rerender(<StatusBar {...p} operations={[]} />);
    expect(p.onCloseOverlay).toHaveBeenCalledTimes(1);
    // The bar itself is still held.
    expect(screen.getByTestId("status-bar-progress")).toBeInTheDocument();
  });

  it("STB-FR-14: the overlay stays open while another operation is in flight", () => {
    const a = operation("op-1");
    const b = operation("op-2");
    const p = props({ operations: [b, a], overlayOpen: true });
    const view = render(<StatusBar {...p} />);
    advance(SHOW_DELAY_MS);
    view.rerender(<StatusBar {...p} operations={[a]} />);
    expect(p.onCloseOverlay).not.toHaveBeenCalled();
  });
});
