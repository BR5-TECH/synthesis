import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, waitFor } from "@testing-library/react";

import { useMainWindowState } from "./useMainWindowState";
import { resetAppPreferencesCache } from "../state/appPreferences";
import { resetLayoutPreferencesCache } from "../state/layoutPreferences";

const invokeMock = vi.fn();

const maximizeMock = vi.fn(async () => {});
const unmaximizeMock = vi.fn(async () => {});
const setSizeMock = vi.fn(async (_s: unknown) => {});
const isMaximizedMock = vi.fn(async () => false);
const outerSizeMock = vi.fn(async () => ({ width: 0, height: 0 }));
const onResizedMock = vi.fn(async (_: unknown) => () => {});
const setResizableMock = vi.fn(async (_: boolean) => {});
const setMaximizableMock = vi.fn(async (_: boolean) => {});
const scaleFactorMock = vi.fn(async () => 1);
const isFullscreenMock = vi.fn(async () => false);
const setFullscreenMock = vi.fn(async (_: boolean) => {});

// Records the order in which the hook's effect makes window calls so tests
// can assert e.g. setResizable(true) happens BEFORE maximize/setSize.
let callOrder: string[] = [];

const availableMonitorsMock = vi.fn(async () => [
  {
    name: "primary",
    size: { width: 2560, height: 1440 },
    position: { x: 0, y: 0 },
    workArea: {
      position: { x: 0, y: 0 },
      size: { width: 2560, height: 1400 },
    },
    scaleFactor: 1,
  },
]);

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/window", () => {
  class LogicalSize {
    width: number;
    height: number;
    constructor(width: number, height: number) {
      this.width = width;
      this.height = height;
    }
  }
  return {
    LogicalSize,
    getCurrentWindow: () => ({
      maximize: () => {
        callOrder.push("maximize");
        return maximizeMock();
      },
      unmaximize: () => unmaximizeMock(),
      setSize: (s: unknown) => {
        callOrder.push("setSize");
        return setSizeMock(s);
      },
      isMaximized: () => isMaximizedMock(),
      outerSize: () => outerSizeMock(),
      onResized: (h: unknown) => onResizedMock(h),
      setResizable: (v: boolean) => {
        callOrder.push("setResizable");
        return setResizableMock(v);
      },
      setMaximizable: (v: boolean) => {
        callOrder.push("setMaximizable");
        return setMaximizableMock(v);
      },
      scaleFactor: () => scaleFactorMock(),
      isFullscreen: () => isFullscreenMock(),
      setFullscreen: (v: boolean) => {
        callOrder.push("setFullscreen");
        return setFullscreenMock(v);
      },
    }),
    availableMonitors: () => availableMonitorsMock(),
  };
});

function Probe() {
  useMainWindowState(true);
  return null;
}

beforeEach(() => {
  invokeMock.mockReset();
  maximizeMock.mockClear();
  unmaximizeMock.mockClear();
  setSizeMock.mockClear();
  isMaximizedMock.mockClear();
  isMaximizedMock.mockResolvedValue(false);
  outerSizeMock.mockClear();
  outerSizeMock.mockResolvedValue({ width: 0, height: 0 });
  onResizedMock.mockClear();
  onResizedMock.mockImplementation(async () => () => {});
  setResizableMock.mockClear();
  setMaximizableMock.mockClear();
  scaleFactorMock.mockClear();
  scaleFactorMock.mockResolvedValue(1);
  isFullscreenMock.mockClear();
  isFullscreenMock.mockResolvedValue(false);
  setFullscreenMock.mockClear();
  // Both records are cached at module scope for the app's life (so the two
  // writers of each compose rather than clobber). Reset them, or one test's
  // persisted state is served to the next.
  resetAppPreferencesCache();
  resetLayoutPreferencesCache();
  callOrder = [];
  availableMonitorsMock.mockClear();
  availableMonitorsMock.mockResolvedValue([
    {
      name: "primary",
      size: { width: 2560, height: 1440 },
      position: { x: 0, y: 0 },
      workArea: {
        position: { x: 0, y: 0 },
        size: { width: 2560, height: 1400 },
      },
      scaleFactor: 1,
    },
  ]);
});

