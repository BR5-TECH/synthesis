import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";

import { SUPERSEDE_RETRY_MS, useSearch } from "./useSearch";
import {
  createEventBus,
  emitEnded,
  emitResults,
  hit,
  resetHitOrdinals,
} from "../test/searchEvents";
import type { SearchMode } from "../types";

// The dispatch/accumulate half of SCH-search.md, exercised without a surface —
// the races here (a search ending before its own dispatch resolves, a
// supersession landing between two batches) are invisible through a render.
const bus = createEventBus();
let searchIds: string[] = [];
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: (ev: { payload: unknown }) => void) =>
    bus.listen(name, handler),
}));

/** Resolve `start_search` only when the test says so, to drive the race cases. */
let releaseDispatch: ((id: string) => void) | null = null;

beforeEach(() => {
  bus.reset();
  searchIds = [];
  resetHitOrdinals();
  releaseDispatch = null;
  let counter = 0;
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd !== "start_search") return undefined;
    const id = `search-${counter++}`;
    searchIds.push(id);
    if (releaseDispatch !== null) {
      return new Promise<string>((resolve) => {
        releaseDispatch = () => resolve(id);
      });
    }
    return id;
  });
});

afterEach(cleanup);

const OPTIONS = {
  query: "needle",
  mode: "literal_insensitive" as SearchMode,
  scope: "capped" as const,
  debounceMs: 0,
};

const cancelledIds = () =>
  invokeMock.mock.calls
    .filter((c) => c[0] === "cancel_search")
    .map((c) => c[1]?.searchId as string);

describe("useSearch — streaming and accumulation (SCH-FR-16 / SCH-FR-19)", () => {
  it("accumulates batches in ascending ordinal regardless of arrival order", async () => {
    const { result } = renderHook(() => useSearch(OPTIONS));
    await waitFor(() => expect(searchIds).toHaveLength(1));

    act(() => {
      emitResults(bus, searchIds[0], [hit({ path: "c", ordinal: 5 })]);
      emitResults(bus, searchIds[0], [hit({ path: "a", ordinal: 1 })]);
      emitResults(bus, searchIds[0], [hit({ path: "b", ordinal: 3 })]);
    });

    await waitFor(() => expect(result.current.hits).toHaveLength(3));
    expect(result.current.hits.map((h) => h.ordinal)).toEqual([1, 3, 5]);
  });

  it("SCC-FR-07: a redelivered batch does not double a rendered result", async () => {
    const { result } = renderHook(() => useSearch(OPTIONS));
    await waitFor(() => expect(searchIds).toHaveLength(1));
    const only = hit({ path: "src/a.rs" });

    act(() => {
      emitResults(bus, searchIds[0], [only]);
      emitResults(bus, searchIds[0], [only]);
    });

    await waitFor(() => expect(result.current.hits).toHaveLength(1));
  });

  it("is running until the terminal event, then ended with its reason", async () => {
    const { result } = renderHook(() => useSearch(OPTIONS));
    await waitFor(() => expect(result.current.running).toBe(true));
    expect(result.current.ended).toBe(false);

    act(() => emitEnded(bus, searchIds[0], "capped"));

    await waitFor(() => expect(result.current.ended).toBe(true));
    expect(result.current.running).toBe(false);
    expect(result.current.reason).toBe("capped");
  });

  it("ignores batches and terminations belonging to another search", async () => {
    const { result } = renderHook(() => useSearch(OPTIONS));
    await waitFor(() => expect(searchIds).toHaveLength(1));

    act(() => {
      emitResults(bus, "some-other-search", [hit({ path: "x" })]);
      emitEnded(bus, "some-other-search");
    });

    expect(result.current.hits).toHaveLength(0);
    expect(result.current.ended).toBe(false);
  });
});

