/**
 * The page width store (`EPW-editor-page-width.md`): the range, the stored
 * value, the root property, and when a width is saved and adopted.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn<(cmd: string, args?: Record<string, unknown>) => Promise<unknown>>();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invokeMock(cmd, args),
}));
const log = vi.hoisted(() => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));
vi.mock("../logging", () => log);
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));

import {
  PAGE_WIDTH_PROPERTY,
  adoptStoredEditorPageWidth,
  clampPageWidth,
  commitEditorPageWidth,
  endEditorPageWidthDrag,
  getEditorPageWidth,
  previewEditorPageWidth,
  readStoredPageWidth,
  resetEditorPageWidthForTest,
  restoreEditorPageWidth,
  startEditorPageWidthDrag,
  subscribeEditorPageWidth,
} from "./editorPageWidth";
import { resetAppPreferencesCache } from "./appPreferences";

/** What a save does: answer, refuse, or wait for the test. */
let saveMode: "ok" | "refuse" | "hold" = "ok";
let releaseSave: (() => void) | null = null;

function route(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
  switch (cmd) {
    case "load_app_preferences":
      return Promise.resolve({ theme: "dark" });
    case "save_app_preferences":
      if (saveMode === "refuse") return Promise.reject(new Error("disk is full"));
      if (saveMode === "hold") return new Promise<void>((res) => (releaseSave = res));
      return Promise.resolve(args);
    default:
      return Promise.reject(new Error(`unexpected command ${cmd}`));
  }
}

const saves = () =>
  invokeMock.mock.calls
    .filter((c) => c[0] === "save_app_preferences")
    .map((c) => (c[1] as { preferences: Record<string, unknown> }).preferences);
const rootWidth = () => document.documentElement.style.getPropertyValue(PAGE_WIDTH_PROPERTY);
/** Let the queued save and its handlers run. */
const settle = () => new Promise<void>((res) => setTimeout(res, 0));

beforeEach(() => {
  saveMode = "ok";
  releaseSave = null;
  invokeMock.mockReset();
  invokeMock.mockImplementation(route);
  resetEditorPageWidthForTest();
  resetAppPreferencesCache();
  Object.values(log).forEach((fn) => fn.mockReset());
});
afterEach(() => {
  resetEditorPageWidthForTest();
});

describe("page width range", () => {
  it("EPW-FR-QGUZ, EPW-FR-AOVQ: a width is a whole number from 560 to 1800", () => {
    for (const [given, expected] of [
      [559, 560],
      [560, 560],
      [1800, 1800],
      [1801, 1800],
      [820.4, 820],
      [820.5, 821],
      [Number.NaN, 820],
      [Number.POSITIVE_INFINITY, 820],
      [Number.NEGATIVE_INFINITY, 820],
    ] as const) {
      expect(clampPageWidth(given), String(given)).toBe(expected);
    }
  });

  it("GSS-FR-UYXH, EPW-FR-ZCBT: a stored width out of range, or not whole, reads as 820", () => {
    for (const [stored, expected] of [
      [560, 560],
      [1800, 1800],
      [1240, 1240],
      [559, 820],
      [1801, 820],
      [820.5, 820],
      ["900", 820],
      [null, 820],
      [undefined, 820],
    ] as const) {
      expect(readStoredPageWidth(stored), String(stored)).toBe(expected);
    }
  });
});

