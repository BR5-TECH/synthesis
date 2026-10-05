import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";

import { useDiffTotals } from "./useDiffTotals";
import { CHANGES_UPDATED } from "../events";
import type { DiffTotals } from "../types";

// STB-FR-25–STB-FR-30 — the status bar's uncommitted diff summary.

const invokeMock = vi.fn();
const listeners: Record<string, Array<(e: { payload: unknown }) => void>> = {};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (name: string, handler: (e: { payload: unknown }) => void) => {
      (listeners[name] ??= []).push(handler);
      return () => {
        listeners[name] = (listeners[name] ?? []).filter((h) => h !== handler);
      };
    },
  ),
}));

function fireChangesUpdated() {
  act(() => {
    for (const h of [...(listeners[CHANGES_UPDATED] ?? [])]) {
      h({ payload: { changeCount: 1 } });
    }
  });
}

let answer: DiffTotals | Error;

beforeEach(() => {
  invokeMock.mockReset();
  answer = { addedLines: 412, removedLines: 87, fileCount: 5 };
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd !== "get_uncommitted_diff_totals") {
      throw new Error(`unexpected invoke ${cmd}`);
    }
    if (answer instanceof Error) throw answer;
    return answer;
  });
  for (const k in listeners) delete listeners[k];
});

afterEach(cleanup);

/** How many times the totals were read. */
function reads() {
  return invokeMock.mock.calls.filter(
    (c) => c[0] === "get_uncommitted_diff_totals",
  ).length;
}

describe("loading the totals (STB-FR-25 / STB-FR-27)", () => {
  it("reads them on mount, independently of the Changes panel", async () => {
    const { result } = renderHook(() => useDiffTotals(0));
    await waitFor(() =>
      expect(result.current).toEqual({
        addedLines: 412,
        removedLines: 87,
        fileCount: 5,
      }),
    );
    // CHC-FR-22 / STB-FR-27: the summary holds no change set of its own, so it
    // never calls the panel's list command.
    expect(invokeMock.mock.calls.map((c) => c[0])).not.toContain(
      "list_uncommitted_changes",
    );
  });

  it("reloads on `changes updated`", async () => {
    const { result } = renderHook(() => useDiffTotals(0));
    await waitFor(() => expect(result.current).not.toBeNull());
    expect(reads()).toBe(1);

    answer = { addedLines: 500, removedLines: 90, fileCount: 6 };
    fireChangesUpdated();

    await waitFor(() => expect(result.current?.addedLines).toBe(500));
    expect(reads()).toBe(2);
  });

  it("renders nothing in its place when the totals operation errors", async () => {
    // STB-FR-29: outside a Git repository the operation returns the typed
    // `"not a git repository"` error. Null, not zeros — zeros would claim a
    // clean tree, which is a different and wrong statement.
    answer = new Error("not a git repository");
    const { result } = renderHook(() => useDiffTotals(0));
    await waitFor(() => expect(reads()).toBe(1));
    expect(result.current).toBeNull();
  });

  it("keeps zeros as a real answer", async () => {
    answer = { addedLines: 0, removedLines: 0, fileCount: 0 };
    const { result } = renderHook(() => useDiffTotals(0));
    await waitFor(() =>
      expect(result.current).toEqual({
        addedLines: 0,
        removedLines: 0,
        fileCount: 0,
      }),
    );
  });

  it("treats a malformed answer as no comparison rather than rendering it", async () => {
    invokeMock.mockImplementation(async () => ({ nonsense: true }));
    const { result } = renderHook(() => useDiffTotals(0));
    await waitFor(() => expect(invokeMock).toHaveBeenCalled());
    expect(result.current).toBeNull();
  });
});

describe("re-rooting on a worktree change (STB-FR-30)", () => {
  it("clears the outgoing root's counts and reloads against the new one", async () => {
    // Worktree A has uncommitted edits, B is clean. Passing a constant instead
    // of the content-root epoch would leave A's counts on screen against B —
    // exactly the failure STB-FR-30 names.
    const { result, rerender } = renderHook(
      ({ epoch }) => useDiffTotals(epoch),
      { initialProps: { epoch: 0 } },
    );
    await waitFor(() => expect(result.current?.addedLines).toBe(412));

    answer = { addedLines: 0, removedLines: 0, fileCount: 0 };
    rerender({ epoch: 1 });

    await waitFor(() => expect(reads()).toBe(2));
    await waitFor(() => expect(result.current?.addedLines).toBe(0));
  });

  it("shows nothing rather than the previous root's counts while reloading", async () => {
    // The clear is synchronous with the epoch change; the reload is not. Without
    // it the bar would keep asserting A's counts about B for the round-trip.
    let release: (v: DiffTotals) => void = () => {};
    const { result, rerender } = renderHook(
      ({ epoch }) => useDiffTotals(epoch),
      { initialProps: { epoch: 0 } },
    );
    await waitFor(() => expect(result.current?.addedLines).toBe(412));

    invokeMock.mockImplementation(
      async () => new Promise<DiffTotals>((r) => (release = r)),
    );
    rerender({ epoch: 1 });
    expect(result.current).toBeNull();

    act(() => release({ addedLines: 0, removedLines: 0, fileCount: 0 }));
    await waitFor(() => expect(result.current?.addedLines).toBe(0));
  });

  it("does not reload when the epoch is unchanged", async () => {
    const { rerender } = renderHook(({ epoch }) => useDiffTotals(epoch), {
      initialProps: { epoch: 0 },
    });
    await waitFor(() => expect(reads()).toBe(1));
    rerender({ epoch: 0 });
    rerender({ epoch: 0 });
    expect(reads()).toBe(1);
  });

  it("drops the previous root's subscription on re-root", async () => {
    const { rerender } = renderHook(({ epoch }) => useDiffTotals(epoch), {
      initialProps: { epoch: 0 },
    });
    await waitFor(() =>
      expect(listeners[CHANGES_UPDATED]?.length).toBeGreaterThan(0),
    );
    rerender({ epoch: 1 });
    await waitFor(() => expect(listeners[CHANGES_UPDATED]).toHaveLength(1));

    // One event, one reload — not two.
    const before = reads();
    fireChangesUpdated();
    await waitFor(() => expect(reads()).toBe(before + 1));
  });
});
