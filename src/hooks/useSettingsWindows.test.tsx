import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";

import { useSettingsWindows } from "./useSettingsWindows";
import {
  noteSettingsInvoker,
  resetSettingsInvoker,
} from "../state/settingsInvoker";

/**
 * What the main window knows about the Settings child window over it
 * (`../../specifications/ui/SWN-settings-windows.md` SWN-FR-01, SWN-FR-05,
 * SWN-FR-FCRT, and `../../specifications/ui/NTF-notifications.md` NTF-FR-08).
 *
 * The Settings window is a webview of its own, so neither fact is visible from
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
  resetSettingsInvoker();
  document.body.innerHTML = "";
});
afterEach(cleanup);

describe("what the main window knows about the Settings window over it", () => {
  it("starts believing nothing is open", () => {
    const { result } = renderHook(() => useSettingsWindows());
    expect(result.current.presence).toEqual({ open: false, focused: false });
  });

  it("SWN-FR-05: follows whether the Settings window is open", async () => {
    const { result } = renderHook(() => useSettingsWindows());
    await waitFor(() => expect(listening(CHANGED)).toBe(1));

    fire(CHANGED, { open: true });
    expect(result.current.presence.open).toBe(true);

    fire(CHANGED, { open: false });
    expect(result.current.presence.open).toBe(false);
  });

  it("NTF-FR-08: follows whether that window holds OS focus", async () => {
    const { result } = renderHook(() => useSettingsWindows());
    await waitFor(() => expect(listening(FOCUS)).toBe(1));

    fire(CHANGED, { open: true });
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

    fire(CHANGED, { open: true });
    fire(FOCUS, { focused: true });
    expect(result.current.presence).toEqual({ open: true, focused: true });

    fire(CHANGED, { open: false });
    expect(result.current.presence).toEqual({ open: false, focused: false });
  });

  it("keeps the ref current, because the facility reads it between renders", async () => {
    // The facility samples the window through getters at raise time rather than
    // from a render: React does not re-render on a window blur, so a rendered
    // value would be as stale as the last render.
    const { result } = renderHook(() => useSettingsWindows());
    await waitFor(() => expect(listening(CHANGED)).toBe(1));

    fire(CHANGED, { open: true });
    fire(FOCUS, { focused: true });

    expect(result.current.presenceRef.current).toEqual({
      open: true,
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

describe("focus returns to the control that invoked the window (SWN-FR-FCRT)", () => {
  const control = (label: string) => {
    const button = document.createElement("button");
    button.textContent = label;
    document.body.appendChild(button);
    return button;
  };

  it("returns focus to the control the request noted, once the window has closed", async () => {
    renderHook(() => useSettingsWindows());
    await waitFor(() => expect(listening(CHANGED)).toBe(1));
    const invoker = control("Settings");
    const other = control("Other");
    noteSettingsInvoker(invoker);

    fire(CHANGED, { open: true });
    // The author's focus may have moved on while the window was open.
    other.focus();
    fire(CHANGED, { open: false });

    expect(invoker).toHaveFocus();
  });

  it("falls back to the control that held focus as the window opened, for a request with none noted", async () => {
    renderHook(() => useSettingsWindows());
    await waitFor(() => expect(listening(CHANGED)).toBe(1));
    // The application menu's entry names no control; the one that held focus is
    // the one that invoked the window.
    const held = control("Held focus");
    const other = control("Other");
    held.focus();

    fire(CHANGED, { open: true });
    other.focus();
    fire(CHANGED, { open: false });

    expect(held).toHaveFocus();
  });

  it("does nothing, and does not throw, when the control has left the document", async () => {
    renderHook(() => useSettingsWindows());
    await waitFor(() => expect(listening(CHANGED)).toBe(1));
    const gone = control("Gone");
    noteSettingsInvoker(gone);
    fire(CHANGED, { open: true });
    gone.remove();

    expect(() => fire(CHANGED, { open: false })).not.toThrow();
  });

  it("forgets the control once focus has returned, so a later opening does not reuse it", async () => {
    renderHook(() => useSettingsWindows());
    await waitFor(() => expect(listening(CHANGED)).toBe(1));
    const first = control("First");
    const second = control("Second");
    noteSettingsInvoker(first);
    fire(CHANGED, { open: true });
    fire(CHANGED, { open: false });
    expect(first).toHaveFocus();

    second.focus();
    fire(CHANGED, { open: true });
    first.focus();
    fire(CHANGED, { open: false });
    expect(second).toHaveFocus();
  });
});
