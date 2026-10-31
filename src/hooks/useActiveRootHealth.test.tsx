import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";

import { useActiveRootHealth } from "./useActiveRootHealth";
import {
  applyActiveRootState,
  isActiveRootGone,
  resetActiveRootHealth,
} from "../state/activeRootHealth";
import { resetLogBufferForTest } from "../logging";
import type { ActiveRootState, WorktreeEntry } from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

type EventHandler = (ev: { payload: unknown }) => void;
let handlers: [string, EventHandler][] = [];
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: EventHandler) => {
    handlers.push([name, handler]);
    return Promise.resolve(() => {
      handlers = handlers.filter(([, h]) => h !== handler);
    });
  },
  emit: () => Promise.resolve(),
}));

const healthy: ActiveRootState = { state: "healthy", primaryPath: "/dev/acme" };
const gone: ActiveRootState = { state: "gone", primaryPath: "/dev/acme" };

const entry = (over: Partial<WorktreeEntry> = {}): WorktreeEntry => ({
  path: "/data/w/w1",
  name: "w1",
  headShortHash: "abc1234",
  isDetached: false,
  isActive: true,
  isPrimary: false,
  isMissing: false,
  ...over,
});

const reads = () => invokeMock.mock.calls.filter((c) => c[0] === "active_root_state").length;

function answering(answer: unknown) {
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "active_root_state") return Promise.resolve(answer);
    if (cmd === "append_log_records") return Promise.resolve(undefined);
    return Promise.reject(new Error(`unexpected command: ${cmd}`));
  });
}

const fireTreeChanged = (removedPaths: string[] | undefined) => {
  const listening = handlers.filter(([name]) => name === "project-tree-changed");
  expect(listening.length).toBeGreaterThan(0);
  for (const [, handler] of listening) {
    handler({ payload: { changeCount: 1, removedPaths } });
  }
};

beforeEach(() => {
  invokeMock.mockReset();
  handlers = [];
  resetLogBufferForTest();
  resetActiveRootHealth();
  answering(healthy);
});
afterEach(cleanup);

describe("when the active root is read (WTS-FR-BIOJ)", () => {
  it("WTS-FR-BIOJ: a linked worktree is read when the project opens", async () => {
    renderHook(() => useActiveRootHealth(entry(), 0));

    await waitFor(() => expect(reads()).toBe(1));
  });

  it("WTS-FR-BIOJ: the primary worktree reads nothing", async () => {
    renderHook(() => useActiveRootHealth(entry({ isPrimary: true }), 0));
    await act(async () => {
      window.dispatchEvent(new Event("focus"));
    });

    expect(reads()).toBe(0);
  });

  it("WTS-FR-BIOJ: no open project reads nothing", async () => {
    renderHook(() => useActiveRootHealth(null, 0));
    await act(async () => {});

    expect(reads()).toBe(0);
  });

  it("WTS-FR-BIOJ: the main window taking focus reads", async () => {
    renderHook(() => useActiveRootHealth(entry(), 0));
    await waitFor(() => expect(reads()).toBe(1));

    window.dispatchEvent(new Event("focus"));

    await waitFor(() => expect(reads()).toBe(2));
  });

  it("WTS-FR-BIOJ: a change of path alone reads", async () => {
    const { rerender } = renderHook(
      ({ path, epoch }) => useActiveRootHealth(entry({ path }), epoch),
      { initialProps: { path: "/data/w/w1", epoch: 0 } },
    );
    await waitFor(() => expect(reads()).toBe(1));

    rerender({ path: "/data/w/w2", epoch: 0 });

    await waitFor(() => expect(reads()).toBe(2));
  });

  it("WTS-FR-BIOJ: a new content root at the same path reads", async () => {
    const { rerender } = renderHook(
      ({ epoch }) => useActiveRootHealth(entry(), epoch),
      { initialProps: { epoch: 0 } },
    );
    await waitFor(() => expect(reads()).toBe(1));

    rerender({ epoch: 1 });

    await waitFor(() => expect(reads()).toBe(2));
  });

  it("WTS-FR-BIOJ: a tree event that carries removed paths reads", async () => {
    renderHook(() => useActiveRootHealth(entry(), 0));
    await waitFor(() => expect(reads()).toBe(1));
    await waitFor(() => expect(handlers.length).toBeGreaterThan(0));

    fireTreeChanged(["a.md"]);

    await waitFor(() => expect(reads()).toBe(2));
  });

  it("WTS-FR-BIOJ: a tree event with no removed path does not read", async () => {
    renderHook(() => useActiveRootHealth(entry(), 0));
    await waitFor(() => expect(reads()).toBe(1));
    await waitFor(() => expect(handlers.length).toBeGreaterThan(0));

    fireTreeChanged([]);
    fireTreeChanged(undefined);
    await act(async () => {});

    expect(reads()).toBe(1);
  });

  it("WTS-FR-BIOJ: no timer drives the read", async () => {
    vi.useFakeTimers();
    try {
      renderHook(() => useActiveRootHealth(entry(), 0));
      await act(async () => {
        await vi.advanceTimersByTimeAsync(60 * 60 * 1000);
      });

      expect(reads()).toBe(1);
    } finally {
      vi.useRealTimers();
    }
  });

  it("WTS-FR-BIOJ: an unmount stops the reads", async () => {
    const { unmount } = renderHook(() => useActiveRootHealth(entry(), 0));
    await waitFor(() => expect(reads()).toBe(1));

    await waitFor(() => expect(handlers.length).toBeGreaterThan(0));

    unmount();
    window.dispatchEvent(new Event("focus"));
    await waitFor(() => expect(handlers).toHaveLength(0));

    expect(reads()).toBe(1);
  });
});

