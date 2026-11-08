/**
 * Create a PR of a stream row on a stack and on a branch with an open pull
 * request (`../../specifications/ui/WSS-work-stream-selector.md` WSS-FR-KHGP,
 * WSS-FR-OBZK, WSS-FR-ALEU; `CPR-create-pull-request.md` CPR-FR-MBTA,
 * CPR-FR-RWOE).
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { WorkStreamSelector } from "./WorkStreamSelector";
import type { BranchStack, PullRequestSummary, WorkStreamSummary } from "../types";
import { resetLogBufferForTest } from "../logging";
import { refreshOpenPullRequests, resetOpenPullRequestsForTest } from "../state/openPullRequests";
import { stackOn, stackUnavailable } from "../test/createPullRequestFixtures";
import { pullRequest } from "../test/gitPanelFixtures";
import { summary } from "../test/streamFixtures";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
  emit: () => Promise.resolve(),
}));

const BRANCH = "synthesis/stream/editor-work";

let openPulls: () => Promise<PullRequestSummary[]>;
let stack: () => Promise<BranchStack>;

function backend(streams: WorkStreamSummary[]) {
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "list_work_streams") return streams;
    if (cmd === "list_pull_requests") return openPulls();
    if (cmd === "get_branch_stack") return stack();
    if (cmd === "append_log_records") return undefined;
    return Promise.reject(new Error(`unexpected command ${cmd}`));
  });
}

const onCreatePullRequest = vi.fn();
const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

async function openRow(streams = [summary({}, { aheadOfBase: 2 })]) {
  backend(streams);
  render(
    <WorkStreamSelector
      inRepository
      activeBranch="main"
      onOpen={vi.fn()}
      onActivate={vi.fn(async () => ({ ok: true as const }))}
      onRequestMergeCommit={vi.fn(async () => null as string | null)}
      onOpenChanges={vi.fn()}
      onOpenRun={vi.fn()}
      onCreatePullRequest={onCreatePullRequest}
    />,
  );
  await userEvent.click(await screen.findByTestId("stream-selector"));
  const menu = await screen.findByTestId("stream-menu");
  return within(menu).getByTestId("stream-row-w1");
}

beforeEach(() => {
  resetLogBufferForTest();
  resetOpenPullRequestsForTest();
  openPulls = async () => [];
  stack = async () => stackUnavailable(BRANCH);
  invokeMock.mockReset();
  onCreatePullRequest.mockReset();
});
afterEach(cleanup);

describe("Create a PR of a row with an open pull request (WSS-FR-OBZK)", () => {
  it("WSS-FR-OBZK, CPR-FR-RWOE: an open pull request of the stream's branch disables it, and the reason is its accessible description", async () => {
    openPulls = async () => [pullRequest(77, { headBranch: BRANCH })];
    const row = await openRow();
    const button = within(row).getByTestId("stream-create-pr-w1");
    await waitFor(() => expect(button).toBeDisabled());
    expect(button).toHaveAccessibleDescription(
      "Pull request #77 is already open for this branch.",
    );
    expect(calls("list_pull_requests")[0][1]).toEqual({ state: "open" });
  });

  it("WSS-FR-OBZK: a failed read blocks nothing", async () => {
    openPulls = () => Promise.reject("github_unreachable");
    const row = await openRow();
    // The shared read has answered, and its answer is in the row.
    await act(() => refreshOpenPullRequests());
    expect(within(row).getByTestId("stream-create-pr-w1")).toBeEnabled();
  });

  it("WSS-FR-OBZK: a read that still runs blocks nothing", async () => {
    openPulls = () => new Promise(() => {});
    const row = await openRow();
    await waitFor(() => expect(calls("list_pull_requests")).toHaveLength(1));
    expect(within(row).getByTestId("stream-create-pr-w1")).toBeEnabled();
  });

  it("WSS-FR-OBZK: a closed pull request of the stream's branch blocks nothing", async () => {
    openPulls = async () => [pullRequest(77, { headBranch: BRANCH, state: "closed" })];
    const row = await openRow();
    await act(() => refreshOpenPullRequests());
    expect(within(row).getByTestId("stream-create-pr-w1")).toBeEnabled();
  });
});

describe("Create a PR of a row on a stack (WSS-FR-ALEU)", () => {
  it("WSS-FR-ALEU, WSS-FR-KHGP, CPR-FR-MBTA: reads the stack first, and the parent in the stack is the base", async () => {
    let answer: (stack: BranchStack) => void = () => {};
    stack = () => new Promise((resolve) => (answer = resolve));
    const row = await openRow();
    const button = within(row).getByTestId("stream-create-pr-w1");
    await userEvent.click(button);

    await waitFor(() => expect(button).toHaveTextContent("Reading the stack…"));
    expect(calls("get_branch_stack")[0][1]).toEqual({ branch: BRANCH });
    expect(onCreatePullRequest).not.toHaveBeenCalled();
    await userEvent.click(button);
    expect(calls("get_branch_stack")).toHaveLength(1);

    answer(stackOn(BRANCH, "feature/pr-tools"));
    await waitFor(() =>
      expect(onCreatePullRequest).toHaveBeenCalledWith({
        head: BRANCH,
        base: "feature/pr-tools",
        title: "editor work",
        stackParent: "feature/pr-tools",
      }),
    );
    expect(onCreatePullRequest).toHaveBeenCalledTimes(1);
    expect(screen.queryByTestId("stream-menu")).toBeNull();
  });

  it("WSS-FR-ALEU: an unavailable stack keeps the stream's base", async () => {
    const row = await openRow();
    await userEvent.click(within(row).getByTestId("stream-create-pr-w1"));
    await waitFor(() =>
      expect(onCreatePullRequest).toHaveBeenCalledWith({
        head: BRANCH,
        base: "main",
        title: "editor work",
      }),
    );
  });

  it("WSS-FR-ALEU: a failed stack read keeps the stream's base and still opens the window", async () => {
    stack = () => Promise.reject("github_unreachable");
    const row = await openRow();
    await userEvent.click(within(row).getByTestId("stream-create-pr-w1"));
    await waitFor(() =>
      expect(onCreatePullRequest).toHaveBeenCalledWith({
        head: BRANCH,
        base: "main",
        title: "editor work",
      }),
    );
  });

  it("WSS-FR-ALEU: a dropdown closed while the stack is read opens no window", async () => {
    let answer: (stack: BranchStack) => void = () => {};
    stack = () => new Promise((resolve) => (answer = resolve));
    const row = await openRow();
    await userEvent.click(within(row).getByTestId("stream-create-pr-w1"));
    await waitFor(() => expect(calls("get_branch_stack")).toHaveLength(1));
    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByTestId("stream-menu")).toBeNull());
    await act(async () => answer(stackOn(BRANCH, "feature/pr-tools")));
    // Open the dropdown again: the row has left its busy state, so the answer
    // was handled.
    await userEvent.click(screen.getByTestId("stream-selector"));
    const menu = await screen.findByTestId("stream-menu");
    await waitFor(() =>
      expect(within(menu).getByTestId("stream-create-pr-w1")).not.toHaveTextContent(
        "Reading the stack…",
      ),
    );
    expect(onCreatePullRequest).not.toHaveBeenCalled();
  });
});
