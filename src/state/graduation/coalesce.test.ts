import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { COALESCE_MS, coalesce } from "./coalesce";

describe("coalesce", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("GRD-FR-EFAU: a burst of events inside one window reads the listing once", () => {
    const reload = vi.fn();
    const coalesced = coalesce(reload);

    coalesced.ask();
    coalesced.ask();
    coalesced.ask();
    expect(reload).not.toHaveBeenCalled();

    vi.advanceTimersByTime(COALESCE_MS);
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it("GRD-FR-EFAU: the first ask arms the window, so a steady stream still reads", () => {
    const reload = vi.fn();
    const coalesced = coalesce(reload, 100);

    // An event every 60ms, which a restarting debounce would starve for good.
    for (let elapsed = 0; elapsed < 300; elapsed += 60) {
      coalesced.ask();
      vi.advanceTimersByTime(60);
    }
    expect(reload.mock.calls.length).toBeGreaterThanOrEqual(2);
  });

  it("GRD-FR-EFAU: a later window reads again", () => {
    const reload = vi.fn();
    const coalesced = coalesce(reload, 100);

    coalesced.ask();
    vi.advanceTimersByTime(100);
    coalesced.ask();
    vi.advanceTimersByTime(100);
    expect(reload).toHaveBeenCalledTimes(2);
  });

  it("GRD-FR-EFAU: a cancelled window reads nothing, and cancelling twice is safe", () => {
    const reload = vi.fn();
    const coalesced = coalesce(reload, 100);

    coalesced.ask();
    coalesced.cancel();
    coalesced.cancel();
    vi.advanceTimersByTime(500);
    expect(reload).not.toHaveBeenCalled();
  });
});

describe("coalesce after cancel", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("GRD-FR-EFAU: an event arriving after cancel reloads nothing", () => {
    const reload = vi.fn();
    const coalesced = coalesce(reload, 100);

    // Both callers unsubscribe asynchronously, so an event can still arrive
    // after the cleanup ran. It must not reload an unmounted surface.
    coalesced.cancel();
    coalesced.ask();
    vi.advanceTimersByTime(500);
    expect(reload).not.toHaveBeenCalled();
  });

  it("GRD-FR-EFAU: a window armed before cancel does not fire after it", () => {
    const reload = vi.fn();
    const coalesced = coalesce(reload, 100);

    coalesced.ask();
    vi.advanceTimersByTime(50);
    coalesced.cancel();
    vi.advanceTimersByTime(500);
    expect(reload).not.toHaveBeenCalled();
  });
});