describe("useSearch — events arriving before the dispatch resolves", () => {
  it("adopts hits that landed before the search id was known", async () => {
    // A search that ends before its own `invoke` settles — an empty project, a
    // cap hit on the first file — would otherwise deliver its whole result set
    // into a hook that did not yet know the id to accept it under, and the
    // overlay would sit on "Searching…" for a search that had already finished.
    releaseDispatch = () => {};
    const { result } = renderHook(() => useSearch(OPTIONS));
    await waitFor(() => expect(searchIds).toHaveLength(1));
    const id = searchIds[0];

    act(() => {
      emitResults(bus, id, [hit({ path: "early.rs" })]);
      emitEnded(bus, id, "capped");
    });
    // Nothing adopted yet: the id is still unknown to the hook.
    expect(result.current.hits).toHaveLength(0);

    await act(async () => {
      releaseDispatch?.(id);
    });

    await waitFor(() => expect(result.current.hits).toHaveLength(1));
    expect(result.current.ended).toBe(true);
    expect(result.current.reason).toBe("capped");
  });
});

describe("useSearch — supersession and cancellation (SCH-FR-15 / SCH-FR-20)", () => {
  it("a new query replaces the result set rather than merging into it", async () => {
    const { result, rerender } = renderHook((props: typeof OPTIONS) => useSearch(props), {
      initialProps: OPTIONS,
    });
    await waitFor(() => expect(searchIds).toHaveLength(1));
    act(() => emitResults(bus, searchIds[0], [hit({ path: "first.rs" })]));
    await waitFor(() => expect(result.current.hits).toHaveLength(1));

    rerender({ ...OPTIONS, query: "other" });
    await waitFor(() => expect(searchIds).toHaveLength(2));

    expect(result.current.hits).toHaveLength(0);
    // A late batch from the superseded search must not reappear under the new
    // query's results.
    act(() => emitResults(bus, searchIds[0], [hit({ path: "stale.rs" })]));
    expect(result.current.hits).toHaveLength(0);
  });

  it("SCH-FR-15: emptying the query cancels the search in flight and clears the results", async () => {
    const { result, rerender } = renderHook((props: typeof OPTIONS) => useSearch(props), {
      initialProps: OPTIONS,
    });
    await waitFor(() => expect(searchIds).toHaveLength(1));
    act(() => emitResults(bus, searchIds[0], [hit({ path: "a.rs" })]));
    await waitFor(() => expect(result.current.hits).toHaveLength(1));

    rerender({ ...OPTIONS, query: "   " });

    await waitFor(() => expect(cancelledIds()).toContain(searchIds[0]));
    expect(result.current.hits).toHaveLength(0);
    expect(result.current.ended).toBe(false);
    expect(searchIds).toHaveLength(1);
  });

  it("SCH-FR-15: emptying the query cancels even a dispatch still in flight", async () => {
    // The realistic case, and the one an immediately-resolving mock hides: at
    // the moment the query is emptied there is no id yet to cancel, so the
    // cancel has to happen when the id arrives. Without that the sweep runs to
    // completion with nobody listening.
    releaseDispatch = () => {};
    const { rerender } = renderHook((props: typeof OPTIONS) => useSearch(props), {
      initialProps: OPTIONS,
    });
    await waitFor(() => expect(searchIds).toHaveLength(1));
    const id = searchIds[0];

    rerender({ ...OPTIONS, query: "" });
    await act(async () => {
      releaseDispatch?.(id);
    });

    await waitFor(() => expect(cancelledIds()).toContain(id));
  });

  it("SCH-FR-20: dismissing the consumer cancels even a dispatch still in flight", async () => {
    releaseDispatch = () => {};
    const { rerender } = renderHook(
      (props: typeof OPTIONS & { enabled: boolean }) => useSearch(props),
      { initialProps: { ...OPTIONS, enabled: true } },
    );
    await waitFor(() => expect(searchIds).toHaveLength(1));
    const id = searchIds[0];

    rerender({ ...OPTIONS, enabled: false });
    await act(async () => {
      releaseDispatch?.(id);
    });

    await waitFor(() => expect(cancelledIds()).toContain(id));
  });

  it("SCH-FR-20: disabling the consumer cancels its search", async () => {
    const { rerender } = renderHook((props: typeof OPTIONS & { enabled: boolean }) =>
      useSearch(props),
    { initialProps: { ...OPTIONS, enabled: true } });
    await waitFor(() => expect(searchIds).toHaveLength(1));

    rerender({ ...OPTIONS, enabled: false });

    await waitFor(() => expect(cancelledIds()).toContain(searchIds[0]));
  });

  it("cancels a search whose dispatch resolved only after the consumer unmounted", async () => {
    // Closing a Search results tab inside the round-trip window would otherwise
    // leave a full sweep running with nobody listening: at unmount there is no
    // id yet to cancel.
    releaseDispatch = () => {};
    const { unmount } = renderHook(() => useSearch(OPTIONS));
    await waitFor(() => expect(searchIds).toHaveLength(1));
    const id = searchIds[0];

    unmount();
    await act(async () => {
      releaseDispatch?.(id);
    });

    await waitFor(() => expect(cancelledIds()).toContain(id));
  });

  it("unsubscribes from the event bus when the consumer unmounts", async () => {
    const { unmount } = renderHook(() => useSearch(OPTIONS));
    await waitFor(() => expect(bus.listenerCount("search-results")).toBe(1));

    unmount();

    await waitFor(() => expect(bus.listenerCount("search-results")).toBe(0));
    expect(bus.listenerCount("search-ended")).toBe(0);
  });
});

