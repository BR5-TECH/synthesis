/**
 * The notification toast stack at the App seam (`NTF-notifications.md`
 * NTF-FR-20, NTF-FR-15, NTF-FR-FNXO).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import App from "./App";
import { NOTIFICATION_ACTIVATED } from "./events";
import { resetAppPreferencesCache } from "./state/appPreferences";
import { resetLayoutPreferencesCache } from "./state/layoutPreferences";
import { mintAddress } from "./state/notificationAddress";
import { resetNotifications } from "./state/notifications";
import { resetPanelReveals } from "./state/panelReveal";
import { resetToasts } from "./state/toasts";
import {
  activeWorktreePath,
  defaultInvoke,
  enterIde,
  resetAppFixture,
} from "./test/appFixtures";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
const { eventHandlers } = vi.hoisted(() => ({
  eventHandlers: {} as Record<string, Array<(e: { payload?: unknown }) => void>>,
}));
vi.mock("@tauri-apps/api/event", () => ({
  emit: vi.fn(),
  listen: vi.fn(async (name: string, handler: (e: { payload?: unknown }) => void) => {
    (eventHandlers[name] ??= []).push(handler);
    return () => {
      eventHandlers[name] = (eventHandlers[name] ?? []).filter((h) => h !== handler);
    };
  }),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: async () => null }));
vi.mock("@tauri-apps/api/window", () => {
  const win = {
    setResizable: async () => {},
    setMaximizable: async () => {},
    setSize: async () => {},
    isMaximized: async () => false,
    unmaximize: async () => {},
    maximize: async () => {},
    outerSize: async () => ({ width: 1440, height: 900 }),
    onResized: async () => () => {},
    scaleFactor: async () => 1,
  };
  return {
    LogicalSize: class {
      constructor(
        public width: number,
        public height: number,
      ) {}
    },
    getCurrentWindow: () => win,
    availableMonitors: async () => [],
  };
});

function fireBusEvent(name: string, payload?: unknown) {
  for (const h of [...(eventHandlers[name] ?? [])]) h({ payload });
}

const activate = (payload: string) =>
  act(() => fireBusEvent(NOTIFICATION_ACTIVATED, { id: "n1", key: "k", payload }));

beforeEach(() => {
  resetAppFixture();
  resetAppPreferencesCache();
  resetLayoutPreferencesCache();
  resetPanelReveals();
  resetNotifications();
  resetToasts();
  for (const k in eventHandlers) delete eventHandlers[k];
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) =>
    defaultInvoke(cmd, args),
  );
  vi.stubGlobal("matchMedia", vi.fn().mockReturnValue({ matches: true }));
  vi.spyOn(document, "hasFocus").mockReturnValue(true);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("the unreachable-address toast", () => {
  it("NTF-FR-19, NTF-FR-20: an activation made over the Project picker shows a Warn toast", async () => {
    render(<App />);
    await screen.findByText("acme");
    await waitFor(() => expect(eventHandlers[NOTIFICATION_ACTIVATED]).toHaveLength(1));
    activate(mintAddress("~/dev/acme", activeWorktreePath, { kind: "dashboard" }));
    const toast = await screen.findByTestId("notification-toast");
    expect(toast).toHaveAttribute("data-level", "Warn");
    expect(toast).toHaveAttribute("role", "alert");
    expect(toast).toHaveAttribute("data-key", "unreachable-address");
  });

  it("NTF-FR-15: the toast shown over the picker does not survive the opening of a project", async () => {
    render(<App />);
    await screen.findByText("acme");
    await waitFor(() => expect(eventHandlers[NOTIFICATION_ACTIVATED]).toHaveLength(1));
    activate("not an address at all");
    await screen.findByTestId("notification-toast");
    await enterIde();
    await waitFor(() =>
      expect(screen.queryByTestId("notification-toast")).toBeNull(),
    );
  });

  it("NTF-FR-20: an address for a draft that is gone shows the toast, and a click on it only dismisses it", async () => {
    render(<App />);
    await enterIde();
    activate(
      mintAddress("~/dev/acme", activeWorktreePath, {
        kind: "draft",
        draftId: "gone",
      }),
    );
    const toast = await screen.findByTestId("notification-toast");
    expect(toast).toHaveAttribute("data-level", "Warn");
    await userEvent.click(screen.getByTestId("notification-toast-target"));
    await waitFor(() =>
      expect(screen.queryByTestId("notification-toast")).toBeNull(),
    );
  });
});
