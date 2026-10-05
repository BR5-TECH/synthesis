import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { useShellSession } from "./useShellSession";
import type {
  DiffTarget,
  WorktreeContext,
  WorktreeEntry,
} from "../types";
import type { SwitchOutcome } from "../components/WorktreeSelector";
import { resetPanelReveals } from "../state/panelReveal";
import {
  closeReview,
  ensureLoaded,
  openReview,
  pendingOf,
  proposalsOf,
  readingStatusOf,
  resetDraftProposals,
  reviewingProposal,
} from "../state/draftProposals";
import {
  FLOW_BODY,
  FLOW_ID,
  handle,
  pendingProposal,
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


// ---------------------------------------------------------------------------
// The worktree-switch transition (OVW-FR-12 / TAB-FR-14 / WTS-FR-22..24)
// ---------------------------------------------------------------------------

describe("useShellSession.switchWorktree (OVW-FR-12 / TAB-FR-14)", () => {
  const uncommittedDiff = (path: string): DiffTarget => ({
    path,
    name: path.split("/").pop()!,
    scope: { kind: "path", path },
    comparisonLabel: "uncommitted",
  });

  const contextOn = (path: string, branch: string): WorktreeContext => ({
    repositoryRoot: "~/dev/acme",
    activeWorktreePath: path,
    worktrees: [
      {
        path,
        name: path.split("/").pop()!,
        branch,
        headShortHash: "4f2a10c",
        isDetached: false,
        isActive: true,
        // The switch below lands in a *linked* worktree, which is what makes
        // the `isPrimary === false` assertion meaningful.
        isPrimary: false,
        isMissing: false,
      },
    ],
    branches: [],
  });

  const WORKTREE_A: WorktreeEntry = {
    path: "~/dev/acme",
    name: "acme",
    branch: "feature/x",
    headShortHash: "4f2a10c",
    isDetached: false,
    isActive: true,
    isPrimary: true,
    isMissing: false,
  };

  /**
   * A project on worktree A with four tabs, one artifact holding an edit.
   *
   * Async because the chrome's worktree label is resolved by a round-trip on
   * open: awaiting it is what makes the "still on worktree A" assertions below
   * about a value that was actually set, rather than about `null`.
   */
  const openWithTabs = async () => {
    // The chrome has to actually be labelled with worktree A, or every
    // "still on A" assertion below would hold vacuously.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "save_artifact_contents") return { checksum: "ck-saved" };
      if (cmd === "get_active_worktree") return WORKTREE_A;
      if (cmd === "load_artifact_contents_by_id") {
        return { body: FLOW_BODY, checksum: "ck-flow" };
      }
      return undefined;
    });
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.loadProject(handle("acme", "~/dev/acme")));
    act(() => result.current.openArtifact({ id: "a.md", name: "a.md" }));
    act(() => result.current.openArtifact({ id: "b.md", name: "b.md" }));
    await act(async () => {
      result.current.openArtifact({
        id: FLOW_ID,
        name: "h.flow",
        artifactType: "flow",
      });
      await result.current.flows.get(FLOW_ID)?.pendingLoad;
    });
    act(() => result.current.openDiff(uncommittedDiff("a.md")));
    act(() => seedDirty(result.current.sessions, "a.md", "v1"));
    act(() => result.current.setPanelSurface("changes"));
    await waitFor(() => expect(result.current.activeWorktree).toEqual(WORKTREE_A));
    return result;
  };

  // OVW-FR-12 / TAB-FR-14, TAB-FR-15, EDT-FR-28: the edits are written, every tab closes, the
  // viewport returns to its fresh state, and the project is unchanged.
  it("writes pending edits, closes every tab, and returns the viewport to Dashboard + Library", async () => {
    const result = await openWithTabs();
    expect(result.current.tabs).toHaveLength(5);
    expect(result.current.activeWorktree).toEqual(WORKTREE_A);

    await act(async () => {
      await result.current.switchWorktree(async () =>
        contextOn("~/dev/acme-main", "main"),
      );
    });

    expect(savedIds()).toEqual(["a.md"]);
    expect(result.current.tabs).toEqual([
      { id: "dashboard", label: "Dashboard" },
    ]);
    expect(result.current.activeTab).toBe("dashboard");
    expect(result.current.panelSurface).toBe("library");
    // The main window is not torn down: the project switcher still names the
    // same project, and the screen never returns to the picker.
    expect(result.current.screen).toBe("ide");
    expect(result.current.projectName).toBe("acme");
    expect(result.current.projectPath).toBe("~/dev/acme");
    // …but the content root moved, and the epoch it is keyed by advanced so
    // every surface reading from it remounts (LIB-FR-13 / CHG-FR-28 /
    // GIT-FR-11).
    expect(result.current.activeWorktree?.path).toBe("~/dev/acme-main");
    expect(result.current.activeWorktree?.branch).toBe("main");
    expect(result.current.contentRootEpoch).toBe(1);
    // WTC-FR-21 / WTS-FR-12: the switch landed in a linked worktree, so the
    // flag both checkout gates read is now false. This is the one assertion
    // connecting the backend's primary determination to the UI's rule.
    expect(result.current.activeWorktree?.isPrimary).toBe(false);
  });

  // TAB-FR-14, TAB-FR-15, second half / EDT-FR-28: no editing session outlives the tabs,
  // because a session's key names a path in the *previous* content root.
  it("discards every retained edit session so the artifact reloads from disk", async () => {
    const result = await openWithTabs();
    expect(result.current.sessions.get("a.md")).toBeDefined();

    await act(async () => {
      await result.current.switchWorktree(async () =>
        contextOn("~/dev/acme-main", "main"),
      );
    });

    expect(result.current.sessions.get("a.md")).toBeUndefined();
    expect(result.current.sessions.get("b.md")).toBeUndefined();
    expect(result.current.flows.get(FLOW_ID)).toBeUndefined();
  });

  // OVW-FR-12, EDT-FR-32 / TAB-FR-14 / WTS-FR-23: a blocked write cancels the whole switch
  // *before anything else happens*.
  // DCR-FR-22 / DCR-FR-33: the proposals every surface reads a draft's pending
  // change from name drafts of the outgoing content root and do not survive it
  // (DCP-FR-02), so the switch takes them — and the review standing over one —
  // with the rest of the session state.
  it("discards the draft proposals and any open review, so the incoming tree is read afresh", async () => {
    const result = await openWithTabs();
    invokeMock.mockImplementationOnce(async () => [pendingProposal]);
    ensureLoaded("d-out");
    await waitFor(() => expect(pendingOf("d-out")?.id).toBe("dp1"));
    act(() => openReview("dp1"));
    expect(reviewingProposal()).toBe("dp1");

    await act(async () => {
      await result.current.switchWorktree(async () =>
        contextOn("~/dev/acme-main", "main"),
      );
    });

    expect(reviewingProposal()).toBeNull();
    expect(proposalsOf("d-out")).toEqual([]);
    // Not merely emptied — unread, so the first surface of the incoming tree
    // takes a reading of its own rather than trusting the outgoing tree's.
    expect(readingStatusOf("d-out")).toBe("unread");
  });

  it("keeps them when the switch is cancelled, nothing having changed root", async () => {
    const result = await openWithTabs();
    invokeMock.mockImplementationOnce(async () => [pendingProposal]);
    ensureLoaded("d-out");
    await waitFor(() => expect(pendingOf("d-out")?.id).toBe("dp1"));
    act(() => openReview("dp1"));
    act(() => result.current.sessions.update("a.md", { conflict: true }));

    await act(async () => {
      await result.current.switchWorktree(async () =>
        contextOn("~/dev/acme-main", "main"),
      );
    });

    // The window is still reading the same tree, so the reading still answers
    // for it and the author's review is still the one they left open.
    expect(reviewingProposal()).toBe("dp1");
    expect(pendingOf("d-out")?.id).toBe("dp1");
    act(() => closeReview());
  });

  it("cancels the switch outright when a write is blocked", async () => {
    const result = await openWithTabs();
    act(() => result.current.sessions.update("a.md", { conflict: true }));
    act(() => result.current.activateTab("dashboard"));
    const operation = vi.fn();

    let outcome: SwitchOutcome | undefined;
    await act(async () => {
      outcome = await result.current.switchWorktree(async () => {
        operation();
        return contextOn("~/dev/acme-main", "main");
      });
    });

    expect(outcome).toEqual({ ok: false, cancelled: true });
    expect(operation).not.toHaveBeenCalled();
    // No tab closed, and the blocking tab is focused so the user can resolve it.
    expect(result.current.tabs).toHaveLength(5);
    expect(result.current.activeTab).toBe("art:a.md");
    expect(result.current.activeWorktree).toEqual(WORKTREE_A);
    expect(result.current.contentRootEpoch).toBe(0);
    expect(result.current.sessions.get("a.md")?.conflict).toBe(true);
  });

  // OVW-FR-12, WTS-FR-24: a refused operation leaves the window in the fresh
  // state, still on the previous worktree, and hands the error back.
  it("returns the typed error and leaves the viewport fresh on the previous worktree", async () => {
    const result = await openWithTabs();

    let outcome: SwitchOutcome | undefined;
    await act(async () => {
      outcome = await result.current.switchWorktree(async () => {
        throw "branch already checked out";
      });
    });

    expect(outcome).toEqual({
      ok: false,
      error: "branch already checked out",
    });
    // The tabs did close — the failure is delivered after they have — and the
    // viewport shows the Dashboard with the Library selected.
    expect(result.current.tabs).toEqual([
      { id: "dashboard", label: "Dashboard" },
    ]);
    expect(result.current.panelSurface).toBe("library");
    // The project was never re-rooted, so the chrome still names worktree A
    // and no remount epoch was spent.
    expect(result.current.activeWorktree).toEqual(WORKTREE_A);
    expect(result.current.contentRootEpoch).toBe(0);
    expect(result.current.screen).toBe("ide");
  });

  // OVW-FR-12: "the layout keeps its shape". The bottom panel is layout, not
  // viewport, so a switch leaves it exactly as the user left it — unlike a
  // project switch, which resets it.
  it("leaves the shell's layout untouched", async () => {
    const result = await openWithTabs();
    act(() => result.current.toggleBottomSurface("git"));

    await act(async () => {
      await result.current.switchWorktree(async () =>
        contextOn("~/dev/acme-main", "main"),
      );
    });

    expect(result.current.bottomVisible).toBe(true);
    expect(result.current.bottomSurface).toBe("git");
  });

  // The ordering claim OVW-FR-12 makes, pinned directly: every write lands
  // before the switching operation is invoked.
  it("writes before it invokes, never after", async () => {
    const result = await openWithTabs();
    const order: string[] = [];
    invokeMock.mockImplementation(async (cmd: string) => {
      order.push(cmd);
      if (cmd === "save_artifact_contents") return { checksum: "ck-saved" };
      return undefined;
    });

    await act(async () => {
      await result.current.switchWorktree(async () => {
        order.push("activate_worktree");
        return contextOn("~/dev/acme-main", "main");
      });
    });

    const relevant = order.filter(
      (c) => c === "save_artifact_contents" || c === "activate_worktree",
    );
    expect(relevant).toEqual(["save_artifact_contents", "activate_worktree"]);
  });
});


