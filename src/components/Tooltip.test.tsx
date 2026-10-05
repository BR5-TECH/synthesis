import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Tooltip, TOOLTIP_DELAY_MS } from "./Tooltip";

// SNV-shell-navigation.md SNV-FR-49: the activity bar's tooltips. Hover after a
// short delay, keyboard focus immediately, dismissed on leave / blur / Escape /
// activation, and placed on the strip's inner side.

function renderToggle(side: "left" | "right" = "left", label = "Notes") {
  return render(
    <Tooltip label={label} side={side}>
      {(aria) => (
        <button type="button" {...aria}>
          icon
        </button>
      )}
    </Tooltip>,
  );
}

describe("Tooltip (SNV-FR-49)", () => {
  // Real timers for the cases that do not depend on the delay itself.
  afterEach(cleanup);

  it("gives the control its accessible name without any hovering", () => {
    renderToggle();
    expect(screen.getByRole("button", { name: "Notes" })).toBeInTheDocument();
    expect(screen.queryByRole("tooltip")).toBeNull();
  });

  it("appears immediately on keyboard focus and clears on blur", async () => {
    renderToggle();
    const button = screen.getByRole("button", { name: "Notes" });

    button.focus();
    expect(await screen.findByRole("tooltip")).toHaveTextContent("Notes");
    // While it is up the control points at it, so a screen reader reads both.
    expect(button).toHaveAttribute(
      "aria-describedby",
      screen.getByRole("tooltip").id,
    );

    button.blur();
    await waitFor(() => expect(screen.queryByRole("tooltip")).toBeNull());
  });

  it("is dismissed by Escape", async () => {
    renderToggle();
    screen.getByRole("button", { name: "Notes" }).focus();
    await screen.findByRole("tooltip");

    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("tooltip")).toBeNull());
  });

  it("is dismissed by activating the control", async () => {
    // The panel the label describes has just moved, so the label is stale.
    renderToggle();
    await userEvent.click(screen.getByRole("button", { name: "Notes" }));
    expect(screen.queryByRole("tooltip")).toBeNull();
  });

  it("faces inward from whichever edge the strip is on (SNV-FR-49 / SNV-FR-06)", async () => {
    // A left-hand strip puts the bubble to its right; a right-hand strip to its
    // left, so it never overhangs the window edge.
    renderToggle("left");
    screen.getByRole("button", { name: "Notes" }).focus();
    expect(await screen.findByRole("tooltip")).toHaveAttribute(
      "data-side",
      "left",
    );

    cleanup();
    renderToggle("right");
    screen.getByRole("button", { name: "Notes" }).focus();
    expect(await screen.findByRole("tooltip")).toHaveAttribute(
      "data-side",
      "right",
    );
  });
});

describe("Tooltip hover delay (SNV-FR-49)", () => {
  // `fireEvent` rather than `userEvent` here: userEvent drives its own timers,
  // which deadlock against fake ones, and the delay is exactly what these two
  // are about.
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  it("waits out the delay before showing, so a sweep across the strip flashes nothing", () => {
    renderToggle();
    const anchor = document.querySelector(".tooltip-anchor") as HTMLElement;

    fireEvent.pointerEnter(anchor);
    expect(screen.queryByRole("tooltip")).toBeNull();

    // Just short of the delay: still nothing.
    act(() => void vi.advanceTimersByTime(TOOLTIP_DELAY_MS - 1));
    expect(screen.queryByRole("tooltip")).toBeNull();

    act(() => void vi.advanceTimersByTime(1));
    expect(screen.getByRole("tooltip")).toHaveTextContent("Notes");
  });

  it("closes again when the pointer leaves after it has appeared", () => {
    // Distinct from the case below: there the bubble never opened, so the
    // leave handler had nothing to close and could be missing entirely.
    renderToggle();
    const anchor = document.querySelector(".tooltip-anchor") as HTMLElement;

    fireEvent.pointerEnter(anchor);
    act(() => void vi.advanceTimersByTime(TOOLTIP_DELAY_MS));
    expect(screen.getByRole("tooltip")).toBeInTheDocument();

    fireEvent.pointerLeave(anchor);
    expect(screen.queryByRole("tooltip")).toBeNull();
  });

  it("shows nothing when the pointer leaves before the delay elapses", () => {
    renderToggle();
    const anchor = document.querySelector(".tooltip-anchor") as HTMLElement;

    fireEvent.pointerEnter(anchor);
    act(() => void vi.advanceTimersByTime(TOOLTIP_DELAY_MS / 2));
    fireEvent.pointerLeave(anchor);
    act(() => void vi.advanceTimersByTime(5000));

    expect(screen.queryByRole("tooltip")).toBeNull();
  });
});
