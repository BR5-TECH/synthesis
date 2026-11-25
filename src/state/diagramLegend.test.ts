import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  diagramLegendHidden,
  refreshDiagramLegend,
  resetDiagramLegendForTest,
  restoreDiagramLegend,
  setDiagramLegendHidden,
} from "./diagramLegend";
import {
  loadLayoutPreferences,
  patchLayoutPreferences,
  resetLayoutPreferencesCache,
  setActiveLayoutProject,
} from "./layoutPreferences";
import * as logging from "../logging";
import { resetLogBufferForTest } from "../logging";
import type { LayoutPreferences } from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

/** The record of the one project slot that the mock backend holds. */
let stored: LayoutPreferences | null;
let saved: LayoutPreferences[];
let failSave: boolean;
/** A read that the test answers itself, to put a toggle under it. */
let heldRead: { resolve: () => void } | null;
let holdReads: boolean;

beforeEach(() => {
  stored = null;
  saved = [];
  failSave = false;
  heldRead = null;
  holdReads = false;
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
    if (cmd === "load_layout_preferences") {
      const snapshot = stored === null ? null : { ...stored };
      if (!holdReads) return Promise.resolve(snapshot);
      return new Promise((resolve) => {
        heldRead = { resolve: () => resolve(snapshot) };
      });
    }
    if (cmd === "save_layout_preferences") {
      if (failSave) return Promise.reject(new Error("disk full"));
      const record = args?.preferences as LayoutPreferences;
      saved.push(record);
      stored = record;
      return Promise.resolve(undefined);
    }
    if (cmd === "append_log_records") return Promise.resolve(undefined);
    return Promise.reject(new Error(`unexpected invoke ${cmd}`));
  });
  resetLayoutPreferencesCache();
  resetDiagramLegendForTest();
  resetLogBufferForTest();
});

afterEach(() => {
  vi.restoreAllMocks();
  resetLayoutPreferencesCache();
  resetDiagramLegendForTest();
});

/** Wait until every queued layout step has run. */
const settle = () => patchLayoutPreferences("/settle", {}).then(() => undefined);

