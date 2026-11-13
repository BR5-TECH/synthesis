/**
 * The set of tabs that state an archived change
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-AKVG,
 * OCW-FR-GVZT).
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  archivedAsOf,
  archivedChangeViewsVersion,
  keepChangeView,
  pruneChangeViews,
  resetChangeViewsForTest,
  subscribeChangeViews,
} from "./changeView";

beforeEach(() => {
  resetChangeViewsForTest();
});

describe("the archived set (OCW-FR-AKVG, OCW-FR-GVZT)", () => {
  it("OCW-FR-GVZT: only a change of the archived set moves the version and calls the listeners", () => {
    const listener = vi.fn();
    subscribeChangeViews(listener);
    keepChangeView("t-1", { selected: "proposal" });
    expect(archivedChangeViewsVersion()).toBe(0);
    expect(listener).not.toHaveBeenCalled();

    keepChangeView("t-1", { archivedAs: "2026-11-11-add-logout" });
    expect(archivedAsOf("t-1")).toBe("2026-11-11-add-logout");
    expect(archivedChangeViewsVersion()).toBe(1);
    expect(listener).toHaveBeenCalledTimes(1);

    // The same name again and a mode change move nothing.
    keepChangeView("t-1", { archivedAs: "2026-11-11-add-logout", modes: { "proposal.md": "source" } });
    expect(archivedChangeViewsVersion()).toBe(1);

    keepChangeView("t-1", { archivedAs: null });
    expect(archivedAsOf("t-1")).toBeNull();
    expect(archivedChangeViewsVersion()).toBe(2);
    expect(listener).toHaveBeenCalledTimes(2);
  });

  it("OCW-FR-GVZT: pruning an archived tab takes it out of the set, and pruning a tab that is not archived moves nothing", () => {
    const listener = vi.fn();
    keepChangeView("t-1", { archivedAs: "2026-11-11-add-logout" });
    keepChangeView("t-2", { selected: "design" });
    subscribeChangeViews(listener);

    pruneChangeViews(new Set(["t-1"]));
    expect(listener).not.toHaveBeenCalled();
    expect(archivedAsOf("t-1")).toBe("2026-11-11-add-logout");

    const before = archivedChangeViewsVersion();
    pruneChangeViews(new Set());
    expect(archivedAsOf("t-1")).toBeNull();
    expect(archivedChangeViewsVersion()).toBe(before + 1);
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("OCW-FR-AKVG: an unsubscribed listener is not called", () => {
    const listener = vi.fn();
    const off = subscribeChangeViews(listener);
    off();
    keepChangeView("t-1", { archivedAs: "x" });
    expect(listener).not.toHaveBeenCalled();
  });
});
