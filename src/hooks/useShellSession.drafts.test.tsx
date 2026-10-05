import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { useShellSession } from "./useShellSession";
import type { DraftSessionStore } from "../state/draftSessions";
import { resetPanelReveals } from "../state/panelReveal";
import { resetDraftProposals } from "../state/draftProposals";
import {
  FLOW_BODY,
  handle,
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


// NAW-new-artifact.md / TAB-tabs.md: a draft is not a file, so a New Artifact
// tab is keyed by the draft's own id (TAB-FR-17) and owns savable content the
// File menu must be able to reach (NAW-FR-13).
describe("draft tabs (NAW-FR-02 / NAW-FR-13 / TAB-FR-17 / TAB-FR-18)", () => {
  /** Two distinct drafts, so the single-tab rule has something to distinguish. */
  function draftBackend() {
    let n = 0;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "create_draft") {
        n += 1;
        // DRS-FR-06: a draft is created holding one Markdown file.
        return {
          draft: {
            id: `d${n}`,
            name: `draft ${n}`,
            status: "active",
            createdAt: "2026-07-31T10:00:00Z",
            updatedAt: "2026-07-31T10:00:00Z",
          },
          file: "Untitled.md",
        };
      }
      if (cmd === "save_draft_file_contents") return { checksum: "c" };
      return undefined;
    });
  }

  /**
   * Put an unsaved buffer into a draft file's edit session — exactly the state
   * the Editor leaves behind after a keystroke (NAW-FR-11 / NAW-FR-12), reached
   * without mounting one.
   */
  function edit(
    drafts: DraftSessionStore,
    draftId: string,
    body: string,
    path = "Untitled.md",
  ) {
    const key = drafts.key(draftId, path);
    drafts.docs.ensure(key).buffer = body;
    drafts.docs.update(key, { dirty: true });
  }

  it("NAW-FR-02 / TAB-FR-17, TAB-FR-04: one tab per draft, and reopening jumps focus to it", async () => {
    draftBackend();
    const { result } = renderHook(() => useShellSession());
    await act(async () => result.current.loadProject(handle("a", "/a")));

    await act(async () => await result.current.createDraft({}));
    const afterFirst = result.current.tabs.length;
    expect(result.current.activeTab).toBe("draft:d1");

    // Open something else, then ask for the same draft again.
    act(() => result.current.activateTab("dashboard"));
    act(() => result.current.openDraft({ id: "d1", name: "draft 1" }));

    expect(result.current.tabs).toHaveLength(afterFirst);
    expect(result.current.activeTab).toBe("draft:d1");
  });

  it("TAB-FR-17, TAB-FR-04: a draft and an artifact of the same name coexist in the strip", async () => {
    draftBackend();
    const { result } = renderHook(() => useShellSession());
    await act(async () => result.current.loadProject(handle("a", "/a")));

    act(() => result.current.openDraft({ id: "spec.md", name: "spec.md" }));
    act(() =>
      result.current.openArtifact({ id: "spec.md", name: "spec.md" }),
    );

    // Two tabs, two ids: `draft:` and `art:` never collide, which is the whole
    // point of keying a draft tab by the draft's id rather than by a path.
    expect(result.current.tabs.some((t) => t.id === "draft:spec.md")).toBe(true);
    expect(result.current.tabs.some((t) => t.id === "art:spec.md")).toBe(true);
    expect(result.current.activeTab).toBe("art:spec.md");
  });

  it("NAW-FR-13 / SNV-FR-28: File → Save reaches the active draft tab", async () => {
    draftBackend();
    const { result } = renderHook(() => useShellSession());
    await act(async () => result.current.loadProject(handle("a", "/a")));
    await act(async () => await result.current.createDraft({}));

    // Clean draft: nothing to save.
    expect(result.current.saveEnabled).toBe(false);
    expect(result.current.saveAllEnabled).toBe(false);

    act(() => edit(result.current.drafts, "d1", "typed"));

    expect(result.current.saveEnabled).toBe(true);
    expect(result.current.saveAllEnabled).toBe(true);

    await act(async () => await result.current.requestSave());
    expect(
      invokeMock.mock.calls.some((c) => c[0] === "save_draft_file_contents"),
    ).toBe(true);
    expect(result.current.saveEnabled).toBe(false);
  });

  it("NAW-FR-23 / NAW-FR-24: a teardown writes the dirty draft before proceeding", async () => {
    draftBackend();
    const { result } = renderHook(() => useShellSession());
    await act(async () => result.current.loadProject(handle("a", "/a")));
    await act(async () => await result.current.createDraft({}));
    act(() => edit(result.current.drafts, "d1", "unsaved"));

    let proceeded = false;
    await act(async () => {
      proceeded = await result.current.flushBeforeTeardown();
    });

    expect(proceeded).toBe(true);
    const save = invokeMock.mock.calls.find(
      (c) => c[0] === "save_draft_file_contents",
    );
    expect(save).toBeTruthy();
    expect((save![1] as { body: string }).body).toBe("unsaved");
  });

  it("a draft whose write fails cancels the teardown and focuses its tab", async () => {
    draftBackend();
    const { result } = renderHook(() => useShellSession());
    await act(async () => result.current.loadProject(handle("a", "/a")));
    await act(async () => await result.current.createDraft({}));
    act(() => edit(result.current.drafts, "d1", "unsaved"));
    act(() => result.current.activateTab("dashboard"));

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "save_draft_file_contents") throw "disk full";
      return undefined;
    });

    let proceeded = true;
    await act(async () => {
      proceeded = await result.current.flushBeforeTeardown();
    });

    // Nothing is torn down over an unwritable draft, and the author is put in
    // front of the tab that blocked it.
    expect(proceeded).toBe(false);
    expect(result.current.activeTab).toBe("draft:d1");
  });

  it("NAW-FR-23 / TAB-FR-18: closing a draft tab writes it, and keeps the draft", async () => {
    draftBackend();
    const { result } = renderHook(() => useShellSession());
    await act(async () => result.current.loadProject(handle("a", "/a")));
    await act(async () => await result.current.createDraft({}));
    act(() => edit(result.current.drafts, "d1", "typed then closed"));

    await act(async () => await result.current.closeTab("draft:d1"));

    const save = invokeMock.mock.calls.find(
      (c) => c[0] === "save_draft_file_contents",
    );
    expect(save).toBeTruthy();
    expect((save![1] as { body: string }).body).toBe("typed then closed");
    expect(result.current.tabs.some((t) => t.draftId === "d1")).toBe(false);
    // NAW-FR-23: the tab is gone, the draft is not — closing what was looking
    // at a draft deletes nothing, and its session is still there to reopen on.
    expect(
      invokeMock.mock.calls.some((c) => c[0] === "delete_draft"),
    ).toBe(false);
    expect(result.current.drafts.get("d1")).toBeDefined();
  });

  it("TAB-FR-18: a draft whose write fails refuses the close and stays focused", async () => {
    draftBackend();
    const { result } = renderHook(() => useShellSession());
    await act(async () => result.current.loadProject(handle("a", "/a")));
    await act(async () => await result.current.createDraft({}));
    act(() => edit(result.current.drafts, "d1", "unwritable"));

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "save_draft_file_contents") throw "disk full";
      return undefined;
    });

    await act(async () => await result.current.closeTab("draft:d1"));

    // The tab stays open in front of the author, with the content still in it.
    expect(result.current.tabs.some((t) => t.draftId === "d1")).toBe(true);
    expect(result.current.activeTab).toBe("draft:d1");
    expect(result.current.drafts.isDirty("d1")).toBe(true);
  });

  it("NAW-FR-20 / DRP-FR-12: dropping a draft tab discards its retained buffer", async () => {
    draftBackend();
    const { result } = renderHook(() => useShellSession());
    await act(async () => result.current.loadProject(handle("a", "/a")));
    await act(async () => await result.current.createDraft({}));
    act(() => edit(result.current.drafts, "d1", "gone with it"));

    act(() => result.current.dropDraftTab("d1"));

    expect(result.current.tabs.some((t) => t.draftId === "d1")).toBe(false);
    expect(result.current.drafts.get("d1")).toBeUndefined();
    // TAB-FR-15: the strip never empties.
    expect(result.current.tabs.length).toBeGreaterThan(0);
  });

  it("NAW-FR-04: renaming a draft relabels its tab", async () => {
    draftBackend();
    const { result } = renderHook(() => useShellSession());
    await act(async () => result.current.loadProject(handle("a", "/a")));
    await act(async () => await result.current.createDraft({}));

    act(() => result.current.renameDraftTab("d1", "artifact-window"));

    expect(result.current.tabs.find((t) => t.draftId === "d1")?.label).toBe(
      "artifact-window",
    );
  });

  it("NAW-FR-25: a rename carries a *background* draft's unsaved buffer to the renamed file", async () => {
    // The New Artifact tab reconciles itself while it is mounted, but it is
    // mounted only while it is the active tab — and the session it would
    // reconcile deliberately outlives it. A draft renamed from the Drafts panel
    // while its tab sits in the background would otherwise keep its buffer keyed
    // to the old path, and the next flush writes it back, recreating the file
    // the rename moved away from.
    let primary = "Untitled.md";
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "create_draft")
        return {
          draft: {
            id: "d1",
            name: "Untitled",
            promptPath: primary,
            status: "active",
            createdAt: "2026-07-31T10:00:00Z",
            updatedAt: "2026-07-31T10:00:00Z",
          },
          file: primary,
        };
      if (cmd === "open_draft")
        return {
          id: "d1",
          name: "overview",
          promptPath: primary,
          status: "active",
          createdAt: "2026-07-31T10:00:00Z",
          updatedAt: "2026-07-31T10:00:00Z",
        };
      if (cmd === "save_draft_file_contents") return { checksum: "c" };
      return undefined;
    });
    const { result } = renderHook(() => useShellSession());
    await act(async () => result.current.loadProject(handle("a", "/a")));
    await act(async () => await result.current.createDraft({}));
    act(() => edit(result.current.drafts, "d1", "typed before the rename"));
    // The tab goes to the background — the workspace unmounts, its buffer does not.
    act(() => result.current.activateTab("dashboard"));

    // The rename lands on disk; the panel reports it the only way it can.
    primary = "overview.md";
    await act(async () => {
      result.current.renameDraftTab("d1", "overview");
    });

    await waitFor(() =>
      expect(
        result.current.drafts.docs.get(result.current.drafts.key("d1", "overview.md"))
          ?.buffer,
      ).toBe("typed before the rename"),
    );
    expect(
      result.current.drafts.docs.get(result.current.drafts.key("d1", "Untitled.md")),
    ).toBeUndefined();

    // And the flush that follows writes the file that exists rather than
    // recreating the one the rename moved away from.
    invokeMock.mockClear();
    await act(async () => {
      await result.current.drafts.flush("d1");
    });
    const written = invokeMock.mock.calls
      .filter((c) => c[0] === "save_draft_file_contents")
      .map((c) => (c[1] as { path: string }).path);
    expect(written).toEqual(["overview.md"]);
  });
});
