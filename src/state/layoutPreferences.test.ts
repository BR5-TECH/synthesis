import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  loadLayoutPreferences,
  patchLayoutPreferences,
  reloadLayoutPreferences,
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

describe("reloadLayoutPreferences", () => {
  it("EDT-FR-DNVJ: it reads past the cache, and a later patch builds on what the other window wrote", async () => {
    stored = { bottomPanelHeight: 200 };
    await loadLayoutPreferences("/p");
    // The Settings window writes the record behind this window's cache.
    stored = { bottomPanelHeight: 200, diagramLegendHidden: true };
    expect(await loadLayoutPreferences("/p")).toEqual({ bottomPanelHeight: 200 });
    expect(await reloadLayoutPreferences("/p")).toEqual({
      bottomPanelHeight: 200,
      diagramLegendHidden: true,
    });
    await patchLayoutPreferences("/p", { bottomPanelHeight: 240 });
    expect(saved[saved.length - 1]).toEqual({ bottomPanelHeight: 240, diagramLegendHidden: true });
  });

  it("EDT-FR-DNVJ: it waits for a patch in flight, so it reads the record that the patch wrote", async () => {
    let releaseSave: () => void = () => {};
    invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "load_layout_preferences") return Promise.resolve(stored && { ...stored });
      if (cmd === "save_layout_preferences") {
        return new Promise<void>((resolve) => {
          releaseSave = () => {
            stored = args?.preferences as LayoutPreferences;
            resolve();
          };
        });
      }
      return Promise.reject(new Error(`unexpected invoke ${cmd}`));
    });
    const loads = () =>
      invokeMock.mock.calls.filter(([cmd]) => cmd === "load_layout_preferences").length;
    const patch = patchLayoutPreferences("/p", { verticalPanelHidden: true });
    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("save_layout_preferences", expect.anything()),
    );
    const reload = reloadLayoutPreferences("/p");
    for (let i = 0; i < 5; i += 1) await Promise.resolve();
    expect(loads()).toBe(1);
    releaseSave();
    await patch;
    expect(await reload).toEqual({ verticalPanelHidden: true });
    expect(loads()).toBe(2);
  });

  it("EDT-FR-DNVJ: a failed read answers an empty record and does not stop later patches", async () => {
    invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "load_layout_preferences") return Promise.reject(new Error("no slot"));
      if (cmd === "save_layout_preferences") {
        saved.push(args?.preferences as LayoutPreferences);
        return Promise.resolve(undefined);
      }
      return Promise.reject(new Error(`unexpected invoke ${cmd}`));
    });
    expect(await reloadLayoutPreferences("/p")).toEqual({});
    await patchLayoutPreferences("/p", { bottomPanelHeight: 1 });
    expect(saved).toEqual([{ bottomPanelHeight: 1 }]);
  });
});

describe("reloadLayoutPreferences and the live-project guard", () => {
  it("EDT-FR-DNVJ: a reload for a project that is no longer open reads nothing and leaves the cache as it was", async () => {
    stored = { bottomPanelSurface: "git" };
    setActiveLayoutProject("/a");
    await loadLayoutPreferences("/a");
    setActiveLayoutProject("/b");
    const loadsBefore = invokeMock.mock.calls.filter(([c]) => c === "load_layout_preferences").length;
    expect(await reloadLayoutPreferences("/a")).toEqual({});
    expect(invokeMock.mock.calls.filter(([c]) => c === "load_layout_preferences")).toHaveLength(
      loadsBefore,
    );
  });

  it("EDT-FR-DNVJ: a reload whose project closes while it reads does not cache the answer", async () => {
    let answer: (value: LayoutPreferences | null) => void = () => {};
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "load_layout_preferences") {
        return new Promise((resolve) => {
          answer = resolve;
        });
      }
      return Promise.resolve(undefined);
    });
    setActiveLayoutProject("/a");
    const reload = reloadLayoutPreferences("/a");
    await vi.waitFor(() =>
      expect(invokeMock.mock.calls.some(([c]) => c === "load_layout_preferences")).toBe(true),
    );
    setActiveLayoutProject("/b");
    answer({ bottomPanelSurface: "history" });
    expect(await reload).toEqual({});
    // The next read of A goes to the backend again.
    setActiveLayoutProject("/a");
    invokeMock.mockImplementation((cmd: string) =>
      Promise.resolve(cmd === "load_layout_preferences" ? { bottomPanelSurface: "git" } : undefined),
    );
    expect(await loadLayoutPreferences("/a")).toEqual({ bottomPanelSurface: "git" });
  });
});
