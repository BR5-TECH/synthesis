import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const patchMock = vi.fn<(patch: Record<string, unknown>) => Promise<unknown>>();

vi.mock("./appPreferences", () => ({
  patchAppPreferences: (patch: Record<string, unknown>) => patchMock(patch),
  // The test setup resets the real cache before each test.
  resetAppPreferencesCache: () => {},
}));
const logWarnMock = vi.fn();
vi.mock("../logging", () => ({
  logDebug: vi.fn(),
  logWarn: (...args: unknown[]) => logWarnMock(...args),
}));

/** Each save waits until the test releases it, in the order the saves came. */
function holdSaves(): Array<(ok: boolean) => void> {
  const releases: Array<(ok: boolean) => void> = [];
  patchMock.mockImplementation(
    () =>
      new Promise<unknown>((resolve, reject) => {
        releases.push((ok) => (ok ? resolve({}) : reject(new Error("disk full"))));
      }),
  );
  return releases;
}

import {
  EDITOR_ZOOM_PROPERTY,
  EDITOR_ZOOM_STEPS,
  WHEEL_PINCH_SETTLE_MS,
  adoptStoredEditorZoom,
  clampZoom,
  displayZoom,
  endEditorZoomPinch,
  getEditorZoom,
  isZoomableTab,
  pinchEditorZoom,
  pinchZoom,
  readStoredZoom,
  resetEditorZoom,
  resetEditorZoomForTest,
  setEditorZoom,
  settleZoom,
  startEditorZoomPinch,
  subscribeEditorZoom,
  zoomEditorIn,
  zoomEditorOut,
  zoomInFrom,
  zoomOptions,
  zoomOutFrom,
} from "./editorZoom";
import type { Tab } from "../types";

function savedPercents(): unknown[] {
  return patchMock.mock.calls.map(([patch]) => patch.editorZoomPercent);
}

function rootFactor(): string {
  return document.documentElement.style.getPropertyValue(EDITOR_ZOOM_PROPERTY);
}

beforeEach(() => {
  resetEditorZoomForTest();
  logWarnMock.mockReset();
  patchMock.mockReset();
  patchMock.mockResolvedValue({});
});

afterEach(() => {
  vi.useRealTimers();
  resetEditorZoomForTest();
});

describe("steps and limits", () => {
  it("EZM-FR-COYK: the steps are 50 to 300 percent, in order", () => {
    expect(EDITOR_ZOOM_STEPS).toEqual([50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200, 250, 300]);
  });

  it("EZM-FR-COYK: a level never goes below 50% or above 300%", () => {
    expect(clampZoom(10)).toBe(50);
    expect(clampZoom(1000)).toBe(300);
    expect(clampZoom(137.4)).toBe(137.4);
    expect(clampZoom(Number.NaN)).toBe(100);
  });

  it("EZM-FR-DMLT: Zoom In goes to the next step above, also from a level between steps", () => {
    expect(zoomInFrom(100)).toBe(110);
    expect(zoomInFrom(137)).toBe(150);
    expect(zoomInFrom(50)).toBe(67);
    // A level that shows as 100% goes to the step after 100%.
    expect(zoomInFrom(99.6)).toBe(110);
    expect(zoomInFrom(300)).toBe(300);
  });

  it("EZM-FR-DMLT: Zoom Out goes to the next step below, also from a level between steps", () => {
    expect(zoomOutFrom(100)).toBe(90);
    expect(zoomOutFrom(137)).toBe(125);
    expect(zoomOutFrom(300)).toBe(250);
    expect(zoomOutFrom(50)).toBe(50);
  });

  it("EZM-FR-GMSX: a pinch multiplies the level and stays in the range", () => {
    expect(pinchZoom(100, 1.5)).toBe(150);
    expect(pinchZoom(250, 2)).toBe(300);
    expect(pinchZoom(60, 0.5)).toBe(50);
    expect(pinchZoom(120, 0)).toBe(120);
    expect(pinchZoom(120, Number.NaN)).toBe(120);
  });

  it("EZM-FR-GMSX: the level shows as a whole percent", () => {
    expect(displayZoom(137.4)).toBe(137);
    expect(displayZoom(137.6)).toBe(138);
  });

  it("EZM-FR-PZSN: a pinch that ends between 97% and 103% goes to 100%", () => {
    expect(settleZoom(97)).toBe(100);
    expect(settleZoom(103.4)).toBe(100);
    expect(settleZoom(96.4)).toBe(96);
    expect(settleZoom(104)).toBe(104);
  });

  it("STB-FR-OQTE: a level between steps is one more option, in order", () => {
    expect(zoomOptions(100)).toEqual([...EDITOR_ZOOM_STEPS]);
    const options = zoomOptions(137.2);
    expect(options).toHaveLength(EDITOR_ZOOM_STEPS.length + 1);
    expect(options.slice(7, 10)).toEqual([125, 137, 150]);
  });
});

