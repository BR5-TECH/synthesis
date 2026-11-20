/**
 * The hooks of the editor zoom (`EZM-editor-zoom.md`, `SNV-shell-navigation.md`
 * SNV-FR-LVDP / SNV-FR-QHMA / SNV-FR-IPCN): the gestures over the regions, the
 * startup read and the sync with other windows, and the Zoom submenu.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render } from "@testing-library/react";

const invokeMock = vi.fn<(cmd: string, args?: Record<string, unknown>) => Promise<unknown>>();
let listeners: Record<string, (event: { payload: unknown }) => void> = {};
let storedRecord: Record<string, unknown> = {};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invokeMock(cmd, args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, cb: (event: { payload: unknown }) => void) => {
    listeners[event] = cb;
    return () => {
      delete listeners[event];
    };
  }),
  emit: vi.fn(async () => {}),
}));

import { useEditorZoomGestures } from "./useEditorZoomGestures";
import { useEditorZoomLevel, useEditorZoomSync } from "./useEditorZoom";
import { useEditorZoomMenu } from "./useEditorZoomMenu";
import {
  EDITOR_ZOOM_PROPERTY,
  endEditorZoomPinch,
  getEditorZoom,
  pinchEditorZoom,
  resetEditorZoomForTest,
  setEditorZoom,
  startEditorZoomPinch,
} from "../state/editorZoom";
import { resetAppPreferencesCache } from "../state/appPreferences";
import { resetLogBufferForTest } from "../logging";
import { applyActiveRootState, resetActiveRootHealth } from "../state/activeRootHealth";
import {
  APP_PREFERENCES_CHANGED,
  MENU_ZOOM_IN,
  MENU_ZOOM_OUT,
  MENU_ZOOM_RESET,
  MENU_ZOOM_SET,
} from "../events";

function route(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
  switch (cmd) {
    case "load_app_preferences":
      return Promise.resolve({ ...storedRecord });
    case "save_app_preferences":
      storedRecord = { ...(args?.preferences as Record<string, unknown>) };
      return Promise.resolve();
    case "set_zoom_menu_state":
    case "append_log_records":
      return Promise.resolve();
    default:
      return Promise.reject(new Error(`unexpected command ${cmd}`));
  }
}

function lastPush(): { level: unknown; enabled: unknown } | undefined {
  const pushes = zoomMenuPushes();
  return pushes[pushes.length - 1];
}

function zoomMenuPushes(): Array<{ level: unknown; enabled: unknown }> {
  return invokeMock.mock.calls
    .filter(([cmd]) => cmd === "set_zoom_menu_state")
    .map(([, args]) => ({ level: args?.level, enabled: args?.enabled }));
}

beforeEach(() => {
  resetEditorZoomForTest();
  resetAppPreferencesCache();
  resetLogBufferForTest();
  resetActiveRootHealth();
  listeners = {};
  storedRecord = {};
  invokeMock.mockReset();
  invokeMock.mockImplementation(route);
});

afterEach(async () => {
  // Let every save of the test end before the stores reset, so no log or
  // timer of this test reaches the next one.
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
  cleanup();
  document.body.innerHTML = "";
  resetEditorZoomForTest();
  resetLogBufferForTest();
});

// ---------------------------------------------------------------------------
// Gestures
// ---------------------------------------------------------------------------

function GestureHost() {
  useEditorZoomGestures();
  return (
    <div>
      <div className="editor-zoom" data-testid="region">
        <p data-testid="text">Text</p>
        <div className="diagram-viewer__area" data-testid="diagram" />
        <div data-own-zoom="" data-testid="own" />
      </div>
      <aside data-testid="discussion">Thread</aside>
    </div>
  );
}

function ctrlWheel(target: Element, deltaY = -40): WheelEvent {
  const e = new WheelEvent("wheel", { bubbles: true, cancelable: true, deltaY, ctrlKey: true });
  target.dispatchEvent(e);
  return e;
}

describe("useEditorZoomGestures", () => {
  it("EZM-FR-GMSX: a Ctrl wheel over a zoom region changes the level", () => {
    const { getByTestId } = render(<GestureHost />);
    const e = ctrlWheel(getByTestId("text"));
    expect(e.defaultPrevented).toBe(true);
    expect(getEditorZoom()).toBeGreaterThan(100);
  });

  it("EZM-FR-UJWE, EZM-FR-COUH: a Ctrl wheel over the discussion column changes nothing", () => {
    const { getByTestId } = render(<GestureHost />);
    const e = ctrlWheel(getByTestId("discussion"));
    expect(e.defaultPrevented).toBe(false);
    expect(getEditorZoom()).toBe(100);
  });

  it("EZM-FR-BNFJ: a Ctrl wheel over a diagram area or an own-zoom surface changes nothing", () => {
    const { getByTestId } = render(<GestureHost />);
    ctrlWheel(getByTestId("diagram"));
    ctrlWheel(getByTestId("own"));
    expect(getEditorZoom()).toBe(100);
  });

  it("EZM-FR-GMSX, EZM-FR-VPYL: a WebKit gesture over a region pinches and saves once at its end", async () => {
    const { getByTestId } = render(<GestureHost />);
    const text = getByTestId("text");
    const fire = (type: string, scale: number) => {
      const e = new Event(type, { bubbles: true, cancelable: true });
      Object.assign(e, { scale, clientX: 0, clientY: 0 });
      text.dispatchEvent(e);
    };
    fire("gesturestart", 1);
    fire("gesturechange", 1.25);
    fire("gesturechange", 1.5);
    expect(getEditorZoom()).toBeCloseTo(150);
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === "save_app_preferences")).toBe(false);
    fire("gestureend", 1.5);
    await vi.waitFor(() => expect(storedRecord.editorZoomPercent).toBe(150));
    const saves = invokeMock.mock.calls.filter(([cmd]) => cmd === "save_app_preferences");
    expect(saves).toHaveLength(1);
  });

  it("EZM-FR-GMSX: the listeners go away with the host", () => {
    const { getByTestId, unmount } = render(<GestureHost />);
    const region = getByTestId("region");
    const text = getByTestId("text");
    unmount();
    // The region, with its text, stays in the document.
    document.body.appendChild(region);
    ctrlWheel(text);
    expect(getEditorZoom()).toBe(100);
  });
});

// ---------------------------------------------------------------------------
// Startup read and sync
// ---------------------------------------------------------------------------

function SyncHost() {
  useEditorZoomSync();
  return <span data-testid="level">{useEditorZoomLevel()}</span>;
}

describe("useEditorZoomSync", () => {
  it("EZM-FR-HYQD: the window reads the stored level at startup", async () => {
    storedRecord = { editorZoomPercent: 175 };
    const { findByText } = render(<SyncHost />);
    await findByText("175");
    expect(document.documentElement.style.getPropertyValue(EDITOR_ZOOM_PROPERTY)).toBe("1.75");
  });

  it("EZM-FR-HYQD: a stored level out of the range reads as 100%", async () => {
    storedRecord = { editorZoomPercent: 150 };
    const { findByText } = render(<SyncHost />);
    await findByText("150");
    await vi.waitFor(() => expect(listeners[APP_PREFERENCES_CHANGED]).toBeTypeOf("function"));
    storedRecord = { editorZoomPercent: 900 };
    act(() => listeners[APP_PREFERENCES_CHANGED]({ payload: null }));
    await findByText("100");
  });

  it("EZM-FR-HYQD, EZM-FR-SOZM: a level that another window saves shows with no reload", async () => {
    const { findByText } = render(<SyncHost />);
    await findByText("100");
    await vi.waitFor(() => expect(listeners[APP_PREFERENCES_CHANGED]).toBeTypeOf("function"));
    storedRecord = { editorZoomPercent: 67 };
    act(() => listeners[APP_PREFERENCES_CHANGED]({ payload: null }));
    await findByText("67");
  });
});

// ---------------------------------------------------------------------------
// The Zoom submenu
// ---------------------------------------------------------------------------

function MenuHost({ zoomable }: { zoomable: boolean }) {
  useEditorZoomMenu(zoomable);
  return null;
}

async function mountMenu(zoomable: boolean) {
  const view = render(<MenuHost zoomable={zoomable} />);
  await vi.waitFor(() => expect(listeners[MENU_ZOOM_SET]).toBeTypeOf("function"));
  return view;
}

describe("useEditorZoomMenu", () => {
  beforeEach(() => {
    vi.spyOn(document, "hasFocus").mockReturnValue(true);
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("SNV-FR-LVDP, EZM-FR-DMLT: Zoom In, Zoom Out, and Actual Size act on the level", async () => {
    await mountMenu(true);
    act(() => listeners[MENU_ZOOM_IN]({ payload: null }));
    expect(getEditorZoom()).toBe(110);
    act(() => listeners[MENU_ZOOM_OUT]({ payload: null }));
    act(() => listeners[MENU_ZOOM_OUT]({ payload: null }));
    expect(getEditorZoom()).toBe(90);
    act(() => listeners[MENU_ZOOM_RESET]({ payload: null }));
    expect(getEditorZoom()).toBe(100);
  });

  it("SNV-FR-LVDP: a step item sets the level to its step", async () => {
    await mountMenu(true);
    act(() => listeners[MENU_ZOOM_SET]({ payload: { percent: 175 } }));
    expect(getEditorZoom()).toBe(175);
  });

  it("SNV-FR-IPCN: on a tab that is not zoomable the items change nothing", async () => {
    await mountMenu(false);
    act(() => listeners[MENU_ZOOM_IN]({ payload: null }));
    act(() => listeners[MENU_ZOOM_SET]({ payload: { percent: 200 } }));
    expect(getEditorZoom()).toBe(100);
  });

  it("SNV-FR-IPCN: while the worktree gone dialog is open the items change nothing", async () => {
    await mountMenu(true);
    applyActiveRootState({ state: "gone", primaryPath: "/repo" });
    act(() => listeners[MENU_ZOOM_IN]({ payload: null }));
    expect(getEditorZoom()).toBe(100);
  });

  it("SNV-FR-QHMA, SNV-FR-IPCN: the focused window pushes the level and whether the tab is zoomable", async () => {
    const view = await mountMenu(true);
    expect(lastPush()).toEqual({ level: 100, enabled: true });
    act(() => setEditorZoom(137));
    await vi.waitFor(() => expect(lastPush()).toEqual({ level: 137, enabled: true }));
    view.rerender(<MenuHost zoomable={false} />);
    await vi.waitFor(() => expect(lastPush()).toEqual({ level: 137, enabled: false }));
  });

  it("SNV-FR-QHMA: a pinch pushes the whole percent once for each change of it, and renders the host no more", async () => {
    let renders = 0;
    function CountingHost() {
      renders += 1;
      useEditorZoomMenu(true);
      return null;
    }
    render(<CountingHost />);
    await vi.waitFor(() => expect(listeners[MENU_ZOOM_SET]).toBeTypeOf("function"));
    const before = renders;
    const pushesBefore = zoomMenuPushes().length;
    act(() => {
      startEditorZoomPinch();
      for (let i = 0; i < 4; i++) pinchEditorZoom(1.001);
    });
    expect(renders).toBe(before);
    expect(zoomMenuPushes().length).toBe(pushesBefore);
    act(() => pinchEditorZoom(1.5));
    expect(lastPush()).toEqual({ level: 151, enabled: true });
    expect(renders).toBe(before);
    act(() => endEditorZoomPinch());
    await vi.waitFor(() => expect(storedRecord.editorZoomPercent).toBe(151));
  });

  it("SNV-FR-QHMA: a window without focus pushes nothing until it gains focus", async () => {
    const focus = vi.spyOn(document, "hasFocus").mockReturnValue(false);
    await mountMenu(true);
    act(() => setEditorZoom(150));
    expect(zoomMenuPushes()).toEqual([]);
    focus.mockReturnValue(true);
    act(() => {
      window.dispatchEvent(new Event("focus"));
    });
    expect(zoomMenuPushes()).toEqual([{ level: 150, enabled: true }]);
  });

  it("SNV-FR-LVDP: the listeners go away with the host", async () => {
    const view = await mountMenu(true);
    view.unmount();
    await vi.waitFor(() => expect(listeners[MENU_ZOOM_IN]).toBeUndefined());
    expect(listeners[MENU_ZOOM_SET]).toBeUndefined();
  });
});
