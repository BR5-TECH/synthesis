import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";

import { useCurrentBranchPullRequest } from "./useCurrentBranchPullRequest";
import { CHANGES_UPDATED } from "../events";
import { createEventBus } from "../test/searchEvents";

const bus = createEventBus();
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: (ev: { payload: unknown }) => void) => bus.listen(name, handler),
}));

const CONTEXT = {
  activeWorktreePath: "/repo",
  worktrees: [{ path: "/repo", isActive: true, isDetached: false, branch: "feature", stream: null }],
};
const HEAD_STATE = {
  head: "feature",
  base: "main",
  hasRemote: true,
  remoteBranchExists: false,
  unpushed: null,
  uncommittedPaths: [],
  aheadOfBase: 1,
};

/** The worktree reads that wait until the test lets each one go. */
let gated = false;
const contextReleases: Array<() => void> = [];

beforeEach(() => {
  bus.reset();
  gated = false;
  contextReleases.length = 0;
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "list_worktrees_and_branches":
        return gated
          ? new Promise((resolve) => contextReleases.push(() => resolve(CONTEXT)))
          : Promise.resolve(CONTEXT);
      case "get_pull_request_head_state":
        return Promise.resolve(HEAD_STATE);
      case "get_branch_stack":
        return Promise.resolve(null);
      case "append_log_records":
        return Promise.resolve(undefined);
      default:
        return Promise.reject(new Error(`unexpected invoke ${cmd}`));
    }
  });
});

afterEach(cleanup);

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd).length;

describe("a burst of changes updated (CHG-FR-RUAG)", () => {
  it("CHG-FR-RUAG: events during a branch read start exactly one more read of the worktree list and the head state", async () => {
    const { result } = renderHook(() => useCurrentBranchPullRequest(true));
    await waitFor(() => expect(result.current.current.status).toBe("ready"));
    await waitFor(() => expect(bus.listenerCount(CHANGES_UPDATED)).toBe(1));

    gated = true;
    const lists = calls("list_worktrees_and_branches");
    const heads = calls("get_pull_request_head_state");
    act(() => {
      for (let i = 0; i < 20; i += 1) bus.emit(CHANGES_UPDATED, { changeCount: 1 });
    });
    expect(calls("list_worktrees_and_branches")).toBe(lists + 1);

    await act(async () => contextReleases[0]());
    await waitFor(() => expect(contextReleases).toHaveLength(2));
    await act(async () => contextReleases[1]());
    await waitFor(() => expect(calls("get_pull_request_head_state")).toBe(heads + 2));
    expect(calls("list_worktrees_and_branches")).toBe(lists + 2);
    expect(result.current.current.status).toBe("ready");
  });
});
