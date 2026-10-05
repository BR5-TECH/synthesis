/**
 * The Library panel state persisted per machine and per worktree
 * (`../../specifications/ui/LIB-library.md`, LIB-FR-13 … LIB-FR-17).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";

const invokeMock = vi.fn();
const unlistenMock = vi.fn();
// Captured event subscribers, keyed by event name, so a test can fire the
// backend `"project tree changed"` event by hand.
let listeners: Record<string, (event: { payload: TreeChangedPayload }) => void> =
  {};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (
      event: string,
      cb: (event: { payload: TreeChangedPayload }) => void,
    ) => {
      listeners[event] = cb;
      return unlistenMock;
    },
  ),
}));

import {
  baseTree,
  file,
  folder,
  LibraryHost,
  makeTreeStubs,
  panelState,
  renderLibrary,
} from "../test/libraryFixtures";
import { PERSIST_DEBOUNCE_MS } from "../hooks/useLibraryPanelState";
import type { LibraryPanelState, TreeChangedPayload, TreeNode } from "../types";
import { PROJECT_TREE_CHANGED } from "../events";
import { pickSelector, selectorValue } from "../test/selectors";
import { resetPanelReveals } from "../state/panelReveal";

const { mockInvoke, setLoadTree } = makeTreeStubs(invokeMock);

/** The payload of the most recent `save_library_panel_state` call, if any. */
/**
 * The commands invoked so far, minus the traffic that is not the panel acting on
 * the user's behalf: its own tree load and panel-state round trip, and
 * `append_log_records`. The log batch is flushed off a timer (`../logging`), so
 * whether it lands inside any given test is a matter of timing rather than
 * behaviour — leaving it in makes every exhaustive assertion below flaky.
 */
function commandsBeyondPanelTraffic(): string[] {
  return invokeMock.mock.calls
    .map((c) => c[0] as string)
    .filter(
      (cmd) =>
        cmd !== "load_project_tree" &&
        cmd !== "load_library_panel_state" &&
        cmd !== "save_library_panel_state" &&
        cmd !== "append_log_records",
    );
}

function lastSave(): LibraryPanelState | null {
  const calls = invokeMock.mock.calls.filter(
    (c) => c[0] === "save_library_panel_state",
  );
  if (calls.length === 0) return null;
  return (calls[calls.length - 1][1] as { state: LibraryPanelState }).state;
}

function saveCount(): number {
  return invokeMock.mock.calls.filter(
    (c) => c[0] === "save_library_panel_state",
  ).length;
}

/**
 * Wait past the coalescing window so a "no write was issued" assertion means
 * it — without this a `waitFor` resolves on its first tick, long before the
 * debounce could have fired, and would pass even with the guard removed.
 */
async function settleDebounce() {
  await new Promise((r) => setTimeout(r, PERSIST_DEBOUNCE_MS + 60));
}

/**
 * Wait for a write to land. The coalescing window eats a third of `waitFor`'s
 * default budget before the call is even issued, which is a thin margin on an
 * oversubscribed CI runner — so these waits get an explicit one rather than
 * relying on the default.
 */
const SAVE_TIMEOUT = { timeout: PERSIST_DEBOUNCE_MS + 2500 };

beforeEach(() => {
  // Module-level, like the preferences cache: reveal nonces are minted from
  // one counter for the app's life, so a suite that does not reset finds its
  // first request already consumed by the previous one.
  resetPanelReveals();
  invokeMock.mockReset();
  unlistenMock.mockReset();
  listeners = {};
});

afterEach(() => {
  cleanup();
});

