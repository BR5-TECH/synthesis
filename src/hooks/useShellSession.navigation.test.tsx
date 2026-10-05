import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook } from "@testing-library/react";
import { useShellSession } from "./useShellSession";
import { addNode } from "../state/flowDocument";
import {
  resetSelectionFollowsTab,
  setSelectionFollowsTab,
} from "../state/selectionFollowsTab";
import type {
  DiffTarget,
  WorktreeContext,
} from "../types";
import { resetPanelReveals } from "../state/panelReveal";
import { resetDraftProposals } from "../state/draftProposals";
import {
  FLOW_BODY,
  FLOW_ID,
  handle,
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


// SNV-FR-09 / SNV-FR-41 / TAB-FR-08: the Dashboard closes like any other tab,
// and the Home affordance is what brings it back — at the head of the strip.
describe("useShellSession.goHome (SNV-FR-41 / TAB-FR-08)", () => {
  // TAB-FR-16, TAB-FR-08 / DSH-FR-02 at the state level: the close request the strip greys
  // out is refused here too, so the Dashboard alone in the strip stays put.
  it("refuses to close the Dashboard while it is the only open tab", async () => {
    const { result } = renderHook(() => useShellSession());
    const before = result.current.tabs;

    await act(async () => {
      await result.current.closeTab("dashboard");
    });

    expect(result.current.tabs.map((t) => t.id)).toEqual(["dashboard"]);
    expect(result.current.activeTab).toBe("dashboard");
    // Identity, deliberately: TAB-FR-16 is a *refusal*, and the end state it
    // produces is indistinguishable from the close-and-reopen TAB-FR-15 would
    // do without it — same tab id, same focus. An untouched array is what says
    // no close ran at all, which is the difference between a refusal and a
    // flicker.
    expect(result.current.tabs).toBe(before);
  });

  // The other half of TAB-FR-16, TAB-FR-08: with something else open the control is live
  // again, and closing the Dashboard leaves that tab alone in the strip.
  it("closes the Dashboard again once another tab is open", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));

    await act(async () => {
      await result.current.closeTab("dashboard");
    });

    expect(result.current.tabs.map((t) => t.id)).toEqual(["art:a/x.md"]);
    expect(result.current.activeTab).toBe("art:a/x.md");
  });

  // TAB-FR-15, SNV-FR-09 / DSH-FR-10: the requested behaviour. The Dashboard is closed and
  // one Editor tab is all that is left; closing it brings the Dashboard back in
  // the leading position, focused, rather than emptying the strip.
  it("reopens the Dashboard focused when the last remaining tab closes", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));
    await act(async () => {
      await result.current.closeTab("dashboard");
    });
    expect(result.current.tabs.map((t) => t.id)).toEqual(["art:a/x.md"]);

    await act(async () => {
      await result.current.closeTab("art:a/x.md");
    });

    expect(result.current.tabs.map((t) => t.id)).toEqual(["dashboard"]);
    expect(result.current.activeTab).toBe("dashboard");
  });

  // TAB-FR-15 holds however the strip was emptied — including a close that had
  // to write first (TAB-FR-10). The write still lands, and the fallback runs
  // after it rather than instead of it.
  it("reopens the Dashboard after the last tab's pending write lands", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));
    await act(async () => {
      await result.current.closeTab("dashboard");
    });
    seedDirty(result.current.sessions, "a/x.md", "v1");

    await act(async () => {
      await result.current.closeTab("art:a/x.md");
    });

    expect(savedIds()).toEqual(["a/x.md"]);
    expect(result.current.tabs.map((t) => t.id)).toEqual(["dashboard"]);
    expect(result.current.activeTab).toBe("dashboard");
  });

  // TAB-FR-11 outranks the fallback: a refused close leaves the tab open and
  // focused, so the strip was never emptied and no Dashboard is opened beside
  // the blocker.
  it("opens no Dashboard when the last tab's close is refused", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));
    await act(async () => {
      await result.current.closeTab("dashboard");
    });
    seedDirty(result.current.sessions, "a/x.md", "v1");
    act(() =>
      result.current.sessions.update("a/x.md", {
        conflict: true,
        pending: "ck2",
      }),
    );

    await act(async () => {
      await result.current.closeTab("art:a/x.md");
    });

    expect(savedIds()).toEqual([]);
    expect(result.current.tabs.map((t) => t.id)).toEqual(["art:a/x.md"]);
    expect(result.current.activeTab).toBe("art:a/x.md");
  });

  // The other refusal source: the write itself fails (EDT-FR-32) rather than a
  // divergence blocking it. Same branch, and the fallback must stay out of it.
  it("opens no Dashboard when the last tab's write fails", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));
    await act(async () => {
      await result.current.closeTab("dashboard");
    });
    seedDirty(result.current.sessions, "a/x.md", "v1");
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "save_artifact_contents"
        ? Promise.reject(new Error("disk full"))
        : Promise.resolve(null),
    );

    await act(async () => {
      await result.current.closeTab("art:a/x.md");
    });

    expect(result.current.tabs.map((t) => t.id)).toEqual(["art:a/x.md"]);
    expect(result.current.activeTab).toBe("art:a/x.md");
  });

  /** A dirty Flow as the only open tab, the strip's last one. */
  const onlyDirtyFlowTab = async () => {
    const { result } = renderHook(() => useShellSession());
    await act(async () => {
      result.current.openArtifact({
        id: FLOW_ID,
        name: "h.flow",
        artifactType: "flow",
      });
      await result.current.flows.get(FLOW_ID)?.pendingLoad;
    });
    act(() =>
      result.current.flows.applyEdit(FLOW_ID, (d) =>
        addNode(d, { x: 10, y: 10 }),
      ),
    );
    await act(async () => {
      await result.current.closeTab("dashboard");
    });
    return result;
  };

  // TAB-FR-15 holds for a Flow tab too (TAB-FR-12): the Flow is written, the
  // tab's session is dropped, and the Dashboard takes the empty strip's place.
  it("reopens the Dashboard when the last tab is a Flow tab", async () => {
    const result = await onlyDirtyFlowTab();

    await act(async () => {
      await result.current.closeTab(`art:${FLOW_ID}`);
    });

    expect(savedIds()).toEqual([FLOW_ID]);
    expect(result.current.tabs.map((t) => t.id)).toEqual(["dashboard"]);
    expect(result.current.activeTab).toBe("dashboard");
    // FLO-FR-28: nothing of the Flow outlives its tab, fallback or not.
    expect(result.current.flows.get(FLOW_ID)).toBeUndefined();
  });

  // TAB-FR-13: a Flow tab whose write fails refuses the close, so no fallback.
  it("opens no Dashboard when the last Flow tab's write fails", async () => {
    const result = await onlyDirtyFlowTab();
    failWrites.add(FLOW_ID);

    await act(async () => {
      await result.current.closeTab(`art:${FLOW_ID}`);
    });

    expect(result.current.tabs.map((t) => t.id)).toEqual([`art:${FLOW_ID}`]);
    expect(result.current.activeTab).toBe(`art:${FLOW_ID}`);
    expect(result.current.flows.get(FLOW_ID)?.dirty).toBe(true);
  });

  // SCH-FR-20: the last tab being a Search results tab must still drop its
  // recorded result set on the way out — the fallback runs after that cleanup,
  // not instead of it.
  it("drops a last Search results tab's result set before falling back", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openSearchResults("needle", "smart_case"));
    const searchTabId = "search:smart_case:needle";
    result.current.searches.save(searchTabId, {
      hits: [],
      reason: "completed",
      error: null,
    });
    await act(async () => {
      await result.current.closeTab("dashboard");
    });
    expect(result.current.searches.get(searchTabId)).toBeDefined();

    await act(async () => {
      await result.current.closeTab(searchTabId);
    });

    expect(result.current.searches.get(searchTabId)).toBeUndefined();
    expect(result.current.tabs.map((t) => t.id)).toEqual(["dashboard"]);
    expect(result.current.activeTab).toBe("dashboard");
  });

  // SWN-FR-18 / TAB-FR-15: opening or closing a settings window touches the
  // strip's never-empty invariant not at all — it creates no tab to become the
  // last one, and closes none to empty it.
  it("leaves the never-empty invariant alone when a settings window opens", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));
    await act(async () => {
      await result.current.closeTab("dashboard");
    });
    expect(result.current.tabs.map((t) => t.id)).toEqual(["art:a/x.md"]);

    act(() => result.current.openGlobalSettings());
    act(() => result.current.openSettings());

    expect(result.current.tabs.map((t) => t.id)).toEqual(["art:a/x.md"]);
    expect(result.current.activeTab).toBe("art:a/x.md");
  });

  it("reopens the Dashboard ahead of the tabs already open, and focuses it", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));
    await act(async () => {
      await result.current.closeTab("dashboard");
    });
    expect(result.current.tabs.map((t) => t.id)).toEqual(["art:a/x.md"]);

    act(() => result.current.goHome());

    expect(result.current.tabs.map((t) => t.id)).toEqual([
      "dashboard",
      "art:a/x.md",
    ]);
    expect(result.current.activeTab).toBe("dashboard");
  });

  it("focuses the existing Dashboard without duplicating it", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));
    expect(result.current.activeTab).toBe("art:a/x.md");

    act(() => result.current.goHome());

    expect(result.current.tabs.filter((t) => t.id === "dashboard")).toHaveLength(
      1,
    );
    expect(result.current.activeTab).toBe("dashboard");
  });

  // Two calls landing in one pass — the affordance double-clicked, or a second
  // caller reaching goHome — must still leave one Dashboard. Reading `tabs` from
  // the closure instead of the updater would prepend two and reproduce the
  // duplicate the Home affordance rule exists to rule out (SNV-FR-09).
  it("opens one Dashboard when called twice before a re-render", async () => {
    const { result } = renderHook(() => useShellSession());
    // Another tab has to be open for the Dashboard to be closable at all
    // (TAB-FR-16) — and it is also what keeps the strip non-empty afterwards,
    // so `goHome` reaches its prepend branch rather than the fallback's.
    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));
    await act(async () => {
      await result.current.closeTab("dashboard");
    });
    expect(result.current.tabs.map((t) => t.id)).toEqual(["art:a/x.md"]);

    act(() => {
      result.current.goHome();
      result.current.goHome();
    });

    expect(result.current.tabs.map((t) => t.id)).toEqual([
      "dashboard",
      "art:a/x.md",
    ]);
  });

  // TAB-FR-08, SNV-FR-09 at the state level: the Dashboard owns nothing savable, so closing
  // it beside dirty Editor tabs writes nothing, disturbs neither tab, and leaves
  // Save enabled on the one that was focused (SNV-FR-28).
  it("closes beside dirty Editor tabs without writing or stealing focus", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));
    act(() => result.current.openArtifact({ id: "a/y.md", name: "y.md" }));
    seedDirty(result.current.sessions, "a/x.md", "v1");
    seedDirty(result.current.sessions, "a/y.md", "v1");

    await act(async () => {
      await result.current.closeTab("dashboard");
    });

    expect(result.current.tabs.map((t) => t.id)).toEqual([
      "art:a/x.md",
      "art:a/y.md",
    ]);
    expect(result.current.activeTab).toBe("art:a/y.md");
    expect(savedIds()).toEqual([]);
    expect(result.current.saveEnabled).toBe(true);
  });

  // Closing the last Editor tab lands the author on the Dashboard (TAB-FR-15),
  // which owns nothing savable — so Save greys out (SNV-FR-28) while SNV-FR-30
  // keeps Save All on offer for the unsaved work no tab is on any more.
  it("greys out Save on the fallback Dashboard while Save All still reaches unsaved work", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));
    await act(async () => {
      await result.current.closeTab("dashboard");
    });
    seedDirty(result.current.sessions, "a/x.md", "v1");

    await act(async () => {
      await result.current.closeTab("art:a/x.md");
    });
    // Closing the Editor tab wrote its buffer (TAB-FR-10); dirty it again so
    // there is unsaved work with no tab on it at all.
    act(() => seedDirty(result.current.sessions, "a/x.md", "v2"));

    expect(result.current.activeTab).toBe("dashboard");
    expect(result.current.saveEnabled).toBe(false);
    expect(result.current.saveAllEnabled).toBe(true);
  });

  // TAB-FR-14 leaves the Dashboard as the one tab standing after a worktree
  // change — including when it was closed at the time, so the strip goes from
  // the Home affordance back to the tab.
  it("restores the Dashboard tab on a worktree switch that began with it closed", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.loadProject(handle("acme", "~/dev/acme")));
    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));
    await act(async () => {
      await result.current.closeTab("dashboard");
    });
    expect(result.current.tabs.map((t) => t.id)).toEqual(["art:a/x.md"]);

    await act(async () => {
      await result.current.switchWorktree(async () => ({
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
            isPrimary: false,
            isMissing: false,
          },
        ],
        branches: [],
      }));
    });

    expect(result.current.tabs.map((t) => t.id)).toEqual(["dashboard"]);
    expect(result.current.activeTab).toBe("dashboard");
  });
});


