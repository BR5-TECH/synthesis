import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";

import { useInFlightOperations } from "./useInFlightOperations";
import { OPERATION_PROGRESS } from "../events";
import type { Operation } from "../types";

// STB-FR-15 / PRG-FR-02 / PRG-FR-08 — the hook that keeps the status bar's
// in-flight set current. All the non-trivial logic of the centre region lives
// here; `StatusBar.tsx` is a pure render of what this produces.

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

function fire(operation: Operation) {
  act(() => {
    for (const h of [...(listeners[OPERATION_PROGRESS] ?? [])]) {
      h({ payload: operation });
    }
  });
}

function op(
  id: string,
  sequence: number,
  overrides: Partial<Operation> = {},
): Operation {
  return {
    id,
    kind: "scan",
    label: `label ${id}`,
    state: "running",
    sequence,
    ...overrides,
  };
}

/** Resolve `list_in_flight_operations` only when the test says so. */
function deferredSnapshot() {
  let release: (value: Operation[]) => void = () => {};
  const promise = new Promise<Operation[]>((resolve) => {
    release = resolve;
  });
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "list_in_flight_operations") return promise;
    throw new Error(`unexpected invoke ${cmd}`);
  });
  return (value: Operation[]) => act(() => void release(value));
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "list_in_flight_operations") return [];
    throw new Error(`unexpected invoke ${cmd}`);
  });
  for (const k in listeners) delete listeners[k];
});

afterEach(cleanup);

describe("the mount read (STB-FR-15)", () => {
  it("establishes the set from the command without waiting for an event", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_in_flight_operations") return [op("op-2", 2), op("op-1", 1)];
      throw new Error(`unexpected invoke ${cmd}`);
    });
    const { result } = renderHook(() => useInFlightOperations());

    await waitFor(() => expect(result.current).toHaveLength(2));
    expect(invokeMock).toHaveBeenCalledWith("list_in_flight_operations");
    expect(result.current.map((o) => o.id)).toEqual(["op-2", "op-1"]);
  });

  it("does not resurrect an operation that terminated before the read landed", async () => {
    // The race STB-FR-15 rules out: the subscription is attached first, so a
    // terminal event can arrive while `list_in_flight_operations` is still in
    // flight — and the snapshot it eventually returns is stale. Merging it
    // wholesale would pin a finished "Indexing project…" on the bar forever.
    const release = deferredSnapshot();
    const { result } = renderHook(() => useInFlightOperations());

    await waitFor(() =>
      expect(listeners[OPERATION_PROGRESS]?.length).toBeGreaterThan(0),
    );
    fire(op("op-1", 1, { state: "finished" }));

    // The backend answers with the state as it was *before* the terminate.
    release([op("op-1", 1)]);

    await waitFor(() => expect(invokeMock).toHaveBeenCalled());
    expect(result.current).toEqual([]);
  });

  it("merges the snapshot with operations the event stream already reported", async () => {
    const release = deferredSnapshot();
    const { result } = renderHook(() => useInFlightOperations());
    await waitFor(() =>
      expect(listeners[OPERATION_PROGRESS]?.length).toBeGreaterThan(0),
    );

    fire(op("op-5", 5));
    release([op("op-1", 1)]);

    await waitFor(() => expect(result.current).toHaveLength(2));
    // Both present, and neither duplicated.
    expect(result.current.map((o) => o.id)).toEqual(["op-5", "op-1"]);
  });

  it("survives a backend that answers with nothing", async () => {
    invokeMock.mockImplementation(async () => undefined);
    const { result } = renderHook(() => useInFlightOperations());
    await waitFor(() => expect(invokeMock).toHaveBeenCalled());
    expect(result.current).toEqual([]);
  });
});

