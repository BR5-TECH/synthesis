import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import {
  DEFAULT_APP_PREFERENCES,
  loadAppPreferences,
  patchAppPreferences,
  peekAppPreferences,
  resetAppPreferencesCache,
} from "./appPreferences";

/**
 * GSS-FR-20 / GLS-FR-14: `save_app_preferences` is a whole-record write, so a
 * caller editing one field must supply the others. These tests pin that the
 * *record*, not a partial, reaches the backend — the failure mode otherwise is
 * silent: the theme still applies, and the user only discovers the full-screen
 * flag was cleared on their next launch.
 */

function savedPayloads(): Record<string, unknown>[] {
  return invokeMock.mock.calls
    .filter((c) => c[0] === "save_app_preferences")
    .map(
      (c) =>
        (c[1] as { preferences: Record<string, unknown> }).preferences ?? {},
    );
}

beforeEach(() => {
  invokeMock.mockReset();
  resetAppPreferencesCache();
});

describe("loadAppPreferences", () => {
  it("returns the backend record, filling in defaults for absent fields", async () => {
    invokeMock.mockResolvedValue({ theme: "dark" });
    const prefs = await loadAppPreferences();
    expect(prefs.theme).toBe("dark");
    expect(prefs.mainWindowFullscreen).toBe(false);
  });

  it("caches, so repeated reads make exactly one round-trip", async () => {
    invokeMock.mockResolvedValue({ theme: "dark", mainWindowFullscreen: true });
    await loadAppPreferences();
    await loadAppPreferences();
    await loadAppPreferences();
    const loads = invokeMock.mock.calls.filter(
      (c) => c[0] === "load_app_preferences",
    );
    expect(loads).toHaveLength(1);
  });

  it("shares one in-flight request between concurrent callers", async () => {
    invokeMock.mockImplementation(
      () =>
        new Promise((resolve) =>
          setTimeout(() => resolve({ theme: "light" }), 5),
        ),
    );
    const [a, b] = await Promise.all([
      loadAppPreferences(),
      loadAppPreferences(),
    ]);
    expect(a.theme).toBe("light");
    expect(b.theme).toBe("light");
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "load_app_preferences"),
    ).toHaveLength(1);
  });

  it("yields defaults rather than throwing when the backend rejects", async () => {
    invokeMock.mockRejectedValue(new Error("no backend"));
    await expect(loadAppPreferences()).resolves.toEqual(
      DEFAULT_APP_PREFERENCES,
    );
  });
});