afterEach(() => {
  cleanup();
});

describe("useMainWindowState — first launch (SNV-FR-11)", () => {
  it("calls maximize() when load_layout_preferences returns null", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences") return null;
      return undefined;
    });

    render(<Probe />);

    await waitFor(() => expect(maximizeMock).toHaveBeenCalledTimes(1));
    // setSize must not be called when there is no persisted geometry.
    expect(setSizeMock).not.toHaveBeenCalled();
  });

  it("calls maximize() when load_layout_preferences returns prefs without main-window fields", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences")
        return { verticalPanelFraction: 0.22, bottomPanelCollapsed: false };
      return undefined;
    });

    render(<Probe />);

    await waitFor(() => expect(maximizeMock).toHaveBeenCalledTimes(1));
    expect(setSizeMock).not.toHaveBeenCalled();
  });
});

describe("useMainWindowState — restore persisted dimensions (SNV-FR-12)", () => {
  it("applies persisted outer dimensions and does not maximize when non-maximized state is persisted", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences")
        return {
          mainWindowOuterWidth: 1600,
          mainWindowOuterHeight: 1000,
          mainWindowMaximized: false,
        };
      return undefined;
    });

    render(<Probe />);

    await waitFor(() => expect(setSizeMock).toHaveBeenCalledTimes(1));
    const sizeArg = setSizeMock.mock.calls[0]?.[0] as
      | { width: number; height: number }
      | undefined;
    expect(sizeArg).toBeDefined();
    expect(sizeArg!.width).toBe(1600);
    expect(sizeArg!.height).toBe(1000);
    expect(maximizeMock).not.toHaveBeenCalled();
  });

  it("calls maximize() when persisted state includes mainWindowMaximized=true", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences")
        return {
          mainWindowOuterWidth: 1600,
          mainWindowOuterHeight: 1000,
          mainWindowMaximized: true,
        };
      return undefined;
    });

    render(<Probe />);

    await waitFor(() => expect(maximizeMock).toHaveBeenCalledTimes(1));
    expect(setSizeMock).not.toHaveBeenCalled();
  });
});

describe("useMainWindowState — clamp/fallback (SNV-FR-13)", () => {
  it("falls back to maximize() when persisted geometry does not fit any available monitor", async () => {
    availableMonitorsMock.mockResolvedValue([
      {
        name: "laptop",
        size: { width: 1280, height: 800 },
        position: { x: 0, y: 0 },
        workArea: {
          position: { x: 0, y: 0 },
          size: { width: 1280, height: 760 },
        },
        scaleFactor: 1,
      },
    ]);

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences")
        return {
          // Was sized for an external monitor that's now disconnected.
          mainWindowOuterWidth: 3000,
          mainWindowOuterHeight: 1800,
          mainWindowMaximized: false,
        };
      return undefined;
    });

    render(<Probe />);

    await waitFor(() => expect(maximizeMock).toHaveBeenCalledTimes(1));
    expect(setSizeMock).not.toHaveBeenCalled();
  });
});

describe("useMainWindowState — channel names (contract)", () => {
  it("invokes load_layout_preferences (byte-for-byte name) on mount", async () => {
    invokeMock.mockResolvedValue(null);
    render(<Probe />);
    await waitFor(() => {
      const calls = invokeMock.mock.calls.map((c) => c[0]);
      expect(calls).toContain("load_layout_preferences");
    });
  });

  it("does not invoke load_layout_preferences when disabled", async () => {
    function DisabledProbe() {
      useMainWindowState(false);
      return null;
    }
    invokeMock.mockResolvedValue(null);
    render(<DisabledProbe />);
    // Yield a microtask
    await Promise.resolve();
    await Promise.resolve();
    expect(invokeMock).not.toHaveBeenCalled();
  });
});

