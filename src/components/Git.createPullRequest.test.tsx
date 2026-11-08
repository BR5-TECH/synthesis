/**
 * Create PR for current branch of the Git panel on a stack and on a branch with
 * an open pull request (`../../specifications/ui/GIT-git.md` GIT-FR-HMRW;
 * `CPR-create-pull-request.md` CPR-FR-MBTA, CPR-FR-RWOE).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Git } from "./Git";
import { resetLogBufferForTest } from "../logging";
import { resetOpenPullRequestsForTest } from "../state/openPullRequests";
import type { BranchStack, PullRequestSummary } from "../types";
import type { PullRequestSource } from "./CreatePullRequest/types";
import { stackOn, stackUnavailable } from "../test/createPullRequestFixtures";
import { pullRequest } from "../test/gitPanelFixtures";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

let openPulls: PullRequestSummary[];
let stack: BranchStack;

beforeEach(() => {
  resetLogBufferForTest();
  resetOpenPullRequestsForTest();
  openPulls = [pullRequest(1)];
  stack = stackUnavailable("feature");
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case "list_pull_requests":
        return openPulls;
      case "get_branch_stack":
        return stack;
      case "get_project_github_token_binding":
        return { tokenId: "t1", resolution: "bound" };
      case "get_upstream_sync_state":
        return { hasRemote: true, hasUpstream: true, ahead: 0, behind: 0 };
      case "list_worktrees_and_branches":
        return {
          repositoryRoot: "/repo",
          activeWorktreePath: "/repo",
          worktrees: [
            {
              path: "/repo",
              name: "repo",
              branch: "feature",
              headShortHash: "abc1234",
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
          head: "feature",
          base: "main",
          hasRemote: true,
          remoteBranchExists: true,
          unpushed: 0,
          uncommittedPaths: [],
          aheadOfBase: 2,
        };
      case "append_log_records":
        return undefined;
      default:
        return Promise.reject(new Error(`unexpected command ${cmd}`));
    }
  });
});

afterEach(cleanup);

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

async function openPrs(onCreatePullRequest = vi.fn<(source: PullRequestSource) => void>()) {
  render(
    <Git
      onSwitchWorktree={vi.fn(async () => ({ ok: true as const }))}
      canCheckOutBranches
      onCreatePullRequest={onCreatePullRequest}
    />,
  );
  await userEvent.click(screen.getByRole("tab", { name: "PRs" }));
  return onCreatePullRequest;
}

describe("Create PR for current branch (GIT-FR-HMRW)", () => {
  it("GIT-FR-HMRW, CPR-FR-MBTA: the parent branch in the stack is the base", async () => {
    stack = stackOn("feature", "feature/base");
    const onCreate = await openPrs();
    const create = screen.getByTestId("git-create-pr");
    await waitFor(() => expect(create).toHaveAttribute("data-state", "available"));
    await waitFor(() => expect(calls("get_branch_stack")).toHaveLength(1));
    await act(async () => {});
    await userEvent.click(create);
    await waitFor(() => expect(onCreate).toHaveBeenCalledTimes(1));
    expect(onCreate).toHaveBeenCalledWith({
      head: "feature",
      base: "feature/base",
      title: "feature",
      stackParent: "feature/base",
    });
  });

  it("GIT-FR-HMRW, CPR-FR-RWOE: an open pull request of the current branch leaves it unavailable, with the reason", async () => {
    openPulls = [pullRequest(1), pullRequest(31, { headBranch: "feature" })];
    const onCreate = await openPrs();
    const create = screen.getByTestId("git-create-pr");
    await waitFor(() =>
      expect(create).toHaveAccessibleDescription(
        "Pull request #31 is already open for this branch.",
      ),
    );
    expect(create).toHaveAttribute("data-state", "unavailable");
    await userEvent.click(create);
    // An open would start a stack read, and the button would be busy.
    await act(async () => {});
    expect(create).not.toHaveAttribute("aria-busy");
    expect(calls("get_branch_stack")).toHaveLength(1);
    expect(onCreate).not.toHaveBeenCalled();
  });
});
