import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { useShellSession } from "./useShellSession";
import { HOME_TAB_TARGET } from "../types";
import type {
  DiffTarget,
  WorktreeContext,
} from "../types";
import { resetPanelReveals } from "../state/panelReveal";
import { resetDraftProposals } from "../state/draftProposals";
import {
  FLOW_BODY,
  seedDirty,
} from "../test/shellSessionFixtures";

// `useShellSession` is UI state plus the artifact write path (EDT-FR-31..33),
// so `invoke` is mocked for the saves a teardown performs. These tests pin the
// per-project shell behaviors that were extracted out of App and are not
// exercised end-to-end by the App integration tests.
const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

// TAB-FR-21: the strip emits its own DEBUG records. Mocked so a test can assert
// on them — the real module batches through `invoke`, which would make the
// assertion about transport timing rather than about what was reported.
const logDebugMock = vi.fn();
// CHG-FR-61: a save that failed while a rollback was being prepared is reported
// as one ERROR record. Mocked for the same reason `logDebug` is — asserting on
// the real module would be asserting about batching rather than about what the
// feature reported.
const logErrorMock = vi.fn();
vi.mock("../logging", () => ({
  logDebug: (...args: unknown[]) => logDebugMock(...args),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: (...args: unknown[]) => logErrorMock(...args),
  flushLogs: vi.fn(),
}));

// The hook keeps the store's external-change watch alive (EXC-FR-LKHZ), which
// subscribes through the real `listen` unless it is stubbed. Left unmocked it
// throws asynchronously on every mount — the suite still reports its tests as
// passing while the run itself fails, which is exactly the kind of false
// positive that hides a broken subscription.
// WTS-FR-25 / WTS-FR-32: the shell subscribes to `"worktree-context-changed"` so
// the chrome label follows a checkout made anywhere, and to `"branches-changed"`
// so it follows a branch set that was re-read. The mock records the channel name
// with each handler so a test fires exactly one of them — firing both at once
// would let a handler registered on the wrong channel pass. Left unmocked,
// `listen` throws asynchronously on every mount and the suite reports false
// passes.
type EventHandler = (ev: { payload: unknown }) => void;
let eventHandlers: [string, EventHandler][] = [];
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: EventHandler) => {
    eventHandlers.push([name, handler]);
    return Promise.resolve(() => {
      eventHandlers = eventHandlers.filter(([, h]) => h !== handler);
    });
  },
}));
/** Fire one channel's handlers, leaving every other channel's untouched. */
const fireEvent = (name: string, payload: unknown) =>
  eventHandlers
    .filter(([n]) => n === name)
    .forEach(([, h]) => h({ payload }));
/** Artifact ids whose `save_artifact_contents` should reject. */
let failWrites = new Set<string>();
beforeEach(() => {
  // Module-level, like the preferences cache: reveal nonces are minted from
  // one counter for the app's life, so a suite that does not reset finds its
  // first request already consumed by the previous one.
  resetPanelReveals();
  // Module-level for the same reason, and holding what the shell's teardown is
  // asserted to discard (DCR-FR-33).
  resetDraftProposals();
  eventHandlers = [];
  failWrites = new Set();
  invokeMock.mockReset();
  logErrorMock.mockReset();
  invokeMock.mockImplementation(
    async (cmd: string, args?: { id?: string }) => {
      if (cmd === "save_artifact_contents") {
        if (args?.id && failWrites.has(args.id)) throw new Error("disk full");
        return { checksum: "ck-saved" };
      }
      // FLO-FR-03: a Flow tab's first open deserializes the file's body, which
      // it reads through the same operation an artifact does.
      // FGV-FR-02 / FLO-FR-46: the backend judges the body before the canvas
      // renders it; the rules themselves are covered by the Rust suite.
      if (cmd === "validate_flow_document") return { valid: true, violations: [] };
      if (cmd === "load_artifact_contents_by_id") {
        return { body: FLOW_BODY, checksum: "ck-flow" };
      }
      return undefined;
    },
  );
});
const savedIds = () =>
  invokeMock.mock.calls
    .filter((c) => c[0] === "save_artifact_contents")
    .map((c) => (c[1] as { id: string }).id);
afterEach(cleanup);