describe("ordering and fallback (STB-FR-05 / STB-FR-06 / PRG-FR-02)", () => {
  it("orders by descending sequence however the events arrive", async () => {
    // The bar renders `operations[0]`, so this sort IS the "most recently
    // started" rule. Firing out of order is what distinguishes a real sort from
    // an unshift that happens to look right when events arrive in order.
    const { result } = renderHook(() => useInFlightOperations());
    await waitFor(() =>
      expect(listeners[OPERATION_PROGRESS]?.length).toBeGreaterThan(0),
    );

    fire(op("op-3", 3));
    fire(op("op-1", 1));
    fire(op("op-2", 2));

    expect(result.current.map((o) => o.id)).toEqual(["op-3", "op-2", "op-1"]);
  });

  it("falls back to the next in-flight operation as each terminates", async () => {
    // STB-FR-05, STB-FR-06, derived rather than hand-fixtured: a scan is running, an
    // install starts and shows, the install ends and the scan comes back.
    const { result } = renderHook(() => useInFlightOperations());
    await waitFor(() =>
      expect(listeners[OPERATION_PROGRESS]?.length).toBeGreaterThan(0),
    );

    fire(op("scan", 1, { label: "Indexing project…" }));
    fire(op("install", 2, { label: "Installing plugin…" }));
    expect(result.current[0].label).toBe("Installing plugin…");

    fire(op("install", 2, { label: "Installing plugin…", state: "finished" }));
    expect(result.current[0].label).toBe("Indexing project…");

    fire(op("scan", 1, { label: "Indexing project…", state: "finished" }));
    expect(result.current).toEqual([]);
  });

  it("replaces an operation in place when it reports progress", async () => {
    // PRG-FR-05 / STB-FR-08: an update carries the same id, so it must update
    // the existing entry rather than appear as a second one.
    const { result } = renderHook(() => useInFlightOperations());
    await waitFor(() =>
      expect(listeners[OPERATION_PROGRESS]?.length).toBeGreaterThan(0),
    );

    fire(op("op-1", 1));
    fire(op("op-1", 1, { completed: 40, total: 500 }));

    expect(result.current).toHaveLength(1);
    expect(result.current[0].completed).toBe(40);
  });

  it("drops an operation on every terminal state, not just `finished`", async () => {
    // PRG-FR-09: a failure or cancellation must leave the in-flight set exactly
    // as a success does, or a failed push sticks on the bar for the session.
    for (const state of ["failed", "cancelled", "finished"] as const) {
      const { result, unmount } = renderHook(() => useInFlightOperations());
      await waitFor(() =>
        expect(listeners[OPERATION_PROGRESS]?.length).toBeGreaterThan(0),
      );
      fire(op("op-1", 1));
      expect(result.current).toHaveLength(1);
      fire(op("op-1", 1, { state }));
      expect(result.current).toEqual([]);
      unmount();
    }
  });

  it("PRG-FR-KXQW, STB-FR-LKRN: carries each operation's destination from the snapshot and from events, and orders concurrent ones newest first", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_in_flight_operations")
        return [
          op("op-2", 2, { activation: { type: "discussion", discussionId: "d-1" } }),
          op("op-1", 1),
        ];
      throw new Error(`unexpected invoke ${cmd}`);
    });
    const { result } = renderHook(() => useInFlightOperations());
    await waitFor(() => expect(result.current).toHaveLength(2));
    expect(result.current[0].activation).toEqual({
      type: "discussion",
      discussionId: "d-1",
    });
    expect(result.current[1].activation).toBeUndefined();

    fire(op("op-3", 3, { activation: { type: "graduation_run", runId: "g-1" } }));
    expect(result.current.map((o) => o.id)).toEqual(["op-3", "op-2", "op-1"]);
    expect(result.current[0].activation).toEqual({ type: "graduation_run", runId: "g-1" });
  });

  it("unsubscribes on unmount", async () => {
    const { unmount } = renderHook(() => useInFlightOperations());
    await waitFor(() =>
      expect(listeners[OPERATION_PROGRESS]?.length).toBeGreaterThan(0),
    );
    unmount();
    await waitFor(() =>
      expect(listeners[OPERATION_PROGRESS] ?? []).toHaveLength(0),
    );
  });
});