describe("useMainWindowState — write path on resize (SNV-FR-12)", () => {
  it("invokes save_layout_preferences with named `preferences` arg containing main-window fields", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences") return null;
      return undefined;
    });

    let resizedHandler: (() => void | Promise<void>) | undefined;
    onResizedMock.mockImplementation(async (h: unknown) => {
      resizedHandler = h as () => void;
      return () => {};
    });

    // After the initial mount-maximize, simulate the user dragging the
    // window to a known outer size and unmaximizing.
    outerSizeMock.mockResolvedValue({ width: 1280, height: 820 });
    isMaximizedMock.mockResolvedValue(false);

    render(<Probe />);

    // Wait for onResized to have been registered (signals mount-path done).
    await waitFor(() => expect(onResizedMock).toHaveBeenCalledTimes(1));
    expect(resizedHandler).toBeDefined();

    // Clear prior invoke calls so we can isolate the save call we triggered.
    invokeMock.mockClear();
    invokeMock.mockResolvedValue(undefined);

    await resizedHandler!();

    // Find the save call.
    const saveCalls = invokeMock.mock.calls.filter(
      (c) => c[0] === "save_layout_preferences",
    );
    expect(saveCalls).toHaveLength(1);

    const [cmd, argObj] = saveCalls[0];
    expect(cmd).toBe("save_layout_preferences");

    // Arg must be a single object with a `preferences` key (not positional,
    // not `prefs`, not spread).
    expect(argObj).toBeTypeOf("object");
    expect(argObj).not.toBeNull();
    const keys = Object.keys(argObj as Record<string, unknown>);
    expect(keys).toContain("preferences");
    expect(keys).not.toContain("prefs");
    // No spread of payload fields into the top-level args object.
    expect(keys).not.toContain("mainWindowOuterWidth");

    const payload = (argObj as { preferences: Record<string, unknown> })
      .preferences;
    expect(payload.mainWindowOuterWidth).toBe(1280);
    expect(payload.mainWindowOuterHeight).toBe(820);
    expect(payload.mainWindowMaximized).toBe(false);
  });

  it("preserves existing panel fields when merging in new main-window fields on save", async () => {
    // load returns a payload with panel fields but no main-window fields.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences")
        return {
          verticalPanelFraction: 0.22,
          verticalPanelSide: "right",
          verticalPanelCollapsed: false,
          bottomPanelHeight: 200,
          bottomPanelHidden: true,
        };
      return undefined;
    });

    let resizedHandler: (() => void | Promise<void>) | undefined;
    onResizedMock.mockImplementation(async (h: unknown) => {
      resizedHandler = h as () => void;
      return () => {};
    });

    outerSizeMock.mockResolvedValue({ width: 1500, height: 900 });
    isMaximizedMock.mockResolvedValue(false);

    render(<Probe />);

    await waitFor(() => expect(onResizedMock).toHaveBeenCalledTimes(1));
    expect(resizedHandler).toBeDefined();

    invokeMock.mockClear();
    invokeMock.mockResolvedValue(undefined);

    await resizedHandler!();

    const saveCalls = invokeMock.mock.calls.filter(
      (c) => c[0] === "save_layout_preferences",
    );
    expect(saveCalls).toHaveLength(1);

    const payload = (
      saveCalls[0][1] as { preferences: Record<string, unknown> }
    ).preferences;

    // Panel fields are preserved byte-for-byte.
    expect(payload.verticalPanelFraction).toBe(0.22);
    expect(payload.verticalPanelSide).toBe("right");
    expect(payload.verticalPanelCollapsed).toBe(false);
    expect(payload.bottomPanelHeight).toBe(200);
    expect(payload.bottomPanelHidden).toBe(true);

    // And new main-window fields are added.
    expect(payload.mainWindowOuterWidth).toBe(1500);
    expect(payload.mainWindowOuterHeight).toBe(900);
    expect(payload.mainWindowMaximized).toBe(false);
  });
});