// LIB-FR-14 … LIB-FR-17: the panel state persisted per machine and per worktree.
describe("persisted panel state (LIB-FR-14 … LIB-FR-17)", () => {
  // LIB-FR-12, LIB-FR-15: a project never opened on this machine renders the defaults —
  // every folder collapsed, the All artifacts lens, an empty text filter — with
  // the root's immediate children visible (LIB-FR-12 / LIB-FR-15).
  it("renders the defaults for a worktree with no persisted state", async () => {
    setLoadTree(baseTree(), panelState());
    renderLibrary();

    // The root's immediate children are visible…
    expect(await screen.findByText(".claude")).toBeInTheDocument();
    expect(screen.getByText("specifications")).toBeInTheDocument();
    expect(screen.getByText("AGENTS.md")).toBeInTheDocument();
    // …and every folder is collapsed, so no nested node renders.
    expect(screen.queryByText("skills")).not.toBeInTheDocument();
    expect(screen.queryByText("onboarding.md")).not.toBeInTheDocument();
    expect(screen.queryByText("LIB-library.md")).not.toBeInTheDocument();
    // The lens is All artifacts (the unclassified root file stays hidden) and
    // the text filter is empty.
    expect(selectorValue("Filter by type")).toBe("artifacts");
    expect(screen.getByLabelText("Filter tree")).toHaveValue("");
    expect(screen.queryByText("notes.txt")).not.toBeInTheDocument();
  });

  // LIB-FR-06, LIB-FR-14, LIB-FR-15: the persisted set is restored — those folders expand, every
  // folder absent from it stays collapsed, and nothing is selected.
  it("restores exactly the persisted expanded folders, with no selection", async () => {
    setLoadTree(
      baseTree(),
      panelState({ expandedPaths: [".claude", ".claude/skills"] }),
    );
    renderLibrary();

    // The two persisted folders are expanded, down to the nested file…
    expect(await screen.findByText("skills")).toBeInTheDocument();
    expect(screen.getByText("onboarding.md")).toBeInTheDocument();
    // …while `specifications`, absent from the set, is collapsed.
    expect(screen.getByText("specifications")).toBeInTheDocument();
    expect(screen.queryByText("LIB-library.md")).not.toBeInTheDocument();
    // Selection is session-local and starts empty on every open (LIB-FR-06).
    expect(document.querySelector('[data-selected="true"]')).toBeNull();
    // The restore itself writes nothing back — asserted past the coalescing
    // window, so this fails if the restore stops seeding the guard.
    await settleDebounce();
    expect(saveCount()).toBe(0);
  });

  // LIB-FR-14: the lens and the text filter are restored alongside the tree.
  it("restores the artifact-type lens and the text filter", async () => {
    setLoadTree(
      baseTree(),
      panelState({
        expandedPaths: ["misc"],
        artifactTypeFilter: "all_files",
        textFilter: "todo",
      }),
    );
    renderLibrary();

    await waitFor(() =>
      expect(selectorValue("Filter by type")).toBe("files"),
    );
    expect(screen.getByLabelText("Filter tree")).toHaveValue("todo");
    // The restored pair is actually applied: the unclassified file inside the
    // artifact-less folder is reachable, and non-matching files are hidden.
    expect(screen.getByText("misc")).toBeInTheDocument();
    expect(screen.getByText("todo.txt")).toBeInTheDocument();
    expect(screen.queryByText("AGENTS.md")).not.toBeInTheDocument();
  });

  // LIB-FR-14: expanding a folder persists the new set, recording expansion
  // rather than collapse (LIB-FR-15).
  it("persists the expanded set when a folder is expanded", async () => {
    setLoadTree(baseTree(), panelState());
    renderLibrary();

    fireEvent.click(await screen.findByText("specifications"));
    expect(screen.getByText("LIB-library.md")).toBeInTheDocument();

    await waitFor(() =>
      expect(lastSave()).toEqual({
        expandedPaths: ["specifications"],
        artifactTypeFilter: "all_artifacts",
        textFilter: "",
      }),
    );
  });

  // LIB-FR-15: collapsing a folder removes it from the persisted set rather
  // than recording a collapse.
  it("removes a folder from the persisted set when it is collapsed", async () => {
    setLoadTree(baseTree(), panelState({ expandedPaths: ["specifications"] }));
    renderLibrary();

    fireEvent.click(await screen.findByText("specifications"));
    expect(screen.queryByText("LIB-library.md")).not.toBeInTheDocument();

    await waitFor(() => expect(lastSave()?.expandedPaths).toEqual([]), SAVE_TIMEOUT);
  });

  // LIB-FR-17: the record is written whole, so a lens change carries the
  // expanded set and the text filter through unchanged.
  it("carries the other two values through when one changes", async () => {
    setLoadTree(
      baseTree(),
      panelState({ expandedPaths: [".claude"], textFilter: "md" }),
    );
    renderLibrary();
    await screen.findByText("skills");

    await pickSelector("Filter by type", "files");

    await waitFor(() =>
      expect(lastSave()).toEqual({
        expandedPaths: [".claude"],
        artifactTypeFilter: "all_files",
        textFilter: "md",
      }),
    );
  });

  // LIB-FR-17: a run of keystrokes coalesces into a single write carrying the
  // final text, not one write per character.
  it("coalesces a burst of text-filter keystrokes into one write", async () => {
    setLoadTree(baseTree(), panelState({ expandedPaths: [".claude"] }));
    renderLibrary();
    await screen.findByText("skills");

    const input = screen.getByLabelText("Filter tree");
    // Four separate changes in quick succession, inside the debounce window.
    fireEvent.change(input, { target: { value: "s" } });
    fireEvent.change(input, { target: { value: "sp" } });
    fireEvent.change(input, { target: { value: "spe" } });
    fireEvent.change(input, { target: { value: "spec" } });

    await waitFor(() => expect(lastSave()?.textFilter).toBe("spec"), SAVE_TIMEOUT);
    expect(saveCount()).toBe(1);
    // …and the expanded set rode along unchanged.
    expect(lastSave()?.expandedPaths).toEqual([".claude"]);
  });

  // LIB-FR-17: a value changed and changed straight back is not written at all.
  it("does not write when the state returns to what was persisted", async () => {
    setLoadTree(
      baseTree(),
      // Two entries, deliberately not in sorted order, so this also pins that
      // the guard compares sets rather than the order they were expanded in.
      panelState({ expandedPaths: ["specifications", ".claude"] }),
    );
    renderLibrary();
    await screen.findByText("LIB-library.md");

    // Collapse and re-expand within the coalescing window.
    fireEvent.click(screen.getByText("specifications"));
    fireEvent.click(screen.getByText("specifications"));

    await waitFor(() =>
      expect(screen.getByText("LIB-library.md")).toBeInTheDocument(),
    );
    await settleDebounce();
    expect(saveCount()).toBe(0);
  });

  // LIB-FR-17: several folders expanded in succession also coalesce — the FR
  // names this case alongside the keystroke burst.
  it("coalesces several folders expanded in succession into one write", async () => {
    setLoadTree(baseTree(), panelState());
    renderLibrary();
    await screen.findByText("specifications");

    fireEvent.click(screen.getByText(".claude"));
    fireEvent.click(screen.getByText("specifications"));
    fireEvent.click(screen.getByText("skills"));

    await waitFor(() => expect(saveCount()).toBe(1), SAVE_TIMEOUT);
    expect(lastSave()?.expandedPaths).toEqual([
      ".claude",
      ".claude/skills",
      "specifications",
    ]);
    // And nothing further is written once the burst has settled.
    await settleDebounce();
    expect(saveCount()).toBe(1);
  });

  // LIB-FR-16: a persisted path naming a folder that is not in the tree is
  // ignored when rendering, surfaces no error, and is RETAINED — so when the
  // folder comes back (a branch checkout restoring it) it renders expanded.
  it("retains a persisted path whose folder is absent, and re-expands on return", async () => {
    // The first tree has no `specifications` folder; the reload brings it back.
    const withoutSpecs = folder("", true, [
      folder(".claude", true, [
        folder(".claude/skills", true, [
          file(".claude/skills/onboarding.md", "skill", "inferred"),
        ]),
      ]),
      file("AGENTS.md", "agent", "inferred"),
    ]);
    mockInvoke(
      {
        load_project_tree: () =>
          invokeMock.mock.calls.filter((c) => c[0] === "load_project_tree")
            .length === 1
            ? withoutSpecs
            : baseTree(),
      },
      panelState({ expandedPaths: ["specifications", ".claude"] }),
    );
    renderLibrary();

    // The absent folder simply does not render, and nothing is reported.
    await screen.findByText("AGENTS.md");
    expect(screen.queryByText("specifications")).not.toBeInTheDocument();
    expect(document.querySelector(".vpanel__body")?.textContent).not.toContain(
      "specifications",
    );

    // A write issued WHILE the folder is absent must still carry its path —
    // pruning on save is the likelier place for this bug than pruning on read.
    fireEvent.click(screen.getByText(".claude"));
    await waitFor(() => expect(lastSave()).not.toBeNull(), SAVE_TIMEOUT);
    expect(lastSave()?.expandedPaths).toContain("specifications");

    // The folder returns on the watcher-driven reload…
    listeners[PROJECT_TREE_CHANGED]?.({ payload: { changeCount: 1 } });

    // …and comes back expanded, because its path was never pruned.
    expect(await screen.findByText("LIB-library.md")).toBeInTheDocument();
  });

  // The panel-state load is issued alongside the tree load rather than chained
  // behind it, so the first painted tree is already in its restored shape
  // (LIB NFR). Holding the tree load open proves it: a chained implementation
  // could not have issued the panel-state load yet.
  it("issues the panel-state load without waiting for the tree load", async () => {
    let releaseTree: (tree: TreeNode) => void = () => {};
    const treeLoad = new Promise<TreeNode>((resolve) => {
      releaseTree = resolve;
    });
    mockInvoke(
      { load_project_tree: () => treeLoad },
      panelState({ expandedPaths: [".claude", ".claude/skills"] }),
    );
    renderLibrary();

    // While the tree load is still pending, the panel-state load has already
    // been issued.
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some((c) => c[0] === "load_library_panel_state"),
      ).toBe(true),
    );

    // And when the tree does arrive it paints already-expanded.
    releaseTree(baseTree());
    expect(await screen.findByText("onboarding.md")).toBeInTheDocument();
  });

  // LIB NFR: a panel state that cannot be read leaves the tree usable on its
  // defaults rather than surfacing a blocking error.
  it("falls back to the defaults when the panel state cannot be read", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_tree") return baseTree();
      if (cmd === "load_library_panel_state") throw "no project open";
      if (cmd === "save_library_panel_state") return undefined;
      throw new Error(`unexpected invoke ${cmd}`);
    });
    renderLibrary();

    // The tree still renders, collapsed to the defaults, with no error banner.
    expect(await screen.findByText("AGENTS.md")).toBeInTheDocument();
    expect(screen.queryByText("onboarding.md")).not.toBeInTheDocument();
    expect(screen.queryByText(/no project open/)).not.toBeInTheDocument();
  });

  // LIB NFR: a failed write is not surfaced, does not break the panel, and the
  // state is written again on the next change rather than being assumed durable.
  it("keeps the tree usable when persisting fails, and retries on the next change", async () => {
    let failWrites = true;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_tree") return baseTree();
      if (cmd === "load_library_panel_state") return panelState();
      if (cmd === "save_library_panel_state") {
        if (failWrites) throw "disk full";
        return undefined;
      }
      throw new Error(`unexpected invoke ${cmd}`);
    });
    renderLibrary();

    fireEvent.click(await screen.findByText("specifications"));
    await waitFor(() => expect(saveCount()).toBe(1), SAVE_TIMEOUT);

    // The expansion still happened and no error is shown.
    expect(screen.getByText("LIB-library.md")).toBeInTheDocument();
    expect(screen.queryByText(/disk full/)).not.toBeInTheDocument();

    // The next change writes again — and carries the value the failed write was
    // meant to persist, so the rejection cost nothing.
    failWrites = false;
    fireEvent.click(screen.getByText(".claude"));
    await waitFor(() => expect(saveCount()).toBe(2), SAVE_TIMEOUT);
    expect(lastSave()?.expandedPaths).toEqual([".claude", "specifications"]);
  });

  // Two writes can overlap when one is slow. Whichever reaches disk last is
  // what the store holds, so an older write settling afterwards must not be
  // recorded as the persisted value — a later change back to it would then be
  // suppressed and never written at all.
  it("does not let an out-of-order write claim the persisted value", async () => {
    const resolvers: Array<() => void> = [];
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_tree") return baseTree();
      if (cmd === "load_library_panel_state") return panelState();
      if (cmd === "save_library_panel_state") {
        return new Promise<void>((resolve) => resolvers.push(resolve));
      }
      throw new Error(`unexpected invoke ${cmd}`);
    });
    renderLibrary();
    await screen.findByText("specifications");

    // Write #1: expand `specifications`. Held open.
    fireEvent.click(screen.getByText("specifications"));
    await waitFor(() => expect(resolvers).toHaveLength(1), SAVE_TIMEOUT);

    // Write #2: also expand `.claude`, dispatched while #1 is still in flight.
    fireEvent.click(screen.getByText(".claude"));
    await waitFor(() => expect(resolvers).toHaveLength(2), SAVE_TIMEOUT);

    // They settle out of order: the newer one first, the stale one after.
    resolvers[1]();
    resolvers[0]();
    await settleDebounce();

    // Returning to write #1's exact state must still issue a write, because
    // that is not what the store ended up holding.
    fireEvent.click(screen.getByText(".claude"));
    await waitFor(() => expect(resolvers).toHaveLength(3), SAVE_TIMEOUT);
    expect(lastSave()?.expandedPaths).toEqual(["specifications"]);
  });

  // LIB-FR-06: choosing another vertical-panel surface unmounts the Library, so
  // the expand/filter state must outlive that component. A pending write is not
  // lost either — the state lives above the surface switch.
  it("preserves the expanded set across a vertical-panel surface switch", async () => {
    setLoadTree(baseTree(), panelState());
    const { rerender } = render(<LibraryHost />);

    // Expand a folder, then switch surface immediately — inside the coalescing
    // window, so any pending write is still in flight.
    fireEvent.click(await screen.findByText("specifications"));
    expect(screen.getByText("LIB-library.md")).toBeInTheDocument();
    rerender(<LibraryHost surface="other" />);
    expect(screen.getByText("other surface")).toBeInTheDocument();

    // Switch back: the folder is still expanded…
    rerender(<LibraryHost />);
    expect(await screen.findByText("LIB-library.md")).toBeInTheDocument();
    // …the panel state was not re-read (the state outlived the unmount rather
    // than being restored from disk)…
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "load_library_panel_state"),
    ).toHaveLength(1);
    // …and the write still landed despite the switch happening mid-window.
    await waitFor(
      () => expect(lastSave()?.expandedPaths).toEqual(["specifications"]),
      SAVE_TIMEOUT,
    );
  });
});

