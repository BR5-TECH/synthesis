/**
 * The page width ruler (`EPW-editor-page-width.md`): its geometry, its drag,
 * its keys, and its double-click.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const invokeMock = vi.fn<(cmd: string, args?: Record<string, unknown>) => Promise<unknown>>();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invokeMock(cmd, args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));

import {
  PageWidthRuler,
  rulerGeometry,
  widthAfterKey,
  widthFromDrag,
} from "./PageWidthRuler";
import {
  PAGE_WIDTH_PROPERTY,
  adoptStoredEditorPageWidth,
  getEditorPageWidth,
  previewEditorPageWidth,
  resetEditorPageWidthForTest,
} from "../../state/editorPageWidth";
import { resetEditorZoomForTest, setEditorZoom } from "../../state/editorZoom";
import { resetAppPreferencesCache } from "../../state/appPreferences";
import { resetLogBufferForTest } from "../../logging";

/** The ruler is this wide and starts at the left of the window. */
let rulerWidth = 2000;
let refuseSave = false;
const observers: (() => void)[] = [];

function route(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
  switch (cmd) {
    case "load_app_preferences":
      return Promise.resolve({});
    case "save_app_preferences":
      return refuseSave ? Promise.reject(new Error("disk is full")) : Promise.resolve(args);
    case "append_log_records":
      return Promise.resolve();
    default:
      return Promise.reject(new Error(`unexpected command ${cmd}`));
  }
}

const saves = () =>
  invokeMock.mock.calls
    .filter((c) => c[0] === "save_app_preferences")
    .map((c) => (c[1] as { preferences: Record<string, unknown> }).preferences.editorPageWidthPx);
const handle = (side: "left" | "right") => screen.getByTestId(`page-ruler-handle-${side}`);
const leftOf = (side: "left" | "right") => parseFloat(handle(side).style.left);
/** Let every queued save and its handlers run. */
const settle = () =>
  act(async () => {
    await new Promise<void>((res) => setTimeout(res, 0));
  });

beforeEach(() => {
  rulerWidth = 2000;
  refuseSave = false;
  observers.length = 0;
  invokeMock.mockReset();
  invokeMock.mockImplementation(route);
  resetEditorPageWidthForTest();
  resetEditorZoomForTest();
  resetAppPreferencesCache();
  resetLogBufferForTest();
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (
    this: HTMLElement,
  ) {
    const width = this.classList.contains("page-ruler") ? rulerWidth : 0;
    return { left: 0, top: 0, right: width, bottom: 16, width, height: 16, x: 0, y: 0, toJSON() {} };
  });
  vi.stubGlobal(
    "ResizeObserver",
    class {
      constructor(cb: () => void) {
        observers.push(cb);
      }
      observe() {}
      disconnect() {}
    },
  );
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  resetEditorPageWidthForTest();
  resetEditorZoomForTest();
});