describe("useMainWindowState — HiDPI logical/physical units (SNV-FR-12)", () => {
  it("persists logical (not physical) pixels on resize when scaleFactor=2", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences") return null;
      return undefined;
    });

    let resizedHandler: (() => void | Promise<void>) | undefined;
    onResizedMock.mockImplementation(async (h: unknown) => {
      resizedHandler = h as () => void;
      return () => {};
    });

    // 2× HiDPI display: outerSize returns physical pixels for a window the
    // user sees as 1280x800 logical.
    scaleFactorMock.mockResolvedValue(2);
    outerSizeMock.mockResolvedValue({ width: 2560, height: 1600 });
    isMaximizedMock.mockResolvedValue(false);

    render(<Probe />);

    await waitFor(() => expect(onResizedMock).toHaveBeenCalledTimes(1));
    expect(resizedHandler).toBeDefined();

    invokeMock.mockClear();
    invokeMock.mockResolvedValue(undefined);

    await resizedHandler!();

    const saveCalls = invokeMock.mock.calls.filter(
      (c) => c[0] === "save_layout_preferences",
    );
    expect(saveCalls).toHaveLength(1);

    const payload = (
      saveCalls[0][1] as { preferences: Record<string, unknown> }
    ).preferences;

    // Must be the LOGICAL values (1280x800), NOT the raw physical
    // 2560x1600 that outerSize() returned.
    expect(payload.mainWindowOuterWidth).toBe(1280);
    expect(payload.mainWindowOuterHeight).toBe(800);
  });

  it("treats persisted logical geometry as fitting a 2× monitor whose physical workArea is 2880x1700", async () => {
    // workArea is physical (Tauri 2 semantics). Logical-equivalent is
    // 1440x850, which comfortably contains the persisted 1280x800 logical.
    availableMonitorsMock.mockResolvedValue([
      {
        name: "retina",
        size: { width: 2880, height: 1800 },
        position: { x: 0, y: 0 },
        workArea: {
          position: { x: 0, y: 0 },
          size: { width: 2880, height: 1700 },
        },
        scaleFactor: 2,
      },
    ]);

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences")
        return {
          mainWindowOuterWidth: 1280,
          mainWindowOuterHeight: 800,
          mainWindowMaximized: false,
        };
      return undefined;
    });

    render(<Probe />);

    // Geometry fits → restore via setSize, NOT maximize.
    await waitFor(() => expect(setSizeMock).toHaveBeenCalledTimes(1));
    const sizeArg = setSizeMock.mock.calls[0]?.[0] as
      | { width: number; height: number }
      | undefined;
    expect(sizeArg).toBeDefined();
    expect(sizeArg!.width).toBe(1280);
    expect(sizeArg!.height).toBe(800);
    expect(maximizeMock).not.toHaveBeenCalled();
  });
});