// LIB-FR-13 / LIB-FR-02 / LIB-FR-14: a worktree switch remounts the shell
// subtree, so the panel restores the incoming worktree's own record and carries
// nothing across.
describe("worktree switch (LIB-FR-13)", () => {
  it("restores worktree B's own state, carrying nothing over from A", async () => {
    // LIB-FR-13, LIB-FR-14: B has a record of its own.
    const treeB = folder("", true, [
      folder("src", true, [file("src/lib.rs", "spec", "inferred")]),
      folder("specifications", true, [
        file("specifications/LIB-library.md", "spec", "inferred"),
      ]),
    ]);
    setLoadTree(
      baseTree(),
      panelState({ expandedPaths: ["specifications"], textFilter: "lib" }),
    );
    const { unmount } = render(<LibraryHost />);
    await screen.findByText("LIB-library.md");
    expect(screen.getByLabelText("Filter tree")).toHaveValue("lib");

    // Change something in A so a write carrying A's paths genuinely exists —
    // otherwise the "nothing from A was written" check below inspects an empty
    // list and asserts nothing at all. Editing the text filter leaves the
    // expanded set alone, so the write still carries A's folders.
    fireEvent.change(screen.getByLabelText("Filter tree"), {
      target: { value: "libr" },
    });
    await waitFor(
      () => expect(lastSave()?.expandedPaths).toContain("specifications"),
      SAVE_TIMEOUT,
    );
    const savesInA = saveCount();

    // The switch remounts the whole subtree against the new content root.
    unmount();
    setLoadTree(treeB, panelState({ expandedPaths: ["src"] }));
    render(<LibraryHost />);

    // B's folder is expanded, A's is not, and A's text filter did not travel.
    expect(await screen.findByText("lib.rs")).toBeInTheDocument();
    expect(screen.getByText("specifications")).toBeInTheDocument();
    expect(screen.queryByText("LIB-library.md")).not.toBeInTheDocument();
    expect(screen.getByLabelText("Filter tree")).toHaveValue("");
    expect(document.querySelector('[data-selected="true"]')).toBeNull();

    // Nothing at all is written after the switch — in particular nothing
    // carrying A's paths. Counted from the switch point, because A legitimately
    // wrote its own record while it was the active worktree.
    await settleDebounce();
    const after = invokeMock.mock.calls
      .filter((c) => c[0] === "save_library_panel_state")
      .slice(savesInA);
    expect(after).toHaveLength(0);
  });

  /**
   * The discard-don't-flush rule, pinned. A worktree switch unmounts the whole
   * shell subtree, and by then the backend already resolves project-local
   * storage against the NEW worktree — so flushing a pending write here would
   * land the outgoing worktree's expanded paths in the incoming one's store.
   *
   * Nothing else covers this: a flush placed in the persist effect's cleanup is
   * caught by the coalescing tests, but a flush added on unmount alone would
   * pass every one of them.
   */
  it("discards a pending write on a worktree switch rather than flushing it", async () => {
    setLoadTree(baseTree(), panelState());
    const { unmount } = render(<LibraryHost />);
    await screen.findByText("specifications");

    // Expand a folder and switch worktree inside the coalescing window, so a
    // write is still pending at the moment the subtree is torn down.
    fireEvent.click(screen.getByText("specifications"));
    expect(screen.getByText("LIB-library.md")).toBeInTheDocument();
    unmount();

    // The pending write was dropped, not flushed.
    await settleDebounce();
    expect(saveCount()).toBe(0);

    // And B mounts on its own record with no trace of A.
    const treeB = folder("", true, [
      folder("src", true, [file("src/lib.rs", "spec", "inferred")]),
    ]);
    setLoadTree(treeB, panelState({ expandedPaths: ["src"] }));
    render(<LibraryHost />);
    expect(await screen.findByText("lib.rs")).toBeInTheDocument();
    await settleDebounce();
    expect(saveCount()).toBe(0);
  });

  it("renders the defaults when the incoming worktree has no record", async () => {
    // LIB-FR-02, LIB-FR-13: B has nothing persisted, so every folder is collapsed, the
    // lens is All artifacts and the text filter is empty.
    setLoadTree(
      baseTree(),
      panelState({
        expandedPaths: [".claude", ".claude/skills"],
        artifactTypeFilter: "all_files",
      }),
    );
    const { unmount } = render(<LibraryHost />);
    await screen.findByText("onboarding.md");
    expect(selectorValue("Filter by type")).toBe("files");

    unmount();
    setLoadTree(baseTree(), panelState());
    render(<LibraryHost />);

    expect(await screen.findByText(".claude")).toBeInTheDocument();
    expect(screen.queryByText("skills")).not.toBeInTheDocument();
    expect(screen.queryByText("onboarding.md")).not.toBeInTheDocument();
    expect(selectorValue("Filter by type")).toBe("artifacts");
    expect(screen.getByLabelText("Filter tree")).toHaveValue("");
  });
});

