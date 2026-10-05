import { afterEach, describe, expect, it, vi } from "vitest";

import {
  registerSettingsSection,
  resetSettingsSections,
  runSettingsSaveSweep,
  settingsSweepRunning,
} from "./settingsSweep";

/**
 * The save-before-close sweep of a settings window
 * (`../../specifications/ui/SWN-settings-windows.md` SWN-FR-08 through
 * SWN-FR-12), covering SWN-FR-08, SWN-FR-10, SWN-FR-09, SWN-FR-07 and SWN-FR-12 at the
 * level the rules actually live.
 */

afterEach(() => resetSettingsSections());

/** A section holding a change, with a save whose outcome the test decides. */
function section(
  name: string,
  opts: { pending?: boolean; ok?: boolean; save?: () => Promise<boolean> } = {},
) {
  const save = vi.fn(opts.save ?? (async () => opts.ok ?? true));
  const state = { pending: opts.pending ?? true };
  registerSettingsSection({
    section: name,
    pending: () => state.pending,
    // A real section is clean once its save has landed; the fake says so too,
    // because "not written a second time" depends on it (SWN-FR-11).
    save: async () => {
      const ok = await save();
      if (ok) state.pending = false;
      return ok;
    },
  });
  return { save, state };
}

describe("the sweep writes every section (SWN-FR-08)", () => {
  it("SWN-FR-08, SWN-FR-10: invokes each dirty section's save, including one not on screen", async () => {
    // SWN-FR-08, SWN-FR-07's point too: a section holding edits the author never scrolled
    // back to is saved on exactly the terms the visible one is. The registry is
    // deliberately blind to which section is presented, which is what makes
    // that true rather than something the window has to remember.
    const visible = section("appearance");
    const hidden = section("mcp");

    await expect(runSettingsSaveSweep()).resolves.toEqual({ ok: true });

    expect(visible.save).toHaveBeenCalledTimes(1);
    expect(hidden.save).toHaveBeenCalledTimes(1);
  });

  it("asks nothing of a section holding no pending change", async () => {
    const clean = section("appearance", { pending: false });
    const dirty = section("mcp");

    await expect(runSettingsSaveSweep()).resolves.toEqual({ ok: true });

    expect(clean.save).not.toHaveBeenCalled();
    expect(dirty.save).toHaveBeenCalledTimes(1);
  });

  it("SWN-FR-09: awaits a write already in flight without issuing a second", async () => {
    // SWN-FR-09: a write a section has scheduled or issued and that has not yet
    // landed is a pending change the sweep WAITS for. The section owns the
    // dedupe — one promise, whoever asks — and the sweep asks exactly once.
    let land: (ok: boolean) => void = () => {};
    const inFlight = new Promise<boolean>((resolve) => {
      land = resolve;
    });
    const issued = vi.fn(() => inFlight);
    registerSettingsSection({
      section: "template",
      pending: () => true,
      save: issued,
    });

    const sweep = runSettingsSaveSweep();
    // The window has not closed: the sweep is still waiting on the write.
    let settled = false;
    void sweep.then(() => {
      settled = true;
    });
    await Promise.resolve();
    expect(settled).toBe(false);

    land(true);
    await expect(sweep).resolves.toEqual({ ok: true });
    expect(issued).toHaveBeenCalledTimes(1);
  });
});

