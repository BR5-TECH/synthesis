import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";

import { useGitTransferState } from "./useGitTransfer";
import { GIT_OPERATION_FINISHED, GIT_OUTPUT_LINE } from "../events";
import { createEventBus } from "../test/searchEvents";

const bus = createEventBus();
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: (ev: { payload: unknown }) => void) =>
    bus.listen(name, handler),
}));

beforeEach(() => {
  bus.reset();
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) =>
    cmd === "get_upstream_sync_state"
      ? { hasRemote: true, hasUpstream: true, ahead: 1, behind: 0 }
      : undefined,
  );
});

afterEach(cleanup);

describe("the window's push state (GIT-FR-QMYB)", () => {
  it("GIT-FR-QMYB discards the transcript but keeps the running state when the worktree changes", async () => {
    const { result, rerender } = renderHook(
      ({ resetKey }) => useGitTransferState({ enabled: true, resetKey }),
      { initialProps: { resetKey: "/p::0" } },
    );
    await waitFor(() => expect(result.current.sync).not.toBeNull());

    await act(async () =>
      bus.emit(GIT_OUTPUT_LINE, { operation: "push", line: "Pushing a to origin" }),
    );
    expect(result.current.lines).toEqual(["Pushing a to origin"]);
    expect(result.current.running).toBe(true);

    rerender({ resetKey: "/p::1" });
    // The previous worktree's standing is not offered for the new one.
    expect(result.current.sync).toBeNull();
    await waitFor(() => expect(result.current.lines).toEqual([]));
    // The push goes on in the backend, so no control may start another yet.
    expect(result.current.running).toBe(true);

    await act(async () =>
      bus.emit(GIT_OPERATION_FINISHED, { operation: "push", ok: true }),
    );
    expect(result.current.running).toBe(false);
  });

  it("GIT-FR-QMYB refuses a second push before any command is invoked", async () => {
    let settle!: () => void;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "push_current_branch")
        return new Promise<void>((resolve) => (settle = resolve));
      return undefined;
    });
    const { result } = renderHook(() => useGitTransferState({ enabled: true }));

    let first!: Promise<unknown>;
    let second: unknown;
    await act(async () => {
      first = result.current.pushBranch();
      second = await result.current.pushBranch();
    });
    expect(second).toEqual({ ok: false, cause: "busy" });
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "push_current_branch"),
    ).toHaveLength(1);

    await act(async () => {
      settle();
      await first;
    });
    expect(result.current.running).toBe(false);
  });

  it("GIT-FR-QMYB ends the running state when the invocation is rejected without a terminal event", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "push_current_branch") throw "boom";
      return undefined;
    });
    const { result } = renderHook(() => useGitTransferState({ enabled: true }));

    let outcome: unknown;
    await act(async () => {
      outcome = await result.current.pushBranch();
    });
    expect(outcome).toEqual({ ok: false, cause: "failed", error: "boom" });
    expect(result.current.running).toBe(false);
  });

  it("GIT-FR-QMYB owns no listener and reads nothing while disabled", async () => {
    renderHook(() => useGitTransferState({ enabled: false }));
    expect(bus.listenerCount(GIT_OUTPUT_LINE)).toBe(0);
    expect(bus.listenerCount(GIT_OPERATION_FINISHED)).toBe(0);
    expect(invokeMock).not.toHaveBeenCalled();
  });
});

