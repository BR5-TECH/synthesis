// The Drafts panel's persisted status position, filter text and expanded folder
// set (DRP-FR-14 / PSS-FR-20).
//
// The panel's own suite covers restore-and-render; these pin the invariants that
// are invisible from there — the pre-restore write window, the coalescing of a
// burst into one write, the cancel-rather-than-flush unmount that PSS-FR-16
// rests on, and what happens when the store itself fails.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";

import { PERSIST_DEBOUNCE_MS, useDraftsPanelState } from "./useDraftsPanelState";
import type { DraftsPanelStateRecord } from "../types";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const DEFAULTS: DraftsPanelStateRecord = {
  statusFilter: "active",
  textFilter: "",
  expandedFolders: [],
};

/** A load whose resolution the test controls, to open the pre-restore window. */
function deferredLoad() {
  let settle!: (state: DraftsPanelStateRecord) => void;
  const pending = new Promise<DraftsPanelStateRecord>((resolve) => {
    settle = resolve;
  });
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "load_drafts_panel_state") return pending;
    return undefined;
  });
  return { settle };
}

function saves(): unknown[][] {
  return invokeMock.mock.calls.filter((c) => c[0] === "save_drafts_panel_state");
}

function lastSaved(): DraftsPanelStateRecord {
  const all = saves();
  return (all[all.length - 1][1] as { state: DraftsPanelStateRecord }).state;
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "load_drafts_panel_state") return DEFAULTS;
    return undefined;
  });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("useDraftsPanelState", () => {
  it("DRP-FR-14: restores the persisted record for this worktree", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_drafts_panel_state")
        return {
          statusFilter: "archived",
          textFilter: "window",
          expandedFolders: ["UI", "UI/Components"],
        };
      return undefined;
    });
    const { result } = renderHook(() => useDraftsPanelState());

    await waitFor(() => expect(result.current.filter).toBe("archived"));
    expect(result.current.text).toBe("window");
    expect([...result.current.expanded].sort()).toEqual(["UI", "UI/Components"]);
  });

  it("DRP-FR-14: expansion is recorded, not collapse — an unknown folder is collapsed", async () => {
    const { result } = renderHook(() => useDraftsPanelState());
    await waitFor(() => expect(saves()).toHaveLength(0));

    // Nothing persisted: every folder, a newly created one included, is
    // collapsed because it is absent from the set rather than present as
    // "collapsed".
    expect(result.current.expanded.size).toBe(0);
    act(() => result.current.toggleExpanded("UI"));
    expect(result.current.expanded.has("UI")).toBe(true);
    act(() => result.current.toggleExpanded("UI"));
    expect(result.current.expanded.has("UI")).toBe(false);
  });

  it("PSS-FR-20: a path naming an absent folder is retained rather than pruned", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_drafts_panel_state")
        return { ...DEFAULTS, expandedFolders: ["gone/for/now"] };
      return undefined;
    });
    const { result } = renderHook(() => useDraftsPanelState());

    // The hook validates nothing: a folder that disappears and returns comes
    // back expanded, and an unresolvable path is never an error.
    await waitFor(() => expect(result.current.expanded.has("gone/for/now")).toBe(true));
    expect(saves()).toHaveLength(0);
  });

  it("DRP-FR-14: a burst of changes coalesces into a single write, carrying every field", async () => {
    vi.useFakeTimers();
    const { result } = renderHook(() => useDraftsPanelState());
    await act(async () => {
      await Promise.resolve();
    });

    act(() => {
      result.current.setText("w");
      result.current.setText("wi");
      result.current.setText("win");
      result.current.toggleExpanded("UI");
      result.current.setFilter("all");
    });
    expect(saves()).toHaveLength(0);
    await act(async () => {
      vi.advanceTimersByTime(PERSIST_DEBOUNCE_MS);
      await Promise.resolve();
    });

    expect(saves()).toHaveLength(1);
    // The record is written whole, so persisting one value carries the other
    // two through unchanged.
    expect(lastSaved()).toEqual({
      statusFilter: "all",
      textFilter: "win",
      expandedFolders: ["UI"],
    });
  });

  it("DRP-FR-14: a value that merely arrived from the store is never written back", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_drafts_panel_state")
        return { statusFilter: "all", textFilter: "spec", expandedFolders: ["UI"] };
      return undefined;
    });
    const { result } = renderHook(() => useDraftsPanelState());
    await waitFor(() => expect(result.current.filter).toBe("all"));

    await new Promise((r) => setTimeout(r, PERSIST_DEBOUNCE_MS + 40));
    expect(saves()).toHaveLength(0);

    // …and a change that goes straight back to the stored value is not written
    // either, because the signature has not moved.
    act(() => result.current.toggleExpanded("UI"));
    act(() => result.current.toggleExpanded("UI"));
    await new Promise((r) => setTimeout(r, PERSIST_DEBOUNCE_MS + 40));
    expect(saves()).toHaveLength(0);
  });

  it("keeps a value the author set before the restore landed", async () => {
    // Both filter controls are interactive from the first paint, so a keystroke
    // can land before the restore resolves. Applying the stored value over it
    // would silently revert what they just did.
    const { settle } = deferredLoad();
    const { result } = renderHook(() => useDraftsPanelState());

    act(() => result.current.setText("mine"));
    act(() => result.current.toggleExpanded("mine-folder"));
    await act(async () => {
      settle({
        statusFilter: "archived",
        textFilter: "stored",
        expandedFolders: ["stored-folder"],
      });
      await Promise.resolve();
    });

    expect(result.current.text).toBe("mine");
    // The untouched field takes the stored value.
    expect(result.current.filter).toBe("archived");
    // Expansion merges rather than being overridden: everything starts
    // collapsed, so the only thing the author can do before the restore lands is
    // expand something, and a union keeps both.
    expect([...result.current.expanded].sort()).toEqual([
      "mine-folder",
      "stored-folder",
    ]);
  });

  it("writes nothing before the restore has landed", async () => {
    // Otherwise the initial defaults would overwrite what was stored.
    vi.useFakeTimers();
    deferredLoad();
    const { result } = renderHook(() => useDraftsPanelState());

    act(() => result.current.setFilter("all"));
    await act(async () => {
      vi.advanceTimersByTime(PERSIST_DEBOUNCE_MS * 4);
      await Promise.resolve();
    });
    expect(saves()).toHaveLength(0);
  });

  it("PSS-FR-16: unmounting cancels a pending write rather than flushing it", async () => {
    // By the time this hook unmounts the backend already resolves project-local
    // storage against the *new* worktree, so a flush would write the outgoing
    // worktree's expanded folders into the incoming one's store.
    vi.useFakeTimers();
    const { result, unmount } = renderHook(() => useDraftsPanelState());
    await act(async () => {
      await Promise.resolve();
    });

    act(() => result.current.toggleExpanded("UI"));
    unmount();
    await act(async () => {
      vi.advanceTimersByTime(PERSIST_DEBOUNCE_MS * 4);
      await Promise.resolve();
    });
    expect(saves()).toHaveLength(0);
  });

  it("a store that cannot be read leaves the panel on its defaults", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_drafts_panel_state") throw new Error("no project open");
      return undefined;
    });
    const { result } = renderHook(() => useDraftsPanelState());

    // DRP-FR-07: the active position, an empty filter, nothing expanded — and
    // no error is surfaced, because none of this is worth blocking the panel.
    await waitFor(() => expect(result.current.filter).toBe("active"));
    expect(result.current.text).toBe("");
    expect(result.current.expanded.size).toBe(0);

    // The panel stays usable and the next change is still written.
    act(() => result.current.setFilter("all"));
    await waitFor(() => expect(saves()).toHaveLength(1));
  });

  it("a failed write is retried by the next change rather than assumed durable", async () => {
    let failing = true;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_drafts_panel_state") return DEFAULTS;
      if (cmd === "save_drafts_panel_state") {
        if (failing) throw new Error("read-only filesystem");
        return undefined;
      }
      return undefined;
    });
    const { result } = renderHook(() => useDraftsPanelState());
    await waitFor(() => expect(result.current.filter).toBe("active"));

    act(() => result.current.setFilter("all"));
    await waitFor(() => expect(saves()).toHaveLength(1));

    // The value never landed, so setting it back and forth writes again rather
    // than being suppressed by a guard that assumed the first write succeeded.
    failing = false;
    act(() => result.current.setFilter("archived"));
    await waitFor(() => expect(saves()).toHaveLength(2));
    expect(lastSaved().statusFilter).toBe("archived");
  });

  it("DRP-FR-24 / DRP-FR-30: reparentExpanded carries a folder's subtree to its new path", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_drafts_panel_state")
        return {
          ...DEFAULTS,
          expandedFolders: ["UI", "UI/Components", "UI/Components/deep", "backend"],
        };
      return undefined;
    });
    const { result } = renderHook(() => useDraftsPanelState());
    await waitFor(() => expect(result.current.expanded.has("UI")).toBe(true));

    act(() => result.current.reparentExpanded("UI", "Interface"));

    expect([...result.current.expanded].sort()).toEqual([
      "Interface",
      "Interface/Components",
      "Interface/Components/deep",
      // Untouched: the folders around it keep their own state.
      "backend",
    ]);
  });

  it("reparentExpanded and expandAll compose within one tick", async () => {
    // A move does both — remap the moved folder's key, then reveal the
    // destination — and a setter reading render-scope state would lose the
    // first of the two.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_drafts_panel_state")
        return { ...DEFAULTS, expandedFolders: ["UI/Components"] };
      return undefined;
    });
    const { result } = renderHook(() => useDraftsPanelState());
    await waitFor(() => expect(result.current.expanded.has("UI/Components")).toBe(true));

    act(() => {
      result.current.reparentExpanded("UI/Components", "backend/Components");
      result.current.expandAll(["backend"]);
    });

    expect([...result.current.expanded].sort()).toEqual([
      "backend",
      "backend/Components",
    ]);
  });

  it("reparentExpanded leaves an unexpanded folder alone and never moves the root", async () => {
    const { result } = renderHook(() => useDraftsPanelState());
    await waitFor(() => expect(result.current.filter).toBe("active"));

    act(() => result.current.expandAll(["backend"]));
    // `UI` was never expanded, so there is nothing to carry.
    act(() => result.current.reparentExpanded("UI", "Interface"));
    expect([...result.current.expanded]).toEqual(["backend"]);
    // …and the implicit root is not a folder anything can be renamed from.
    act(() => result.current.reparentExpanded("", "anything"));
    expect([...result.current.expanded]).toEqual(["backend"]);
  });

  it("the newest write claims the guard even when an older one settles last", async () => {
    // Whichever write reaches disk last is what the store holds, so an older
    // dispatch settling later must not record what is persisted — or a change
    // back to its value would be suppressed and never written.
    const settlers: Array<() => void> = [];
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_drafts_panel_state") return DEFAULTS;
      if (cmd === "save_drafts_panel_state")
        return new Promise<void>((resolve) => settlers.push(() => resolve()));
      return undefined;
    });
    const { result } = renderHook(() => useDraftsPanelState());
    await waitFor(() => expect(result.current.filter).toBe("active"));

    act(() => result.current.setFilter("all"));
    await waitFor(() => expect(saves()).toHaveLength(1));
    act(() => result.current.setFilter("archived"));
    await waitFor(() => expect(saves()).toHaveLength(2));

    // The OLDER write settles last.
    await act(async () => {
      settlers[1]();
      await Promise.resolve();
      settlers[0]();
      await Promise.resolve();
    });

    // Going back to `all` — the older write's value — must still be written.
    act(() => result.current.setFilter("all"));
    await waitFor(() => expect(saves()).toHaveLength(3));
    expect(lastSaved().statusFilter).toBe("all");
  });

  it("expandAll adds paths and ignores the implicit root", async () => {
    const { result } = renderHook(() => useDraftsPanelState());
    await waitFor(() => expect(result.current.filter).toBe("active"));

    act(() => result.current.expandAll(["UI", "UI/Components", ""]));
    expect([...result.current.expanded].sort()).toEqual(["UI", "UI/Components"]);

    await waitFor(() => expect(saves()).toHaveLength(1));

    // Already-expanded paths change nothing, so no second write is provoked.
    act(() => result.current.expandAll(["UI"]));
    await new Promise((r) => setTimeout(r, PERSIST_DEBOUNCE_MS + 40));
    expect(saves()).toHaveLength(1);
  });
});