describe("useMainWindowState — full-screen restore (SNV-FR-38 / SNV-FR-39)", () => {
  /** Mount with a given persisted layout + app-preferences record. */
  function mountWith(opts: {
    layout?: unknown;
    fullscreen?: boolean;
    theme?: string;
  }) {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences") return opts.layout ?? null;
      if (cmd === "load_app_preferences")
        return {
          theme: opts.theme ?? "system",
          mainWindowFullscreen: !!opts.fullscreen,
        };
      return undefined;
    });
    return render(<Probe />);
  }

  it("enters full-screen on mount when the persisted preference is set (SNV-FR-38, SNV-FR-39)", async () => {
    mountWith({ fullscreen: true });
    await waitFor(() => expect(setFullscreenMock).toHaveBeenCalledWith(true));
  });

  it("does not enter full-screen when the preference is unset", async () => {
    mountWith({ fullscreen: false });
    await waitFor(() => expect(onResizedMock).toHaveBeenCalledTimes(1));
    expect(setFullscreenMock).not.toHaveBeenCalled();
  });

  it("restores the project's geometry underneath, then goes full-screen (SNV-FR-39)", async () => {
    // The order is load-bearing: geometry first, full-screen over it, so
    // leaving full-screen reveals the window at the project's own size rather
    // than at whatever the OS last left it.
    mountWith({
      fullscreen: true,
      layout: {
        mainWindowOuterWidth: 1600,
        mainWindowOuterHeight: 1000,
        mainWindowMaximized: false,
      },
    });

    await waitFor(() => expect(setFullscreenMock).toHaveBeenCalledWith(true));
    const sizeArg = setSizeMock.mock.calls[0]?.[0] as
      | { width: number; height: number }
      | undefined;
    expect(sizeArg).toBeDefined();
    expect(sizeArg!.width).toBe(1600);
    expect(sizeArg!.height).toBe(1000);

    const setSizeIdx = callOrder.indexOf("setSize");
    const fullscreenIdx = callOrder.indexOf("setFullscreen");
    expect(setSizeIdx).toBeGreaterThanOrEqual(0);
    expect(fullscreenIdx).toBeGreaterThan(setSizeIdx);
  });

  it("reads the flag from the app-preferences record, not from the layout payload", async () => {
    // SNV-FR-08 / GSS-FR-19: full-screen is user-global. A layout payload
    // carrying the field (a stale or hand-edited store) must not drive it.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences")
        return { mainWindowFullscreen: true };
      if (cmd === "load_app_preferences")
        return { theme: "system", mainWindowFullscreen: false };
      return undefined;
    });
    render(<Probe />);
    await waitFor(() => expect(onResizedMock).toHaveBeenCalledTimes(1));
    expect(setFullscreenMock).not.toHaveBeenCalled();
  });

  it("persists the flag when the user enters full-screen (SNV-FR-38, SNV-FR-39)", async () => {
    let resizedHandler: (() => void | Promise<void>) | undefined;
    onResizedMock.mockImplementation(async (h: unknown) => {
      resizedHandler = h as () => void;
      return () => {};
    });

    mountWith({ fullscreen: false });
    await waitFor(() => expect(onResizedMock).toHaveBeenCalledTimes(1));

    // The user goes full-screen: Tauri reports it as a resize.
    isFullscreenMock.mockResolvedValue(true);
    invokeMock.mockClear();
    await resizedHandler!();

    const saves = invokeMock.mock.calls.filter(
      (c) => c[0] === "save_app_preferences",
    );
    expect(saves).toHaveLength(1);
    const payload = (saves[0][1] as { preferences: Record<string, unknown> })
      .preferences;
    expect(payload.mainWindowFullscreen).toBe(true);
  });

  it("clears the flag when the user leaves full-screen (SNV-FR-38, SNV-FR-39, SNV-FR-12)", async () => {
    let resizedHandler: (() => void | Promise<void>) | undefined;
    onResizedMock.mockImplementation(async (h: unknown) => {
      resizedHandler = h as () => void;
      return () => {};
    });

    isFullscreenMock.mockResolvedValue(true);
    mountWith({ fullscreen: true });
    await waitFor(() => expect(onResizedMock).toHaveBeenCalledTimes(1));

    isFullscreenMock.mockResolvedValue(false);
    outerSizeMock.mockResolvedValue({ width: 1400, height: 900 });
    invokeMock.mockClear();
    await resizedHandler!();

    const saves = invokeMock.mock.calls.filter(
      (c) => c[0] === "save_app_preferences",
    );
    expect(saves).toHaveLength(1);
    const payload = (saves[0][1] as { preferences: Record<string, unknown> })
      .preferences;
    expect(payload.mainWindowFullscreen).toBe(false);
  });

  it("carries the theme through the full-screen write (GSS-FR-20)", async () => {
    let resizedHandler: (() => void | Promise<void>) | undefined;
    onResizedMock.mockImplementation(async (h: unknown) => {
      resizedHandler = h as () => void;
      return () => {};
    });

    mountWith({ fullscreen: false, theme: "dark" });
    await waitFor(() => expect(onResizedMock).toHaveBeenCalledTimes(1));

    isFullscreenMock.mockResolvedValue(true);
    invokeMock.mockClear();
    await resizedHandler!();

    const payload = (
      invokeMock.mock.calls.find((c) => c[0] === "save_app_preferences")![1] as {
        preferences: Record<string, unknown>;
      }
    ).preferences;
    expect(payload.mainWindowFullscreen).toBe(true);
    expect(payload.theme).toBe(
      "dark",
      // A bare `{ mainWindowFullscreen }` write would reset the user's theme to
      // the backend default on their next launch.
    );
  });

  it("writes only on a transition, not on every resize while full-screen", async () => {
    let resizedHandler: (() => void | Promise<void>) | undefined;
    onResizedMock.mockImplementation(async (h: unknown) => {
      resizedHandler = h as () => void;
      return () => {};
    });

    isFullscreenMock.mockResolvedValue(true);
    mountWith({ fullscreen: true });
    await waitFor(() => expect(onResizedMock).toHaveBeenCalledTimes(1));

    invokeMock.mockClear();
    await resizedHandler!();
    await resizedHandler!();
    await resizedHandler!();

    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "save_app_preferences"),
    ).toHaveLength(0);
  });

  it("does not overwrite the project's geometry while full-screen (SNV-FR-39)", async () => {
    // The full-screen size is the display's, not a size the user chose. Writing
    // it would destroy the geometry the window must return to on exit.
    let resizedHandler: (() => void | Promise<void>) | undefined;
    onResizedMock.mockImplementation(async (h: unknown) => {
      resizedHandler = h as () => void;
      return () => {};
    });

    isFullscreenMock.mockResolvedValue(true);
    mountWith({
      fullscreen: true,
      layout: {
        mainWindowOuterWidth: 1600,
        mainWindowOuterHeight: 1000,
        mainWindowMaximized: false,
      },
    });
    await waitFor(() => expect(onResizedMock).toHaveBeenCalledTimes(1));

    // A resize arrives carrying the full display's dimensions.
    outerSizeMock.mockResolvedValue({ width: 3840, height: 2160 });
    invokeMock.mockClear();
    await resizedHandler!();

    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "save_layout_preferences"),
    ).toHaveLength(0);
  });

  it("stores the settled size, not the mid-transition one, on leaving full-screen", async () => {
    // macOS resolves `setFullscreen(false)` when the transition is *dispatched*,
    // not when the Space-exit animation finishes — so the resize that announces
    // the exit can still report the display's dimensions. Persisting that would
    // destroy the geometry SNV-FR-39 exists to restore, and would leave the next
    // launch opening near-display-sized (or failing the fit check and
    // maximizing).
    let resizedHandler: (() => void | Promise<void>) | undefined;
    onResizedMock.mockImplementation(async (h: unknown) => {
      resizedHandler = h as () => void;
      return () => {};
    });

    isFullscreenMock.mockResolvedValue(true);
    mountWith({ fullscreen: true });
    await waitFor(() => expect(onResizedMock).toHaveBeenCalledTimes(1));

    isMaximizedMock.mockResolvedValue(false);
    invokeMock.mockClear();

    // The resize announcing the exit still reports the full display.
    isFullscreenMock.mockResolvedValue(false);
    outerSizeMock.mockResolvedValue({ width: 3840, height: 2160 });
    await resizedHandler!();
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "save_layout_preferences"),
    ).toHaveLength(0);

    // The next resize carries the settled, windowed size — that one is stored.
    outerSizeMock.mockResolvedValue({ width: 1440, height: 880 });
    await resizedHandler!();

    const saves = invokeMock.mock.calls.filter(
      (c) => c[0] === "save_layout_preferences",
    );
    expect(saves).toHaveLength(1);
    const payload = (saves[0][1] as { preferences: Record<string, unknown> })
      .preferences;
    expect(payload.mainWindowOuterWidth).toBe(1440);
    expect(payload.mainWindowOuterHeight).toBe(880);
  });

  it("does not persist a 0x0 outer size (minimize/iconify)", async () => {
    // Some platforms emit a Resized with 0x0 on minimize. Storing it would
    // restore the next launch to an invisible window.
    let resizedHandler: (() => void | Promise<void>) | undefined;
    onResizedMock.mockImplementation(async (h: unknown) => {
      resizedHandler = h as () => void;
      return () => {};
    });

    mountWith({ fullscreen: false });
    await waitFor(() => expect(onResizedMock).toHaveBeenCalledTimes(1));

    outerSizeMock.mockResolvedValue({ width: 0, height: 0 });
    invokeMock.mockClear();
    await resizedHandler!();

    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "save_layout_preferences"),
    ).toHaveLength(0);
  });

  it("treats a zeroed persisted geometry as unset rather than sizing to 0x0", async () => {
    // The Rust struct's geometry fields are plain f64 under #[serde(default)],
    // so a record written without them reads back as 0.0. A `typeof === number`
    // check would accept that and call setSize(0, 0).
    mountWith({
      layout: {
        verticalPanelFraction: 0.3,
        mainWindowOuterWidth: 0,
        mainWindowOuterHeight: 0,
        mainWindowMaximized: false,
      },
    });

    await waitFor(() => expect(maximizeMock).toHaveBeenCalledTimes(1));
    expect(setSizeMock).not.toHaveBeenCalled();
  });

  it("seeds the full-screen baseline from the window, not from the request", async () => {
    // The OS can restore a window into a full-screen space independently of our
    // stored preference. Seeding from the *desired* value would make the first
    // resize look like a transition and write the wrong answer.
    let resizedHandler: (() => void | Promise<void>) | undefined;
    onResizedMock.mockImplementation(async (h: unknown) => {
      resizedHandler = h as () => void;
      return () => {};
    });

    // Preference says windowed; the window is actually full-screen.
    isFullscreenMock.mockResolvedValue(true);
    mountWith({ fullscreen: false });
    await waitFor(() => expect(onResizedMock).toHaveBeenCalledTimes(1));

    invokeMock.mockClear();
    await resizedHandler!();

    // No spurious transition write, and no geometry stored while full-screen.
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "save_app_preferences"),
    ).toHaveLength(0);
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "save_layout_preferences"),
    ).toHaveLength(0);
  });

  it("re-reads and re-applies geometry when the open project changes (SNV-FR-35, SNV-FR-40, OVW-FR-11)", async () => {
    // A switch opens a different layout slot. Leaving A's geometry applied
    // would also write A's layout into B's slot on the next resize.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences")
        return {
          mainWindowOuterWidth: 1600,
          mainWindowOuterHeight: 1000,
          mainWindowMaximized: false,
        };
      if (cmd === "load_app_preferences")
        return { theme: "system", mainWindowFullscreen: false };
      return undefined;
    });

    function Switchable({ projectKey }: { projectKey: string }) {
      useMainWindowState(true, projectKey);
      return null;
    }

    const view = render(<Switchable projectKey="/dev/a" />);
    await waitFor(() => expect(setSizeMock).toHaveBeenCalledTimes(1));

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences")
        return {
          mainWindowOuterWidth: 1200,
          mainWindowOuterHeight: 800,
          mainWindowMaximized: false,
        };
      if (cmd === "load_app_preferences")
        return { theme: "system", mainWindowFullscreen: false };
      return undefined;
    });
    view.rerender(<Switchable projectKey="/dev/b" />);

    await waitFor(() => expect(setSizeMock).toHaveBeenCalledTimes(2));
    const second = setSizeMock.mock.calls[1]?.[0] as {
      width: number;
      height: number;
    };
    expect(second.width).toBe(1200);
    expect(second.height).toBe(800);
  });

  it("carries the panel fraction through a geometry write (lost-update guard)", async () => {
    // `useVerticalPanel` writes the panel's fraction into this same record on
    // drag release. A geometry write built from a mount-time snapshot would
    // revert the user's last drag.
    let stored: Record<string, unknown> = { verticalPanelFraction: 0.25 };
    invokeMock.mockImplementation(
      async (cmd: string, args?: Record<string, unknown>) => {
        if (cmd === "load_layout_preferences") return stored;
        if (cmd === "save_layout_preferences") {
          stored = (args as { preferences: Record<string, unknown> })
            .preferences;
          return undefined;
        }
        if (cmd === "load_app_preferences")
          return { theme: "system", mainWindowFullscreen: false };
        return undefined;
      },
    );

    let resizedHandler: (() => void | Promise<void>) | undefined;
    onResizedMock.mockImplementation(async (h: unknown) => {
      resizedHandler = h as () => void;
      return () => {};
    });

    render(<Probe />);
    await waitFor(() => expect(onResizedMock).toHaveBeenCalledTimes(1));

    // The panel writer persists a new fraction after this hook mounted.
    const { patchLayoutPreferences } = await import(
      "../state/layoutPreferences"
    );
    await patchLayoutPreferences("", { verticalPanelFraction: 0.4 });

    outerSizeMock.mockResolvedValue({ width: 1500, height: 900 });
    isMaximizedMock.mockResolvedValue(false);
    await resizedHandler!();

    expect(stored.verticalPanelFraction).toBe(0.4);
    expect(stored.mainWindowOuterWidth).toBe(1500);
    expect(stored.mainWindowOuterHeight).toBe(900);
  });

  it("still mounts usably when the platform refuses full-screen", async () => {
    setFullscreenMock.mockRejectedValue(new Error("unsupported"));
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    mountWith({ fullscreen: true });
    // The geometry path still completes and the resize listener installs.
    await waitFor(() => expect(onResizedMock).toHaveBeenCalledTimes(1));
    warn.mockRestore();
  });
});

