/**
 * GitHub polling at the App seam: the window-owned schedule
 * (`GIT-git.md` GIT-FR-CVSB), the new-task raise (`NTF-notifications.md`
 * NTF-FR-JLXL, NTF-FR-VALU, NTF-FR-24), and the shell-mounted start dialog a
 * claim opens (GIT-FR-NQTZ, GSD-FR-QMTF, GSD-FR-LXAF).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import App from "./App";
import { resetAppPreferencesCache } from "./state/appPreferences";
import { mintAddress } from "./state/notificationAddress";
import { resetNotifications } from "./state/notifications";
import { resetLayoutPreferencesCache } from "./state/layoutPreferences";
import { resetPanelReveals } from "./state/panelReveal";
import {
  activeWorktreePath,
  defaultInvoke,
  enterIde,
  resetAppFixture,
} from "./test/appFixtures";
import { pollingView, shadowRow } from "./test/githubPollingFixtures";
import type { GithubPollingView, WorkStreamSummary } from "./types";

/** One live stream; `busy` puts a run on it, so a start waits and is asked more. */
const stream = (id: string, busy = false): WorkStreamSummary => ({
  stream: {
    id,
    name: id,
    projectKey: "~/dev/acme",
    branch: `synthesis/stream/${id}`,
    baseBranch: "main",
    baseRevision: "a91bc04",
    worktreePath: `/tmp/${id}`,
    createdAt: "2026-09-06T09:00:00Z",
    isMissing: false,
    ...(busy ? { busyRunId: "g-9" } : {}),
  },
  queuedRunCount: 0,
  aheadOfBase: 0,
  behindBase: 0,
  baseTipRevision: "a91bc04",
  missingCommits: [],
});

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
const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);
const posts = () =>
  calls("post_notification").map(
    (c) => (c[1] as { request: Record<string, unknown> }).request,
  );

let polling: GithubPollingView;
/** The order the claim flow's steps ran in, the dialog's mount among them. */
let order: string[];