// ---------------------------------------------------------------------------
// Selection follows tab (SNV-FR-64 … SNV-FR-69)
// ---------------------------------------------------------------------------

/**
 * The half of the behaviour this hook owns: the **trigger** (SNV-FR-65) and the
 * reveal request it produces (SNV-FR-64 / SNV-FR-66). Whether the panel then
 * un-hides and which surface it renders is the shell component's, and the reveal
 * itself is each panel's — both tested where they live.
 *
 * The trigger is what these are about, because it is the part that cannot be
 * recovered from `activeTab` after the fact: a click on a tab and the Dashboard
 * that replaces an emptied strip look identical from there.
 */
describe("selection follows tab (SNV-FR-64 … SNV-FR-69)", () => {
  const uncommittedDiff = (path: string): DiffTarget => ({
    path,
    name: path.split("/").pop()!,
    scope: { kind: "path", path },
    comparisonLabel: "uncommitted",
  });

  afterEach(() => resetSelectionFollowsTab());

  it("SNV-FR-64: an Editor tab activation asks the Project panel for its file", () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));

    expect(result.current.panelReveal).toMatchObject({
      panel: "library",
      id: "a/x.md",
    });
  });

  it("SNV-FR-66: a draft tab asks Drafts and a Diff tab asks Changes", () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.openDraft({ id: "d1", name: "Untitled" }));
    expect(result.current.panelReveal).toMatchObject({
      panel: "drafts",
      id: "d1",
    });

    act(() => result.current.openDiff(uncommittedDiff("src/App.tsx")));
    expect(result.current.panelReveal).toMatchObject({
      panel: "changes",
      id: "src/App.tsx",
    });
  });

  it("SNV-FR-66: a Dashboard or Search tab, and a settings window, leave the request alone", () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));
    const afterEditor = result.current.panelReveal;

    act(() => result.current.goHome());
    act(() => result.current.openSearchResults("q", "literal_insensitive"));
    // SWN-FR-01 / SNV-FR-64: a settings window is no tab, so opening one is no
    // tab activation and there is nothing for the panel to follow.
    act(() => result.current.openGlobalSettings());
    act(() => result.current.openSettings());

    // Not merely "no new panel": the request is the same object it was, so
    // nothing was re-asserted either.
    expect(result.current.panelReveal).toBe(afterEditor);
  });

  it("SNV-FR-65: re-activating the tab that is already active is not a trigger", () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));
    const first = result.current.panelReveal;

    // The same tab, requested again from the strip and from a second route.
    act(() => result.current.activateTab(result.current.activeTab));
    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));

    expect(result.current.panelReveal).toBe(first);
  });

  it("SNV-FR-68: returning to a tab after a no-op tab re-asserts its selection", () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));
    const first = result.current.panelReveal!;
    const editorTab = result.current.activeTab;

    // A Dashboard names no item, so the request still stands unchanged...
    act(() => result.current.goHome());
    expect(result.current.panelReveal).toBe(first);

    // ...and coming back is a fresh qualifying activation of the same file. A
    // request keyed on the id alone would be indistinguishable from the first
    // and the panel would never re-select it.
    act(() => result.current.activateTab(editorTab));
    expect(result.current.panelReveal).toMatchObject({
      panel: "library",
      id: "a/x.md",
    });
    expect(result.current.panelReveal!.nonce).toBeGreaterThan(first.nonce);
  });

  it("SNV-FR-65: focus landing on a tab because one was removed is not a trigger", async () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));
    act(() => result.current.openArtifact({ id: "b/y.md", name: "y.md" }));
    const before = result.current.panelReveal;
    const closing = result.current.activeTab;

    // Closing the active tab lands focus on a neighbour — a consequence rather
    // than a navigation, and the moment the requirement singles out as the wrong
    // one to move the panel in.
    await act(async () => {
      await result.current.closeTab(closing);
    });

    expect(result.current.activeTab).not.toBe(closing);
    expect(result.current.panelReveal).toBe(before);
  });

  it("SNV-FR-65: the Dashboard that replaces an emptied strip is not a trigger", async () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));
    await act(async () => {
      await result.current.closeTab("dashboard");
    });
    const before = result.current.panelReveal;

    await act(async () => {
      await result.current.closeTab(result.current.activeTab);
    });

    expect(result.current.activeTab).toBe("dashboard");
    expect(result.current.panelReveal).toBe(before);
  });

  it("SNV-FR-65: an automatic close of a removed path does not move the panel", () => {
    const { result } = renderHook(() => useShellSession());

    // The ACTIVE tab has to be the one that gets removed, or `activeTab` never
    // changes and the follow effect could not have fired whatever the
    // implementation did — the test would pass against the bug it is named for.
    act(() => result.current.openArtifact({ id: "src/x.ts", name: "x.ts" }));
    act(() => result.current.openArtifact({ id: "docs/a.md", name: "a.md" }));
    const removedTab = result.current.activeTab;
    const before = result.current.panelReveal;
    expect(before).toMatchObject({ id: "docs/a.md" });

    // TAB-FR-19: `docs` goes, so the active tab closes and focus lands on the
    // neighbour — a consequence rather than a navigation.
    act(() =>
      fireEvent("project-tree-changed", {
        changeCount: 1,
        removedPaths: ["docs"],
      }),
    );

    expect(result.current.tabs.some((t) => t.id === removedTab)).toBe(false);
    expect(result.current.activeTab).not.toBe(removedTab);
    expect(result.current.panelReveal).toBe(before);
  });

  it("SNV-FR-65: the commit-driven closure of a Diff tab does not move the panel", async () => {
    // TAB-FR-22, and the moment SNV-FR-65 singles out as "exactly the wrong one"
    // — the author is committing.
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.openArtifact({ id: "SKILL.md", name: "SKILL.md" }));
    act(() => result.current.openDiff(uncommittedDiff("SKILL.md")));
    const diffTab = result.current.activeTab;
    const before = result.current.panelReveal;

    await act(async () => {
      await result.current.closeDiffTabsForCommittedPaths(["SKILL.md"]);
    });

    expect(result.current.tabs.some((t) => t.id === diffTab)).toBe(false);
    expect(result.current.panelReveal).toBe(before);
  });

  it("SNV-FR-65: activating a panel, and editing inside the active tab, are not triggers", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));
    const before = result.current.panelReveal;

    // A panel toggle changes no tab (SNV-FR-65).
    act(() => result.current.setPanelSurface("comments"));
    act(() => result.current.setPanelSurface("notes"));
    expect(result.current.panelReveal).toBe(before);

    // And neither does anything happening *inside* the active tab. This is what
    // pins the follow effect's `[activeTab]`-only dependency: adding `tabs` to
    // it — the obvious way to "fix" the lint warning — would re-assert the
    // selection on every dirty transition, i.e. as the author typed.
    act(() => seedDirty(result.current.sessions, "a/x.md", "v2"));
    expect(result.current.panelReveal).toBe(before);
  });

  it("SNV-FR-67: a worktree change drops the request rather than carrying it across", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));
    expect(result.current.panelReveal).not.toBeNull();

    // A switch that actually SUCCEEDS. A callback that throws would make this
    // pass on the failure path instead, where the viewport also resets — and the
    // test would then be blind to the case it is named for.
    const arrived: WorktreeContext = {
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
          isPrimary: false,
          isMissing: false,
        },
      ],
      branches: [],
    };
    await act(async () => {
      await result.current.switchWorktree(async () => arrived);
    });
    expect(result.current.contentRootEpoch).toBeGreaterThan(0);

    // The path names a node in the outgoing content root; left standing, the
    // remounted Project panel would expand phantom ancestors against a tree
    // that has never held it.
    expect(result.current.panelReveal).toBeNull();
  });

  it("SNV-FR-69: while the switch is off, no activation produces a request", () => {
    setSelectionFollowsTab(false);
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));
    act(() => result.current.openDraft({ id: "d1", name: "Untitled" }));
    act(() => result.current.openDiff(uncommittedDiff("src/App.tsx")));

    expect(result.current.panelReveal).toBeNull();
    // And the panel is on the surface it opened with, untouched by any of it.
    expect(result.current.panelSurface).toBe("library");
  });

  it("SNV-FR-69: the gate is read per activation, not captured at mount", () => {
    setSelectionFollowsTab(false);
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));
    expect(result.current.panelReveal).toBeNull();

    // Turned on mid-session from the Navigation section (GLS-FR-28): in force
    // from the next qualifying activation, with no relaunch.
    setSelectionFollowsTab(true);
    act(() => result.current.openArtifact({ id: "b/y.md", name: "y.md" }));

    expect(result.current.panelReveal).toMatchObject({
      panel: "library",
      id: "b/y.md",
    });
  });

  it("SNV-FR-66: a tab that names no item at all leaves the request standing", () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));
    const before = result.current.panelReveal;

    // A Dashboard canned row: an Editor tab with no backing project file.
    act(() => result.current.openArtifact({ name: "Canned row" }));

    expect(result.current.panelReveal).toBe(before);
  });
});