describe("page width store", () => {
  it("EPW-FR-NSNH, EPW-FR-XOMR: a new width goes on the root property and tells each listener once", () => {
    const listener = vi.fn();
    subscribeEditorPageWidth(listener);
    previewEditorPageWidth(1000);
    expect(rootWidth()).toBe("1000px");
    expect(getEditorPageWidth()).toBe(1000);
    expect(listener).toHaveBeenCalledTimes(1);
    previewEditorPageWidth(1000);
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("EPW-FR-JNCM: a drag preview saves nothing, and its end saves once", async () => {
    startEditorPageWidthDrag();
    previewEditorPageWidth(900);
    previewEditorPageWidth(1100);
    await settle();
    expect(saves()).toHaveLength(0);
    await commitEditorPageWidth(1100);
    expect(saves()).toHaveLength(1);
    expect(saves()[0]).toMatchObject({ editorPageWidthPx: 1100, theme: "dark" });
    expect(log.logInfo).toHaveBeenCalledWith(["frontend"], "page width saved", { px: 1100 });
  });

  it("EPW-FR-JNCM, EPW-FR-AOVQ: a commit at the saved width saves nothing, and a commit is held in range", async () => {
    await commitEditorPageWidth(820);
    expect(saves()).toHaveLength(0);
    await commitEditorPageWidth(5000);
    expect(getEditorPageWidth()).toBe(1800);
    expect(saves().map((p) => p.editorPageWidthPx)).toEqual([1800]);
  });

  it("EPW-FR-LSZF: a refused save keeps the width, rejects, logs a warning, and a commit of it later saves again", async () => {
    saveMode = "refuse";
    await expect(commitEditorPageWidth(1200)).rejects.toThrow("disk is full");
    expect(getEditorPageWidth()).toBe(1200);
    expect(rootWidth()).toBe("1200px");
    const warnings = log.logWarn.mock.calls.filter((c) => c[1] === "page width save failed");
    expect(warnings).toHaveLength(1);
    saveMode = "ok";
    await commitEditorPageWidth(1200);
    expect(saves()).toHaveLength(2);
  });

  it("EPW-FR-ZCBT: a record is adopted, and an out-of-range one reads as 820", () => {
    adoptStoredEditorPageWidth({ editorPageWidthPx: 1300 });
    expect(getEditorPageWidth()).toBe(1300);
    expect(rootWidth()).toBe("1300px");
    adoptStoredEditorPageWidth({ editorPageWidthPx: 9000 });
    expect(getEditorPageWidth()).toBe(820);
  });

  it("EPW-FR-ZCBT: a record that comes during a drag waits, and the drag's own save wins over it", async () => {
    startEditorPageWidthDrag();
    previewEditorPageWidth(1000);
    adoptStoredEditorPageWidth({ editorPageWidthPx: 1500 });
    expect(getEditorPageWidth()).toBe(1000);
    await commitEditorPageWidth(1000);
    expect(getEditorPageWidth()).toBe(1000);
    expect(saves().map((p) => p.editorPageWidthPx)).toEqual([1000]);
  });

  it("EPW-FR-ZCBT: a record that comes during a drag that changes nothing shows when the drag ends", async () => {
    startEditorPageWidthDrag();
    adoptStoredEditorPageWidth({ editorPageWidthPx: 1500 });
    expect(getEditorPageWidth()).toBe(820);
    endEditorPageWidthDrag();
    expect(getEditorPageWidth()).toBe(1500);
    // A drag that returns to the saved width also shows it.
    startEditorPageWidthDrag();
    previewEditorPageWidth(1000);
    adoptStoredEditorPageWidth({ editorPageWidthPx: 1600 });
    await commitEditorPageWidth(1500);
    expect(getEditorPageWidth()).toBe(1600);
    expect(saves()).toHaveLength(0);
  });

  it("EPW-FR-ZCBT: a record that comes while a save is out is not adopted, and one after it is", async () => {
    saveMode = "hold";
    const pending = commitEditorPageWidth(1100);
    await settle();
    adoptStoredEditorPageWidth({ editorPageWidthPx: 820 });
    expect(getEditorPageWidth()).toBe(1100);
    releaseSave?.();
    await pending;
    adoptStoredEditorPageWidth({ editorPageWidthPx: 1400 });
    expect(getEditorPageWidth()).toBe(1400);
  });

  it("GLS-FR-DCMT: a restore shows the stored width and knows it as saved", async () => {
    previewEditorPageWidth(1300);
    restoreEditorPageWidth(1000);
    expect(getEditorPageWidth()).toBe(1000);
    await commitEditorPageWidth(1000);
    expect(saves()).toHaveLength(0);
  });

  it("a save from before a reset does not change the state after it", async () => {
    saveMode = "hold";
    const pending = commitEditorPageWidth(1100);
    await settle();
    const release = releaseSave;
    resetEditorPageWidthForTest();
    release?.();
    await pending;
    // With no save counted as out, a record is adopted at once.
    adoptStoredEditorPageWidth({ editorPageWidthPx: 1250 });
    expect(getEditorPageWidth()).toBe(1250);
  });
});