describe("ruler geometry", () => {
  it("EPW-FR-WOZO, EPW-FR-GNNZ: the handles stand at the edges of the text, at the zoom of the page", () => {
    expect(rulerGeometry(2000, 820, 1)).toMatchObject({ textLeft: 590, textRight: 1410 });
    // 820 × 1.5 = 1230 pixels of text.
    expect(rulerGeometry(2000, 820, 1.5)).toMatchObject({ textLeft: 385, textRight: 1615 });
    const g = rulerGeometry(2000, 820, 1);
    expect(g.textLeft - g.pageLeft).toBe(49);
  });

  it("EPW-FR-KCZU: in a narrow column the page is the column less its inset", () => {
    const g = rulerGeometry(600, 1800, 1);
    expect(g.pageLeft).toBe(16);
    expect(g.pageRight).toBe(584);
    expect(g.textLeft).toBe(65);
    const none = rulerGeometry(0, 820, 1);
    expect(none.pageRight - none.pageLeft).toBe(0);
    expect(none.textRight - none.textLeft).toBe(0);
  });

  it("EPW-FR-AOVQ, EPW-FR-GNNZ: a drag adds twice the outward movement over the zoom to the start width, held in range", () => {
    expect(widthFromDrag(820, 90, 1)).toBe(1000);
    expect(widthFromDrag(820, -130, 1)).toBe(560);
    expect(widthFromDrag(1000, -100, 1)).toBe(800);
    expect(widthFromDrag(820, 90, 1.5)).toBe(940);
    expect(widthFromDrag(820, 0.4, 1)).toBe(821);
    expect(widthFromDrag(820, 5000, 1)).toBe(1800);
    expect(widthFromDrag(820, 90, 0)).toBe(820);
    expect(widthFromDrag(820, 90, Number.NaN)).toBe(820);
    expect(widthFromDrag(820, Number.NaN, 1)).toBe(820);
  });

  it("EPW-FR-IUHL: the outward arrow widens, the other narrows, Shift steps 100, Home and End go to the limits", () => {
    expect(widthAfterKey(820, "left", "ArrowLeft", false)).toBe(840);
    expect(widthAfterKey(820, "left", "ArrowRight", false)).toBe(800);
    expect(widthAfterKey(820, "right", "ArrowRight", false)).toBe(840);
    expect(widthAfterKey(820, "right", "ArrowLeft", false)).toBe(800);
    expect(widthAfterKey(820, "right", "ArrowRight", true)).toBe(920);
    expect(widthAfterKey(820, "left", "ArrowRight", true)).toBe(720);
    expect(widthAfterKey(820, "left", "Home", false)).toBe(560);
    expect(widthAfterKey(820, "right", "End", false)).toBe(1800);
    expect(widthAfterKey(1790, "right", "ArrowRight", false)).toBe(1800);
    expect(widthAfterKey(570, "right", "ArrowLeft", true)).toBe(560);
    expect(widthAfterKey(820, "right", "ArrowUp", false)).toBeNull();
  });
});

