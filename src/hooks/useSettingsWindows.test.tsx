import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";

import { useSettingsWindows } from "./useSettingsWindows";

/**
 * What the main window knows about the settings child window over it
 * (`../../specifications/ui/SWN-settings-windows.md` SWN-FR-01, SWN-FR-05, and
 * `../../specifications/ui/NTF-notifications.md` NTF-FR-08).
 *
 * A settings window is a webview of its own, so neither fact is visible from
 * this window without the backend saying so — and the notification facility
 * reads both at raise time. A subscription that missed an announcement, or a
 * ref that lagged a render, would suppress a raise the author should have had
 * (NTF-FR-10 makes that silent).
 */

const { eventHandlers } = vi.hoisted(() => ({
  eventHandlers: {} as Record<string, Array<(e: { payload?: unknown }) => void>>,
}));
const unlistened: string[] = [];
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (name: string, handler: (e: { payload?: unknown }) => void) => {
      (eventHandlers[name] ??= []).push(handler);
      return () => {
        unlistened.push(name);
        eventHandlers[name] = (eventHandlers[name] ?? []).filter(
          (h) => h !== handler,
        );
      };
    },
  ),
}));

function fire(name: string, payload?: unknown) {
  act(() => {
    for (const h of [...(eventHandlers[name] ?? [])]) h({ payload });
  });
}

const CHANGED = "settings-window:changed";
const FOCUS = "settings-window:focus";

const listening = (name: string) => (eventHandlers[name] ?? []).length;

beforeEach(() => {
  for (const k in eventHandlers) delete eventHandlers[k];
  unlistened.length = 0;
});
afterEach(cleanup);

describe("what the main window knows about the settings window over it", () => {
  it("starts believing nothing is open", () => {
    const { result } = renderHook(() => useSettingsWindows());
    expect(result.current.presence).toEqual({ open: null, focused: false });
  });

  it("SWN-FR-05: follows which settings window is open", async () => {
    const { result } = renderHook(() => useSettingsWindows());
    await waitFor(() => expect(listening(CHANGED)).toBe(1));

    fire(CHANGED, { open: "global" });
    expect(result.current.presence.open).toBe("global");

    // A switch: at most one is ever open, so this replaces rather than adds.
    fire(CHANGED, { open: "project" });
    expect(result.current.presence.open).toBe("project");

    fire(CHANGED, { open: null });
    expect(result.current.presence.open).toBeNull();
  });

  it("NTF-FR-08: follows whether that window holds OS focus", async () => {
    const { result } = renderHook(() => useSettingsWindows());
    await waitFor(() => expect(listening(FOCUS)).toBe(1));

    fire(CHANGED, { open: "global" });
    fire(FOCUS, { focused: true });
    expect(result.current.presence.focused).toBe(true);

    fire(FOCUS, { focused: false });
    expect(result.current.presence.focused).toBe(false);
  });

  it("a window that has gone holds no focus either", async () => {
    // The two facts move together, or a closed window stays remembered as
    // focused — and `decidePost` would then believe the application is in the
    // foreground when it is not, suppressing raises silently (NTF-FR-10).
    const { result } = renderHook(() => useSettingsWindows());
    await waitFor(() => expect(listening(CHANGED)).toBe(1));

    fire(CHANGED, { open: "global" });
    fire(FOCUS, { focused: true });
    expect(result.current.presence).toEqual({ open: "global", focused: true });

    fire(CHANGED, { open: null });
    expect(result.current.presence).toEqual({ open: null, focused: false });
  });

  it("keeps the ref current, because the facility reads it between renders", async () => {
    // The facility samples the window through getters at raise time rather than
    // from a render: React does not re-render on a window blur, so a rendered
    // value would be as stale as the last render.
    const { result } = renderHook(() => useSettingsWindows());
    await waitFor(() => expect(listening(CHANGED)).toBe(1));

    fire(CHANGED, { open: "project" });
    fire(FOCUS, { focused: true });

    expect(result.current.presenceRef.current).toEqual({
      open: "project",
      focused: true,
    });
  });

  it("detaches both subscriptions when the shell goes", async () => {
    const { unmount } = renderHook(() => useSettingsWindows());
    await waitFor(() => expect(listening(CHANGED)).toBe(1));
    await waitFor(() => expect(listening(FOCUS)).toBe(1));

    unmount();

    expect(listening(CHANGED)).toBe(0);
    expect(listening(FOCUS)).toBe(0);
    expect(unlistened.sort()).toEqual([CHANGED, FOCUS].sort());
  });
});