// TAB-FR-19 / TAB-FR-20 / TAB-FR-21: a file that no longer exists holds no tab.
// The strip learns which paths went away from `"project tree changed"`
// (ASC-FR-22) and drops the tabs that named them, writing nothing on the way out.
describe("useShellSession closes tabs for removed paths (TAB-FR-19)", () => {
  const uncommitted = (path: string): DiffTarget => ({
    path,
    name: path.split("/").pop()!,
    scope: { kind: "path", path },
    comparisonLabel: "uncommitted",
  });

  const treeChanged = (removedPaths: string[]) =>
    fireEvent("project-tree-changed", {
      changeCount: removedPaths.length,
      removedPaths,
    });

  it("closes every tab naming a removed path, including Diff tabs, and leaves the rest", async () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.openArtifact({ id: "docs/a.md", name: "a.md" }));
    act(() => result.current.openArtifact({ id: "src/x.ts", name: "x.ts" }));
    act(() => result.current.openDiff(uncommitted("docs/a.md")));

    await waitFor(() => expect(eventHandlers.some(([n]) => n === "project-tree-changed")).toBe(true));
    act(() => treeChanged(["docs/a.md"]));

    await waitFor(() => {
      const ids = result.current.tabs.map((t) => t.id);
      // Both the Editor tab and the Diff tab on that path are gone…
      expect(ids).not.toContain("art:docs/a.md");
      expect(result.current.tabs.some((t) => t.kind === "diff")).toBe(false);
      // …and the unrelated tab is untouched.
      expect(ids).toContain("art:src/x.ts");
    });
  });

  // A removed folder takes what was beneath it without the backend enumerating
  // the subtree — the path boundary is what makes that work, and `docs2` proves
  // it is a boundary rather than a string prefix.
  it("closes tabs beneath a removed folder but not siblings that merely share a prefix", async () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.openArtifact({ id: "docs/a.md", name: "a.md" }));
    act(() => result.current.openArtifact({ id: "docs/sub/b.md", name: "b.md" }));
    act(() => result.current.openArtifact({ id: "docs2/c.md", name: "c.md" }));

    await waitFor(() => expect(eventHandlers.some(([n]) => n === "project-tree-changed")).toBe(true));
    act(() => treeChanged(["docs"]));

    await waitFor(() => {
      const ids = result.current.tabs.map((t) => t.id);
      expect(ids).not.toContain("art:docs/a.md");
      expect(ids).not.toContain("art:docs/sub/b.md");
      expect(ids).toContain("art:docs2/c.md");
    });
  });

  // TAB-FR-20: the write-before-close of TAB-FR-10 does not apply — writing
  // would recreate the file the user just deleted.
  it("writes nothing for a dirty tab whose file was removed", async () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.openArtifact({ id: "docs/a.md", name: "a.md" }));
    act(() => {
      result.current.sessions.adoptLoad("docs/a.md", "on disk", "sha-1");
      result.current.sessions.update("docs/a.md", {
        buffer: "unsaved edit",
        dirty: true,
      });
    });
    expect(result.current.sessions.get("docs/a.md")?.dirty).toBe(true);

    invokeMock.mockClear();
    await waitFor(() => expect(eventHandlers.some(([n]) => n === "project-tree-changed")).toBe(true));
    act(() => treeChanged(["docs/a.md"]));

    await waitFor(() =>
      expect(result.current.tabs.map((t) => t.id)).not.toContain("art:docs/a.md"),
    );
    expect(savedIds()).toEqual([]);
    // The retained editing session goes with the tab, so a file later recreated
    // at this path opens from disk rather than resuming the discarded edit.
    expect(result.current.sessions.get("docs/a.md")).toBeUndefined();
  });

  // TAB-FR-15 still holds: the strip never empties, however it was emptied.
  it("falls back to the Dashboard when the removals empty the strip", async () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.openArtifact({ id: "docs/a.md", name: "a.md" }));
    await act(async () => {
      await result.current.closeTab("dashboard");
    });
    expect(result.current.tabs.map((t) => t.id)).toEqual(["art:docs/a.md"]);

    await waitFor(() => expect(eventHandlers.some(([n]) => n === "project-tree-changed")).toBe(true));
    act(() => treeChanged(["docs/a.md"]));

    await waitFor(() => {
      expect(result.current.tabs.map((t) => t.id)).toEqual(["dashboard"]);
      expect(result.current.activeTab).toBe("dashboard");
    });
  });

  // TAB-FR-21: one DEBUG record per closed tab, naming the path and the tab
  // kind, and saying so when unsaved work was discarded — the one thing here a
  // user could otherwise not account for, since nothing prompted them.
  it("logs one DEBUG record per auto-closed tab and flags discarded work", async () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.openArtifact({ id: "docs/a.md", name: "a.md" }));
    act(() => result.current.openArtifact({ id: "docs/b.md", name: "b.md" }));
    act(() => {
      result.current.sessions.adoptLoad("docs/a.md", "on disk", "sha-1");
      result.current.sessions.update("docs/a.md", {
        buffer: "unsaved edit",
        dirty: true,
      });
    });

    logDebugMock.mockClear();
    await waitFor(() => expect(eventHandlers.some(([n]) => n === "project-tree-changed")).toBe(true));
    act(() => treeChanged(["docs"]));

    await waitFor(() => expect(logDebugMock).toHaveBeenCalledTimes(2));
    const byPath = new Map(
      logDebugMock.mock.calls.map((c) => [
        (c[2] as { path: string }).path,
        c[2] as { tabKind: string; discardedUnsavedChanges: boolean },
      ]),
    );
    expect([...byPath.keys()].sort()).toEqual(["docs/a.md", "docs/b.md"]);
    expect(byPath.get("docs/a.md")?.tabKind).toBe("editor");
    // The dirty one says its work was dropped; the clean one says it was not.
    expect(byPath.get("docs/a.md")?.discardedUnsavedChanges).toBe(true);
    expect(byPath.get("docs/b.md")?.discardedUnsavedChanges).toBe(false);
    // Every record is a frontend-domain DEBUG.
    for (const call of logDebugMock.mock.calls) {
      expect(call[0]).toEqual(["frontend"]);
    }
  });

  // A close the user performed themselves is not this event and produces no
  // such record — otherwise the log could not tell the two apart.
  it("logs nothing when the user closes a tab themselves", async () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.openArtifact({ id: "docs/a.md", name: "a.md" }));
    logDebugMock.mockClear();
    await act(async () => {
      await result.current.closeTab("art:docs/a.md");
    });

    expect(logDebugMock).not.toHaveBeenCalled();
  });

  // The Flow branch of the close loop (`flows.closeTab`) and the draft-tab
  // exemption are otherwise unexercised: a draft is keyed by `draftId` and has
  // no path (TAB-FR-17), so `tabPath` returns null and it must survive. Adding
  // `?? t.draftId` to `tabPath` would silently close drafts — this catches that.
  it("closes a removed Flow tab and leaves draft tabs alone", async () => {
    const { result } = renderHook(() => useShellSession());

    act(() =>
      result.current.openArtifact({
        id: "flows/pipeline.flow",
        name: "pipeline.flow",
        artifactType: "flow",
      }),
    );
    act(() => result.current.openDraft({ id: "d1", name: "Untitled" }));
    expect(result.current.tabs.some((t) => t.kind === "flow")).toBe(true);
    expect(result.current.tabs.some((t) => t.kind === "draft")).toBe(true);

    await waitFor(() => expect(eventHandlers.some(([n]) => n === "project-tree-changed")).toBe(true));
    act(() => treeChanged(["flows/pipeline.flow"]));

    await waitFor(() => {
      expect(result.current.tabs.some((t) => t.kind === "flow")).toBe(false);
      // The draft tab is untouched — it names no path.
      expect(result.current.tabs.some((t) => t.kind === "draft")).toBe(true);
    });
  });

  it("leaves every tab alone when the burst removed nothing", async () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.openArtifact({ id: "docs/a.md", name: "a.md" }));
    const before = result.current.tabs.map((t) => t.id);

    await waitFor(() => expect(eventHandlers.some(([n]) => n === "project-tree-changed")).toBe(true));
    act(() => treeChanged([]));

    expect(result.current.tabs.map((t) => t.id)).toEqual(before);
  });
});


