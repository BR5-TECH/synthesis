/**
 * The Terminal theme of Settings in the main window (`GLS-global-settings.md`
 * GLS-FR-VTVK, `TRM-terminals.md` TRM-FR-IUMC): the startup read and the sync
 * with the Settings window, which saves the record.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, waitFor } from "@testing-library/react";

let listeners: Record<string, (event: { payload: unknown }) => void> = {};
let storedRecord: Record<string, unknown> = {};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string) => {
    if (cmd === "load_app_preferences") return Promise.resolve({ ...storedRecord });
    if (cmd === "append_log_records") return Promise.resolve();
    return Promise.reject(new Error(`unexpected command ${cmd}`));
  },
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
vi.mock("../logging", () => ({ logDebug: vi.fn(), logInfo: vi.fn(), logWarn: vi.fn(), logError: vi.fn() }));

import { APP_PREFERENCES_CHANGED } from "../events";
import { resetAppPreferencesCache } from "../state/appPreferences";
import { getTerminalThemePreference, resolveTerminalTheme } from "../state/terminalThemes";
import { useTerminalThemePreference, useTerminalThemeSync } from "./useTerminalThemeSync";

beforeEach(() => {
  listeners = {};
  storedRecord = { theme: "light" };
  resetAppPreferencesCache();
});
afterEach(cleanup);

function Probe() {
  useTerminalThemeSync();
  return <span data-testid="pref">{useTerminalThemePreference()}</span>;
}

describe("useTerminalThemeSync", () => {
  it("GLS-FR-VTVK: the stored Terminal theme is adopted on mount", async () => {
    storedRecord = { theme: "light", terminalTheme: "sand" };
    const view = render(<Probe />);
    await waitFor(() => expect(view.getByTestId("pref")).toHaveTextContent("sand"));
    expect(resolveTerminalTheme(null).key).toBe("sand");
  });

  it("GLS-FR-VTVK, TRM-FR-IUMC: a save in another window reaches this window on the change event", async () => {
    const view = render(<Probe />);
    await waitFor(() => expect(listeners[APP_PREFERENCES_CHANGED]).toBeDefined());
    expect(getTerminalThemePreference()).toBe("app");
    storedRecord = { theme: "light", terminalTheme: "night" };
    await act(async () => listeners[APP_PREFERENCES_CHANGED]({ payload: null }));
    await waitFor(() => expect(view.getByTestId("pref")).toHaveTextContent("night"));
  });

  it("GLS-FR-VTVK: an unmount stops the follow", async () => {
    const view = render(<Probe />);
    await waitFor(() => expect(listeners[APP_PREFERENCES_CHANGED]).toBeDefined());
    view.unmount();
    expect(listeners[APP_PREFERENCES_CHANGED]).toBeUndefined();
  });
});