describe("a failed save cancels the transition (SWN-FR-11)", () => {
  it("SWN-FR-08, SWN-FR-11: names the failed section, and writes the ones that can be written", async () => {
    const failing = section("mcp", { ok: false });
    const other = section("appearance");

    await expect(runSettingsSaveSweep()).resolves.toEqual({
      ok: false,
      section: "mcp",
    });

    // Every pending section was asked, not only up to the first failure: the
    // author asked to leave, so what can be written should be.
    expect(failing.save).toHaveBeenCalledTimes(1);
    expect(other.save).toHaveBeenCalledTimes(1);
  });

  it("reports the FIRST failure, so one section is presented rather than the last", async () => {
    section("appearance", { ok: false });
    section("mcp", { ok: false });

    await expect(runSettingsSaveSweep()).resolves.toEqual({
      ok: false,
      section: "appearance",
    });
  });

  it("treats a section that throws when ASKED as a failed save, not an abort", async () => {
    // The sweep runs to the end whatever a section does: one that threw from
    // `pending()` outside the try would abort the pass, the window would never
    // answer, and the author would be left with a settings window that cannot
    // be closed over a parent that cannot be reached (SWN-FR-02, SWN-FR-12).
    const later = section("mcp");
    registerSettingsSection({
      section: "appearance",
      pending: () => {
        throw new Error("state is gone");
      },
      save: async () => true,
    });

    await expect(runSettingsSaveSweep()).resolves.toEqual({
      ok: false,
      section: "appearance",
    });
    expect(later.save).toHaveBeenCalledTimes(1);
  });

  it("treats a section that threw as a failed save rather than a crash", async () => {
    registerSettingsSection({
      section: "mcp",
      pending: () => true,
      save: async () => {
        throw new Error("disk full");
      },
    });

    await expect(runSettingsSaveSweep()).resolves.toEqual({
      ok: false,
      section: "mcp",
    });
  });

  it("SWN-FR-11: does not write a section again that already saved in the sweep", async () => {
    const saved = section("appearance");
    const failing = section("mcp", { ok: false });

    await expect(runSettingsSaveSweep()).resolves.toEqual({
      ok: false,
      section: "mcp",
    });
    // The author retries the close.
    await expect(runSettingsSaveSweep()).resolves.toEqual({
      ok: false,
      section: "mcp",
    });

    expect(saved.save).toHaveBeenCalledTimes(1);
    expect(failing.save).toHaveBeenCalledTimes(2);
  });
});

describe("the sweep is inert while it runs (SWN-FR-12)", () => {
  it("SWN-FR-12: a second request starts no further save and duplicates no write", async () => {
    let land: (ok: boolean) => void = () => {};
    const inFlight = new Promise<boolean>((resolve) => {
      land = resolve;
    });
    const save = vi.fn(() => inFlight);
    registerSettingsSection({
      section: "template",
      pending: () => true,
      save,
    });

    const first = runSettingsSaveSweep();
    expect(settingsSweepRunning()).toBe(true);

    // A second close request, and a request for the other settings window,
    // arriving while the sweep runs. Each JOINS the sweep already running
    // rather than starting one — and rather than being told, falsely, that
    // there was nothing to write.
    const second = runSettingsSaveSweep();
    const third = runSettingsSaveSweep();
    expect(save).toHaveBeenCalledTimes(1);

    land(true);
    await expect(first).resolves.toEqual({ ok: true });
    await expect(second).resolves.toEqual({ ok: true });
    await expect(third).resolves.toEqual({ ok: true });
    expect(save).toHaveBeenCalledTimes(1);
    // …and once it lands the window closes exactly once, so a later request is
    // a fresh sweep rather than one joined to a finished pass.
    expect(settingsSweepRunning()).toBe(false);
  });
});

describe("what the registry holds", () => {
  it("replaces a section's entry on a remount rather than keeping both", async () => {
    const first = section("appearance");
    const second = section("appearance");

    await runSettingsSaveSweep();

    expect(first.save).not.toHaveBeenCalled();
    expect(second.save).toHaveBeenCalledTimes(1);
  });

  it("forgets a section that unmounted", async () => {
    const save = vi.fn(async () => true);
    const drop = registerSettingsSection({
      section: "appearance",
      pending: () => true,
      save,
    });
    drop();

    await expect(runSettingsSaveSweep()).resolves.toEqual({ ok: true });
    expect(save).not.toHaveBeenCalled();
  });

  it("keeps a sweep whole even when a save unmounts another section", async () => {
    // A save that tears a section down mid-sweep must not shorten the pass out
    // from under it: the set is snapshotted before the loop. Registration order
    // decides which is reached first, so `appearance` runs and drops `mcp`
    // while the sweep still owes `mcp` a save.
    const laterSave = vi.fn(async () => true);
    let dropLater = () => {};
    registerSettingsSection({
      section: "appearance",
      pending: () => true,
      save: async () => {
        dropLater();
        return true;
      },
    });
    dropLater = registerSettingsSection({
      section: "mcp",
      pending: () => true,
      save: laterSave,
    });

    await runSettingsSaveSweep();

    expect(laterSave).toHaveBeenCalledTimes(1);
  });
});
