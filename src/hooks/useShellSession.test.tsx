import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { useShellSession } from "./useShellSession";
import {
  patchLayoutPreferences,
  resetLayoutPreferencesCache,
} from "../state/layoutPreferences";
import type { DiffTarget } from "../types";
import type { DiscussionReveal } from "../state/revealDiscussion";
import { resetPanelReveals } from "../state/panelReveal";
import { resetDraftProposals } from "../state/draftProposals";
import {
  FLOW_BODY,
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


describe("useShellSession.openArtifact (TAB-FR-04)", () => {
  it("keys tabs by id so same-basename files in different folders open distinct tabs", () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));
    act(() => result.current.openArtifact({ id: "b/x.md", name: "x.md" }));

    const artTabs = result.current.tabs.filter((t) => t.id.startsWith("art:"));
    expect(artTabs.map((t) => t.id)).toEqual(["art:a/x.md", "art:b/x.md"]);
    // The most-recently opened is focused.
    expect(result.current.activeTab).toBe("art:b/x.md");
  });

  it("re-opening the same id focuses the existing tab instead of duplicating it", () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));
    act(() => result.current.openArtifact({ id: "b/x.md", name: "x.md" }));
    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));

    const artTabs = result.current.tabs.filter((t) => t.id.startsWith("art:"));
    expect(artTabs).toHaveLength(2);
    expect(result.current.activeTab).toBe("art:a/x.md");
  });

  it("opens a Flow tab for a Flow and an Editor tab for any other type (LIB-FR-03)", () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.openArtifact({ id: "f.md", name: "f", artifactType: "flow" }));
    act(() => result.current.openArtifact({ id: "s.md", name: "s", artifactType: "spec" }));

    const byId = Object.fromEntries(result.current.tabs.map((t) => [t.id, t]));
    expect(byId["art:f.md"].kind).toBe("flow");
    expect(byId["art:s.md"].kind).toBe("editor");
  });
});


/**
 * SNV-FR-08: the shell tells the layout store which project is live, so a
 * layout write issued for the outgoing project and still in flight across a
 * switch is dropped rather than written into the incoming project's slot.
 *
 * `layoutPreferences` guards on that declaration; these tests pin that the
 * shell actually makes it, at the moments that matter. Without them the guard
 * is proven only in isolation and the `setActiveLayoutProject` call could be
 * dropped from the hook with nothing noticing.
 */
describe("useShellSession declares the live layout project (SNV-FR-08)", () => {
  /** Layout records that actually reached the backend. */
  const savedLayouts = () =>
    invokeMock.mock.calls
      .filter((c) => c[0] === "save_layout_preferences")
      .map((c) => (c[1] as { preferences: unknown }).preferences);

  beforeEach(() => resetLayoutPreferencesCache());
  afterEach(() => resetLayoutPreferencesCache());

  it("drops a write for the outgoing project when a switch overtakes it", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.loadProject(handle("a", "/a")));
    await waitFor(() => expect(result.current.projectPath).toBe("/a"));

    // A toggle's write for /a, still queued when the user switches to /b.
    const pending = patchLayoutPreferences("/a", { bottomPanelSurface: "git" });
    act(() => result.current.loadProject(handle("b", "/b")));
    await waitFor(() => expect(result.current.projectPath).toBe("/b"));
    await pending;

    expect(savedLayouts()).toHaveLength(0);
  });

  it("drops a write for a project once it has closed", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.loadProject(handle("a", "/a")));
    await waitFor(() => expect(result.current.projectPath).toBe("/a"));

    await act(async () => {
      await result.current.openAnother();
    });
    await waitFor(() => expect(result.current.screen).toBe("picker"));

    // With no project open the backend falls back to its global default slot,
    // so a late write for /a would overwrite that rather than /a's own.
    await patchLayoutPreferences("/a", { verticalPanelHidden: true });
    expect(savedLayouts()).toHaveLength(0);
  });

  it("still writes for the project that is actually open", async () => {
    // The guard must not swallow the ordinary case.
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.loadProject(handle("a", "/a")));
    await waitFor(() => expect(result.current.projectPath).toBe("/a"));

    await patchLayoutPreferences("/a", { bottomPanelSurface: "git" });
    expect(savedLayouts()).toEqual([{ bottomPanelSurface: "git" }]);
  });
});


