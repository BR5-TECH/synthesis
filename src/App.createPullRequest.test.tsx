import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import App from "./App";
import { MENU_NEW_FILE } from "./events";
import { resetAppPreferencesCache } from "./state/appPreferences";
import { resetLayoutPreferencesCache } from "./state/layoutPreferences";

/**
 * App-level wiring for the Create a PR window
 * (`specifications/ui/CPR-create-pull-request.md`).
 *
 * The window is mounted by `App` and asked for by three surfaces, and it gives
 * way to the token picker the shell also owns. The seams these tests cover —
 * which overlay wins, what the window comes back with, and where the notice
 * stands — exist only here: a component-level test hands the window its props.
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


const WORK = {
  id: "t1",
  label: "work laptop",
  accountLogin: "raver119",
  scopes: ["repo"],
  maskedHint: "a3f9",
  addedAt: "2026-03-12T10:00:00Z",
  lastVerifiedAt: null,
  state: "valid" as const,
};
const PERSONAL = { ...WORK, id: "t2", label: "personal", maskedHint: "1b04" };

const CREATED = { number: 42, url: "https://github.com/acme/platform/pull/42" };

/** What `create_pull_request` answers; a test may change it. */
let createResult: () => unknown;

function defaultInvoke(cmd: string, args?: Record<string, unknown>) {
  switch (cmd) {
    case "list_recent_projects":
      return [{ name: "acme", path: "~/dev/acme", lastOpenedAt: "2026-05-15T10:00:00Z" }];
    case "open_project_at_path":
      return { name: "acme", path: "~/dev/acme", activeWorktreePath: "~/dev/acme" };
    case "load_app_preferences":
      return { theme: "system", mainWindowFullscreen: false };
    case "list_github_tokens":
      return [WORK, PERSONAL];
    case "set_project_github_token_binding":
      return { tokenId: "t2", resolution: "bound" };
    case "list_branches":
      return [
        { name: "main", kind: "local", isCurrent: false },
        { name: "feature", kind: "local", isCurrent: true },
      ];
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
    case "get_pull_request_head_state":
      return {
        head: String(args?.head),
        base: "main",
        hasRemote: true,
        remoteBranchExists: true,
        unpushed: 0,
        uncommittedPaths: [],
        aheadOfBase: 2,
      };
    case "create_pull_request":
      return createResult();
    case "load_changes_panel_state":
      return { mode: "uncommitted" };
    case "get_default_branch":
      return "main";
    case "list_uncommitted_changes":
      return { comparison: { kind: "uncommitted" }, entries: [] };
    case "get_upstream_sync_state":
      return { hasRemote: true, hasUpstream: true, ahead: 0, behind: 0 };
    case "list_drafts":
      return { folders: [], drafts: [] };
    case "append_log_records":
      return undefined;
    default:
      return undefined;
  }
}

beforeEach(() => {
  createResult = () => CREATED;
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) =>
    defaultInvoke(cmd, args),
  );
  openUrlMock.mockReset();
  openUrlMock.mockResolvedValue(undefined);
  resetAppPreferencesCache();
  resetLayoutPreferencesCache();
  confirmMock.mockReset();
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
  await screen.findByRole("button", { name: "Global settings" });
}

/** Open the Changes panel and press its footer's Create a PR. */
async function openFromChanges() {
  await userEvent.click(screen.getByRole("button", { name: "Changes" }));
  const button = await screen.findByTestId("changes-create-pr");
  await waitFor(() => expect(button).toHaveAttribute("data-state", "available"));
  await userEvent.click(button);
  return screen.findByTestId("create-pr-window");
}

const submit = () => screen.getByTestId("create-pr-submit");
const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