beforeEach(() => {
  resetPanelReveals();
  resetAppFixture();
  resetAppPreferencesCache();
  resetLayoutPreferencesCache();
  resetNotifications();
  for (const k in eventHandlers) delete eventHandlers[k];
  polling = pollingView();
  order = [];
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "get_github_polling_state":
      case "poll_github_ready_tasks":
        return polling;
      case "claim_github_task":
        order.push(cmd);
        return { draftId: "gh-42", draftName: "Add retry to the sync job" };
      case "acknowledge_github_claim":
        // The dialog must already be on screen when this is called.
        order.push(
          screen.queryByRole("dialog", { name: /Graduate/ }) ? `${cmd}:dialog-open` : cmd,
        );
        return null;
      case "list_work_streams":
        return [stream("editor-work", true)];
      case "start_graduation":
        throw "docker_backend_unverified";
      default:
        return defaultInvoke(cmd, args);
    }
  });
  vi.stubGlobal("matchMedia", vi.fn().mockReturnValue({ matches: true }));
  vi.spyOn(document, "hasFocus").mockReturnValue(false);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("GitHub polling in the main window", () => {
  it("GIT-FR-CVSB: the launch poll runs with the Git panel hidden", async () => {
    render(<App />);
    await enterIde();
    await waitFor(() => expect(calls("poll_github_ready_tasks")).toHaveLength(1));
    expect(screen.queryByTestId("ready-tasks")).toBeNull();
  });

  it("NTF-FR-JLXL / NTF-FR-24: new issues raise once with the github-ready-tasks key and the bottom/git address", async () => {
    render(<App />);
    await enterIde();
    act(() =>
      fireBusEvent("github-polling-changed", {
        newIssues: [
          { issueNumber: 42, title: "Add retry" },
          { issueNumber: 43, title: "Fix login" },
        ],
      }),
    );
    await waitFor(() => expect(posts()).toHaveLength(1));
    const posted = posts()[0];
    expect(posted.key).toBe("github-ready-tasks");
    expect(posted.body).toBe("Add retry, Fix login");
    expect(posted.payload).toBe(
      mintAddress("~/dev/acme", activeWorktreePath, { kind: "bottom", surface: "git" }),
    );
  });

  it("NTF-FR-JLXL: more than three new issues name their count", async () => {
    render(<App />);
    await enterIde();
    act(() =>
      fireBusEvent("github-polling-changed", {
        newIssues: [1, 2, 3, 4, 5].map((n) => ({ issueNumber: n, title: `T${n}` })),
      }),
    );
    await waitFor(() => expect(posts()).toHaveLength(1));
    expect(posts()[0].body).toBe("5 new ready tasks");
  });

  it("NTF-FR-VALU: a change with no new issue raises nothing", async () => {
    render(<App />);
    await enterIde();
    act(() => fireBusEvent("github-polling-changed", { newIssues: [] }));
    // The view is re-read, so the event was heard.
    await waitFor(() =>
      expect(calls("get_github_polling_state").length).toBeGreaterThan(1),
    );
    expect(posts()).toHaveLength(0);
  });

  it("GIT-FR-NQTZ / GSD-FR-QMTF: Claim and graduate opens the shell's start dialog, then acknowledges", async () => {
    render(<App />);
    await enterIde();
    await userEvent.click(screen.getByRole("button", { name: "Git" }));
    await userEvent.click(await screen.findByRole("tab", { name: /Ready tasks/ }));
    await userEvent.click(
      await screen.findByRole("button", {
        name: "Claim and graduate #42 Add retry to the sync job",
      }),
    );
    const dialog = await screen.findByRole("dialog", { name: /Graduate/ });
    expect(dialog).toHaveTextContent("Add retry to the sync job");
    await waitFor(() =>
      expect(order).toEqual([
        "claim_github_task",
        "acknowledge_github_claim:dialog-open",
      ]),
    );

    // GSD-FR-LXAF: cancelling changes nothing about the draft or its issue.
    const before = invokeMock.mock.calls.length;
    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: /Graduate/ })).toBeNull(),
    );
    const after = invokeMock.mock.calls.slice(before).map((c) => c[0]);
    expect(after).not.toContain("start_graduation");
    expect(after).not.toContain("set_draft_status");
    expect(after).not.toContain("claim_github_task");
  });

  it("NTF-FR-JLXL: two polls with new issues raise twice under one key, and one raise per event with the Git panel open", async () => {
    render(<App />);
    await enterIde();
    // The Ready tasks section mounted too, so a second subscriber would show.
    await userEvent.click(screen.getByRole("button", { name: "Git" }));
    await userEvent.click(await screen.findByRole("tab", { name: /Ready tasks/ }));
    await screen.findByTestId("ready-tasks");

    act(() =>
      fireBusEvent("github-polling-changed", {
        newIssues: [{ issueNumber: 42, title: "Add retry" }],
      }),
    );
    await waitFor(() => expect(posts()).toHaveLength(1));
    act(() =>
      fireBusEvent("github-polling-changed", {
        newIssues: [{ issueNumber: 50, title: "Fix login" }],
      }),
    );
    await waitFor(() => expect(posts()).toHaveLength(2));
    // Let any late duplicate land before counting.
    await act(async () => {
      await new Promise((r) => setTimeout(r, 50));
    });
    expect(posts()).toHaveLength(2);
    expect(posts().map((p) => p.key)).toEqual([
      "github-ready-tasks",
      "github-ready-tasks",
    ]);
    expect(posts().map((p) => p.body)).toEqual(["Add retry", "Fix login"]);
  });

  it("GIT-FR-FTHT / GSD-FR-LXAF: a shadow draft's dialog asks the same questions; cancel or a refused start keeps the row and its Graduate", async () => {
    polling = pollingView({ tasks: [], shadows: [shadowRow()] });
    render(<App />);
    await enterIde();
    await userEvent.click(screen.getByRole("button", { name: "Git" }));
    await userEvent.click(await screen.findByRole("tab", { name: /Ready tasks/ }));
    const graduateButton = () =>
      screen.getByRole("button", { name: "Graduate Cache invalidation (#9)" });

    // GSD-FR-LXAF: the same stream and standing-work questions an ordinary
    // draft's dialog asks of a stream a run holds (GSD-FR-VKLD).
    await userEvent.click(await screen.findByRole("button", { name: "Graduate Cache invalidation (#9)" }));
    let dialog = await screen.findByRole("dialog", { name: /Graduate/ });
    await within(dialog).findByRole("combobox", { name: "Work stream" });
    const answered = Array.from(
      dialog.querySelectorAll<HTMLElement>("input, select, textarea"),
    )
      .map((control) => control.id || control.getAttribute("value"))
      .sort();
    expect(answered).toEqual([
      "commit",
      "commit_and_push",
      // GSD-FR-BZHW: the destination is asked first, for a shadow draft as for
      // any other.
      "direct",
      "graduation-stream",
      "keep",
      "new",
      "standing-work-message-graduation-start",
      "stream",
    ]);

    // GIT-FR-FTHT: cancelling keeps the row and its Graduate.
    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: /Graduate/ })).toBeNull());
    expect(screen.getByTestId("shadow-draft-gh-1")).toBeInTheDocument();
    expect(graduateButton()).toBeEnabled();

    // GIT-FR-FTHT / GSD-FR-LXAF: a refused start changes nothing either.
    await userEvent.click(graduateButton());
    dialog = await screen.findByRole("dialog", { name: /Graduate/ });
    await within(dialog).findByRole("combobox", { name: "Work stream" });
    const before = invokeMock.mock.calls.length;
    await userEvent.click(screen.getByTestId("graduation-start-confirm"));
    await screen.findByTestId("graduation-start-error");
    const after = invokeMock.mock.calls.slice(before).map((c) => c[0]);
    expect(after).toContain("start_graduation");
    for (const cmd of [
      "set_draft_status",
      "claim_github_task",
      "retry_github_claim",
      "acknowledge_github_claim",
    ])
      expect(after).not.toContain(cmd);
    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: /Graduate/ })).toBeNull());
    expect(screen.getByTestId("shadow-draft-gh-1")).toBeInTheDocument();
    expect(graduateButton()).toBeEnabled();
  });
});