// ---------------------------------------------------------------------------
// The chrome label follows the context-changed event (WTS-FR-25 / WTC-FR-16)
// ---------------------------------------------------------------------------

describe("useShellSession worktree label (WTS-FR-25)", () => {
  const ON_MAIN: WorktreeEntry = {
    path: "~/dev/acme",
    name: "acme",
    branch: "main",
    headShortHash: "4f2a10c",
    isDetached: false,
    isActive: true,
    isPrimary: true,
    isMissing: false,
  };

  /**
   * What the backend currently reports for `get_active_worktree`. The shell
   * re-reads it on every context-changed event — the event payload does not
   * carry `isPrimary`, and that flag decides whether a branch may be checked
   * out at all — so a test that fires the event moves this first, exactly as
   * the backend would have.
   */
  let reported: WorktreeEntry;

  const openOnMain = () => {
    reported = ON_MAIN;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_active_worktree") return reported;
      return undefined;
    });
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.loadProject(handle("acme", "~/dev/acme")));
    return result;
  };

  // WTC-FR-17: the user is told when the project could not resume where it
  // left off, rather than being silently rerouted to the primary worktree.
  it("says so when the remembered worktree is gone", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_active_worktree") return ON_MAIN;
      return undefined;
    });
    const { result } = renderHook(() => useShellSession());
    act(() =>
      result.current.loadProject({
        ...handle("acme", "~/dev/acme"),
        rememberedWorktreeUnavailable: true,
      }),
    );

    await waitFor(() => expect(result.current.toast).toContain("no longer on disk"));
  });

  it("says nothing when the project resumed where it left off", async () => {
    const result = openOnMain();
    await waitFor(() => expect(result.current.activeWorktree).not.toBeNull());
    expect(result.current.toast).toBeNull();
  });

  it("labels itself from the worktree the open resolved to", async () => {
    const result = openOnMain();
    await waitFor(() =>
      expect(result.current.activeWorktree).toEqual(ON_MAIN),
    );
  });

  // WTS-FR-25, label half: a checkout made from the Git panel relabels the
  // chrome without the selector's dropdown being involved at all.
  it("relabels when the active worktree changes elsewhere", async () => {
    const result = openOnMain();
    await waitFor(() => expect(result.current.activeWorktree).not.toBeNull());

    reported = { ...ON_MAIN, branch: "develop" };
    await act(async () => {
      fireEvent("worktree-context-changed", {
        activeWorktreePath: "~/dev/acme",
        branch: "develop",
        isDetached: false,
      });
    });

    await waitFor(() =>
      expect(result.current.activeWorktree?.branch).toBe("develop"),
    );
    expect(result.current.activeWorktree?.isDetached).toBe(false);
  });

  /**
   * The payload names the new worktree but not everything about it. Merging it
   * onto the previous entry would carry `isPrimary` over from the worktree just
   * left — and that flag is what decides whether the selector offers branches
   * at all (WTS-FR-12), so a stale `true` inside a linked worktree would offer
   * checkouts the backend refuses.
   */
  it("re-reads the authoritative entry rather than trusting the payload alone", async () => {
    const result = openOnMain();
    await waitFor(() => expect(result.current.activeWorktree?.isPrimary).toBe(true));

    reported = {
      ...ON_MAIN,
      path: "~/dev/acme-linked",
      name: "acme-linked",
      branch: "alpha",
      isPrimary: false,
    };
    await act(async () => {
      fireEvent("worktree-context-changed", {
        activeWorktreePath: "~/dev/acme-linked",
        branch: "alpha",
        isDetached: false,
      });
    });

    await waitFor(() =>
      expect(result.current.activeWorktree?.isPrimary).toBe(false),
    );
    expect(result.current.activeWorktree?.path).toBe("~/dev/acme-linked");
  });

  it("carries a detached HEAD through the event", async () => {
    const result = openOnMain();
    await waitFor(() => expect(result.current.activeWorktree).not.toBeNull());

    reported = { ...ON_MAIN, branch: undefined, isDetached: true };
    await act(async () => {
      fireEvent("worktree-context-changed", {
        activeWorktreePath: "~/dev/acme",
        isDetached: true,
      });
    });

    await waitFor(() =>
      expect(result.current.activeWorktree?.isDetached).toBe(true),
    );
    expect(result.current.activeWorktree?.branch).toBeUndefined();
  });

  // WTS-FR-02: a project outside a Git repository leaves the chrome with no
  // selector at all, and an event cannot conjure one.
  it("stays null when the project is not inside a Git repository", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_active_worktree") throw "not a git repository";
      return undefined;
    });
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.loadProject(handle("plain", "/tmp/plain")));

    await waitFor(() => expect(result.current.activeWorktree).toBeNull());
    await act(async () => {
      fireEvent("worktree-context-changed", {
        activeWorktreePath: "/tmp/plain",
        isDetached: false,
      });
    });
    expect(result.current.activeWorktree).toBeNull();
  });

  /**
   * WTS-FR-32 / WTC-FR-25: the branch set was re-read, so the chrome label is
   * re-read with it — a branch checked out in a terminal since the last look
   * relabels without the dropdown being opened.
   *
   * WTS-FR-33 / WTC-FR-24: and emphatically not a switch. The content-root epoch
   * is what remounts every surface bound to the content root, so leaving it alone
   * is what keeps a refresh from wiping the viewport.
   */
  it("re-reads the chrome label on a branch set change without remounting anything", async () => {
    const result = openOnMain();
    await waitFor(() => expect(result.current.activeWorktree).not.toBeNull());
    const epochBefore = result.current.contentRootEpoch;

    reported = { ...ON_MAIN, branch: "develop" };
    await act(async () => {
      fireEvent("branches-changed", { repositoryRoot: "~/dev/acme" });
    });

    await waitFor(() =>
      expect(result.current.activeWorktree?.branch).toBe("develop"),
    );
    expect(result.current.contentRootEpoch).toBe(epochBefore);
  });
});