// ---------------------------------------------------------------------------
// TAB-FR-34 through TAB-FR-39: pinning, and the tab context menu's mass closes.
// ---------------------------------------------------------------------------

describe("tab pinning and mass closes (TAB-FR-34 / TAB-FR-37 / TAB-FR-38)", () => {
  /**
   * Open `names` as Editor tabs, in order, on top of the Dashboard the strip
   * starts with. Returns the hook plus the tab ids, so a test can talk in
   * `a.md` while asserting against the `art:` ids the strip keys by.
   */
  const openTabs = (names: string[]) => {
    const { result } = renderHook(() => useShellSession());
    for (const name of names) {
      act(() => result.current.openArtifact({ id: name, name }));
    }
    return { result, id: (name: string) => `art:${name}` };
  };

  const ids = (result: { current: ReturnType<typeof useShellSession> }) =>
    result.current.tabs.map((t) => t.id);

  // TAB-FR-32, TAB-FR-33, TAB-FR-34, TAB-FR-35: pinning changes that tab's pinned state and nothing else — not
  // the active tab, not the order of the strip.
  it("pins and unpins without changing the active tab or the strip's order", () => {
    const { result, id } = openTabs(["a.md", "b.md", "c.md"]);
    const orderBefore = ids(result);
    act(() => result.current.activateTab(id("c.md")));

    act(() => result.current.setTabPinned(id("b.md"), true));

    expect(result.current.pinnedTabs.has(id("b.md"))).toBe(true);
    expect(result.current.pinnedTabs.has(id("a.md"))).toBe(false);
    expect(result.current.activeTab).toBe(id("c.md"));
    expect(ids(result)).toEqual(orderBefore);

    act(() => result.current.setTabPinned(id("b.md"), false));

    expect(result.current.pinnedTabs.has(id("b.md"))).toBe(false);
    expect(result.current.activeTab).toBe(id("c.md"));
    expect(ids(result)).toEqual(orderBefore);
  });

  // TAB-FR-37: pinned tabs on both sides of the target survive; the target
  // survives whichever direction is closed.
  it("closes to the left and to the right, keeping pinned tabs and the target", async () => {
    const { result, id } = openTabs(["a.md", "b.md", "c.md", "d.md", "e.md", "f.md", "g.md"]);
    act(() => {
      result.current.setTabPinned(id("b.md"), true);
      result.current.setTabPinned(id("f.md"), true);
      // The Dashboard sits at the head of the strip, and is unpinned — pin it
      // so this test is about a.md and c.md rather than about the Dashboard.
      result.current.setTabPinned("dashboard", true);
    });

    await act(async () => {
      await result.current.closeTabGroup("left", id("d.md"));
    });

    expect(ids(result)).toEqual([
      "dashboard",
      id("b.md"),
      id("d.md"),
      id("e.md"),
      id("f.md"),
      id("g.md"),
    ]);

    await act(async () => {
      await result.current.closeTabGroup("right", id("d.md"));
    });

    expect(ids(result)).toEqual([
      "dashboard",
      id("b.md"),
      id("d.md"),
      id("f.md"),
    ]);
  });

  // TAB-FR-37: the target survives whether or not it is itself pinned.
  it("closes other tabs, keeping the target and every pinned tab", async () => {
    const run = async (targetPinned: boolean) => {
      const { result, id } = openTabs(["b.md", "d.md", "e.md", "g.md"]);
      act(() => {
        result.current.setTabPinned(id("b.md"), true);
        result.current.setTabPinned("dashboard", true);
        if (targetPinned) result.current.setTabPinned(id("d.md"), true);
      });

      await act(async () => {
        await result.current.closeTabGroup("others", id("d.md"));
      });

      return ids(result);
    };

    expect(await run(false)).toEqual(["dashboard", "art:b.md", "art:d.md"]);
    expect(await run(true)).toEqual(["dashboard", "art:b.md", "art:d.md"]);
  });

  // TAB-FR-34 / TAB-FR-39: enablement is the eligible set, never the clicked
  // tab's position — a first tab with unpinned tabs after it can close right,
  // and a middle tab whose neighbours are all pinned can close nothing.
  it("reports an empty eligible set exactly when a mass close cannot act", () => {
    const { result, id } = openTabs(["a.md", "b.md", "c.md", "d.md"]);
    act(() => {
      result.current.setTabPinned("dashboard", true);
      result.current.setTabPinned(id("a.md"), true);
      result.current.setTabPinned(id("b.md"), true);
      result.current.setTabPinned(id("d.md"), true);
    });

    const target = id("c.md");
    expect(result.current.massCloseTargets("left", target)).toEqual([]);
    expect(result.current.massCloseTargets("right", target)).toEqual([]);
    expect(result.current.massCloseTargets("others", target)).toEqual([]);

    act(() => result.current.setTabPinned(id("d.md"), false));

    expect(result.current.massCloseTargets("left", target)).toEqual([]);
    expect(result.current.massCloseTargets("right", target)).toEqual([id("d.md")]);
    expect(result.current.massCloseTargets("others", target)).toEqual([id("d.md")]);
  });

  // TAB-FR-37: the context-clicked tab is never eligible, pinned or not.
  it("never lists the context-clicked tab among a mass close's targets", () => {
    const { result, id } = openTabs(["a.md", "b.md"]);
    for (const scope of ["left", "right", "others"] as const) {
      expect(result.current.massCloseTargets(scope, id("a.md"))).not.toContain(
        id("a.md"),
      );
    }
  });

  // TAB-FR-38, TAB-FR-11, TAB-FR-21: a refusal does not end the pass, and the first refusal in strip
  // order takes the focus with its blocker visible.
  it("keeps refusing tabs open, closes the rest, and focuses the first refusal", async () => {
    const { result, id } = openTabs(["a.md", "b.md", "c.md", "d.md"]);
    // a.md's write fails; b.md is showing an unresolved external change; c.md
    // is dirty but writes cleanly.
    failWrites.add("a.md");
    seedDirty(result.current.sessions, "a.md", "v1");
    seedDirty(result.current.sessions, "b.md", "v1");
    seedDirty(result.current.sessions, "c.md", "v1");
    act(() =>
      result.current.sessions.update("b.md", { conflict: true, pending: "ck2" }),
    );
    act(() => {
      result.current.setTabPinned("dashboard", true);
      result.current.activateTab(id("c.md"));
    });

    await act(async () => {
      await result.current.closeTabGroup("left", id("d.md"));
    });

    // c.md was written and closed; the two blocked tabs stand.
    expect(savedIds()).toContain("c.md");
    expect(ids(result)).toEqual([
      "dashboard",
      id("a.md"),
      id("b.md"),
      id("d.md"),
    ]);
    expect(result.current.activeTab).toBe(id("a.md"));

    // Resolved, the same action takes them.
    failWrites.delete("a.md");
    act(() =>
      result.current.sessions.update("b.md", { conflict: false, pending: null }),
    );
    await act(async () => {
      await result.current.closeTabGroup("left", id("d.md"));
    });

    expect(ids(result)).toEqual(["dashboard", id("d.md")]);
  });

  // TAB-FR-38: where the previously-active tab was closed, the context-clicked
  // tab takes the focus; where it survived, the active tab does not move.
  it("lands focus on the context-clicked tab only when the active tab was closed", async () => {
    const closed = openTabs(["a.md", "b.md", "c.md"]);
    act(() => closed.result.current.activateTab(closed.id("a.md")));
    await act(async () => {
      await closed.result.current.closeTabGroup("left", closed.id("c.md"));
    });
    expect(closed.result.current.activeTab).toBe(closed.id("c.md"));

    const survived = openTabs(["a.md", "b.md", "c.md"]);
    act(() => survived.result.current.activateTab(survived.id("c.md")));
    await act(async () => {
      await survived.result.current.closeTabGroup("left", survived.id("c.md"));
    });
    expect(survived.result.current.activeTab).toBe(survived.id("c.md"));
  });

  // TAB-FR-15, TAB-FR-34, TAB-FR-36: **Close tab** on the last remaining tab still brings the
  // Dashboard back, pinning having guarded it from no close of its own.
  it("preserves the never-empty invariant when the last tab is pinned", async () => {
    const { result, id } = openTabs(["spec.md"]);
    await act(async () => {
      await result.current.closeTab("dashboard");
    });
    act(() => result.current.setTabPinned(id("spec.md"), true));
    expect(ids(result)).toEqual([id("spec.md")]);

    await act(async () => {
      await result.current.closeTab(id("spec.md"));
    });

    expect(ids(result)).toEqual(["dashboard"]);
    expect(result.current.activeTab).toBe("dashboard");
  });

  // TAB-FR-19, TAB-FR-22, TAB-FR-35, TAB-FR-14 / TAB-FR-34: pinning guards a tab against the author's own bulk
  // gesture and against nothing else — the automatic closures run over it.
  it("does not protect a pinned tab from the removed-path closure", async () => {
    const { result, id } = openTabs(["docs/a.md"]);
    act(() => result.current.setTabPinned(id("docs/a.md"), true));
    await waitFor(() =>
      expect(eventHandlers.some(([n]) => n === "project-tree-changed")).toBe(
        true,
      ),
    );

    act(() =>
      fireEvent("project-tree-changed", {
        changeCount: 1,
        removedPaths: ["docs"],
      }),
    );

    await waitFor(() => expect(ids(result)).toEqual(["dashboard"]));
  });

  // TAB-FR-19, TAB-FR-22, TAB-FR-34, TAB-FR-35, TAB-FR-14, second half: a pinned Diff tab closes on the commit that
  // included its file, exactly as an unpinned one does (TAB-FR-22).
  it("does not protect a pinned Diff tab from the commit closure", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() =>
      result.current.openDiff({
        path: "SKILL.md",
        name: "SKILL.md",
        scope: { kind: "path", path: "SKILL.md" },
        comparisonLabel: "uncommitted",
      }),
    );
    const diffId = result.current.tabs.find((t) => t.kind === "diff")!.id;
    act(() => result.current.setTabPinned(diffId, true));

    await act(async () => {
      await result.current.closeDiffTabsForCommittedPaths(["SKILL.md"]);
    });

    expect(result.current.tabs.some((t) => t.kind === "diff")).toBe(false);
  });

  // TAB-FR-35: pinned state belongs to the tab and is dropped with it, so
  // reopening the same target opens an unpinned tab.
  it("drops a tab's pinned state when the tab closes, however it closed", async () => {
    const { result, id } = openTabs(["a.md", "b.md"]);
    act(() => {
      result.current.setTabPinned(id("a.md"), true);
      result.current.setTabPinned(id("b.md"), true);
    });

    await act(async () => {
      await result.current.closeTab(id("a.md"));
    });
    await waitFor(() =>
      expect(result.current.pinnedTabs.has(id("a.md"))).toBe(false),
    );
    // The tab that is still open keeps its pin.
    expect(result.current.pinnedTabs.has(id("b.md"))).toBe(true);

    act(() => result.current.openArtifact({ id: "a.md", name: "a.md" }));
    expect(result.current.pinnedTabs.has(id("a.md"))).toBe(false);
  });
});