// The filter controls are interactive from the first paint, so a keystroke can
// land before the restore resolves. What the user just did must win.
describe("restore racing user interaction", () => {
  it("keeps a filter the user typed before the restore landed", async () => {
    let releaseState: (state: LibraryPanelState) => void = () => {};
    const stateLoad = new Promise<LibraryPanelState>((resolve) => {
      releaseState = resolve;
    });
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_tree") return baseTree();
      if (cmd === "load_library_panel_state") return stateLoad;
      if (cmd === "save_library_panel_state") return undefined;
      throw new Error(`unexpected invoke ${cmd}`);
    });
    render(<LibraryHost />);

    // The tree has painted but the panel state has not arrived yet.
    await screen.findByText("AGENTS.md");
    fireEvent.change(screen.getByLabelText("Filter tree"), {
      target: { value: "agents" },
    });

    // The stored record names a different text filter and a different lens.
    releaseState(
      panelState({
        expandedPaths: [".claude"],
        artifactTypeFilter: "all_files",
        textFilter: "onboarding",
      }),
    );

    // The user's text survives; the fields they did not touch still restore.
    await waitFor(() =>
      expect(selectorValue("Filter by type")).toBe("files"),
    );
    expect(screen.getByLabelText("Filter tree")).toHaveValue("agents");
  });

  it("keeps a folder expanded before the restore landed, and the stored ones too", async () => {
    let releaseState: (state: LibraryPanelState) => void = () => {};
    const stateLoad = new Promise<LibraryPanelState>((resolve) => {
      releaseState = resolve;
    });
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_tree") return baseTree();
      if (cmd === "load_library_panel_state") return stateLoad;
      if (cmd === "save_library_panel_state") return undefined;
      throw new Error(`unexpected invoke ${cmd}`);
    });
    render(<LibraryHost />);

    // The tree has painted but the panel state has not arrived. Everything is
    // collapsed, so the only thing the user can do is expand.
    await screen.findByText("specifications");
    fireEvent.click(screen.getByText("specifications"));
    expect(screen.getByText("LIB-library.md")).toBeInTheDocument();

    releaseState(panelState({ expandedPaths: [".claude", ".claude/skills"] }));

    // Both survive: the stored folders open, and the one the user clicked
    // stays open rather than being replaced by the restore.
    expect(await screen.findByText("onboarding.md")).toBeInTheDocument();
    expect(screen.getByText("LIB-library.md")).toBeInTheDocument();

    // And the merged result is what gets persisted — the merge has to reach the
    // store, not just the DOM, or the next open loses one half of it. Editing
    // the text filter forces a write without disturbing the expanded set.
    fireEvent.change(screen.getByLabelText("Filter tree"), {
      target: { value: "md" },
    });
    await waitFor(
      () =>
        expect(lastSave()?.expandedPaths).toEqual([
          ".claude",
          ".claude/skills",
          "specifications",
        ]),
      SAVE_TIMEOUT,
    );
  });
});