describe("useMainWindowState — chrome unlock on entry (SNV-FR-12 / SNV-FR-13)", () => {
  it("calls setResizable(true) and setMaximizable(true) before applying geometry", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences") return null;
      return undefined;
    });

    render(<Probe />);

    await waitFor(() => expect(setResizableMock).toHaveBeenCalledWith(true));
    await waitFor(() =>
      expect(setMaximizableMock).toHaveBeenCalledWith(true),
    );
    // Both unlock calls must precede the maximize/setSize decision so that
    // the platform accepts the subsequent maximize() / setSize() — and so
    // the IDE shell isn't permanently locked down by the picker's chrome.
    await waitFor(() => expect(maximizeMock).toHaveBeenCalledTimes(1));

    const resizableIdx = callOrder.indexOf("setResizable");
    const maximizableIdx = callOrder.indexOf("setMaximizable");
    const maximizeIdx = callOrder.indexOf("maximize");
    const setSizeIdx = callOrder.indexOf("setSize");
    const firstGeometryIdx =
      setSizeIdx === -1
        ? maximizeIdx
        : maximizeIdx === -1
          ? setSizeIdx
          : Math.min(setSizeIdx, maximizeIdx);

    expect(resizableIdx).toBeGreaterThanOrEqual(0);
    expect(maximizableIdx).toBeGreaterThanOrEqual(0);
    expect(firstGeometryIdx).toBeGreaterThanOrEqual(0);
    expect(resizableIdx).toBeLessThan(firstGeometryIdx);
    expect(maximizableIdx).toBeLessThan(firstGeometryIdx);
  });
});
