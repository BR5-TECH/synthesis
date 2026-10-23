import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import App from "./App";
import { MENU_NEW_FILE } from "./events";
import { resetAppPreferencesCache } from "./state/appPreferences";
import { resetLayoutPreferencesCache } from "./state/layoutPreferences";

/**
 * App-level wiring for the merge-or-rebase dialog of a diverged pull
 * (`specifications/ui/GIT-git.md` GIT-FR-MRBQ, GIT-FR-RCDL,
 * `specifications/ui/WTS-worktree-selector.md` WTS-FR-DVGQ).
 *
 * The dialog is mounted by the shell and asked for by two Pull controls. Which
 * overlay wins, and where Open run goes, exist only here: a component-level
 * test hands the dialog its props.
 */

const invokeMock = vi.fn();
const confirmMock = vi.fn();
const openUrlMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const { eventHandlers } = vi.hoisted(() => ({
  eventHandlers: {} as Record<
    string,
    Array<(e: { payload?: unknown }) => void>
  >,
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (name: string, handler: (e: { payload?: unknown }) => void) => {
      (eventHandlers[name] ??= []).push(handler);
      return () => {
        eventHandlers[name] = (eventHandlers[name] ?? []).filter(
          (h) => h !== handler,
        );
      };
    },
  ),
}));
function fireBusEvent(name: string, payload?: unknown) {
  for (const h of [...(eventHandlers[name] ?? [])]) h({ payload });
}
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: async () => null }));
vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: (...args: unknown[]) => openUrlMock(...args),
}));
vi.mock("@tauri-apps/api/window", () => {
  class LogicalSize {
    constructor(
      public width: number,
      public height: number,
    ) {}
  }
  return {
    LogicalSize,
    getCurrentWindow: () => ({
      setResizable: async () => {},
      setMaximizable: async () => {},
      setSize: async () => {},
      isMaximized: async () => false,
      unmaximize: async () => {},
      isFullscreen: async () => false,
      setFullscreen: async () => {},
      onResized: async () => () => {},
      outerSize: async () => new LogicalSize(1600, 1000),
      scaleFactor: async () => 1,
    }),
  };
});



const DIVERGED = {
  state: "diverged",
  branch: "feature",
  upstream: "origin/feature",
  localTip: "1".repeat(40),
  remoteTip: "2".repeat(40),
  ahead: 1,
  behind: 1,
  updatedPaths: [],
  refresh: { remoteState: "refreshed" },
};

function defaultInvoke(cmd: string) {
  switch (cmd) {
    case "list_recent_projects":
      return [{ name: "acme", path: "~/dev/acme", lastOpenedAt: "2026-05-15T10:00:00Z" }];
    case "open_project_at_path":
      return { name: "acme", path: "~/dev/acme", activeWorktreePath: "~/dev/acme" };
    case "load_app_preferences":
      return { theme: "system", mainWindowFullscreen: false };
    case "list_worktrees_and_branches":
      return {
        repositoryRoot: "~/dev/acme",
        activeWorktreePath: "~/dev/acme",
        worktrees: [
          {
            path: "~/dev/acme",
            name: "acme",
            branch: "feature",
            headShortHash: "aaa1111",
            isDetached: false,
            isActive: true,
            isPrimary: true,
            isMissing: false,
          },
        ],
        branches: [],
      };
    case "get_active_worktree":
      return {
        path: "~/dev/acme",
        name: "acme",
        branch: "feature",
        headShortHash: "aaa1111",
        isDetached: false,
        isActive: true,
        isPrimary: true,
        isMissing: false,
      };
    case "pull_current_branch":
      return DIVERGED;
    case "reconcile_pull":
      return {
        state: "handed_to_run",
        runId: "run-pull",
        runTitle: "Pull origin/feature into feature",
        updatedPaths: [],
      };
    case "get_upstream_sync_state":
      return { hasRemote: true, hasUpstream: true, ahead: 1, behind: 1 };
    case "list_drafts":
      return { folders: [], drafts: [] };
    case "append_log_records":
      return undefined;
    default:
      return undefined;
  }
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => defaultInvoke(cmd));
  openUrlMock.mockReset();
  resetAppPreferencesCache();
  resetLayoutPreferencesCache();
  for (const k in eventHandlers) delete eventHandlers[k];
  vi.stubGlobal("confirm", confirmMock);
  vi.stubGlobal("matchMedia", vi.fn().mockReturnValue({ matches: true }));
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

async function enterIde() {
  await userEvent.click(await screen.findByText("acme"));
  await screen.findByRole("button", { name: "Settings" });
}

async function pullDiverged() {
  const pull = await screen.findByTestId("worktree-refresh");
  await waitFor(() => expect(pull).toBeEnabled());
  await userEvent.click(pull);
  return screen.findByTestId("pull-reconcile-dialog");
}

describe("the merge-or-rebase dialog at the App seam", () => {
  it("WTS-FR-DVGQ, GIT-FR-MRBQ, SNV-FR-56: a diverged pull opens the dialog, and another overlay closes it", async () => {
    render(<App />);
    await enterIde();
    await pullDiverged();

    fireBusEvent(MENU_NEW_FILE);
    await waitFor(() => expect(screen.queryByTestId("pull-reconcile-dialog")).toBeNull());
    expect(screen.getByRole("dialog")).toHaveTextContent("New File");
  });

  it("WTS-FR-DVGQ, SNV-FR-56: the dialog closes the overlays and dropdowns that were open", async () => {
    render(<App />);
    await enterIde();
    await userEvent.click(await screen.findByTestId("project-switcher"));
    await screen.findByTestId("project-switcher-menu");
    fireBusEvent(MENU_NEW_FILE);
    await screen.findByText("New File");

    // A pull that returns while both stand takes their place.
    await userEvent.click(screen.getByTestId("worktree-refresh"));
    await screen.findByTestId("pull-reconcile-dialog");
    expect(screen.queryByText("New File")).toBeNull();
    expect(screen.queryByTestId("project-switcher-menu")).toBeNull();
    expect(screen.getAllByRole("dialog")).toHaveLength(1);
  });

  it("GIT-FR-RCDL: Open run closes the dialog and opens the Runs panel on the run", async () => {
    render(<App />);
    await enterIde();
    await pullDiverged();
    await userEvent.click(screen.getByTestId("pull-reconcile-merge"));
    await userEvent.click(await screen.findByTestId("pull-reconcile-open-run"));
    await waitFor(() => expect(screen.queryByTestId("pull-reconcile-dialog")).toBeNull());
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "reconcile_pull")[0][1],
    ).toEqual({ strategy: "merge", localTip: "1".repeat(40), remoteTip: "2".repeat(40) });
    await waitFor(() =>
      expect(screen.getByTestId("bottom-panel-title")).toHaveTextContent("Runs"),
    );
  });
});
