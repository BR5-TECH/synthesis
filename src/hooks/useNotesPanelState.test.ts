// The Notes panel's persisted selector position and filter text (NTS-FR-09 /
// NTS-FR-13 / PSS-FR-19).
//
// The panel's own suite covers restore-and-render; these pin the invariants
// that are invisible from there — the pre-restore write window, the
// cancel-rather-than-flush unmount that PSS-FR-16 rests on, and what happens
// when the store itself fails.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";

import {
  NOTES_PERSIST_DEBOUNCE_MS,
  useNotesPanelState,
} from "./useNotesPanelState";
import type { NotesPanelState } from "../types";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

/** A load whose resolution the test controls, to open the pre-restore window. */
function deferredLoad() {
  let settle!: (state: NotesPanelState) => void;
  const pending = new Promise<NotesPanelState>((resolve) => {
    settle = resolve;
  });
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "load_notes_panel_state") return pending;
    return undefined;
  });
  return { settle };
}

function saves(): unknown[][] {
  return invokeMock.mock.calls.filter((c) => c[0] === "save_notes_panel_state");
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "load_notes_panel_state")
      return { scopePosition: "entity", textFilter: "" };
    return undefined;
  });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("useNotesPanelState", () => {
  it("restores the persisted record", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_notes_panel_state")
        return { scopePosition: "all", textFilter: "handoff" };
      return undefined;
    });
    const { result } = renderHook(() => useNotesPanelState());

    await waitFor(() => expect(result.current.restored).toBe(true));
    expect(result.current.position).toBe("all");
    expect(result.current.text).toBe("handoff");
    // Nothing is written back: the value came from the store.
    expect(saves()).toHaveLength(0);
  });

  it("persists a choice made before the restore resolves", async () => {
    // The selector is interactive from the first paint, so a click can land
    // inside the restore window. It must survive — and be written.
    const { settle } = deferredLoad();
    const { result } = renderHook(() => useNotesPanelState());

    act(() => result.current.setPosition("all"));
    expect(result.current.position).toBe("all");

    await act(async () => {
      settle({ scopePosition: "entity", textFilter: "" });
    });

    // The user's choice survives the restore…
    expect(result.current.position).toBe("all");
    // …and reaches the store, rather than being silently dropped.
    await waitFor(
      () =>
        expect(saves()[0][1]).toEqual({
          state: { scopePosition: "all", textFilter: "" },
        }),
      { timeout: 2000 },
    );
  });

  it("fills in only the fields the user has not already changed", async () => {
    const { settle } = deferredLoad();
    const { result } = renderHook(() => useNotesPanelState());

    act(() => result.current.setText("mine"));
    await act(async () => {
      settle({ scopePosition: "all", textFilter: "stored" });
    });

    expect(result.current.text).toBe("mine");
    expect(result.current.position).toBe("all");
  });

  it("falls back to defaults, still restored, when the store cannot be read", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_notes_panel_state") throw "no project open";
      return undefined;
    });
    const { result } = renderHook(() => useNotesPanelState());

    // `restored` must still flip, or the panel would never load its notes.
    await waitFor(() => expect(result.current.restored).toBe(true));
    expect(result.current.position).toBe("entity");
    expect(result.current.text).toBe("");
  });

  it("cancels rather than flushes a pending write on unmount (PSS-FR-16)", async () => {
    vi.useFakeTimers();
    const { result, unmount } = renderHook(() => useNotesPanelState());
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });

    act(() => result.current.setPosition("all"));
    // Unmount inside the debounce window. The hook is unmounted by a project or
    // worktree switch, and by then the backend already resolves project-local
    // storage against the NEW worktree — so a flush would write the outgoing
    // worktree's position into the incoming one's store.
    unmount();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(NOTES_PERSIST_DEBOUNCE_MS * 4);
    });

    expect(saves()).toHaveLength(0);
  });

  it("retries after a failed write instead of assuming it landed", async () => {
    let failNext = true;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_notes_panel_state")
        return { scopePosition: "entity", textFilter: "" };
      if (cmd === "save_notes_panel_state") {
        if (failNext) {
          failNext = false;
          throw "disk full";
        }
        return undefined;
      }
      return undefined;
    });
    const { result } = renderHook(() => useNotesPanelState());
    await waitFor(() => expect(result.current.restored).toBe(true));

    act(() => result.current.setPosition("all"));
    await waitFor(() => expect(saves()).toHaveLength(1), { timeout: 2000 });

    // The write failed, so the value is NOT recorded as persisted — changing
    // back and forth must write again rather than being suppressed as a no-op.
    act(() => result.current.setText("x"));
    await waitFor(() => expect(saves()).toHaveLength(2), { timeout: 2000 });
    expect(saves()[1][1]).toEqual({
      state: { scopePosition: "all", textFilter: "x" },
    });
  });
});