describe("useSearch — retrying a superseded sweep (SCH-FR-20)", () => {
  it("a consumer that retries stays running and re-dispatches, rather than ending truncated", async () => {
    // At most one search runs at a time (SCC-FR-12), so a keystroke in the
    // overlay supersedes the Search results tab's full sweep. Ending there
    // would leave the tab showing a partial answer — or "No results" — for a
    // query it would never ask again.
    vi.useFakeTimers();
    try {
      const { result } = renderHook(() =>
        useSearch({ ...OPTIONS, retryOnSuperseded: true }),
      );
      await act(async () => {});
      expect(searchIds).toHaveLength(1);
      act(() => emitResults(bus, searchIds[0], [hit({ path: "partial.rs" })]));
      expect(result.current.hits).toHaveLength(1);

      act(() => emitEnded(bus, searchIds[0], "superseded"));

      // Not ended: the answer is about to be asked again.
      expect(result.current.ended).toBe(false);
      expect(result.current.running).toBe(true);
      // And the partial result set is dropped rather than shown as final.
      expect(result.current.hits).toHaveLength(0);

      await act(async () => {
        vi.advanceTimersByTime(SUPERSEDE_RETRY_MS + 10);
      });
      expect(searchIds).toHaveLength(2);

      // The retry runs to completion and that is what the tab settles on.
      act(() => {
        emitResults(bus, searchIds[1], [hit({ path: "complete.rs" })]);
        emitEnded(bus, searchIds[1], "completed");
      });
      expect(result.current.ended).toBe(true);
      expect(result.current.reason).toBe("completed");
      expect(result.current.hits).toHaveLength(1);
    } finally {
      vi.useRealTimers();
    }
  });

  it("a consumer that does NOT retry ends on supersession", async () => {
    // The overlay's own search being superseded means the user typed again,
    // and that newer search is the one they want.
    const { result } = renderHook(() => useSearch(OPTIONS));
    await waitFor(() => expect(searchIds).toHaveLength(1));

    act(() => emitEnded(bus, searchIds[0], "superseded"));

    await waitFor(() => expect(result.current.ended).toBe(true));
    expect(result.current.reason).toBe("superseded");
    expect(searchIds).toHaveLength(1);
  });

  it("a retry is abandoned when the consumer goes away before it fires", async () => {
    vi.useFakeTimers();
    try {
      const { result, rerender } = renderHook(
        (props: { enabled: boolean }) =>
          useSearch({ ...OPTIONS, retryOnSuperseded: true, enabled: props.enabled }),
        { initialProps: { enabled: true } },
      );
      await act(async () => {});
      act(() => emitEnded(bus, searchIds[0], "superseded"));

      rerender({ enabled: false });
      await act(async () => {
        vi.advanceTimersByTime(SUPERSEDE_RETRY_MS + 10);
      });

      expect(searchIds).toHaveLength(1);
      expect(result.current.hits).toHaveLength(0);
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("useSearch — the typing pause (SCH-FR-15) and mode changes (SCH-FR-14)", () => {
  it("SCH-FR-15: every keystroke restarts the pause, so a burst dispatches once", async () => {
    vi.useFakeTimers();
    try {
      const { rerender } = renderHook((props: typeof OPTIONS) => useSearch(props), {
        initialProps: { ...OPTIONS, query: "n", debounceMs: 200 },
      });
      for (const query of ["ne", "nee", "need", "needl", "needle"]) {
        rerender({ ...OPTIONS, query, debounceMs: 200 });
        await act(async () => {
          vi.advanceTimersByTime(50);
        });
      }
      expect(searchIds).toHaveLength(0);

      await act(async () => {
        vi.advanceTimersByTime(200);
      });

      expect(searchIds).toHaveLength(1);
      const dispatched = invokeMock.mock.calls.filter((c) => c[0] === "start_search");
      expect(dispatched[0][1].query).toBe("needle");
    } finally {
      vi.useRealTimers();
    }
  });

  it("SCH-FR-14: a mode change dispatches immediately, without waiting out the pause", async () => {
    vi.useFakeTimers();
    try {
      const { rerender } = renderHook((props: typeof OPTIONS) => useSearch(props), {
        initialProps: { ...OPTIONS, debounceMs: 200 },
      });
      await act(async () => {
        vi.advanceTimersByTime(200);
      });
      expect(searchIds).toHaveLength(1);

      rerender({ ...OPTIONS, mode: "regex", debounceMs: 200 });
      // No timer advanced at all: the change is a deliberate click, not a
      // keystroke in progress.
      await act(async () => {});

      expect(searchIds).toHaveLength(2);
      const dispatched = invokeMock.mock.calls.filter((c) => c[0] === "start_search");
      expect(dispatched[1][1]).toMatchObject({ query: "needle", mode: "regex" });
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("useSearch — the typed invalid-query error (SCH-FR-21)", () => {
  it("surfaces the error, ends, and starts nothing", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "start_search") throw new Error("invalid query");
      return undefined;
    });
    const { result } = renderHook(() =>
      useSearch({ ...OPTIONS, query: "foo(", mode: "regex" }),
    );

    await waitFor(() => expect(result.current.error).toBe("invalid query"));
    expect(result.current.running).toBe(false);
    expect(result.current.ended).toBe(true);
    expect(result.current.hits).toHaveLength(0);
  });

  it("clears a previous error when the corrected query is dispatched", async () => {
    let failing = true;
    let counter = 0;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd !== "start_search") return undefined;
      if (failing) throw new Error("invalid query");
      const id = `search-${counter++}`;
      searchIds.push(id);
      return id;
    });
    const { result, rerender } = renderHook((props: typeof OPTIONS) => useSearch(props), {
      initialProps: { ...OPTIONS, query: "foo(", mode: "regex" as SearchMode },
    });
    await waitFor(() => expect(result.current.error).toBe("invalid query"));

    failing = false;
    rerender({ ...OPTIONS, query: "foo\\(", mode: "regex" as SearchMode });

    await waitFor(() => expect(result.current.error).toBeNull());
    expect(result.current.running).toBe(true);
  });
});