// SNV-FR-48 / HVW-FR-01: the History toggle is live only while the active tab
// has an artifact behind it for the version list to describe.
describe("useShellSession.historyEnabled (SNV-FR-48)", () => {
  it("is false on a tab bound to no artifact", () => {
    const { result } = renderHook(() => useShellSession());
    // The Dashboard is the fresh-open tab and owns no artifact.
    expect(result.current.historyEnabled).toBe(false);
  });

  it("is true for an Editor tab and for a Flow tab alike", () => {
    const { result } = renderHook(() => useShellSession());

    act(() =>
      result.current.openArtifact({ id: "s.md", name: "s", artifactType: "spec" }),
    );
    expect(result.current.historyEnabled).toBe(true);

    // A Flow is an artifact too, so its Flow tab enables the toggle just as an
    // Editor tab does.
    act(() =>
      result.current.openArtifact({
        id: "f.md",
        name: "f",
        artifactType: "flow",
      }),
    );
    expect(result.current.activeT?.kind).toBe("flow");
    expect(result.current.historyEnabled).toBe(true);
  });

  it("is false again on a tab derived from artifacts without being bound to one", () => {
    const { result } = renderHook(() => useShellSession());
    act(() =>
      result.current.openArtifact({ id: "s.md", name: "s", artifactType: "spec" }),
    );
    expect(result.current.historyEnabled).toBe(true);

    // A Diff tab renders a comparison, not an artifact's own history.
    act(() =>
      result.current.openDiff({
        path: "s.md",
        name: "s.md",
        scope: { kind: "path", path: "s.md" },
        comparisonLabel: "uncommitted",
      }),
    );
    expect(result.current.activeT?.kind).toBe("diff");
    expect(result.current.historyEnabled).toBe(false);
  });
});


describe("useShellSession settings windows (SWN-FR-01, SWN-FR-13, SWN-FR-18)", () => {
  it("opens the Global settings window and adds no tab to the strip", () => {
    const { result } = renderHook(() => useShellSession());
    const before = result.current.tabs.map((t) => t.id);

    act(() => result.current.openGlobalSettings());

    // SWN-FR-01 / TAB-FR-03: a settings window is a native child window, not a
    // tab of the main viewport, so the strip is exactly as it was.
    expect(result.current.tabs.map((t) => t.id)).toEqual(before);
    expect(invokeMock).toHaveBeenCalledWith("open_settings_window", {
      kind: "global",
      section: null,
    });
  });

  it("opens the Project settings window and adds no tab either", () => {
    const { result } = renderHook(() => useShellSession());
    const before = result.current.tabs.map((t) => t.id);

    act(() => result.current.openSettings());

    expect(result.current.tabs.map((t) => t.id)).toEqual(before);
    expect(invokeMock).toHaveBeenCalledWith("open_settings_window", {
      kind: "project",
      section: null,
    });
  });

  // SWN-FR-13: a route that names a section carries it through, so the window
  // it opens (or focuses) presents that section.
  it("carries a named section to the window it asks for", () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.openGlobalSettings("agents:agent-1"));
    expect(invokeMock).toHaveBeenCalledWith("open_settings_window", {
      kind: "global",
      section: "agents:agent-1",
    });

    act(() => result.current.openSettings("agents"));
    expect(invokeMock).toHaveBeenCalledWith("open_settings_window", {
      kind: "project",
      section: "agents",
    });
  });

  it("logs rather than throwing when the window cannot be opened", async () => {
    // The ask is fire-and-forget from here, so a rejected invoke would surface
    // as an unhandled rejection and take nothing else down with it — but also
    // explain nothing. The strip must be untouched either way.
    const wired = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "open_settings_window") throw new Error("no parent window");
      return wired(cmd, args as { id?: string });
    });
    const { result } = renderHook(() => useShellSession());
    const before = result.current.tabs.map((t) => t.id);

    act(() => result.current.openGlobalSettings());
    await act(async () => {
      await Promise.resolve();
    });

    expect(result.current.tabs.map((t) => t.id)).toEqual(before);
  });

  // SWN-FR-05 through SWN-FR-07: which window ends up on screen is one decision
  // and it is the backend's. This side asks every time and arbitrates nothing —
  // a second request is a second ask, not a no-op computed here.
  it("asks again rather than deciding for itself when asked twice", () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.openGlobalSettings());
    act(() => result.current.openGlobalSettings());

    const asks = invokeMock.mock.calls.filter(
      (c) => c[0] === "open_settings_window",
    );
    expect(asks).toHaveLength(2);
  });
});