// ---------------------------------------------------------------------------
// The remaining halves of TAB-FR-35 and TAB-FR-37 that the tests above isolate
// away from: pins across a teardown, and mass closes over the Dashboard the
// real strip actually starts with.
// ---------------------------------------------------------------------------

describe("pinned state across a teardown, and mass closes over the Dashboard", () => {
  /** The worktree the switch below lands in. */
  const switchedContext: WorktreeContext = {
    repositoryRoot: "~/dev/acme",
    activeWorktreePath: "~/dev/acme-main",
    worktrees: [
      {
        path: "~/dev/acme-main",
        name: "acme-main",
        branch: "main",
        headShortHash: "4f2a10c",
        isDetached: false,
        isActive: true,
        isPrimary: true,
        isMissing: false,
      },
    ],
    branches: [],
  };

  const openTabs = (names: string[]) => {
    const { result } = renderHook(() => useShellSession());
    for (const name of names) {
      act(() => result.current.openArtifact({ id: name, name }));
    }
    return { result, id: (name: string) => `art:${name}` };
  };
  const ids = (result: { current: ReturnType<typeof useShellSession> }) =>
    result.current.tabs.map((t) => t.id);

  // TAB-FR-35 / TAB-FR-19, TAB-FR-22, TAB-FR-34, TAB-FR-14: nothing about a pinned tab survives a worktree
  // change. The Dashboard is the case that nearly slipped through — the
  // teardown puts a fresh one back under the same id, so a prune keyed on ids
  // alone would have left its pin standing.
  it("drops every pin on a worktree change, the Dashboard's included", async () => {
    const { result, id } = openTabs(["a.md"]);
    act(() => {
      result.current.setTabPinned("dashboard", true);
      result.current.setTabPinned(id("a.md"), true);
    });
    expect(result.current.pinnedTabs.size).toBe(2);

    await act(async () => {
      await result.current.switchWorktree(async () => switchedContext);
    });

    expect(ids(result)).toEqual(["dashboard"]);
    expect(result.current.pinnedTabs.size).toBe(0);
  });

  // TAB-FR-37: the Dashboard is an ordinary unpinned tab to a mass close. The
  // three tests above pin it for isolation, so the default strip — Dashboard
  // unpinned at the leading position — is only exercised here.
  it("closes an unpinned Dashboard like any other tab, and the Home affordance takes its place", async () => {
    const { result, id } = openTabs(["a.md", "b.md"]);

    await act(async () => {
      await result.current.closeTabGroup("left", id("b.md"));
    });

    // Dashboard and a.md both stood to the left of b.md and neither was pinned.
    expect(ids(result)).toEqual([id("b.md")]);
    expect(result.current.activeTab).toBe(id("b.md"));
  });

  // TAB-FR-39: an empty eligible set closes nothing and moves nothing, rather
  // than falling through to a pass over zero tabs that still settles focus.
  it("leaves the strip and the active tab untouched when nothing is eligible", async () => {
    const { result, id } = openTabs(["a.md", "b.md"]);
    act(() => {
      result.current.setTabPinned("dashboard", true);
      result.current.setTabPinned(id("a.md"), true);
      result.current.activateTab(id("a.md"));
    });
    const before = ids(result);

    await act(async () => {
      await result.current.closeTabGroup("left", id("b.md"));
    });

    expect(ids(result)).toEqual(before);
    expect(result.current.activeTab).toBe(id("a.md"));
  });

  // TAB-FR-21: a mass close is the user closing tabs themselves, so it writes
  // no log record for any of them — unlike the automatic closures, which do.
  it("writes no log record for a mass close", async () => {
    const { result, id } = openTabs(["a.md", "b.md", "c.md"]);
    logDebugMock.mockClear();

    await act(async () => {
      await result.current.closeTabGroup("left", id("c.md"));
    });

    expect(ids(result)).toEqual([id("c.md")]);
    expect(logDebugMock).not.toHaveBeenCalled();
  });
});


