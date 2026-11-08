/**
 * The footer's Create a PR button on a stack and on a branch with an open pull
 * request (`../../specifications/ui/CHG-changes.md` CHG-FR-NKGY,
 * CHG-FR-JQSD; `CPR-create-pull-request.md` CPR-FR-MBTA, CPR-FR-RWOE).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetLogBufferForTest } from "../logging";
import { resetAppPreferencesCache } from "../state/appPreferences";
import { refreshOpenPullRequests, resetOpenPullRequestsForTest } from "../state/openPullRequests";
import { resetPanelReveals } from "../state/panelReveal";
import { stackOn, stackUnavailable } from "../test/createPullRequestFixtures";
import { pullRequest } from "../test/gitPanelFixtures";
import {
  makeBackend,
  onCreatePullRequest,
  renderPanel,
  resetHandlers,
} from "../test/changesFixtures";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

const backend = makeBackend(invokeMock);
const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

beforeEach(() => {
  resetLogBufferForTest();
  resetPanelReveals();
  resetOpenPullRequestsForTest();
  invokeMock.mockReset();
  resetAppPreferencesCache();
  resetHandlers();
});
afterEach(cleanup);

const button = () => screen.getByTestId("changes-create-pr");
const available = () =>
  waitFor(() => expect(button()).toHaveAttribute("data-state", "available"));
/** The stack read was answered, and its answer is in the panel's state. */
const stackRead = async () => {
  await waitFor(() => expect(calls("get_branch_stack")).toHaveLength(1));
  await act(async () => {});
};

describe("Create a PR in the Changes footer on a stack (CHG-FR-JQSD, CPR-FR-MBTA)", () => {
  it("CHG-FR-JQSD, CPR-FR-MBTA: the parent branch in the stack is the base and the stack parent", async () => {
    backend({ stack: stackOn("feature/x", "feature/base") });
    renderPanel();
    await available();
    await stackRead();
    expect(calls("get_branch_stack")[0][1]).toEqual({ branch: "feature/x" });
    await userEvent.click(button());
    await waitFor(() => expect(onCreatePullRequest).toHaveBeenCalled());
    expect(onCreatePullRequest).toHaveBeenCalledWith({
      head: "feature/x",
      base: "feature/base",
      title: "feature/x",
      stackParent: "feature/base",
    });
  });

  it("CPR-FR-MBTA: a parent with no pull request is still the base", async () => {
    backend({
      stack: stackOn("feature/x", "feature/base", { pullRequest: undefined }),
      openPulls: [],
    });
    renderPanel();
    await available();
    await stackRead();
    await userEvent.click(button());
    await waitFor(() => expect(onCreatePullRequest).toHaveBeenCalled());
    expect(onCreatePullRequest).toHaveBeenCalledWith(
      expect.objectContaining({ base: "feature/base", stackParent: "feature/base" }),
    );
  });

  it("CHG-FR-JQSD, CPR-FR-MBTA: an unavailable stack keeps the default branch", async () => {
    backend({ stack: stackUnavailable("feature/x") });
    renderPanel();
    await available();
    await stackRead();
    await userEvent.click(button());
    await waitFor(() => expect(onCreatePullRequest).toHaveBeenCalled());
    expect(onCreatePullRequest).toHaveBeenCalledWith({
      head: "feature/x",
      base: "main",
      title: "feature/x",
    });
  });

  it("CHG-FR-JQSD: a failed stack read keeps the default branch and blocks nothing", async () => {
    backend({ fail: { get_branch_stack: "github_unreachable" } });
    renderPanel();
    await available();
    await stackRead();
    await userEvent.click(button());
    await waitFor(() => expect(onCreatePullRequest).toHaveBeenCalled());
    expect(onCreatePullRequest).toHaveBeenCalledWith(
      expect.objectContaining({ base: "main" }),
    );
    expect(onCreatePullRequest.mock.calls[0][0]).not.toHaveProperty("stackParent");
  });

  it("CHG-FR-JQSD, CPR-FR-MBTA: a click before the stack answer waits for it, and the button is busy", async () => {
    let answer: (stack: ReturnType<typeof stackOn>) => void = () => {};
    backend({ stack: () => new Promise((resolve) => (answer = resolve)) });
    renderPanel();
    await available();
    await waitFor(() => expect(calls("get_branch_stack")).toHaveLength(1));
    await userEvent.click(button());
    await userEvent.click(button());
    expect(button()).toHaveAttribute("aria-busy", "true");
    expect(onCreatePullRequest).not.toHaveBeenCalled();
    await act(async () => answer(stackOn("feature/x", "feature/base")));
    await waitFor(() => expect(onCreatePullRequest).toHaveBeenCalledTimes(1));
    expect(onCreatePullRequest).toHaveBeenCalledWith(
      expect.objectContaining({ base: "feature/base", stackParent: "feature/base" }),
    );
    expect(calls("get_branch_stack")).toHaveLength(1);
    expect(button()).not.toHaveAttribute("aria-busy");
  });

  it("CHG-FR-JQSD, CPR-FR-MBTA: a click after a failed stack read reads the stack again", async () => {
    let reads = 0;
    backend({
      stack: () =>
        ++reads === 1
          ? Promise.reject("github_unreachable")
          : Promise.resolve(stackOn("feature/x", "feature/base")),
    });
    renderPanel();
    await available();
    await stackRead();
    await userEvent.click(button());
    await waitFor(() => expect(onCreatePullRequest).toHaveBeenCalledTimes(1));
    expect(calls("get_branch_stack")).toHaveLength(2);
    expect(onCreatePullRequest).toHaveBeenCalledWith(
      expect.objectContaining({ base: "feature/base", stackParent: "feature/base" }),
    );
  });

  it("CHG-FR-JQSD, CPR-FR-MBTA: a click after an answered stack read reads the stack again and uses the new answer", async () => {
    let reads = 0;
    backend({
      stack: () =>
        Promise.resolve(
          ++reads === 1
            ? stackOn("feature/x", "feature/old")
            : stackOn("feature/x", "feature/base"),
        ),
    });
    renderPanel();
    await available();
    await stackRead();
    await userEvent.click(button());
    await waitFor(() => expect(onCreatePullRequest).toHaveBeenCalledTimes(1));
    expect(calls("get_branch_stack")).toHaveLength(2);
    expect(onCreatePullRequest).toHaveBeenCalledWith(
      expect.objectContaining({ base: "feature/base", stackParent: "feature/base" }),
    );
  });

  it("CHG-FR-JQSD: a branch that is not in a stack keeps the default branch", async () => {
    backend({
      stack: {
        branch: "feature/x",
        available: true,
        inStack: false,
        isTop: false,
        topHasPullRequest: false,
      },
    });
    renderPanel();
    await available();
    await stackRead();
    await userEvent.click(button());
    await waitFor(() => expect(onCreatePullRequest).toHaveBeenCalled());
    expect(onCreatePullRequest.mock.calls[0][0]).toEqual({
      head: "feature/x",
      base: "main",
      title: "feature/x",
    });
  });
});