// ---------------------------------------------------------------------------
// Diff tabs (CHG-FR-18..FR-20)
// ---------------------------------------------------------------------------

describe("useShellSession.openDiff (CHG-FR-19 / CHG-FR-20)", () => {
  const uncommitted = (path: string): DiffTarget => ({
    path,
    name: path.split("/").pop()!,
    scope: { kind: "path", path },
    comparisonLabel: "uncommitted",
  });

  const againstMain = (path: string): DiffTarget => ({
    path,
    name: path.split("/").pop()!,
    scope: { kind: "branch", path, targetBranch: "main" },
    comparisonLabel: "against main",
  });

  it("jumps focus to an already-open Diff tab instead of duplicating it", () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.openDiff(uncommitted("src/components/Library.tsx")));
    const opened = result.current.tabs.filter((t) => t.kind === "diff");
    expect(opened).toHaveLength(1);
    const id = opened[0].id;

    // Focus something else, then request the same (file, comparison) again.
    act(() => result.current.activateTab("dashboard"));
    act(() => result.current.openDiff(uncommitted("src/components/Library.tsx")));

    expect(result.current.tabs.filter((t) => t.kind === "diff")).toHaveLength(1);
    expect(result.current.activeTab).toBe(id);
  });

  it("opens a separate tab for the same file under a different comparison", () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.openDiff(uncommitted("src/App.tsx")));
    act(() => result.current.openDiff(againstMain("src/App.tsx")));

    const diffs = result.current.tabs.filter((t) => t.kind === "diff");
    expect(diffs).toHaveLength(2);
    expect(new Set(diffs.map((t) => t.id)).size).toBe(2);
    // DFV-FR-02 / DFV-FR-03: the label names only the file, so both read
    // identically; the tooltip is what names the comparison and tells the two
    // tabs apart.
    expect(diffs.every((t) => t.label === "Diff: App.tsx")).toBe(true);
    expect(new Set(diffs.map((t) => t.tooltip)).size).toBe(2);
    expect(diffs.map((t) => t.tooltip)).toEqual(
      expect.arrayContaining([
        "Diff: src/App.tsx — uncommitted",
        "Diff: src/App.tsx — against main",
      ]),
    );
  });

  it("coexists with a live Editor tab on the same artifact (TAB-FR-06)", () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.openArtifact({ id: "src/App.tsx", name: "App.tsx" }));
    act(() => result.current.openDiff(uncommitted("src/App.tsx")));

    expect(result.current.tabs.find((t) => t.id === "art:src/App.tsx")).toBeDefined();
    expect(result.current.tabs.filter((t) => t.kind === "diff")).toHaveLength(1);
  });

  it("DFV-FR-05, DFV-FR-42, TAB-FR-06 enables Save over a Diff tab whose target is dirty (SNV-FR-28)", () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.openArtifact({ id: "a.md", name: "a.md" }));
    act(() => seedDirty(result.current.sessions, "a.md", "v1"));
    expect(result.current.saveEnabled).toBe(true);

    // DFV-FR-05 / DFV-FR-42: the Diff tab's target IS that artifact's editing
    // session, so Save acts on the tab exactly as it does over the Editor tab.
    act(() => result.current.openDiff(uncommitted("a.md")));
    expect(result.current.activeTab.startsWith("diff:")).toBe(true);
    expect(result.current.saveEnabled).toBe(true);
    expect(result.current.saveAllEnabled).toBe(true);

    // And is greyed out again once the target has reached disk.
    act(() => {
      result.current.sessions.update("a.md", { dirty: false });
    });
    expect(result.current.saveEnabled).toBe(false);
  });

  it("TAB-FR-10 writes the target when a Diff tab closes and leaves the session for the tabs that remain", async () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.openArtifact({ id: "a.md", name: "a.md" }));
    act(() => seedDirty(result.current.sessions, "a.md", "v1"));
    act(() => result.current.openDiff(uncommitted("a.md")));
    const diffId = result.current.activeTab;

    await act(async () => {
      await result.current.closeTab(diffId);
    });

    // The pending write is brought forward before the tab goes (TAB-FR-10),
    // and the artifact's editing session stands for the Editor tab still
    // showing it (DFV-FR-42, TAB-FR-09, EDT-FR-22, EDT-FR-30).
    expect(savedIds()).toEqual(["a.md"]);
    expect(result.current.sessions.get("a.md")?.dirty).toBe(false);
    expect(result.current.sessions.get("a.md")?.tabOpen).toBe(true);
    expect(result.current.tabs.some((t) => t.id === diffId)).toBe(false);
    expect(result.current.tabs.some((t) => t.id === "art:a.md")).toBe(true);
  });

  it("DFV-FR-55 closes only the Diff tabs a commit recorded (TAB-FR-22)", async () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.openArtifact({ id: "SKILL.md", name: "SKILL.md" }));
    act(() => seedDirty(result.current.sessions, "SKILL.md", "v1"));
    act(() => result.current.openDiff(uncommitted("SKILL.md")));
    act(() => result.current.openDiff(againstMain("SKILL.md")));
    act(() => result.current.openDiff(uncommitted("README.md")));

    // Never a fallback: a commit that names nothing closes nothing.
    await act(async () => {
      await result.current.closeDiffTabsForCommittedPaths([]);
    });
    expect(result.current.tabs.filter((t) => t.kind === "diff")).toHaveLength(3);

    await act(async () => {
      await result.current.closeDiffTabsForCommittedPaths(["SKILL.md"]);
    });

    const diffs = result.current.tabs.filter((t) => t.kind === "diff");
    expect(diffs).toHaveLength(1);
    expect(diffs[0].diff?.path).toBe("README.md");
    // The Editor tab stays open with its editing session intact, and the
    // pending target write was brought forward before the tabs were removed.
    expect(result.current.tabs.some((t) => t.id === "art:SKILL.md")).toBe(true);
    expect(savedIds()).toEqual(["SKILL.md"]);
    expect(result.current.sessions.get("SKILL.md")).toBeDefined();
  });
});


