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