describe("the Create a PR window at the App seam", () => {
  it("CPR-FR-FDVO, CPR-FR-JOIG: the Changes footer opens the window for the current branch", async () => {
    render(<App />);
    await enterIde();
    await openFromChanges();
    expect(screen.getByLabelText("Title")).toHaveValue("feature");
    expect(screen.getByTestId("create-pr-head")).toHaveTextContent("feature");
    expect(screen.getByLabelText("Base branch")).toHaveValue("main");
  });

  it("CPR-FR-IWDK, SNV-FR-56: opening another overlay closes the window, and the window closes the others", async () => {
    render(<App />);
    await enterIde();
    await openFromChanges();

    fireBusEvent(MENU_NEW_FILE);
    await waitFor(() => expect(screen.queryByTestId("create-pr-window")).toBeNull());
    expect(screen.getByRole("dialog")).toHaveTextContent("New File");

    // And back: the window opening closes the New File modal.
    expect(screen.getByRole("dialog")).toHaveTextContent("New File");
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    fireBusEvent(MENU_NEW_FILE);
    await screen.findByText("New File");
    await userEvent.click(await screen.findByTestId("changes-create-pr"));
    await screen.findByTestId("create-pr-window");
    expect(screen.queryByText("New File")).toBeNull();
  });

  it("CPR-FR-FDVO, WSS-FR-KHGP, WSS-FR-XZRD: a stream row's Create a PR closes the dropdown and opens the window for the stream", async () => {
    const stream = {
      stream: {
        id: "w1",
        projectKey: "p",
        name: "editor work",
        branch: "synthesis/stream/editor-work",
        worktreePath: "/data/w/w1",
        baseBranch: "main",
        baseRevision: "abc",
        createdAt: "2026-05-01T00:00:00Z",
        busyRunId: null,
        isMissing: false,
      },
      aheadOfBase: 2,
      behindBase: 0,
      baseTipRevision: "abc",
      missingCommits: [],
      queuedRunCount: 0,
      mergeRun: null,
      update: null,
    };
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) =>
      cmd === "list_work_streams"
        ? [stream]
        : cmd === "get_active_worktree"
          ? {
              path: "~/dev/acme",
              branch: "main",
              isPrimary: true,
              isActive: true,
              headShortHash: "aaa1111",
              missing: false,
            }
          : base(cmd, args),
    );
    render(<App />);
    await enterIde();
    await userEvent.click(await screen.findByTestId("stream-selector"));
    await userEvent.click(await screen.findByTestId("stream-create-pr-w1"));

    await screen.findByTestId("create-pr-window");
    expect(screen.queryByTestId("stream-menu")).toBeNull();
    expect(screen.getByLabelText("Title")).toHaveValue("editor work");
    expect(screen.getByTestId("create-pr-head")).toHaveTextContent("synthesis/stream/editor-work");
    expect(screen.getByLabelText("Base branch")).toHaveValue("main");
  });

  it("CPR-FR-FDVO, GIT-FR-05: the Git panel's Create PR for current branch opens the same window", async () => {
    render(<App />);
    await enterIde();
    await userEvent.click(screen.getByRole("button", { name: "Git" }));
    await userEvent.click(await screen.findByRole("tab", { name: "PRs" }));
    const button = await screen.findByTestId("git-create-pr");
    await waitFor(() => expect(button).toHaveAttribute("data-state", "available"));
    await userEvent.click(button);
    await screen.findByTestId("create-pr-window");
    expect(screen.getByLabelText("Title")).toHaveValue("feature");
  });

  it("CPR-FR-ITWJ, CPR-FR-RDJP: a created pull request closes the window and leaves a notice with a link that opens in the browser", async () => {
    render(<App />);
    await enterIde();
    await openFromChanges();
    await waitFor(() => expect(submit()).toBeEnabled());
    await userEvent.click(submit());

    const notice = await screen.findByTestId("pull-request-notice");
    expect(notice).toHaveTextContent("Pull request #42 created");
    expect(screen.queryByTestId("create-pr-window")).toBeNull();
    await userEvent.click(screen.getByTestId("pull-request-notice-link"));
    expect(openUrlMock).toHaveBeenCalledWith(CREATED.url);

    // The notice is no overlay: another overlay opening leaves it standing.
    fireBusEvent(MENU_NEW_FILE);
    await screen.findByText("New File");
    expect(screen.getByTestId("pull-request-notice")).toBeInTheDocument();
  });

  it("CPR-FR-VZUZ: a selection-required failure opens the token picker in the window's place, and a confirmed token submits again with what was typed", async () => {
    let attempt = 0;
    createResult = () => {
      attempt += 1;
      if (attempt === 1) throw "github_token_selection_required";
      return CREATED;
    };
    render(<App />);
    await enterIde();
    await openFromChanges();
    await userEvent.type(screen.getByLabelText("Description"), "my words");
    await waitFor(() => expect(submit()).toBeEnabled());
    await userEvent.click(submit());

    const picker = await screen.findByTestId("github-token-picker");
    // GHA-FR-21: one overlay at a time, so the window is not on screen.
    expect(screen.queryByTestId("create-pr-window")).toBeNull();
    await userEvent.click(within(picker).getByRole("radio", { name: "personal" }));
    await userEvent.click(within(picker).getByRole("button", { name: "Use" }));

    await screen.findByTestId("pull-request-notice");
    expect(calls("create_pull_request")).toHaveLength(2);
    expect(calls("create_pull_request")[1][1]).toEqual({
      title: "feature",
      body: "my words",
      base: "main",
      head: "feature",
      draft: false,
    });
  });

  it("CPR-FR-VZUZ, GHA-FR-17: a cancelled picker brings the window back with its input and an inline note", async () => {
    createResult = () => {
      throw "github_token_selection_required";
    };
    render(<App />);
    await enterIde();
    await openFromChanges();
    await userEvent.type(screen.getByLabelText("Description"), "kept words");
    await waitFor(() => expect(submit()).toBeEnabled());
    await userEvent.click(submit());

    const picker = await screen.findByTestId("github-token-picker");
    await userEvent.click(within(picker).getByRole("button", { name: /Cancel/ }));

    expect(await screen.findByTestId("create-pr-failure")).toHaveTextContent(
      "No GitHub token was selected",
    );
    expect(screen.getByLabelText("Description")).toHaveValue("kept words");
    expect(calls("create_pull_request")).toHaveLength(1);
  });

  it("CPR-FR-FGGU, GHA-FR-19: a missing token stays inline in the window", async () => {
    createResult = () => {
      throw "github_token_missing";
    };
    render(<App />);
    await enterIde();
    await openFromChanges();
    await waitFor(() => expect(submit()).toBeEnabled());
    await userEvent.click(submit());
    expect(await screen.findByTestId("create-pr-failure")).toHaveTextContent(/GitHub token/);
    expect(screen.getByTestId("create-pr-global-settings")).toBeInTheDocument();
    expect(screen.queryByTestId("github-token-picker")).toBeNull();
    expect(screen.getByTestId("create-pr-window")).toBeInTheDocument();
  });
});