// ---------------------------------------------------------------------------
// Universal search wiring (SCH-search.md SCH-FR-08 / SCH-FR-09 / SCH-FR-11)
// ---------------------------------------------------------------------------

describe("Search results tabs (SCH-FR-08 / TAB-FR-05)", () => {
  it("opens a tab for the query and focuses it", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openSearchResults("needle", "literal_insensitive"));

    const tab = result.current.tabs.find((t) => t.kind === "search");
    expect(tab).toBeDefined();
    expect(tab?.label).toBe("Search: needle");
    expect(tab?.search).toEqual({ query: "needle", mode: "literal_insensitive" });
    expect(result.current.activeTab).toBe(tab?.id);
    // SNV-FR-28: a Search results tab owns no savable content.
    expect(tab?.artifactId).toBeUndefined();
  });

  it("TAB-FR-05: re-submitting the same query and mode focuses the existing tab", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openSearchResults("needle", "literal_insensitive"));
    act(() => result.current.activateTab("dashboard"));

    act(() => result.current.openSearchResults("needle", "literal_insensitive"));

    expect(result.current.tabs.filter((t) => t.kind === "search")).toHaveLength(1);
    expect(result.current.activeTab).toBe("search:literal_insensitive:needle");
  });

  it("a different query, or the same query in a different mode, gets its own tab", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openSearchResults("needle", "literal_insensitive"));
    act(() => result.current.openSearchResults("other", "literal_insensitive"));
    act(() => result.current.openSearchResults("needle", "regex"));

    expect(result.current.tabs.filter((t) => t.kind === "search")).toHaveLength(3);
  });

  it("SCH-FR-20: closing a Search results tab forgets its recorded result set", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openSearchResults("needle", "literal_insensitive"));
    const id = "search:literal_insensitive:needle";
    act(() =>
      result.current.searches.save(id, { hits: [], reason: "completed", error: null }),
    );
    expect(result.current.searches.get(id)).toBeDefined();

    await act(async () => {
      await result.current.closeTab(id);
    });

    expect(result.current.tabs.some((t) => t.id === id)).toBe(false);
    // Reopening the same query is a fresh sweep, not a resumed one.
    expect(result.current.searches.get(id)).toBeUndefined();
  });
});