describe("the legend state of the diagram overlays", () => {
  it("EDT-FR-MSCV, EDT-FR-NGCW: a project with no stored legend state shows the legend", async () => {
    await restoreDiagramLegend("/p");
    expect(diagramLegendHidden()).toBe(false);
  });

  it("EDT-FR-NGCW, SNV-FR-08: a stored hidden state is restored when the project opens", async () => {
    stored = { diagramLegendHidden: true };
    await restoreDiagramLegend("/p");
    expect(diagramLegendHidden()).toBe(true);
  });

  it("EDT-FR-NGCW, GSS-FR-17: a toggle is stored in the layout record and keeps its other fields", async () => {
    stored = { verticalPanelFraction: 0.3, fileDiscussionHidden: { "a.md": true } };
    await restoreDiagramLegend("/p");
    setDiagramLegendHidden(true);
    expect(diagramLegendHidden()).toBe(true);
    await settle();
    expect(saved[0]).toEqual({
      verticalPanelFraction: 0.3,
      fileDiscussionHidden: { "a.md": true },
      diagramLegendHidden: true,
    });
  });

  it("EDT-FR-NGCW: with no project, a toggle applies and writes nothing", async () => {
    setDiagramLegendHidden(true);
    expect(diagramLegendHidden()).toBe(true);
    await settle();
    expect(saved.some((record) => "diagramLegendHidden" in record)).toBe(false);
  });

  it("EDT-FR-BHJR: each change logs one DEBUG record with the new state", () => {
    const debug = vi.spyOn(logging, "logDebug");
    setDiagramLegendHidden(true);
    setDiagramLegendHidden(true);
    setDiagramLegendHidden(false);
    const records = debug.mock.calls.filter(
      ([, message]) => message === "the legend of the diagram overlays changed",
    );
    expect(records.map(([, , fields]) => fields)).toEqual([
      { legend: "hidden" },
      { legend: "shown" },
    ]);
    expect(records.every(([domains]) => domains.includes("frontend"))).toBe(true);
    // Each change writes this one record and no other.
    expect(debug).toHaveBeenCalledTimes(2);
  });

  it("EDT-FR-BHJR: a stored value that applies logs one DEBUG record, and a read of the same value logs none", async () => {
    const debug = vi.spyOn(logging, "logDebug");
    stored = { diagramLegendHidden: true };
    await restoreDiagramLegend("/p");
    expect(debug.mock.calls.map(([, , fields]) => fields)).toEqual([{ legend: "hidden" }]);
    await refreshDiagramLegend();
    expect(debug).toHaveBeenCalledTimes(1);
  });

  it("EDT-FR-BHJR: a write that fails logs one WARN record and keeps the state that the author chose", async () => {
    const warn = vi.spyOn(logging, "logWarn");
    await restoreDiagramLegend("/p");
    failSave = true;
    setDiagramLegendHidden(true);
    await vi.waitFor(() =>
      expect(warn).toHaveBeenCalledWith(
        ["frontend"],
        "the legend state of the diagram overlays was not stored",
        { error: "Error: disk full" },
      ),
    );
    expect(warn).toHaveBeenCalledTimes(1);
    expect(diagramLegendHidden()).toBe(true);
  });

  it("EDT-FR-DNVJ: a refresh shows a value that the other window stored", async () => {
    await restoreDiagramLegend("/p");
    // The main window cached the record. The Settings window then hides the legend.
    await loadLayoutPreferences("/p");
    stored = { diagramLegendHidden: true };
    await refreshDiagramLegend();
    expect(diagramLegendHidden()).toBe(true);
  });

  it("EDT-FR-DNVJ: after a refresh, a later write of this window keeps the value of the other window", async () => {
    stored = { bottomPanelHeight: 200 };
    await restoreDiagramLegend("/p");
    stored = { bottomPanelHeight: 200, diagramLegendHidden: true };
    await refreshDiagramLegend();
    await patchLayoutPreferences("/p", { bottomPanelHeight: 240 });
    expect(saved[saved.length - 1]).toEqual({ bottomPanelHeight: 240, diagramLegendHidden: true });
  });

  it("EDT-FR-DNVJ: a read that started before a toggle does not undo the toggle", async () => {
    stored = { diagramLegendHidden: false };
    await restoreDiagramLegend("/p");
    holdReads = true;
    const refresh = refreshDiagramLegend();
    await vi.waitFor(() => expect(heldRead).not.toBeNull());
    setDiagramLegendHidden(true);
    heldRead!.resolve();
    await refresh;
    expect(diagramLegendHidden()).toBe(true);
    holdReads = false;
    await settle();
    expect(stored).toMatchObject({ diagramLegendHidden: true });
  });

  it("EDT-FR-NGCW: a read that finds no value keeps the state on screen", async () => {
    await restoreDiagramLegend("/p");
    setDiagramLegendHidden(true);
    await settle();
    stored = null;
    await refreshDiagramLegend();
    expect(diagramLegendHidden()).toBe(true);
  });

  it("EDT-FR-NGCW: a project switch shows the legend of the new project, and a late read of the old project is discarded", async () => {
    stored = { diagramLegendHidden: true };
    await restoreDiagramLegend("/a");
    expect(diagramLegendHidden()).toBe(true);

    holdReads = true;
    const lateRead = refreshDiagramLegend();
    await vi.waitFor(() => expect(heldRead).not.toBeNull());
    const readOfA = heldRead!;
    heldRead = null;
    setActiveLayoutProject("/b");
    stored = null;
    const openB = restoreDiagramLegend("/b");
    // The new project shows the legend until its own read returns.
    expect(diagramLegendHidden()).toBe(false);
    readOfA.resolve();
    await lateRead;
    expect(diagramLegendHidden()).toBe(false);
    await vi.waitFor(() => expect(heldRead).not.toBeNull());
    heldRead!.resolve();
    await openB;
    expect(diagramLegendHidden()).toBe(false);
  });

  it("EDT-FR-BHJR: a write that failed is not undone by the next refresh", async () => {
    stored = { diagramLegendHidden: false };
    await restoreDiagramLegend("/p");
    failSave = true;
    setDiagramLegendHidden(true);
    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("save_layout_preferences", expect.anything()),
    );
    await settle().catch(() => undefined);
    await refreshDiagramLegend();
    expect(diagramLegendHidden()).toBe(true);
    // A later write that lands lets reads apply again.
    failSave = false;
    setDiagramLegendHidden(false);
    await settle();
    stored = { diagramLegendHidden: true };
    await refreshDiagramLegend();
    expect(diagramLegendHidden()).toBe(true);
  });

  it("EDT-FR-DNVJ: the window reads the stored value again when it gets its focus", async () => {
    stored = { diagramLegendHidden: false };
    await restoreDiagramLegend("/p");
    stored = { diagramLegendHidden: true };
    window.dispatchEvent(new Event("focus"));
    await vi.waitFor(() => expect(diagramLegendHidden()).toBe(true));
  });

  it("EDT-FR-NGCW: an empty project key reads and writes nothing", async () => {
    await restoreDiagramLegend("");
    setDiagramLegendHidden(true);
    await refreshDiagramLegend();
    expect(diagramLegendHidden()).toBe(true);
    const layoutCalls = invokeMock.mock.calls.filter(([cmd]) => String(cmd).includes("layout"));
    expect(layoutCalls).toEqual([]);
  });

  it("EDT-FR-BHJR: the reset to shown for a new project logs one DEBUG record", async () => {
    stored = { diagramLegendHidden: true };
    await restoreDiagramLegend("/a");
    const debug = vi.spyOn(logging, "logDebug");
    stored = null;
    await restoreDiagramLegend("/b");
    const fields = debug.mock.calls
      .filter(([, message]) => String(message).startsWith("the legend of the diagram overlays"))
      .map(([, , f]) => f);
    expect(fields).toEqual([{ legend: "shown" }]);
  });
});