describe("patchAppPreferences — GSS-FR-20 whole-record write", () => {
  it("carries the full-screen flag through a theme-only edit (GLS-FR-14, GSS-FR-20)", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_app_preferences")
        return { theme: "system", mainWindowFullscreen: true };
      return undefined;
    });

    await patchAppPreferences({ theme: "dark" });

    const payloads = savedPayloads();
    expect(payloads).toHaveLength(1);
    expect(payloads[0].theme).toBe("dark");
    expect(payloads[0].mainWindowFullscreen).toBe(true);
  });

  it("GSS-FR-20, GSS-FR-QDNV: a paths-width patch carries every other field through, and another patch carries it through", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_app_preferences")
        return {
          theme: "dark",
          selectionFollowsTab: false,
          graduationRailWidthFraction: 0.25,
          graduationPathsWidthFraction: 0.42,
        };
      return undefined;
    });

    await patchAppPreferences({ graduationPathsWidthFraction: 0.5 });
    await patchAppPreferences({ graduationRailWidthFraction: 0.26 });

    const [paths, rail] = savedPayloads();
    expect(paths).toMatchObject({
      theme: "dark",
      selectionFollowsTab: false,
      graduationRailWidthFraction: 0.25,
      graduationPathsWidthFraction: 0.5,
    });
    expect(rail).toMatchObject({
      theme: "dark",
      graduationRailWidthFraction: 0.26,
      graduationPathsWidthFraction: 0.5,
    });
  });

  it("GSS-FR-20, GSS-FR-MSPQ: a git files-width patch carries every other field through", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_app_preferences")
        return {
          theme: "dark",
          graduationPathsWidthFraction: 0.42,
          gitFilesWidthFraction: 0.25,
        };
      return undefined;
    });

    await patchAppPreferences({ gitFilesWidthFraction: 0.55 });

    const [saved] = savedPayloads();
    expect(saved).toMatchObject({
      theme: "dark",
      graduationPathsWidthFraction: 0.42,
      gitFilesWidthFraction: 0.55,
    });
  });

  it("carries the theme through a full-screen-only edit", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_app_preferences")
        return { theme: "dark", mainWindowFullscreen: false };
      return undefined;
    });

    await patchAppPreferences({ mainWindowFullscreen: true });

    const payloads = savedPayloads();
    expect(payloads).toHaveLength(1);
    expect(payloads[0].mainWindowFullscreen).toBe(true);
    expect(payloads[0].theme).toBe("dark");
  });

  it("reads the record first when patching before anything has been loaded", async () => {
    // The dangerous ordering: a write arrives before any read. Sending
    // defaults here would clear whatever is genuinely on disk.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_app_preferences")
        return { theme: "light", mainWindowFullscreen: true };
      return undefined;
    });

    await patchAppPreferences({ theme: "dark" });

    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "load_app_preferences"),
    ).toHaveLength(1);
    expect(savedPayloads()[0].mainWindowFullscreen).toBe(true);
  });

  it("sends the record under the named `preferences` arg", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_app_preferences") return { theme: "system" };
      return undefined;
    });

    await patchAppPreferences({ theme: "dark" });

    const call = invokeMock.mock.calls.find(
      (c) => c[0] === "save_app_preferences",
    );
    expect(call).toBeDefined();
    const keys = Object.keys(call![1] as Record<string, unknown>);
    expect(keys).toContain("preferences");
    // Not spread into the top-level args object, not renamed.
    expect(keys).not.toContain("theme");
    expect(keys).not.toContain("prefs");
  });

  it("advances the cache so a second patch builds on the first", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_app_preferences")
        return { theme: "system", mainWindowFullscreen: false };
      return undefined;
    });

    await patchAppPreferences({ mainWindowFullscreen: true });
    await patchAppPreferences({ theme: "dark" });

    const payloads = savedPayloads();
    expect(payloads).toHaveLength(2);
    // The second write still carries the flag the first one set.
    expect(payloads[1]).toMatchObject({
      theme: "dark",
      mainWindowFullscreen: true,
    });
    // And only one load was needed for both.
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "load_app_preferences"),
    ).toHaveLength(1);
  });

  it("refuses to write rather than clobber an unreadable record", async () => {
    // A transient load failure at startup leaves us not knowing what is stored.
    // Writing `{ ...defaults, ...patch }` there would silently reset the OTHER
    // field — a full-screen toggle would quietly restore the theme to `system`.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_app_preferences") throw new Error("unreadable");
      return undefined;
    });

    await expect(
      patchAppPreferences({ mainWindowFullscreen: true }),
    ).rejects.toThrow(/unreadable|refusing/i);
    expect(savedPayloads()).toHaveLength(0);
  });

  it("recovers once the record becomes readable again", async () => {
    let failing = true;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_app_preferences") {
        if (failing) throw new Error("unreadable");
        return { theme: "dark", mainWindowFullscreen: false };
      }
      return undefined;
    });

    await expect(patchAppPreferences({ theme: "light" })).rejects.toThrow();
    failing = false;
    await patchAppPreferences({ mainWindowFullscreen: true });

    expect(savedPayloads()[0]).toEqual({
      theme: "dark",
      mainWindowFullscreen: true,
      searchQueryMode: "literal_insensitive",
      diffVisualizationMode: "unified",
      diffRenderingMode: "source",
      changesCommitAction: "commit",
      notificationsEnabled: true,
      selectionFollowsTab: true,
      graduationRailWidthFraction: 0.2,
      graduationPathsWidthFraction: 0.3,
      gitFilesWidthFraction: 0.3,
    });
  });

  it("composes two overlapping patches instead of losing one", async () => {
    // Both would otherwise read the same base and write independently, and the
    // later completion would drop the earlier field.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_app_preferences")
        return { theme: "system", mainWindowFullscreen: false };
      if (cmd === "save_app_preferences")
        return new Promise((resolve) => setTimeout(resolve, 5));
      return undefined;
    });

    await Promise.all([
      patchAppPreferences({ mainWindowFullscreen: true }),
      patchAppPreferences({ theme: "dark" }),
    ]);

    const payloads = savedPayloads();
    expect(payloads).toHaveLength(2);
    // The last write to reach the backend carries BOTH edits.
    expect(payloads[payloads.length - 1]).toEqual({
      theme: "dark",
      mainWindowFullscreen: true,
      searchQueryMode: "literal_insensitive",
      diffVisualizationMode: "unified",
      diffRenderingMode: "source",
      changesCommitAction: "commit",
      notificationsEnabled: true,
      selectionFollowsTab: true,
      graduationRailWidthFraction: 0.2,
      graduationPathsWidthFraction: 0.3,
      gitFilesWidthFraction: 0.3,
    });
  });

  it("GSS-FR-20 / SCH-FR-13: a query-mode patch carries the theme and full-screen through", async () => {
    // The search input edits only `searchQueryMode`. Sending a bare
    // `{ searchQueryMode }` to a whole-record write would clear the other two,
    // which is exactly the clobber GLS-FR-14 requires not to happen.
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "load_app_preferences"
        ? { theme: "dark", mainWindowFullscreen: true }
        : undefined,
    );

    await patchAppPreferences({ searchQueryMode: "regex" });

    expect(savedPayloads()[0]).toEqual({
      theme: "dark",
      mainWindowFullscreen: true,
      searchQueryMode: "regex",
      diffVisualizationMode: "unified",
      diffRenderingMode: "source",
      changesCommitAction: "commit",
      notificationsEnabled: true,
      selectionFollowsTab: true,
      graduationRailWidthFraction: 0.2,
      graduationPathsWidthFraction: 0.3,
      gitFilesWidthFraction: 0.3,
    });
  });

  it("does not hand out the cached object by reference", async () => {
    // A consumer mutating what it was handed would corrupt the process-wide
    // record for every other reader.
    invokeMock.mockResolvedValue({ theme: "dark", mainWindowFullscreen: true });
    const first = await loadAppPreferences();
    first.theme = "light";
    first.mainWindowFullscreen = false;

    const second = await loadAppPreferences();
    expect(second.theme).toBe("dark");
    expect(second.mainWindowFullscreen).toBe(true);
  });

  it("does not advance the cache when the write fails", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_app_preferences")
        return { theme: "system", mainWindowFullscreen: true };
      throw new Error("disk full");
    });

    await expect(patchAppPreferences({ theme: "dark" })).rejects.toThrow(
      "disk full",
    );
    // A failed write must not leave the next patch building on a value that was
    // never stored — the cache still holds the loaded record.
    expect(peekAppPreferences()).toEqual({
      theme: "system",
      mainWindowFullscreen: true,
      searchQueryMode: "literal_insensitive",
      diffVisualizationMode: "unified",
      diffRenderingMode: "source",
      changesCommitAction: "commit",
      notificationsEnabled: true,
      selectionFollowsTab: true,
      graduationRailWidthFraction: 0.2,
      graduationPathsWidthFraction: 0.3,
      gitFilesWidthFraction: 0.3,
    });
  });
});
