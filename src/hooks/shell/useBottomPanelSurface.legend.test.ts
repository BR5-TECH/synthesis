import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { useBottomPanelSurface } from "./useBottomPanelSurface";
import { diagramLegendHidden, resetDiagramLegendForTest } from "../../state/diagramLegend";
import { resetDiscussionColumns } from "../../state/discussionColumn";
import * as layout from "../../state/layoutPreferences";
import { resetLayoutPreferencesCache } from "../../state/layoutPreferences";
import { resetLogBufferForTest } from "../../logging";
import type { LayoutPreferences } from "../../types";

/** The layout record of each project slot. */
let slots: Record<string, LayoutPreferences>;
/** The project that the backend serves the layout calls from. */
let current: string;

beforeEach(() => {
  slots = {};
  current = "";
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "load_layout_preferences") return Promise.resolve(slots[current] ?? null);
    if (cmd === "append_log_records") return Promise.resolve(undefined);
    return Promise.reject(new Error(`unexpected invoke ${cmd}`));
  });
  resetLayoutPreferencesCache();
  resetDiagramLegendForTest();
  resetDiscussionColumns();
  resetLogBufferForTest();
});

afterEach(() => {
  resetLayoutPreferencesCache();
  resetDiagramLegendForTest();
});

describe("the legend state when a project opens", () => {
  it("EDT-FR-NGCW, SNV-FR-08: each project opens on its own stored legend state", async () => {
    slots = { "/a": { diagramLegendHidden: true } };
    current = "/a";
    const { rerender } = renderHook(
      ({ path }: { path: string }) => useBottomPanelSurface("ide", path),
      { initialProps: { path: "/a" } },
    );
    await waitFor(() => expect(diagramLegendHidden()).toBe(true));

    current = "/b";
    rerender({ path: "/b" });
    // A project with no stored value shows the legend.
    expect(diagramLegendHidden()).toBe(false);
    await waitFor(() =>
      expect(invokeMock.mock.calls.filter(([c]) => c === "load_layout_preferences")).toHaveLength(2),
    );
    expect(diagramLegendHidden()).toBe(false);
  });
});

describe("the legend state when the project closes", () => {
  it("EDT-FR-NGCW, EDT-FR-DNVJ: a focus after the close reads nothing, and the project opens again on its own record", async () => {
    slots = { "/a": { diagramLegendHidden: true, bottomPanelSurface: "git" } };
    current = "/a";
    const { rerender } = renderHook(
      ({ screen, path }: { screen: "picker" | "ide"; path: string }) =>
        useBottomPanelSurface(screen, path),
      { initialProps: { screen: "ide" as "picker" | "ide", path: "/a" } },
    );
    await waitFor(() => expect(diagramLegendHidden()).toBe(true));

    current = "";
    slots[""] = { bottomPanelSurface: "history" };
    rerender({ screen: "picker", path: "" });
    const loads = () => invokeMock.mock.calls.filter(([c]) => c === "load_layout_preferences").length;
    const before = loads();
    const reload = vi.spyOn(layout, "reloadLayoutPreferences");
    window.dispatchEvent(new Event("focus"));
    // Let every queued layout step run before the count.
    await layout.reloadLayoutPreferences("");
    expect(reload).toHaveBeenCalledTimes(1);
    expect(loads()).toBe(before + 1);
    expect(diagramLegendHidden()).toBe(false);

    current = "/a";
    const { result } = renderHook(() => useBottomPanelSurface("ide", "/a"));
    await waitFor(() => expect(result.current.bottomSurface).toBe("git"));
    await waitFor(() => expect(diagramLegendHidden()).toBe(true));
  });
});