describe("reading a stored level", () => {
  it("EZM-FR-HYQD: a stored level outside the range, or not a number, reads as 100%", () => {
    expect(readStoredZoom(125)).toBe(125);
    expect(readStoredZoom(137)).toBe(137);
    expect(readStoredZoom(49)).toBe(100);
    expect(readStoredZoom(301)).toBe(100);
    expect(readStoredZoom(900)).toBe(100);
    expect(readStoredZoom(undefined)).toBe(100);
    expect(readStoredZoom("150")).toBe(100);
    expect(readStoredZoom(Number.POSITIVE_INFINITY)).toBe(100);
  });

  it("EZM-FR-HYQD, EZM-FR-MXAB: an adopted level shows on the root and saves nothing", () => {
    adoptStoredEditorZoom({ editorZoomPercent: 150 });
    expect(getEditorZoom()).toBe(150);
    expect(rootFactor()).toBe("1.5");
    expect(patchMock).not.toHaveBeenCalled();
  });

  it("EZM-FR-HYQD: a record with no level shows 100%", () => {
    adoptStoredEditorZoom({ editorZoomPercent: 150 });
    adoptStoredEditorZoom({});
    expect(getEditorZoom()).toBe(100);
    expect(rootFactor()).toBe("1");
  });
});

describe("changing the level", () => {
  it("EZM-FR-DMLT, EZM-FR-VPYL: Zoom In, Zoom Out, and Actual Size save at once", () => {
    zoomEditorIn();
    expect(getEditorZoom()).toBe(110);
    zoomEditorOut();
    zoomEditorOut();
    expect(getEditorZoom()).toBe(90);
    resetEditorZoom();
    expect(getEditorZoom()).toBe(100);
    expect(savedPercents()).toEqual([110, 100, 90, 100]);
  });

  it("EZM-FR-DMLT: Zoom In at 300% and Zoom Out at 50% do nothing and save nothing", () => {
    setEditorZoom(300);
    patchMock.mockClear();
    zoomEditorIn();
    expect(getEditorZoom()).toBe(300);
    setEditorZoom(50);
    patchMock.mockClear();
    zoomEditorOut();
    expect(getEditorZoom()).toBe(50);
    expect(patchMock).not.toHaveBeenCalled();
  });

  it("EZM-FR-MXAB: a change writes the factor on the root and tells each subscriber", () => {
    const seen: number[] = [];
    const unsubscribe = subscribeEditorZoom(() => seen.push(getEditorZoom()));
    setEditorZoom(175);
    expect(rootFactor()).toBe("1.75");
    unsubscribe();
    setEditorZoom(200);
    expect(seen).toEqual([175]);
  });

  it("EZM-FR-VPYL: a wheel pinch saves once, when no wheel came for the settle time", () => {
    vi.useFakeTimers();
    pinchEditorZoom(1.1);
    pinchEditorZoom(1.1);
    pinchEditorZoom(1.1);
    expect(getEditorZoom()).toBeCloseTo(133.1);
    expect(patchMock).not.toHaveBeenCalled();
    vi.advanceTimersByTime(WHEEL_PINCH_SETTLE_MS - 1);
    expect(patchMock).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(savedPercents()).toEqual([133]);
    expect(getEditorZoom()).toBe(133);
  });

  it("EZM-FR-VPYL: a gesture pinch saves once, at its end, and starts no settle timer", () => {
    vi.useFakeTimers();
    startEditorZoomPinch();
    pinchEditorZoom(1.2);
    pinchEditorZoom(1.2);
    vi.advanceTimersByTime(WHEEL_PINCH_SETTLE_MS * 4);
    expect(patchMock).not.toHaveBeenCalled();
    endEditorZoomPinch();
    expect(savedPercents()).toEqual([144]);
  });

  it("EZM-FR-PZSN: a pinch that ends at 102% goes to 100%", () => {
    setEditorZoom(125);
    patchMock.mockClear();
    startEditorZoomPinch();
    pinchEditorZoom(102 / 125);
    endEditorZoomPinch();
    expect(getEditorZoom()).toBe(100);
    expect(savedPercents()).toEqual([100]);
  });

  it("EZM-FR-VPYL: a pinch that ends at the saved level saves nothing", () => {
    vi.useFakeTimers();
    startEditorZoomPinch();
    pinchEditorZoom(1.01);
    endEditorZoomPinch();
    // A wheel pinch at the limit changes nothing either.
    setEditorZoom(300);
    patchMock.mockClear();
    pinchEditorZoom(1.2);
    vi.advanceTimersByTime(WHEEL_PINCH_SETTLE_MS);
    expect(patchMock).not.toHaveBeenCalled();
  });

  it("EZM-FR-VPYL: a pinch back to a level that another window saved is not saved again", () => {
    adoptStoredEditorZoom({ editorZoomPercent: 150 });
    startEditorZoomPinch();
    pinchEditorZoom(1.2);
    pinchEditorZoom(1 / 1.2);
    endEditorZoomPinch();
    expect(getEditorZoom()).toBe(150);
    expect(patchMock).not.toHaveBeenCalled();
  });

  it("EZM-FR-GMSX: a small wheel step at 50% still adds up instead of rounding away", () => {
    setEditorZoom(50);
    startEditorZoomPinch();
    for (let i = 0; i < 10; i++) pinchEditorZoom(1.01);
    endEditorZoomPinch();
    expect(getEditorZoom()).toBe(55);
  });

  it("EZM-FR-HYQD: a stored level is not adopted during a pinch or while a save is out", async () => {
    const releases = holdSaves();
    startEditorZoomPinch();
    pinchEditorZoom(1.5);
    adoptStoredEditorZoom({ editorZoomPercent: 100 });
    expect(getEditorZoom()).toBe(150);
    endEditorZoomPinch();
    adoptStoredEditorZoom({ editorZoomPercent: 100 });
    expect(getEditorZoom()).toBe(150);
    expect(releases).toHaveLength(1);
    releases[0](true);
    await vi.waitFor(() => {
      adoptStoredEditorZoom({ editorZoomPercent: 200 });
      expect(getEditorZoom()).toBe(200);
    });
  });

  it("GSS-FR-KKYO, EZM-FR-VPYL: every saved level is a whole percent", () => {
    vi.useFakeTimers();
    pinchEditorZoom(1.237);
    vi.advanceTimersByTime(WHEEL_PINCH_SETTLE_MS);
    startEditorZoomPinch();
    pinchEditorZoom(1.111);
    endEditorZoomPinch();
    setEditorZoom(141.6);
    expect(savedPercents().length).toBe(3);
    for (const percent of savedPercents()) expect(Number.isInteger(percent)).toBe(true);
  });

  it("EZM-FR-HYQD: a save that ends after a reset does not count against the saves after it", async () => {
    const releases = holdSaves();
    setEditorZoom(125);
    resetEditorZoomForTest();
    // A save after the reset is out, and the one from before the reset ends.
    setEditorZoom(150);
    releases[0](true);
    await vi.waitFor(() => expect(patchMock).toHaveBeenCalledTimes(2));
    await Promise.resolve();
    await Promise.resolve();
    // The save of 150 is still out, so a stored level is not adopted.
    adoptStoredEditorZoom({ editorZoomPercent: 200 });
    expect(getEditorZoom()).toBe(150);
    releases[1](true);
    await vi.waitFor(() => {
      adoptStoredEditorZoom({ editorZoomPercent: 200 });
      expect(getEditorZoom()).toBe(200);
    });
  });

  it("EZM-FR-VPYL: a failed save keeps the level the author chose", async () => {
    patchMock.mockRejectedValue(new Error("disk full"));
    setEditorZoom(125);
    await vi.waitFor(() => expect(logWarnMock).toHaveBeenCalled());
    expect(getEditorZoom()).toBe(125);
  });

  it("EZM-FR-VPYL: after a failed save, a pinch that ends at that level saves it again", async () => {
    const releases = holdSaves();
    setEditorZoom(125);
    releases[0](false);
    await vi.waitFor(() => expect(logWarnMock).toHaveBeenCalled());
    startEditorZoomPinch();
    pinchEditorZoom(1.001);
    endEditorZoomPinch();
    expect(savedPercents()).toEqual([125, 125]);
    releases[1](true);
  });
});