describe("the window's pull as one transfer (GIT-FR-QMYB, WTS-FR-31)", () => {
  const upToDate = {
    state: "up_to_date",
    ahead: 0,
    behind: 0,
    updatedPaths: [],
    refresh: { remoteState: "refreshed" },
  };

  it("GIT-FR-QMYB: a pull holds the shared running state, named as a pull, until it returns", async () => {
    let settle!: (value: unknown) => void;
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "pull_current_branch")
        return new Promise((resolve) => {
          settle = resolve;
        });
      if (cmd === "get_upstream_sync_state")
        return Promise.resolve({ hasRemote: true, hasUpstream: true, ahead: 1, behind: 0 });
      if (cmd === "append_log_records") return Promise.resolve();
      return Promise.reject(new Error(`unexpected ${cmd}`));
    });
    const { result } = renderHook(() => useGitTransferState({ enabled: true }));

    let pulled!: Promise<unknown>;
    act(() => {
      pulled = result.current.pullBranch();
    });
    await waitFor(() => expect(result.current.running).toBe(true));
    expect(result.current.operation).toBe("pull");

    // GIT-FR-QMYB: neither a push nor a second pull starts meanwhile, and
    // neither invokes anything.
    await expect(result.current.pushBranch()).resolves.toEqual({ ok: false, cause: "busy" });
    await expect(result.current.pullBranch()).resolves.toEqual({ ok: false, cause: "busy" });
    const invoked = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd).length;
    expect(invoked("push_current_branch")).toBe(0);
    expect(invoked("pull_current_branch")).toBe(1);

    await act(async () => {
      settle(upToDate);
      await pulled;
    });
    expect(result.current.running).toBe(false);
    expect(result.current.operation).toBeNull();
    await expect(pulled).resolves.toEqual({ ok: true, outcome: upToDate });
  });

  it("GIT-FR-QMYB: a pull's output lines mark the transfer running as a pull", async () => {
    const { result } = renderHook(() => useGitTransferState({ enabled: true }));
    await act(async () =>
      bus.emit(GIT_OUTPUT_LINE, { operation: "pull", line: "Pulling main from origin/main" }),
    );
    expect(result.current.running).toBe(true);
    expect(result.current.operation).toBe("pull");
    expect(result.current.lines).toEqual(["Pulling main from origin/main"]);
    await act(async () =>
      bus.emit(GIT_OPERATION_FINISHED, { operation: "pull", ok: true }),
    );
    expect(result.current.running).toBe(false);
    expect(result.current.lines).toEqual(["Pulling main from origin/main", "pull finished"]);
  });

  it("GIT-FR-RCDL: a reconcile is one transfer at the tips it was given", async () => {
    const merged = { state: "merged", commit: "c".repeat(40), updatedPaths: ["a.md"] };
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "reconcile_pull") return merged;
      if (cmd === "get_upstream_sync_state")
        return { hasRemote: true, hasUpstream: true, ahead: 1, behind: 0 };
      return undefined;
    });
    const { result } = renderHook(() => useGitTransferState({ enabled: true }));
    let call: unknown;
    await act(async () => {
      call = await result.current.reconcilePull("rebase", "a".repeat(40), "b".repeat(40));
    });
    expect(call).toEqual({ ok: true, outcome: merged });
    expect(invokeMock).toHaveBeenCalledWith("reconcile_pull", {
      strategy: "rebase",
      localTip: "a".repeat(40),
      remoteTip: "b".repeat(40),
    });
    expect(result.current.running).toBe(false);
  });

  it("GIT-FR-QMYB, GIT-FR-RCDL: a reconcile is refused as busy, invoking nothing, while a push runs", async () => {
    const { result } = renderHook(() => useGitTransferState({ enabled: true }));
    await act(async () =>
      bus.emit(GIT_OUTPUT_LINE, { operation: "push", line: "Pushing a to origin" }),
    );
    let call: unknown;
    await act(async () => {
      call = await result.current.reconcilePull("merge", "a", "b");
    });
    expect(call).toEqual({ ok: false, cause: "busy" });
    expect(invokeMock.mock.calls.some((c) => c[0] === "reconcile_pull")).toBe(false);
    expect(result.current.operation).toBe("push");
  });

  it("GIT-FR-QMYB: a rejected pull is a failure with its text, and ends the running state", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "pull_current_branch"
        ? Promise.reject("not a git repository")
        : Promise.resolve(undefined),
    );
    const { result } = renderHook(() => useGitTransferState({ enabled: true }));
    let call: unknown;
    await act(async () => {
      call = await result.current.pullBranch();
    });
    expect(call).toEqual({ ok: false, cause: "failed", error: "not a git repository" });
    expect(result.current.running).toBe(false);
  });
});