describe("routing a search result (SCH-FR-09)", () => {
  function hitFor(overrides: Record<string, unknown>) {
    return {
      id: "x",
      name: "x",
      path: "x",
      ordinal: 0,
      group: "file",
      matchKind: "name",
      ...overrides,
    } as never;
  }

  it("SCH-FR-09, ESH-FR-ATDS, TAB-FR-02: a Files result opens an Editor tab carrying no artifact type", () => {
    const { result } = renderHook(() => useShellSession());
    act(() =>
      result.current.activateSearchHit(
        hitFor({ id: "src/main.rs", name: "main.rs", path: "src/main.rs" }),
      ),
    );

    const tab = result.current.tabs.find((t) => t.artifactId === "src/main.rs");
    expect(tab?.kind).toBe("editor");
    // ESH-FR-ATDS: no artifact type is what makes the Editor drop the
    // artifact-scoped actions.
    expect(tab?.type).toBeUndefined();
  });

  it("an artifact in flow context opens a Flow tab", () => {
    const { result } = renderHook(() => useShellSession());
    act(() =>
      result.current.activateSearchHit(
        hitFor({
          id: "flows/review.flow",
          name: "review.flow",
          group: "artifact",
          subtype: "flow",
          editContext: "flow",
        }),
      ),
    );

    expect(
      result.current.tabs.find((t) => t.artifactId === "flows/review.flow")
        ?.kind,
    ).toBe("flow");
  });

  it("a Run result opens the Runs bottom panel rather than a tab", () => {
    const { result } = renderHook(() => useShellSession());
    const before = result.current.tabs.length;

    act(() => result.current.activateSearchHit(hitFor({ group: "run", id: "r" })));

    expect(result.current.bottomVisible).toBe(true);
    expect(result.current.bottomSurface).toBe("runs");
    expect(result.current.tabs).toHaveLength(before);
  });

  it("a Workstream result reveals it in the Library rather than opening a tab", () => {
    const { result } = renderHook(() => useShellSession());
    const before = result.current.tabs.length;

    act(() =>
      result.current.activateSearchHit(
        hitFor({ group: "workstream", id: ".synthesis/workstreams/w.md" }),
      ),
    );

    expect(result.current.panelSurface).toBe("library");
    expect(result.current.panelReveal).toMatchObject({
      panel: "library",
      id: ".synthesis/workstreams/w.md",
    });
    expect(result.current.tabs).toHaveLength(before);
  });

  it("TAB-FR-04: two routes to the same file share one tab", () => {
    const { result } = renderHook(() => useShellSession());
    const file = hitFor({ id: "src/main.rs", name: "main.rs", path: "src/main.rs" });

    act(() => result.current.activateSearchHit(file));
    act(() => result.current.openArtifact({ id: "src/main.rs", name: "main.rs" }));

    expect(
      result.current.tabs.filter((t) => t.artifactId === "src/main.rs"),
    ).toHaveLength(1);
  });
});