describe("reading position", () => {
  function scroller(top: number, height: number, client = 100) {
    const el = document.createElement("div");
    el.setAttribute("data-zoom-scroller", "");
    Object.defineProperty(el, "scrollHeight", { get: () => height, configurable: true });
    Object.defineProperty(el, "clientHeight", { get: () => client, configurable: true });
    el.getBoundingClientRect = () =>
      ({ left: 0, top: 50, right: 400, bottom: 150, width: 400, height: 100 }) as DOMRect;
    el.scrollTop = top;
    document.body.appendChild(el);
    return { el };
  }

  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("EZM-FR-OXBL: the line at the top stays at the top after a step", () => {
    const s = scroller(400, 2000);
    // jsdom has no layout: the document is as tall as it will be at 200%
    // when the store reads it after the change.
    let reads = 0;
    Object.defineProperty(s.el, "scrollHeight", {
      get: () => (reads++ === 0 ? 2000 : 4000),
      configurable: true,
    });
    setEditorZoom(200);
    expect(s.el.scrollTop).toBe(800);
  });

  it("EZM-FR-OXBL: for a pinch, the text under the pointer stays under it", () => {
    const s = scroller(400, 2000);
    let reads = 0;
    Object.defineProperty(s.el, "scrollHeight", {
      get: () => (reads++ === 0 ? 2000 : 4000),
      configurable: true,
    });
    // The pointer is 60 px below the top of the scroller (top 50 + 60).
    pinchEditorZoom(2, { clientX: 100, clientY: 110 });
    expect(s.el.scrollTop).toBe((400 + 60) * 2 - 60);
  });

  it("EZM-FR-OXBL, EZM-FR-PZSN: the snap at the end of a pinch keeps the last pointer", () => {
    const s = scroller(400, 2000);
    // jsdom has no layout: each read gives the height before, then after, a change.
    const heights = [2000, 2040, 2040, 2000];
    Object.defineProperty(s.el, "scrollHeight", {
      get: () => heights.shift() ?? 2000,
      configurable: true,
    });
    startEditorZoomPinch();
    pinchEditorZoom(1.02, { clientX: 100, clientY: 110 });
    const afterPinch = s.el.scrollTop;
    expect(afterPinch).toBeCloseTo(((400 + 60) * 2040) / 2000 - 60, 5);
    endEditorZoomPinch();
    // The pointer is 60 px below the top of the scroller: the text under it
    // stays there, so the snap moves the top by the anchor rule, not the top rule.
    expect(s.el.scrollTop).toBeCloseTo(((afterPinch + 60) * 2000) / 2040 - 60, 5);
  });

  it("EZM-FR-OXBL: when the lines wrap again, the line under the pointer goes back under it", () => {
    const { el } = scroller(300, 3000);
    const line = document.createElement("p");
    el.appendChild(line);
    // Before the change the line is at 100..150 on the screen. The text wraps
    // again, so after it the line is 90 px lower than a proportional growth.
    let lineTop = 100;
    line.getBoundingClientRect = () =>
      ({ top: lineTop, bottom: lineTop + 50, height: 50, left: 0, right: 400, width: 400 }) as DOMRect;
    const original = document.elementFromPoint;
    document.elementFromPoint = () => line;
    try {
      const reads = [3000, 3700];
      Object.defineProperty(el, "scrollHeight", {
        get: () => reads.shift() ?? 3700,
        configurable: true,
      });
      const before = el.scrollTop;
      // The layout moves the line when the factor is written: model it on the
      // first read after the change.
      const moved = { done: false };
      const read = line.getBoundingClientRect;
      line.getBoundingClientRect = () => {
        if (getEditorZoom() !== 100 && !moved.done) {
          moved.done = true;
          lineTop = 220;
        }
        return read();
      };
      // The pointer is in the middle of the line.
      pinchEditorZoom(1.2, { clientX: 100, clientY: 125 });
      // The middle of the line was at 125 and is now at 245, so the page
      // scrolls down by 120 to put it back under the pointer.
      expect(el.scrollTop).toBe(before + 120);
    } finally {
      document.elementFromPoint = original;
    }
  });

  it("EZM-FR-OXBL: a scroller that does not scroll is left alone", () => {
    const s = scroller(0, 80);
    let reads = 0;
    Object.defineProperty(s.el, "scrollHeight", {
      get: () => (reads++ === 0 ? 80 : 160),
      configurable: true,
    });
    setEditorZoom(200);
    expect(s.el.scrollTop).toBe(0);
  });
});

