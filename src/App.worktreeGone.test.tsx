/**
 * A gone active worktree at the App seam (`WTS-worktree-selector.md`
 * WTS-FR-WIFO, WTS-FR-YKSZ, WTS-FR-BTMD, `SNV-shell-navigation.md` SNV-FR-56).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import App from "./App";
import { isActiveRootGone, resetActiveRootHealth } from "./state/activeRootHealth";
import { resetAppPreferencesCache } from "./state/appPreferences";
import { resetLayoutPreferencesCache } from "./state/layoutPreferences";
import { resetNotifications } from "./state/notifications";
import { resetPanelReveals } from "./state/panelReveal";
import { resetToasts } from "./state/toasts";
import { resetLogBufferForTest } from "./logging";
import { defaultInvoke, enterIde, resetAppFixture } from "./test/appFixtures";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  emit: vi.fn(),
  listen: vi.fn(async () => () => {}),
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
    getCurrentWindow: () => win,
    availableMonitors: async () => [],
  };
});

const LINKED = {
  path: "/data/w/w1",
  name: "w1",
  branch: "synthesis/stream/one",
  headShortHash: "4f2a10c",
  isDetached: false,
  isActive: true,
  isPrimary: false,
  isMissing: false,
};
const PRIMARY = { ...LINKED, path: "~/dev/acme", name: "acme", branch: "main", isPrimary: true };

/** The project on a linked worktree, which the test can remove. */
const disk = {
  rootGone: false,
  left: false,
  /** The typed refusal of a leave, where a test wants one. */
  refusal: null as string | null,
};

function backend() {
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    if (cmd === "get_active_worktree") return disk.left ? PRIMARY : LINKED;
    if (cmd === "active_root_state") {
      return {
        state: disk.rootGone ? "gone" : "healthy",
        primaryPath: "~/dev/acme",
        primaryBranch: "main",
      };
    }
    if (cmd === "leave_gone_worktree") {
      if (disk.refusal) throw disk.refusal;
      disk.left = true;
      disk.rootGone = false;
      return {
        repositoryRoot: "~/dev/acme",
        activeWorktreePath: "~/dev/acme",
        worktrees: [PRIMARY],
        branches: [],
      };
    }
    return defaultInvoke(cmd, args);
  });
}

const called = (cmd: string) => invokeMock.mock.calls.some((c) => c[0] === cmd);

/** The worktree goes while the author is elsewhere, and they come back. */
const removeAndComeBack = async () => {
  disk.rootGone = true;
  await act(async () => {
    fireEvent(window, new Event("focus"));
  });
};

beforeEach(() => {
  resetAppFixture();
  resetAppPreferencesCache();
  resetLayoutPreferencesCache();
  resetPanelReveals();
  resetNotifications();
  resetToasts();
  resetLogBufferForTest();
  resetActiveRootHealth();
  disk.rootGone = false;
  disk.left = false;
  disk.refusal = null;
  invokeMock.mockReset();
  backend();
  vi.stubGlobal("matchMedia", vi.fn().mockReturnValue({ matches: true }));
  vi.spyOn(document, "hasFocus").mockReturnValue(true);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

async function openOnLinkedWorktree() {
  render(<App />);
  await enterIde();
  await waitFor(() => expect(called("active_root_state")).toBe(true));
}

describe("a gone active worktree", () => {
  it("WTS-FR-WIFO: a healthy linked worktree raises nothing", async () => {
    await openOnLinkedWorktree();

    expect(screen.queryByTestId("worktree-gone-dialog")).toBeNull();
  });

  it("WTS-FR-WIFO, WTS-FR-BIOJ: coming back to the window raises the dialog", async () => {
    await openOnLinkedWorktree();

    await removeAndComeBack();

    const dialog = await screen.findByTestId("worktree-gone-dialog");
    expect(dialog).toHaveTextContent("w1");
    expect(screen.getByRole("button", { name: "Switch to main" })).toBeInTheDocument();
  });

  it("WTS-FR-YKSZ, WTS-FR-FEEK: Switch moves to the primary worktree, and the dialog goes", async () => {
    await openOnLinkedWorktree();
    await removeAndComeBack();
    await screen.findByTestId("worktree-gone-dialog");

    await userEvent.click(screen.getByTestId("worktree-gone-switch"));

    await waitFor(() => expect(screen.queryByTestId("worktree-gone-dialog")).toBeNull());
    expect(called("leave_gone_worktree")).toBe(true);
    expect(called("activate_worktree")).toBe(false);
  });

  it("WTS-FR-BTMD, SNV-FR-25: Close project leaves for the picker, and the state goes with it", async () => {
    await openOnLinkedWorktree();
    await removeAndComeBack();
    await screen.findByTestId("worktree-gone-dialog");

    await userEvent.click(screen.getByTestId("worktree-gone-close"));

    await waitFor(() => expect(called("close_project")).toBe(true));
    await waitFor(() => expect(screen.queryByTestId("worktree-gone-dialog")).toBeNull());
    expect(isActiveRootGone()).toBe(false);
  });

  it("WTS-FR-YKSZ: a refused leave renders its error in the dialog", async () => {
    await openOnLinkedWorktree();
    await removeAndComeBack();
    await screen.findByTestId("worktree-gone-dialog");
    disk.refusal = "not a git repository";

    await userEvent.click(screen.getByTestId("worktree-gone-switch"));

    expect(await screen.findByTestId("worktree-gone-error")).toHaveTextContent(
      "not a git repository",
    );
    expect(screen.getByTestId("worktree-gone-dialog")).toBeInTheDocument();
  });

  it("WTS-FR-FEEK: Switch on a worktree that came back closes the dialog and leaves nothing", async () => {
    await openOnLinkedWorktree();
    await removeAndComeBack();
    await screen.findByTestId("worktree-gone-dialog");
    disk.rootGone = false;

    await userEvent.click(screen.getByTestId("worktree-gone-switch"));

    await waitFor(() => expect(screen.queryByTestId("worktree-gone-dialog")).toBeNull());
    expect(called("leave_gone_worktree")).toBe(false);
  });

  it("WTS-FR-BYBD, SNV-FR-56: the dialog closes the New worktree dialog", async () => {
    await openOnLinkedWorktree();
    await userEvent.click(await screen.findByTestId("worktree-selector"));
    await userEvent.click(await screen.findByText(/New worktree/));
    await waitFor(() => expect(document.querySelectorAll(".scrim").length).toBe(1));

    await removeAndComeBack();

    await screen.findByTestId("worktree-gone-dialog");
    await waitFor(() => expect(document.querySelectorAll(".scrim").length).toBe(1));
  });

  it("WTS-FR-BYBD, SNV-FR-56: the dialog closes the New stream dialog", async () => {
    await openOnLinkedWorktree();
    await userEvent.click(await screen.findByTestId("stream-selector"));
    await userEvent.click(await screen.findByText(/New stream/));
    await waitFor(() => expect(document.querySelectorAll(".scrim").length).toBe(1));

    await removeAndComeBack();

    await screen.findByTestId("worktree-gone-dialog");
    await waitFor(() => expect(document.querySelectorAll(".scrim").length).toBe(1));
  });

  it("WTS-FR-WIFO, SNV-FR-56: the dialog closes a dropdown that was open", async () => {
    await openOnLinkedWorktree();
    await userEvent.click(await screen.findByTestId("stream-selector"));
    await screen.findByTestId("stream-menu");

    await removeAndComeBack();

    await screen.findByTestId("worktree-gone-dialog");
    expect(screen.queryByTestId("stream-menu")).toBeNull();
  });
});