describe("Create a PR in the Changes footer with an open pull request (CHG-FR-NKGY, CPR-FR-RWOE)", () => {
  it("CHG-FR-NKGY, CPR-FR-RWOE: an open pull request of the branch makes it unavailable, with its number", async () => {
    backend({ openPulls: [pullRequest(12, { headBranch: "feature/x" })] });
    renderPanel();
    await waitFor(() =>
      expect(button()).toHaveAccessibleDescription(
        "Pull request #12 is already open for this branch.",
      ),
    );
    expect(button()).toHaveAttribute("data-state", "unavailable");
    expect(button()).toHaveAttribute("aria-disabled", "true");
    expect(calls("list_pull_requests")[0][1]).toEqual({ state: "open" });
    await userEvent.click(button());
    // An open would start a stack read, and the button would be busy.
    await act(async () => {});
    expect(button()).not.toHaveAttribute("aria-busy");
    expect(calls("get_branch_stack")).toHaveLength(1);
    expect(onCreatePullRequest).not.toHaveBeenCalled();
  });

  it("CPR-FR-RWOE: an open pull request of another branch blocks nothing", async () => {
    backend({ openPulls: [pullRequest(12, { headBranch: "feature/other" })] });
    renderPanel();
    await waitFor(() => expect(calls("list_pull_requests")).toHaveLength(1));
    // The shared read has answered, and its answer is in the panel.
    await act(() => refreshOpenPullRequests());
    await available();
  });

  it("CPR-FR-RWOE: a closed or merged pull request of the branch blocks nothing", async () => {
    backend({
      openPulls: [
        pullRequest(12, { headBranch: "feature/x", state: "closed" }),
        pullRequest(13, { headBranch: "feature/x", state: "merged" }),
      ],
    });
    renderPanel();
    await waitFor(() => expect(calls("list_pull_requests")).toHaveLength(1));
    // The shared read has answered, and its answer is in the panel.
    await act(() => refreshOpenPullRequests());
    await available();
  });

  it("CPR-FR-RWOE: a failed read of the open pull requests blocks nothing", async () => {
    backend({ fail: { list_pull_requests: "github_unreachable" } });
    renderPanel();
    await waitFor(() => expect(calls("list_pull_requests")).toHaveLength(1));
    // The shared read has answered, and its answer is in the panel.
    await act(() => refreshOpenPullRequests());
    await available();
  });

  it("CPR-FR-RWOE: a read of the open pull requests that still runs blocks nothing", async () => {
    backend({ openPulls: () => new Promise(() => {}) });
    renderPanel();
    await waitFor(() => expect(calls("list_pull_requests")).toHaveLength(1));
    await available();
  });
});
