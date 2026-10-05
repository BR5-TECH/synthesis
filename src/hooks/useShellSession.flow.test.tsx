import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook } from "@testing-library/react";
import { useShellSession } from "./useShellSession";
import { FlowSessionStore } from "../state/flowSessions";
import { addNode } from "../state/flowDocument";
import type { WorktreeContext } from "../types";
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


// TAB-FR-09 / TAB-FR-12 / TAB-FR-13 / FLO-FR-28..31: a Flow tab owns its Flow's
// editing session, where an Editor tab is only a view onto one.
describe("Flow tab lifecycle (TAB-FR-12 / FLO-FR-28)", () => {
  /**
   * A Flow tab open on `FLOW_ID`, loaded and then edited — the state a canvas
   * edit leaves behind. The store is real and writes through the real
   * `"save artifact contents"` path, so the write-refusal branches run through
   * the store's own logic rather than a mocked `flush` that would pass even if
   * that logic were wrong; `failWrites` is what makes a write reject.
   */
  const openDirtyFlow = async (flows?: FlowSessionStore) => {
    const { result } = renderHook(() => useShellSession({ flows }));
    act(() => result.current.loadProject(handle("acme", "~/dev/acme")));
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
        addNode(d, { x: 120, y: 80 }),
      ),
    );
    return result;
  };

  const flowTabId = `art:${FLOW_ID}`;

  /** The worktree context a successful switch resolves to. */
  const contextOnMain = (): WorktreeContext => ({
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
  });

  // FLO-FR-02: each Flow gets its own tab, and a request to open one that
  // already has a tab jumps focus to it (TAB-FR-04 / TAB-FR-05).
  it("gives each Flow its own tab and jumps focus rather than opening a second", async () => {
    const result = await openDirtyFlow();
    const second = "workflows/other.flow";

    await act(async () => {
      result.current.openArtifact({
        id: second,
        name: "other.flow",
        artifactType: "flow",
      });
      await result.current.flows.get(second)?.pendingLoad;
    });

    expect(result.current.tabs.filter((t) => t.kind === "flow")).toHaveLength(2);
    expect(result.current.activeTab).toBe(`art:${second}`);

    act(() =>
      result.current.openArtifact({
        id: FLOW_ID,
        name: "h.flow",
        artifactType: "flow",
      }),
    );

    expect(result.current.tabs.filter((t) => t.kind === "flow")).toHaveLength(2);
    expect(result.current.activeTab).toBe(flowTabId);
    // The focus jump neither reloaded the first Flow nor lost its edits.
    expect(result.current.flows.get(FLOW_ID)?.dirty).toBe(true);
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "load_artifact_contents_by_id"),
    ).toHaveLength(2);
  });

  // TAB-FR-12 / FLO-FR-27, FLO-FR-28: the close writes first, and retains nothing.
  it("writes the Flow on close and retains nothing, so a reopen loads from disk", async () => {
    const result = await openDirtyFlow();
    expect(result.current.flows.get(FLOW_ID)?.doc?.nodes).toHaveLength(2);

    await act(async () => {
      await result.current.closeTab(flowTabId);
    });

    expect(result.current.tabs.find((t) => t.id === flowTabId)).toBeUndefined();
    expect(savedIds()).toEqual([FLOW_ID]);
    expect(result.current.flows.get(FLOW_ID)).toBeUndefined();

    // The reopen deserializes the on-disk document afresh — clean, and back to
    // what the (unchanged) fixture body describes.
    await act(async () => {
      result.current.openArtifact({
        id: FLOW_ID,
        name: "h.flow",
        artifactType: "flow",
      });
      await result.current.flows.get(FLOW_ID)?.pendingLoad;
    });
    const reopened = result.current.flows.get(FLOW_ID)!;
    expect(reopened.dirty).toBe(false);
    expect(reopened.doc?.nodes).toHaveLength(1);
  });

  // TAB-FR-13: a failed write refuses the close. Driven through a real store
  // whose write rejects — not a stubbed-out `flush` — so the refusal is proven
  // end to end, from the rejected write to the tab staying open.
  it("refuses the close while the Flow write fails, and closes once it succeeds", async () => {
    const result = await openDirtyFlow();
    failWrites.add(FLOW_ID);

    await act(async () => {
      await result.current.closeTab(flowTabId);
    });

    expect(result.current.tabs.find((t) => t.id === flowTabId)).toBeDefined();
    expect(result.current.activeTab).toBe(flowTabId);
    // The edits survive the refused close, and the error is on the record for
    // the canvas to show.
    expect(result.current.flows.get(FLOW_ID)?.dirty).toBe(true);
    expect(result.current.flows.get(FLOW_ID)?.error).toContain("disk full");

    failWrites.delete(FLOW_ID);
    await act(async () => {
      await result.current.closeTab(flowTabId);
    });
    expect(result.current.tabs.find((t) => t.id === flowTabId)).toBeUndefined();
  });

  // TAB-FR-09 / FLO-FR-30: switching away and back keeps the unsaved graph.
  it("keeps the unsaved graph and dirty state across a tab switch", async () => {
    const result = await openDirtyFlow();
    const graph = result.current.flows.get(FLOW_ID)!.doc;

    act(() => result.current.activateTab("dashboard"));
    act(() => result.current.activateTab(flowTabId));

    const after = result.current.flows.get(FLOW_ID)!;
    expect(after.dirty).toBe(true);
    expect(after.doc).toBe(graph);
    expect(result.current.tabs.find((t) => t.id === flowTabId)?.dirty).toBe(true);
    // FLO-FR-30: making the tab active again performs no reload.
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "load_artifact_contents_by_id"),
    ).toHaveLength(1);
  });

  // FLO-FR-29: a project close writes artifacts and Flows alike, before any tab
  // is torn down.
  it("writes both a dirty artifact and a dirty Flow before the project closes", async () => {
    const result = await openDirtyFlow();
    act(() => result.current.openArtifact({ id: "a.md", name: "a.md" }));
    act(() => seedDirty(result.current.sessions, "a.md", "v1"));

    await act(async () => {
      await result.current.requestCloseProject();
    });

    expect(savedIds()).toEqual(["a.md", FLOW_ID]);
    expect(result.current.screen).toBe("picker");
    // Both stores are discarded with the project (EDT-FR-28 / FLO-FR-29).
    expect(result.current.flows.get(FLOW_ID)).toBeUndefined();
  });

  // FLO-FR-13 / SNV-FR-43: Find and Find & Replace are Editor-scoped, which is
  // what makes ⌘F and ⌘R do nothing on a Flow tab — a node's inline prompt is
  // plain text with no find surface at all. Asserted at the shell, because the
  // menu enablement is where the rule actually lives.
  it("disables Find and Find & Replace while a Flow tab is active", async () => {
    const result = await openDirtyFlow();
    expect(result.current.findEnabled).toBe(false);
    const findMenuCalls = invokeMock.mock.calls.filter(
      (c) => c[0] === "set_find_menu_state",
    );
    expect(findMenuCalls[findMenuCalls.length - 1]?.[1]).toEqual({
      enabled: false,
    });

    // The accelerator arriving anyway opens nothing.
    act(() => result.current.requestFind("find"));
    expect(result.current.sessions.get(FLOW_ID)?.find.form).toBeUndefined();

    // …and an Editor tab enables them again.
    act(() => result.current.openArtifact({ id: "a.md", name: "a.md" }));
    expect(result.current.findEnabled).toBe(true);
  });

  // FLO-FR-29 names four transitions; the worktree change is the one that also
  // has to leave the project where it was when a write is refused.
  it("writes a dirty Flow before a worktree change, and a failed write cancels it", async () => {
    const result = await openDirtyFlow();
    failWrites.add(FLOW_ID);
    const operation = vi.fn();

    await act(async () => {
      await result.current.switchWorktree(async () => {
        operation();
        throw new Error("unreachable");
      });
    });

    // Cancelled before anything else happened: no operation, no tab closed.
    expect(operation).not.toHaveBeenCalled();
    expect(result.current.tabs.find((t) => t.id === flowTabId)).toBeDefined();
    expect(result.current.contentRootEpoch).toBe(0);
    expect(result.current.flows.get(FLOW_ID)?.dirty).toBe(true);

    failWrites.delete(FLOW_ID);
    await act(async () => {
      await result.current.switchWorktree(async () => contextOnMain());
    });

    expect(savedIds()).toContain(FLOW_ID);
    // TAB-FR-14 / FLO-FR-29: the tabs closed and no Flow session outlived them.
    expect(result.current.tabs.map((t) => t.id)).toEqual(["dashboard"]);
    expect(result.current.flows.get(FLOW_ID)).toBeUndefined();
    expect(result.current.contentRootEpoch).toBe(1);
  });

  // SNV-FR-26 / FLO-FR-29: the application quit is held until the writes land.
  it("writes a dirty Flow before answering a held quit, and refuses the quit when it fails", async () => {
    const result = await openDirtyFlow();
    failWrites.add(FLOW_ID);

    await act(async () => {
      await result.current.requestExit();
    });

    const refused = invokeMock.mock.calls.filter((c) => c[0] === "finish_exit");
    expect(refused[refused.length - 1]?.[1]).toEqual({ proceed: false });

    failWrites.delete(FLOW_ID);
    await act(async () => {
      await result.current.requestExit();
    });

    expect(savedIds()).toContain(FLOW_ID);
    const answered = invokeMock.mock.calls.filter((c) => c[0] === "finish_exit");
    expect(answered[answered.length - 1]?.[1]).toEqual({ proceed: true });
  });

  // FLO-FR-29 / EDT-FR-32: a Flow whose write fails cancels the teardown.
  it("cancels the project close when a Flow write fails", async () => {
    const result = await openDirtyFlow();
    act(() => result.current.activateTab("dashboard"));
    failWrites.add(FLOW_ID);

    await act(async () => {
      await result.current.requestCloseProject();
    });

    expect(result.current.screen).toBe("ide");
    expect(result.current.activeTab).toBe(flowTabId);
    // Nothing was torn down: the backend teardown never ran and the Flow keeps
    // its unsaved graph.
    expect(
      invokeMock.mock.calls.some((c) => c[0] === "close_project"),
    ).toBe(false);
    expect(result.current.flows.get(FLOW_ID)?.dirty).toBe(true);
  });

  // SNV-FR-31 / EDT-FR-36: the Save All sweep reaches Flows too, and a blocked
  // Flow is focused the same way a blocked artifact is.
  it("Save All focuses the Flow tab whose write failed", async () => {
    const result = await openDirtyFlow();
    act(() => result.current.openArtifact({ id: "a.md", name: "a.md" }));
    act(() => seedDirty(result.current.sessions, "a.md", "v1"));
    failWrites.add(FLOW_ID);

    await act(async () => {
      await result.current.requestSaveAll();
    });

    // The artifact still lands — a blocked Flow does not abort the sweep.
    expect(result.current.sessions.get("a.md")?.dirty).toBe(false);
    expect(result.current.activeTab).toBe(flowTabId);
    expect(result.current.flows.get(FLOW_ID)?.dirty).toBe(true);
    expect(result.current.saveAllEnabled).toBe(true);
  });
});
