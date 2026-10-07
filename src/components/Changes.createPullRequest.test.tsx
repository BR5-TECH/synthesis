/**
 * The footer's Create a PR button
 * (`../../specifications/ui/CHG-changes.md` CHG-FR-UPFP, CHG-FR-UCRL,
 * CHG-FR-33, CHG-FR-56, CHG-FR-21).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetAppPreferencesCache } from "../state/appPreferences";
import { resetPanelReveals } from "../state/panelReveal";
import { pickSelector } from "../test/selectors";
import {
  LIB,
  changeSet,
  checkbox,
  makeBackend,
  onCreatePullRequest,
  renderPanel,
  resetHandlers,
  showAllFiles,
} from "../test/changesFixtures";

const invokeMock = vi.fn();
let listeners: Record<string, ((event: { payload: unknown }) => void)[]> = {};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, cb: (event: { payload: unknown }) => void) => {
    (listeners[event] ??= []).push(cb);
    return () => {};
  }),
}));

const backend = makeBackend(invokeMock);
const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

beforeEach(() => {
  resetPanelReveals();
  invokeMock.mockReset();
  resetAppPreferencesCache();
  resetHandlers();
  listeners = {};
});
afterEach(cleanup);

const button = () => screen.getByTestId("changes-create-pr");
const available = () =>
  waitFor(() => expect(button()).toHaveAttribute("data-state", "available"));

describe("Create a PR in the Changes footer", () => {
  it("CHG-FR-UPFP, CHG-FR-33, CHG-FR-56: stands on the leading edge after the rollback button, the split control keeps the trailing edge", async () => {
    backend({ uncommitted: changeSet([LIB]) });
    const { container } = renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");
    await available();

    const footer = container.querySelector(".changes-actions")!;
    const buttons = [...footer.querySelectorAll("button")];
    expect(buttons[0]).toHaveAttribute("aria-label", "Discard selected changes");
    expect(buttons[1]).toBe(button());
    expect(button()).toHaveTextContent("Create a PR");
    expect(buttons[buttons.length - 1]).toHaveAttribute("aria-label", "Choose action");
  });

  it("CHG-FR-UPFP, CHG-FR-56: takes the first place of the leading edge in Branch mode, which has no rollback button", async () => {
    backend({ uncommitted: changeSet([LIB]) });
    const { container } = renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");
    await pickSelector("Comparison mode", "branch");
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "Discard selected changes" })).toBeNull(),
    );
    const footer = container.querySelector(".changes-actions")!;
    expect([...footer.querySelectorAll("button")][0]).toBe(button());
  });

  it("CHG-FR-UPFP, CPR-FR-FDVO: opens the shared window with the current branch as head, the default branch as base and the branch name as title", async () => {
    backend({});
    renderPanel();
    await available();
    await userEvent.click(button());
    expect(onCreatePullRequest).toHaveBeenCalledWith({
      head: "feature/x",
      base: "main",
      title: "feature/x",
    });
  });

  it("CHG-FR-UPFP: a branch that belongs to a work stream takes the stream's name as the title", async () => {
    backend({ pullRequest: { streamName: "editor work" } });
    renderPanel();
    await available();
    await userEvent.click(button());
    expect(onCreatePullRequest).toHaveBeenCalledWith(
      expect.objectContaining({ title: "editor work" }),
    );
  });

  it("CHG-FR-21, CHG-FR-UPFP: the panel creates no pull request itself", async () => {
    backend({});
    renderPanel();
    await available();
    await userEvent.click(button());
    expect(calls("create_pull_request")).toHaveLength(0);
  });

  it("CHG-FR-UCRL: a branch with no commit of its own is unavailable, says why, stays reachable and opens nothing", async () => {
    backend({ pullRequest: { headState: { aheadOfBase: 0 } } });
    renderPanel();
    await waitFor(() =>
      expect(button()).toHaveAccessibleDescription("This branch has no commit of its own yet."),
    );
    expect(button()).toHaveAttribute("data-state", "unavailable");
    expect(button()).toHaveAttribute("aria-disabled", "true");
    expect(button()).not.toBeDisabled();
    expect(button()).toHaveAttribute("title", "This branch has no commit of its own yet.");
    await userEvent.click(button());
    expect(onCreatePullRequest).not.toHaveBeenCalled();
  });

  it("CHG-FR-UCRL: a detached HEAD is unavailable and says so", async () => {
    backend({ pullRequest: { branch: null } });
    renderPanel();
    await waitFor(() =>
      expect(button()).toHaveAccessibleDescription(
        "HEAD is detached, so there is no branch to propose.",
      ),
    );
    expect(calls("get_pull_request_head_state")).toHaveLength(0);
  });

  it("CHG-FR-UCRL: while the branch is being read it is unavailable and says so", async () => {
    let release: (v: unknown) => void = () => {};
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_worktrees_and_branches") return new Promise((r) => (release = r));
      if (cmd === "load_changes_panel_state") return { mode: "uncommitted" };
      if (cmd === "load_app_preferences") return { theme: "system" };
      if (cmd === "list_uncommitted_changes") return changeSet([]);
      return undefined;
    });
    renderPanel();
    await waitFor(() => expect(button()).toHaveAccessibleDescription("Reading the branch…"));
    release({ worktrees: [], activeWorktreePath: "/x", repositoryRoot: "/x", branches: [] });
  });

  it("CHG-FR-UCRL: reads again on changes updated, so a commit that lands makes it available", async () => {
    let ahead = 0;
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_pull_request_head_state")
        return {
          head: String(args?.head),
          base: "main",
          hasRemote: true,
          remoteBranchExists: true,
          unpushed: 0,
          uncommittedPaths: [],
          aheadOfBase: ahead,
        };
      return makeBackendDefault(cmd);
    });
    function makeBackendDefault(cmd: string) {
      if (cmd === "list_worktrees_and_branches")
        return {
          repositoryRoot: "/repo",
          activeWorktreePath: "/repo",
          worktrees: [
            {
              path: "/repo",
              name: "repo",
              branch: "feature/x",
              headShortHash: "a",
              isDetached: false,
              isActive: true,
              isPrimary: true,
              isMissing: false,
            },
          ],
          branches: [],
        };
      if (cmd === "load_changes_panel_state") return { mode: "uncommitted" };
      if (cmd === "load_app_preferences") return { theme: "system" };
      if (cmd === "list_uncommitted_changes") return changeSet([]);
      if (cmd === "get_default_branch") return "main";
      return undefined;
    }
    renderPanel();
    await waitFor(() =>
      expect(button()).toHaveAccessibleDescription("This branch has no commit of its own yet."),
    );
    expect(calls("get_pull_request_head_state")).toHaveLength(1);
    await waitFor(() => expect(listeners["changes-updated"]?.length).toBeGreaterThanOrEqual(2));
    ahead = 1;
    listeners["changes-updated"].forEach((cb) => cb({ payload: {} }));
    await available();
    expect(calls("get_pull_request_head_state").length).toBeGreaterThanOrEqual(2);
  });

  it("CHG-FR-UCRL: ticking files changes nothing about its availability", async () => {
    backend({ uncommitted: changeSet([LIB]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");
    await available();
    await userEvent.click(checkbox("Library.tsx"));
    expect(button()).toHaveAttribute("data-state", "available");
  });
});