describe("what the answer does", () => {
  it("WTS-FR-WIFO: a gone answer reaches the store", async () => {
    answering(gone);
    renderHook(() => useActiveRootHealth(entry(), 0));

    await waitFor(() => expect(isActiveRootGone()).toBe(true));
  });

  it("WTS-FR-FEEK: a move to the primary worktree clears the state", async () => {
    applyActiveRootState(gone);
    renderHook(() => useActiveRootHealth(entry({ isPrimary: true }), 1));

    await waitFor(() => expect(isActiveRootGone()).toBe(false));
  });

  it("WTS-FR-BIOJ: an older answer that lands late does not overwrite a newer one", async () => {
    let finishOld: (answer: ActiveRootState) => void = () => {};
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "active_root_state") {
        return new Promise((resolve) => (finishOld = resolve));
      }
      return Promise.resolve(undefined);
    });
    renderHook(() => useActiveRootHealth(entry(), 0));
    await waitFor(() => expect(reads()).toBe(1));

    answering(healthy);
    window.dispatchEvent(new Event("focus"));
    await waitFor(() => expect(reads()).toBe(2));
    await act(async () => {
      finishOld(gone);
    });

    expect(isActiveRootGone()).toBe(false);
  });

  it("WTS-FR-BIOJ: a read that fails changes nothing and escapes nowhere", async () => {
    applyActiveRootState(gone);
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "append_log_records") return Promise.resolve(undefined);
      return Promise.reject(new Error("no project open"));
    });
    renderHook(() => useActiveRootHealth(entry(), 0));

    await waitFor(() => expect(reads()).toBe(1));
    await act(async () => {});

    expect(isActiveRootGone()).toBe(true);
  });

  it("WTS-FR-BIOJ: a mock that throws synchronously is caught", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "append_log_records") return Promise.resolve(undefined);
      throw new Error("strict mock");
    });
    renderHook(() => useActiveRootHealth(entry(), 0));

    await waitFor(() => expect(reads()).toBe(1));
    expect(isActiveRootGone()).toBe(false);
  });

  it("WTS-FR-BIOJ: an answer that is not an object reads as nothing", async () => {
    applyActiveRootState(gone);
    answering(undefined);
    renderHook(() => useActiveRootHealth(entry(), 0));

    await waitFor(() => expect(reads()).toBe(1));
    await act(async () => {});

    expect(isActiveRootGone()).toBe(true);
  });
});
