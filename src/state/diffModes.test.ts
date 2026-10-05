import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  DEFAULT_DIFF_MODES,
  hydrateDiffModes,
  peekDiffModes,
  resetDiffModes,
  setRenderingMode,
  setVisualizationMode,
  subscribe,
} from "./diffModes";
import { resetAppPreferencesCache } from "./appPreferences";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const saves = () =>
  invokeMock.mock.calls
    .filter((c) => c[0] === "save_app_preferences")
    .map((c) => (c[1] as { preferences: Record<string, unknown> }).preferences);

beforeEach(() => {
  invokeMock.mockReset();
  resetDiffModes();
  resetAppPreferencesCache();
});

afterEach(() => {
  resetDiffModes();
  resetAppPreferencesCache();
});

describe("diff modes store (DFV-FR-23 / DFV-FR-24)", () => {
  it("starts a user who has never chosen in Unified and Source", async () => {
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "load_app_preferences" ? { theme: "system" } : undefined,
    );
    await hydrateDiffModes();
    expect(peekDiffModes()).toEqual(DEFAULT_DIFF_MODES);
  });

  it("reads both stored modes, as a relaunch would", async () => {
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "load_app_preferences"
        ? { theme: "dark", diffVisualizationMode: "final", diffRenderingMode: "rich" }
        : undefined,
    );
    await hydrateDiffModes();
    expect(peekDiffModes()).toEqual({ visualization: "final", rendering: "rich" });
  });

  it("reads the record once however many tabs mount", async () => {
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "load_app_preferences" ? { theme: "system" } : undefined,
    );
    await Promise.all([hydrateDiffModes(), hydrateDiffModes(), hydrateDiffModes()]);
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "load_app_preferences"),
    ).toHaveLength(1);
  });

  it("writes a patch, so the rest of the record survives (GSS-FR-20 / GLS-FR-14)", async () => {
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "load_app_preferences"
        ? {
            theme: "dark",
            mainWindowFullscreen: true,
            searchQueryMode: "regex",
            diffVisualizationMode: "unified",
            diffRenderingMode: "source",
          }
        : undefined,
    );
    await hydrateDiffModes();

    setVisualizationMode("side_by_side");
    await vi.waitFor(() => expect(saves()).toHaveLength(1));

    // A bare `{ diffVisualizationMode }` would clear the theme, the full-screen
    // flag, and the query mode — all edited from elsewhere entirely.
    expect(saves()[0]).toEqual({
      theme: "dark",
      mainWindowFullscreen: true,
      searchQueryMode: "regex",
      diffVisualizationMode: "side_by_side",
      diffRenderingMode: "source",
      // GSS-FR-25: the Changes panel's action is carried through untouched too.
      changesCommitAction: "commit",
      notificationsEnabled: true,
      selectionFollowsTab: true,
      graduationRailWidthFraction: 0.2,
      graduationPathsWidthFraction: 0.3,
      gitFilesWidthFraction: 0.3,
    });
  });

  it("changes one group without disturbing the other", async () => {
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "load_app_preferences"
        ? { theme: "system", diffVisualizationMode: "final", diffRenderingMode: "source" }
        : undefined,
    );
    await hydrateDiffModes();

    setRenderingMode("rich");
    expect(peekDiffModes()).toEqual({ visualization: "final", rendering: "rich" });
  });

  it("applies the value immediately and rolls it back if the write fails", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_app_preferences") return { theme: "system" };
      if (cmd === "save_app_preferences") throw new Error("disk full");
      return undefined;
    });
    await hydrateDiffModes();

    setVisualizationMode("final");
    // Applied optimistically — the toggles have no error affordance of their own.
    expect(peekDiffModes().visualization).toBe("final");

    // …and reverted, rather than showing a choice that will not survive relaunch.
    await vi.waitFor(() => expect(peekDiffModes().visualization).toBe("unified"));
  });

  it("notifies every subscriber, which is what re-renders every open Diff tab", async () => {
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "load_app_preferences" ? { theme: "system" } : undefined,
    );
    await hydrateDiffModes();

    // Three tabs' worth of subscriptions, taken through the same store the hook
    // subscribes through.
    const seen: string[][] = [[], [], []];
    const unsubscribes = seen.map((log, index) =>
      subscribe(() => log.push(`${index}:${peekDiffModes().visualization}`)),
    );

    setVisualizationMode("side_by_side");

    expect(seen).toEqual([
      ["0:side_by_side"],
      ["1:side_by_side"],
      ["2:side_by_side"],
    ]);
    unsubscribes.forEach((fn) => fn());

    // And an unsubscribed tab stops hearing about it.
    setVisualizationMode("final");
    expect(seen[0]).toHaveLength(1);
  });

  it("does not notify when the activated mode is already active", async () => {
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "load_app_preferences" ? { theme: "system" } : undefined,
    );
    await hydrateDiffModes();

    let notifications = 0;
    const stop = subscribe(() => notifications++);

    // DFV-FR-23: nothing changed, so nothing is published and nothing is
    // written.
    setVisualizationMode("unified");
    expect(notifications).toBe(0);
    expect(saves()).toHaveLength(0);
    stop();
  });

  it("does not let a slow read revert a choice made while it was in flight", async () => {
    // The toolbar renders before the stored record arrives, so the click is
    // genuinely reachable. Letting the record land on top would revert the
    // user's choice on screen while the queued patch wrote it to disk — the
    // store and the file would disagree until relaunch.
    let settle: ((prefs: unknown) => void) | undefined;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_app_preferences")
        return new Promise((resolve) => (settle = resolve));
      return undefined;
    });
    const hydrating = hydrateDiffModes();
    await vi.waitFor(() => expect(settle).toBeDefined());

    setVisualizationMode("final");
    expect(peekDiffModes().visualization).toBe("final");

    settle!({ theme: "system", diffVisualizationMode: "unified" });
    await hydrating;

    expect(peekDiffModes().visualization).toBe("final");
    await vi.waitFor(() => expect(saves()).toHaveLength(1));
    expect(saves()[0].diffVisualizationMode).toBe("final");
  });

  it("leaves the defaults in place when the record cannot be read", async () => {
    invokeMock.mockImplementation(async () => {
      throw new Error("unreadable");
    });
    await hydrateDiffModes();
    expect(peekDiffModes()).toEqual(DEFAULT_DIFF_MODES);
  });
});
