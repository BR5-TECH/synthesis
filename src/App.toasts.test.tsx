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
import { reportDockerSocketRefusal } from "./state/dockerSocketRefusal";
import { mintAddress } from "./state/notificationAddress";
import { resetNotifications } from "./state/notifications";
import { resetPanelReveals } from "./state/panelReveal";
import { resetToasts } from "./state/toasts";
import { resetLogBufferForTest } from "./logging";
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

describe("the Docker socket refusal toast (GSD-FR-BXKR, NTF-FR-24)", () => {
  it("GSD-FR-BXKR, NTF-FR-24: a refusal reported while a project is open shows an Error toast that names the path", async () => {
    render(<App />);
    await enterIde();
    act(() => reportDockerSocketRefusal("/run/x/docker.sock"));
    const toast = await screen.findByTestId("notification-toast");
    expect(toast).toHaveAttribute("data-level", "Error");
    expect(toast).toHaveAttribute("role", "alert");
    expect(toast).toHaveAttribute("data-key", "docker-socket-unavailable");
    expect(toast).toHaveTextContent("/run/x/docker.sock");
  });

  it("NTF-FR-02: a refusal reported with no project open raises nothing", async () => {
    render(<App />);
    await screen.findByText("acme");
    act(() => reportDockerSocketRefusal("/run/x/docker.sock"));
    expect(screen.queryByTestId("notification-toast")).toBeNull();
  });
});

describe("the publication metadata startup toast", () => {
  it("NTF-FR-POEW, NTF-FR-15: a startup failure that the backend kept before the project opened here shows one Error toast once the project is open", async () => {
    let kept: { errorCode: string; error: string } | null = {
      errorCode: "token_unavailable",
      error: "The project has no GitHub token.",
    };
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "take_publication_metadata_startup_failure") {
        const failure = kept;
        kept = null;
        return failure;
      }
      return defaultInvoke(cmd, args);
    });
    render(<App />);
    await enterIde();
    const toast = await screen.findByTestId("notification-toast");
    expect(toast).toHaveAttribute("data-level", "Error");
    expect(toast).toHaveAttribute("data-key", "publication-metadata-refresh-failed");
    expect(toast).toHaveTextContent("GitHub publication data not loaded");
    expect(toast).toHaveTextContent("The project has no GitHub token.");
    expect(kept).toBeNull();
  });
});

describe("the worktree fallback toast", () => {
  /** Open `acme` with its remembered worktree gone, on `worktreePath`. */
  async function openOnFallback(worktreePath: string | null) {
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      const answer = await defaultInvoke(cmd, args);
      if (cmd === "open_project_at_path") {
        return { ...(answer as object), rememberedWorktreeUnavailable: true };
      }
      if (cmd === "get_active_worktree" && worktreePath !== null) {
        return { ...(answer as object), path: worktreePath };
      }
      return answer;
    });
    render(<App />);
    await enterIde();
    return waitFor(() => {
      const shown = document.querySelector<HTMLElement>(
        '[data-testid="notification-toast"][data-key="worktree-fallback"]',
      );
      expect(shown).not.toBeNull();
      return shown!;
    });
  }

  it("WTC-FR-17, NTF-FR-WTFB: a project that opens on the primary worktree shows a Warn toast that the opening does not remove", async () => {
    const toast = await openOnFallback(null);
    expect(toast).toHaveAttribute("data-level", "Warn");
    expect(toast).toHaveTextContent("Worktree not found");
    expect(toast).toHaveTextContent("no longer on disk");
  });

  it("NTF-FR-WTFB: the toast stays when the primary worktree is at another path than the project", async () => {
    // The root moves from the project path to the worktree path when the
    // worktree resolves, and NTF-FR-15 removes every toast at that move.
    const toast = await openOnFallback("~/dev/acme-repo");
    expect(toast).toHaveAttribute("data-level", "Warn");
    await waitFor(() =>
      expect(invokeMock.mock.calls.some((c) => c[0] === "get_active_worktree")).toBe(true),
    );
    expect(
      document.querySelector('[data-testid="notification-toast"][data-key="worktree-fallback"]'),
    ).not.toBeNull();
  });
});