describe("useShellSession.revealDiscussion on an available artifact (CMP-FR-10 / CMP-FR-11 / CMT-FR-36)", () => {
  const reveal = (artifactId: string, discussionId: string, resolved = false): DiscussionReveal => ({
    discussionId,
    target: { kind: "artifact", artifactId },
    ownerLabel: artifactId,
    subject: "the first session",
    resolved,
  });
  const target = reveal("specs/onboarding.md", "t1");

  it("opens the owning artifact in an Editor tab and focuses it", () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.revealDiscussion(target));

    const tab = result.current.tabs.find((t) => t.id === "art:specs/onboarding.md");
    expect(tab).toBeDefined();
    expect(tab?.kind).toBe("editor");
    expect(result.current.activeTab).toBe("art:specs/onboarding.md");
    expect(result.current.focusThread).toEqual({
      artifactId: "specs/onboarding.md",
      threadId: "t1",
    });
  });

  it("lands a source file on the one surface it has, without asking for a rich one", () => {
    // CMP-FR-11 / CMT-FR-02: the activation puts the tab on the surface that
    // renders the rail. For a source file (ESH-FR-SSDV) that is the surface it is
    // already on — writing `mode: "wysiwyg"` would ask for a rich rendering the
    // file never gets, and the rail would then be beside nothing.
    const { result } = renderHook(() => useShellSession());
    const sourceTarget = reveal("src/main.rs", "t9");

    act(() => result.current.revealDiscussion(sourceTarget));

    const session = result.current.sessions.get("src/main.rs")!;
    expect(session.mode).toBe("text");
    expect(session.railOpen).toBe(true);
    expect(result.current.focusThread).toEqual({
      artifactId: "src/main.rs",
      threadId: "t9",
    });
  });

  it("puts the tab in WYSIWYG with the rail showing, whatever state it was left in", () => {
    const { result } = renderHook(() => useShellSession());

    // The author had this artifact open in raw-text mode with the rail closed.
    act(() => result.current.openArtifact({ id: "specs/onboarding.md", name: "onboarding.md" }));
    act(() =>
      result.current.sessions.update("specs/onboarding.md", {
        mode: "text",
        railOpen: false,
        // Expanded by the author. A non-resolved click-through must not collapse
        // it — `resolvedOpen: target.resolved` would, and would pass a test that
        // only ever saw the `false` default.
        resolvedOpen: true,
      }),
    );

    act(() => result.current.revealDiscussion(target));

    const session = result.current.sessions.get("specs/onboarding.md")!;
    // CMT-FR-36: the rail renders only in WYSIWYG, so the navigation has to
    // switch the mode or it lands on nothing.
    expect(session.mode).toBe("wysiwyg");
    expect(session.railOpen).toBe(true);
    // Not a resolved thread, so the disclosure is left as the author had it.
    expect(session.resolvedOpen).toBe(true);
  });

  it("expands the resolved disclosure only for a resolved thread (CMP-FR-11)", () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.revealDiscussion({ ...target, resolved: true }));
    expect(result.current.sessions.get("specs/onboarding.md")?.resolvedOpen).toBe(
      true,
    );
  });

  it("focuses an already-open tab rather than opening a second one (CMP-FR-10)", () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.openArtifact({ id: "specs/onboarding.md", name: "onboarding.md" }));
    act(() => result.current.openArtifact({ id: "specs/other.md", name: "other.md" }));
    act(() => result.current.revealDiscussion(target));

    const matching = result.current.tabs.filter(
      (t) => t.id === "art:specs/onboarding.md",
    );
    expect(matching).toHaveLength(1);
    expect(result.current.activeTab).toBe("art:specs/onboarding.md");
  });

  it("clears the pending focus once the Editor has taken it", () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.revealDiscussion(target));
    expect(result.current.focusThread).not.toBeNull();

    act(() => result.current.clearFocusThread());
    expect(result.current.focusThread).toBeNull();
  });
});