describe("zoomable tabs", () => {
  const tab = (over: Partial<Tab>): Tab => ({ id: "t", label: "t", ...over }) as Tab;

  it("EZM-FR-JGRP: the tabs that hold a zoom region are zoomable", () => {
    expect(isZoomableTab(tab({ kind: "editor", artifactId: "a.md" }))).toBe(true);
    expect(isZoomableTab(tab({ artifactId: "a.md" }))).toBe(true);
    expect(isZoomableTab(tab({ kind: "draft" }))).toBe(true);
    expect(isZoomableTab(tab({ kind: "document" }))).toBe(true);
    expect(isZoomableTab(tab({ kind: "draftPreview" }))).toBe(true);
    expect(isZoomableTab(tab({ kind: "openspecChange" }))).toBe(true);
    expect(isZoomableTab(tab({ kind: "diff", artifactId: "a.md" }))).toBe(true);
    // A Diff tab whose comparison deletes the file shows its original text.
    expect(isZoomableTab(tab({ kind: "diff" }))).toBe(true);
  });

  it("EZM-FR-JGRP: the other tabs and the Dashboard are not zoomable", () => {
    for (const kind of ["flow", "search", "conversation", "map", "pdf"] as const) {
      expect(isZoomableTab(tab({ kind })), kind).toBe(false);
    }
    expect(isZoomableTab(tab({ id: "dashboard" }))).toBe(false);
    expect(isZoomableTab(null)).toBe(false);
    expect(isZoomableTab(undefined)).toBe(false);
  });
});
