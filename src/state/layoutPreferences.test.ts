import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  loadLayoutPreferences,
  patchLayoutPreferences,
  resetLayoutPreferencesCache,
  setActiveLayoutProject,
} from "./layoutPreferences";
import type { LayoutPreferences } from "../types";

// The shared per-project layout record (SNV-FR-08 / GSS-FR-17). Several hooks
// write different fields into it on different triggers, so the merge, the
// serialisation, and the project guard are what keep one writer from reverting
// another — or from writing into the wrong project's slot.

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

/** Every record handed to `save_layout_preferences`, in order. */
let saved: LayoutPreferences[];
let stored: LayoutPreferences | null;

beforeEach(() => {
  saved = [];
  stored = null;
  invokeMock.mockReset();
  invokeMock.mockImplementation(
    async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "load_layout_preferences") return stored;
      if (cmd === "save_layout_preferences") {
        saved.push(args?.preferences as LayoutPreferences);
        return undefined;
      }
      return undefined;
    },
  );
  resetLayoutPreferencesCache();
});

afterEach(() => resetLayoutPreferencesCache());

describe("patchLayoutPreferences merging", () => {
  it("composes writes from independent writers rather than reverting them", async () => {
    // The failure this exists to prevent: drag the panel, then resize the
    // window, and the panel width is gone on relaunch.
    await patchLayoutPreferences("/p", { verticalPanelFraction: 0.42 });
    await patchLayoutPreferences("/p", { mainWindowMaximized: true });

    expect(saved[saved.length - 1]).toEqual({
      verticalPanelFraction: 0.42,
      mainWindowMaximized: true,
    });
  });

  it("serialises overlapping patches so neither edit is lost", async () => {
    await Promise.all([
      patchLayoutPreferences("/p", { bottomPanelSurface: "git" }),
      patchLayoutPreferences("/p", { verticalPanelHidden: true }),
    ]);

    expect(saved[saved.length - 1]).toEqual({
      bottomPanelSurface: "git",
      verticalPanelHidden: true,
    });
  });
});

describe("the live-project guard", () => {
  it("writes normally while the patch's project is the live one", async () => {
    setActiveLayoutProject("/a");
    await patchLayoutPreferences("/a", { bottomPanelSurface: "git" });

    expect(saved).toHaveLength(1);
    expect(saved[0].bottomPanelSurface).toBe("git");
  });

  it("drops a patch whose project is no longer open", async () => {
    // `save_layout_preferences` writes to whichever slot the backend resolves
    // to now, so a patch for A landing after the switch to B would put A's
    // record in B's slot.
    setActiveLayoutProject("/a");
    const pending = patchLayoutPreferences("/a", { bottomPanelSurface: "git" });
    setActiveLayoutProject("/b");
    await pending;

    expect(saved).toHaveLength(0);
  });

  it("drops a patch issued while the project is closing", async () => {
    setActiveLayoutProject("/a");
    const pending = patchLayoutPreferences("/a", { verticalPanelHidden: true });
    setActiveLayoutProject("");
    await pending;

    expect(saved).toHaveLength(0);
  });

  it("writes when no project has been declared, so the guard is opt-in", async () => {
    await patchLayoutPreferences("/a", { verticalPanelHidden: true });
    expect(saved).toHaveLength(1);
  });

  it("does not poison the cache with a dropped patch", async () => {
    // A dropped write must not leave the module believing it succeeded: the
    // cached record has to stay as the backend has it, without the field the
    // abandoned patch carried.
    stored = { verticalPanelFraction: 0.25 };
    setActiveLayoutProject("/a");
    const pending = patchLayoutPreferences("/a", { verticalPanelHidden: true });
    setActiveLayoutProject("/b");
    await pending;

    setActiveLayoutProject("/a");
    expect(await loadLayoutPreferences("/a")).toEqual({
      verticalPanelFraction: 0.25,
    });
    // And the next accepted patch builds on that, not on the dropped one.
    await patchLayoutPreferences("/a", { bottomPanelSurface: "runs" });
    expect(saved[saved.length - 1]).toEqual({
      verticalPanelFraction: 0.25,
      bottomPanelSurface: "runs",
    });
  });
});