describe("the ruler", () => {
  it("EPW-FR-IUHL, EPW-FR-WOZO: each handle is a focusable separator that carries the width", () => {
    render(<PageWidthRuler />);
    for (const side of ["left", "right"] as const) {
      const el = handle(side);
      expect(el).toHaveAttribute("role", "separator");
      expect(el).toHaveAttribute("tabindex", "0");
      expect(el).toHaveAttribute("aria-valuemin", "560");
      expect(el).toHaveAttribute("aria-valuemax", "1800");
      expect(el).toHaveAttribute("aria-valuenow", "820");
      expect(el).toHaveAttribute("aria-valuetext", "820 pixels");
    }
    expect(leftOf("left")).toBe(590);
    expect(leftOf("right")).toBe(1410);
  });

  it("EPW-FR-WOZO, EPW-FR-GNNZ: the handles follow a change of the zoom and of the column, and save nothing", async () => {
    render(<PageWidthRuler />);
    act(() => setEditorZoom(150));
    expect(leftOf("left")).toBe(385);
    // The zoom saves its own level; only the page width saves count here.
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("save_app_preferences", expect.anything()));
    invokeMock.mockClear();
    rulerWidth = 1000;
    act(() => observers.forEach((cb) => cb()));
    // The page is capped at 1000 − 32, so the text is 968 − 98 wide.
    expect(leftOf("left")).toBe(65);
    await settle();
    expect(saves()).toEqual([]);
  });

  it("EPW-FR-IUHL, EPW-FR-JNCM: a key on a handle changes the width and saves at once", async () => {
    render(<PageWidthRuler />);
    const pressed = fireEvent.keyDown(handle("right"), { key: "ArrowRight" });
    expect(pressed).toBe(false);
    expect(getEditorPageWidth()).toBe(840);
    await waitFor(() => expect(saves()).toEqual([840]));
    fireEvent.keyDown(handle("left"), { key: "ArrowRight", shiftKey: true });
    await waitFor(() => expect(saves()).toEqual([840, 740]));
    expect(handle("left")).toHaveAttribute("aria-valuenow", "740");
    expect(fireEvent.keyDown(handle("left"), { key: "a" })).toBe(true);
    fireEvent.keyDown(handle("left"), { key: "End" });
    await waitFor(() => expect(saves()).toEqual([840, 740, 1800]));
    fireEvent.keyDown(handle("right"), { key: "Home" });
    await waitFor(() => expect(saves()).toEqual([840, 740, 1800, 560]));
  });

  it("EPW-FR-TQFY: a drag of the left handle labels the left handle", () => {
    render(<PageWidthRuler />);
    fireEvent.pointerDown(handle("left"), { button: 0, clientX: 590 });
    act(() => {
      fireEvent.pointerMove(window, { clientX: 500 });
    });
    expect(handle("left").querySelector(".page-ruler__label")).toHaveTextContent("1000 px");
    expect(handle("right").querySelector(".page-ruler__label")).toBeNull();
    act(() => {
      fireEvent.pointerUp(window, { clientX: 500 });
    });
  });

  it("EPW-FR-ZCBT: a ruler that goes away after a press with no move saves nothing and adopts records again", async () => {
    const { unmount } = render(<PageWidthRuler />);
    fireEvent.pointerDown(handle("right"), { button: 0 });
    unmount();
    await settle();
    expect(saves()).toEqual([]);
    adoptStoredEditorPageWidth({ editorPageWidthPx: 1700 });
    expect(getEditorPageWidth()).toBe(1700);
  });

  it("EPW-FR-NEFQ: a double-click on a handle sets 820 and saves at once", async () => {
    adoptStoredEditorPageWidth({ editorPageWidthPx: 1400 });
    render(<PageWidthRuler />);
    fireEvent.doubleClick(handle("left"));
    expect(getEditorPageWidth()).toBe(820);
    await waitFor(() => expect(saves()).toEqual([820]));
  });

  it("EPW-FR-AOVQ, EPW-FR-JNCM, EPW-FR-TQFY: a drag shows each width, labels the held handle, and saves once at its end", async () => {
    render(<PageWidthRuler />);
    fireEvent.pointerDown(handle("right"), { button: 0, clientX: 1410 });
    expect(handle("right")).toHaveFocus();
    // 90 pixels outward add 180 pixels of width.
    act(() => {
      fireEvent.pointerMove(window, { clientX: 1500 });
    });
    expect(getEditorPageWidth()).toBe(1000);
    expect(document.documentElement.style.getPropertyValue(PAGE_WIDTH_PROPERTY)).toBe("1000px");
    act(() => {
      fireEvent.pointerMove(window, { clientX: 1600 });
    });
    expect(getEditorPageWidth()).toBe(1200);
    expect(screen.getByTestId("page-ruler-label")).toHaveTextContent("1200 px");
    expect(handle("left").querySelector(".page-ruler__label")).toBeNull();
    expect(saves()).toEqual([]);
    act(() => {
      fireEvent.pointerUp(window, { clientX: 1650 });
    });
    expect(getEditorPageWidth()).toBe(1300);
    await waitFor(() => expect(saves()).toEqual([1300]));
    // A second drag counts from the width that the first one set.
    fireEvent.pointerDown(handle("right"), { button: 0, clientX: 1000 });
    act(() => {
      fireEvent.pointerMove(window, { clientX: 950 });
    });
    act(() => {
      fireEvent.pointerUp(window, { clientX: 950 });
    });
    expect(getEditorPageWidth()).toBe(1200);
    await waitFor(() => expect(saves()).toEqual([1300, 1200]));
    expect(screen.queryByTestId("page-ruler-label")).toBeNull();
  });

  it("EPW-FR-GNNZ: at 150% zoom a drag sets the distance over the zoom", async () => {
    setEditorZoom(150);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("save_app_preferences", expect.anything()));
    invokeMock.mockClear();
    render(<PageWidthRuler />);
    fireEvent.pointerDown(handle("right"), { button: 0, clientX: 1615 });
    // 135 pixels outward at 150% add 180 pixels of width.
    act(() => {
      fireEvent.pointerMove(window, { clientX: 1750 });
    });
    act(() => {
      fireEvent.pointerUp(window, { clientX: 1750 });
    });
    expect(getEditorPageWidth()).toBe(1000);
    await waitFor(() => expect(saves()).toEqual([1000]));
  });

  it("EPW-FR-JNCM: a click with no move and a right button change nothing and save nothing", async () => {
    render(<PageWidthRuler />);
    fireEvent.pointerDown(handle("right"), { button: 0, clientX: 1420 });
    act(() => {
      fireEvent.pointerUp(window, { clientX: 1420 });
    });
    fireEvent.pointerDown(handle("left"), { button: 2, clientX: 300 });
    act(() => {
      fireEvent.pointerMove(window, { clientX: 300 });
    });
    expect(getEditorPageWidth()).toBe(820);
    await settle();
    expect(saves()).toEqual([]);
  });

  it("EPW-FR-IMWS, EPW-FR-KCZU: on a page that the column caps, a drag outward makes the width larger, and the handles stay at the capped edges", async () => {
    rulerWidth = 700;
    render(<PageWidthRuler />);
    // The page is 700 − 32 wide, so the text is 570 wide and starts at 65.
    expect(leftOf("right")).toBe(635);
    fireEvent.pointerDown(handle("right"), { button: 0, clientX: 635 });
    act(() => {
      fireEvent.pointerMove(window, { clientX: 695 });
    });
    expect(getEditorPageWidth()).toBe(940);
    expect(screen.getByTestId("page-ruler-label")).toHaveTextContent("940 px");
    expect(handle("right")).toHaveAttribute("aria-valuenow", "940");
    expect(leftOf("right")).toBe(635);
    act(() => {
      fireEvent.pointerUp(window, { clientX: 695 });
    });
    await waitFor(() => expect(saves()).toEqual([940]));
  });

  it("EPW-FR-JNCM: a cancelled drag keeps and saves the width it showed", async () => {
    render(<PageWidthRuler />);
    fireEvent.pointerDown(handle("left"), { button: 0, clientX: 590 });
    // The left handle widens the page when it moves to the left.
    act(() => {
      fireEvent.pointerMove(window, { clientX: 450 });
    });
    act(() => {
      fireEvent.pointerCancel(window);
    });
    expect(getEditorPageWidth()).toBe(1100);
    await waitFor(() => expect(saves()).toEqual([1100]));
  });

  it("EPW-FR-ZCBT: a record from another window during a drag does not move the handles", async () => {
    render(<PageWidthRuler />);
    fireEvent.pointerDown(handle("right"), { button: 0, clientX: 1410 });
    act(() => {
      fireEvent.pointerMove(window, { clientX: 1500 });
    });
    act(() => adoptStoredEditorPageWidth({ editorPageWidthPx: 1700 }));
    expect(getEditorPageWidth()).toBe(1000);
    act(() => {
      fireEvent.pointerUp(window, { clientX: 1500 });
    });
    await waitFor(() => expect(saves()).toEqual([1000]));
    expect(getEditorPageWidth()).toBe(1000);
  });

  it("EPW-FR-ZCBT: a ruler that goes away during a drag ends the drag, so a later record is adopted", async () => {
    const { unmount } = render(<PageWidthRuler />);
    fireEvent.pointerDown(handle("right"), { button: 0, clientX: 1410 });
    act(() => {
      fireEvent.pointerMove(window, { clientX: 1500 });
    });
    unmount();
    await waitFor(() => expect(saves()).toEqual([1000]));
    adoptStoredEditorPageWidth({ editorPageWidthPx: 1700 });
    expect(getEditorPageWidth()).toBe(1700);
  });

  it("EPW-FR-LSZF: a refused drag save keeps the width and shows no error", async () => {
    refuseSave = true;
    render(<PageWidthRuler />);
    fireEvent.pointerDown(handle("right"), { button: 0, clientX: 1410 });
    act(() => {
      fireEvent.pointerMove(window, { clientX: 1600 });
    });
    act(() => {
      fireEvent.pointerUp(window, { clientX: 1600 });
    });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("save_app_preferences", expect.anything()));
    await settle();
    expect(getEditorPageWidth()).toBe(1200);
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("EPW-FR-WOZO: the page band and the scale follow the width", () => {
    render(<PageWidthRuler />);
    act(() => previewEditorPageWidth(1000));
    const band = document.querySelector<HTMLElement>(".page-ruler__page")!;
    expect(parseFloat(band.style.left)).toBe(451);
    expect(parseFloat(band.style.width)).toBe(1098);
    const scale = document.querySelector<HTMLElement>(".page-ruler__scale")!;
    expect(scale.style.getPropertyValue("--page-ruler-origin")).toBe("500px");
    expect(scale.style.getPropertyValue("--page-ruler-minor")).toBe("20px");
    expect(scale.style.getPropertyValue("--page-ruler-major")).toBe("100px");
  });
});