// ---------------------------------------------------------------------------
// TAB-FR-40: a mass close asked for from the Home affordance, which has no tab
// id but does have a position — the head of the strip.
// ---------------------------------------------------------------------------

describe("mass closes from the Home affordance (TAB-FR-40)", () => {
  const openTabs = (names: string[]) => {
    const { result } = renderHook(() => useShellSession());
    for (const name of names) {
      act(() => result.current.openArtifact({ id: name, name }));
    }
    return { result, id: (name: string) => `art:${name}` };
  };
  const ids = (result: { current: ReturnType<typeof useShellSession> }) =>
    result.current.tabs.map((t) => t.id);

  // Standing before every tab: nothing on its left, and the whole strip on its
  // right — which is the same set **Close other tabs** takes from there.
  it("computes its eligible set from the head of the strip", async () => {
    const { result, id } = openTabs(["a.md", "b.md"]);
    await act(async () => {
      await result.current.closeTab("dashboard");
    });
    act(() => result.current.setTabPinned(id("b.md"), true));

    expect(result.current.massCloseTargets("left", HOME_TAB_TARGET)).toEqual([]);
    expect(result.current.massCloseTargets("right", HOME_TAB_TARGET)).toEqual([
      id("a.md"),
    ]);
    expect(result.current.massCloseTargets("others", HOME_TAB_TARGET)).toEqual([
      id("a.md"),
    ]);
  });

  // TAB-FR-40, TAB-FR-39, TAB-FR-15, SNV-FR-09, second clause: the unpinned tabs go and the pinned one stays.
  it("closes every unpinned tab and keeps the pinned ones", async () => {
    const { result, id } = openTabs(["a.md", "b.md", "c.md"]);
    await act(async () => {
      await result.current.closeTab("dashboard");
    });
    act(() => result.current.setTabPinned(id("b.md"), true));

    await act(async () => {
      await result.current.closeTabGroup("right", HOME_TAB_TARGET);
    });

    expect(ids(result)).toEqual([id("b.md")]);
    expect(result.current.activeTab).toBe(id("b.md"));
  });

  // TAB-FR-40, TAB-FR-39, TAB-FR-15, SNV-FR-09, third clause: with nothing pinned the strip empties, so the
  // never-empty invariant puts the Dashboard back and the affordance gives way.
  it("brings the Dashboard back when it empties the strip", async () => {
    const { result } = openTabs(["a.md", "b.md"]);
    await act(async () => {
      await result.current.closeTab("dashboard");
    });

    await act(async () => {
      await result.current.closeTabGroup("others", HOME_TAB_TARGET);
    });

    expect(ids(result)).toEqual(["dashboard"]);
    expect(result.current.activeTab).toBe("dashboard");
  });
});
