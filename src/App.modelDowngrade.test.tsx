/**
 * The model downgrade raise at the App seam (`NTF-notifications.md`
 * NTF-FR-AGTB, NTF-FR-CYTU, NTF-FR-15).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";

import App from "./App";
import { MODEL_DOWNGRADE_DETECTED } from "./events";
import { resetAppPreferencesCache } from "./state/appPreferences";
import { resetLayoutPreferencesCache } from "./state/layoutPreferences";
import { resetNotifications } from "./state/notifications";
import { resetPanelReveals } from "./state/panelReveal";
import { resetToasts } from "./state/toasts";
import { resetLogBufferForTest } from "./logging";
import {
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

const downgrade = {
  source: "agent",
  requestedModel: "claude-opus-5-5",
  servedModel: "claude-opus-4-8",
  provider: "anthropic",
  projectKey: "~/dev/acme",
  worktree: "~/dev/acme",
  origin: { discussionId: "d1", target: { kind: "artifact", artifactId: "README.md" } },
  runId: null,
};

beforeEach(() => {
  resetAppFixture();
  resetAppPreferencesCache();
  resetLayoutPreferencesCache();
  resetPanelReveals();
  resetNotifications();
  resetToasts();
  resetLogBufferForTest();
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

describe("the model downgrade raise", () => {
  it("NTF-FR-AGTB: an event while a project is open shows the Warn toast with its Suppress menu", async () => {
    render(<App />);
    await enterIde();
    await waitFor(() => expect(eventHandlers[MODEL_DOWNGRADE_DETECTED]).toHaveLength(1));
    act(() => fireBusEvent(MODEL_DOWNGRADE_DETECTED, downgrade));
    const toast = await screen.findByTestId("notification-toast");
    expect(toast).toHaveAttribute("data-level", "Warn");
    expect(toast).toHaveAttribute("data-key", "model-downgrade:claude-opus-5-5→claude-opus-4-8");
    expect(toast).toHaveTextContent("Model changed");
    expect(screen.getByTestId("notification-toast-menu-control")).toHaveTextContent("Suppress");
    expect(invokeMock.mock.calls.some((c) => c[0] === "load_model_downgrade_suppressions")).toBe(true);
  });

  it("NTF-FR-CYTU: a pair muted by a saved rule shows no toast", async () => {
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) =>
      cmd === "load_model_downgrade_suppressions"
        ? [{ requestedModel: "claude-opus-5-5", servedModel: "claude-opus-4-8", expiresAt: null }]
        : defaultInvoke(cmd, args),
    );
    render(<App />);
    await enterIde();
    await waitFor(() => expect(eventHandlers[MODEL_DOWNGRADE_DETECTED]).toHaveLength(1));
    // The muted pair first, then a pair no rule covers: the handler of the
    // second event ends after the handler of the first, so its toast shows
    // that the first event was handled.
    act(() => fireBusEvent(MODEL_DOWNGRADE_DETECTED, downgrade));
    act(() =>
      fireBusEvent(MODEL_DOWNGRADE_DETECTED, { ...downgrade, servedModel: "claude-sonnet-5-5" }),
    );
    const toast = await screen.findByTestId("notification-toast");
    expect(toast).toHaveAttribute("data-key", "model-downgrade:claude-opus-5-5→claude-sonnet-5-5");
    expect(screen.getAllByTestId("notification-toast")).toHaveLength(1);
  });
});
