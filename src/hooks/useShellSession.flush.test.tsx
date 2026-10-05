import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook } from "@testing-library/react";
import { useShellSession } from "./useShellSession";
import { addNode } from "../state/flowDocument";
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


describe("useShellSession.openAnother (OVW-FR-04/05)", () => {
  it("resets the shell to its per-project defaults and returns to the picker", async () => {
    const { result } = renderHook(() => useShellSession());

    act(() => result.current.loadProject(handle("acme", "~/dev/acme")));
    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));
    act(() => result.current.toggleBottomSurface("runs"));
    await act(async () => {
      await result.current.openAnother();
    });

    expect(result.current.screen).toBe("picker");
    expect(result.current.tabs).toEqual([{ id: "dashboard", label: "Dashboard" }]);
    expect(result.current.activeTab).toBe("dashboard");
    expect(result.current.bottomVisible).toBe(false);
    expect(result.current.projectPath).toBe("");
  });
});


// TAB-FR-10/TAB-FR-11 + EDT-FR-31/EDT-FR-32: closing an Editor tab writes its
// pending changes first, and refuses to close when that write cannot proceed.
describe("useShellSession.closeTab flush (TAB-FR-10 / TAB-FR-11)", () => {
  const openDirty = (id = "a/x.md") => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openArtifact({ id, name: "x.md" }));
    seedDirty(result.current.sessions, id, "v1");
    return result;
  };

  // TAB-FR-10 / EDT-FR-24, EDT-FR-29, EDT-FR-31: the changes are written, the tab goes, the artifact's
  // edit state stays.
  it("writes pending changes and retains the artifact's history", async () => {
    const result = openDirty();

    await act(async () => {
      await result.current.closeTab("art:a/x.md");
    });

    expect(savedIds()).toEqual(["a/x.md"]);
    expect(result.current.tabs.some((t) => t.id === "art:a/x.md")).toBe(false);
    // EDT-FR-28: retained past the tab, and flagged for revalidation on reopen.
    const retained = result.current.sessions.get("a/x.md");
    expect(retained?.dirty).toBe(false);
    expect(retained?.revalidate).toBe(true);
    expect(retained?.history.steps).toHaveLength(2);
  });

  // TAB-FR-11 / EDT-FR-32: an unresolved divergence refuses the close and the
  // tab is focused so the user sees the modal.
  it("refuses to close over an unresolved external change and focuses the tab", async () => {
    const result = openDirty();
    act(() =>
      result.current.sessions.update("a/x.md", { conflict: true, pending: "ck2" }),
    );
    act(() => result.current.activateTab("dashboard"));

    await act(async () => {
      await result.current.closeTab("art:a/x.md");
    });

    expect(savedIds()).toEqual([]);
    expect(result.current.tabs.some((t) => t.id === "art:a/x.md")).toBe(true);
    expect(result.current.activeTab).toBe("art:a/x.md");
  });

  // EDT-FR-70 / EDT-FR-24: an emptied buffer is no longer a blocker. The close
  // brings its write forward like any other and the tab goes, because undo — not
  // a modal — is what recovers a file the author emptied by accident.
  it("closes over an emptied buffer, writing it", async () => {
    const result = openDirty();
    act(() => {
      result.current.sessions.ensure("a/x.md").buffer = "";
    });

    await act(async () => {
      await result.current.closeTab("art:a/x.md");
    });

    expect(savedIds()).toEqual(["a/x.md"]);
    expect(result.current.tabs.some((t) => t.id === "art:a/x.md")).toBe(false);
  });

  it("refuses to close when the write fails", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "save_artifact_contents") throw new Error("disk full");
      return undefined;
    });
    const result = openDirty();

    await act(async () => {
      await result.current.closeTab("art:a/x.md");
    });

    expect(result.current.tabs.some((t) => t.id === "art:a/x.md")).toBe(true);
    expect(result.current.sessions.get("a/x.md")?.error).toMatch(/disk full/);
  });

  // TAB-FR-11 (second half) / EDT-FR-32: the refusal is not a dead end — once
  // the user resolves the blocker, closing again works and still retains the
  // artifact's history. This is the only route by which a diverged tab can ever
  // close, so a regression here would wedge the tab permanently.
  it("closes once the divergence is resolved", async () => {
    const result = openDirty();
    act(() =>
      result.current.sessions.update("a/x.md", { conflict: true, pending: "ck2" }),
    );
    await act(async () => {
      await result.current.closeTab("art:a/x.md");
    });
    expect(result.current.tabs.some((t) => t.id === "art:a/x.md")).toBe(true);

    // "Keep my version" (EXC-FR-WDEJ): adopt the acknowledged checksum, drop the modal.
    act(() =>
      result.current.sessions.update("a/x.md", {
        conflict: false,
        baseline: "ck2",
        pending: null,
      }),
    );
    await act(async () => {
      await result.current.closeTab("art:a/x.md");
    });

    expect(savedIds()).toEqual(["a/x.md"]);
    expect(result.current.tabs.some((t) => t.id === "art:a/x.md")).toBe(false);
    expect(result.current.sessions.get("a/x.md")?.history.steps).toHaveLength(2);
  });

  // EDT-FR-32 without a dirty buffer: an artifact the user only READ can still
  // be sitting on the external-change modal, and closing over it would dismiss
  // the divergence and silently adopt one of the two versions.
  it("refuses to close a clean tab that is showing an unresolved divergence", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));
    act(() => result.current.sessions.adoptLoad("a/x.md", "v1", "ck1"));
    act(() =>
      result.current.sessions.update("a/x.md", { conflict: true, pending: "ck2" }),
    );

    await act(async () => {
      await result.current.closeTab("art:a/x.md");
    });

    expect(result.current.tabs.some((t) => t.id === "art:a/x.md")).toBe(true);
    expect(result.current.sessions.get("a/x.md")?.conflict).toBe(true);
  });

  // A tab with no unsaved changes closes without writing anything.
  it("writes nothing when the artifact is clean", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));
    act(() => result.current.sessions.adoptLoad("a/x.md", "v1", "ck1"));

    await act(async () => {
      await result.current.closeTab("art:a/x.md");
    });

    expect(savedIds()).toEqual([]);
    expect(result.current.tabs.some((t) => t.id === "art:a/x.md")).toBe(false);
  });
});


// EDT-FR-33 / SNV-FR-25 / SNV-FR-26 / OVW-FR-11: every teardown writes first,
// and a blocked write cancels the teardown outright.
describe("useShellSession teardown flush (EDT-FR-33)", () => {
  const openTwoDirty = () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.loadProject(handle("acme", "~/dev/acme")));
    act(() => result.current.openArtifact({ id: "a.md", name: "a.md" }));
    act(() => result.current.openArtifact({ id: "b.md", name: "b.md" }));
    seedDirty(result.current.sessions, "a.md", "v1");
    seedDirty(result.current.sessions, "b.md", "w1");
    return result;
  };

  // SNV-FR-25, EDT-FR-33 / EDT-FR-28: File → Close project writes both artifacts, tears the
  // project down, and returns to the picker with the retained state discarded.
  it("writes every dirty artifact before closing the project", async () => {
    const result = openTwoDirty();

    await act(async () => {
      await result.current.requestCloseProject();
    });

    expect(savedIds()).toEqual(["a.md", "b.md"]);
    // The backend teardown runs only after the writes (a closed project cannot
    // be saved to). Three commands are filtered out because none is part of
    // the write-then-tear-down ordering: `set_save_menu_state`, pushed whenever
    // the Save enablement changes (SNV-FR-28 / SNV-FR-30), which a flush does;
    // `set_find_menu_state`, pushed whenever the active tab's type changes
    // (SNV-FR-43); and `get_active_worktree`, the one-shot label fetch the
    // chrome's worktree selector makes when a project opens (WTS-FR-03).
    const order = invokeMock.mock.calls
      .map((c) => c[0])
      .filter(
        (cmd) =>
          cmd !== "set_save_menu_state" &&
          cmd !== "set_find_menu_state" &&
          cmd !== "get_active_worktree",
      );
    expect(order).toEqual([
      "save_artifact_contents",
      "save_artifact_contents",
      "close_project",
    ]);
    expect(result.current.screen).toBe("picker");
    expect(result.current.sessions.get("a.md")).toBeUndefined();
  });

  // SNV-FR-25, SNV-FR-26, EDT-FR-32: a blocker cancels the close — the project stays open, the tab is
  // focused, and nothing is torn down.
  it("cancels the project close when a write is blocked", async () => {
    const result = openTwoDirty();
    act(() => result.current.sessions.update("b.md", { conflict: true }));
    act(() => result.current.activateTab("dashboard"));

    await act(async () => {
      await result.current.requestCloseProject();
    });

    expect(result.current.screen).toBe("ide");
    expect(result.current.activeTab).toBe("art:b.md");
    expect(invokeMock.mock.calls.map((c) => c[0])).not.toContain("close_project");
    expect(result.current.sessions.get("b.md")?.dirty).toBe(true);
  });

  // SNV-FR-25, EDT-FR-32 (Exit half) / SNV-FR-26: the held quit is released only after the
  // writes, and answered with `false` when one is blocked so the app keeps running.
  it("answers the held quit with the flush outcome", async () => {
    const result = openTwoDirty();

    await act(async () => {
      await result.current.requestExit();
    });

    expect(savedIds()).toEqual(["a.md", "b.md"]);
    expect(invokeMock).toHaveBeenCalledWith("finish_exit", { proceed: true });
  });

  // SNV-FR-26: the backend re-announces a held quit each time the user asks
  // again — clicking the window's close button twice because nothing visibly
  // happened is the expected case. A second pass must not start while the first
  // is still writing: it would race the writes in flight and could answer the
  // hold (quitting the application) mid-write.
  it("ignores a repeated quit while the first one is still writing", async () => {
    const result = openTwoDirty();

    // Park every write until released, so the repeat is guaranteed to arrive
    // mid-flush rather than after it.
    let release = () => {};
    const parked = new Promise<void>((r) => {
      release = r;
    });
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "save_artifact_contents") {
        await parked;
        return { checksum: "ck-saved" };
      }
      return undefined;
    });

    let first: Promise<void> | null = null;
    let repeat: Promise<void> | null = null;
    try {
      await act(async () => {
        first = result.current.requestExit();
        // The repeat lands while the first write is parked. It must return
        // without waiting on anything — a repeat that fell through would block
        // on `parked` instead, so this settles only if the guard held.
        repeat = result.current.requestExit();
        await repeat;
      });

      expect(invokeMock).not.toHaveBeenCalledWith(
        "finish_exit",
        expect.anything(),
      );
    } finally {
      // Unpark whatever is waiting even if an assertion above threw: a promise
      // left parked poisons every test that runs after this one.
      await act(async () => {
        release();
        await first;
      });
    }

    // One pass, one answer — the repeat neither re-wrote nor answered early.
    expect(savedIds()).toEqual(["a.md", "b.md"]);
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "finish_exit"),
    ).toHaveLength(1);
  });

  // The other half of the guard: it is a re-entrancy guard, not a latch. If it
  // were never released, the first quit would work and every later one would be
  // silently swallowed — an application that cannot be closed, which is the
  // failure this whole path exists to prevent.
  it("holds a later quit again once the first one has answered", async () => {
    const result = openTwoDirty();
    act(() => result.current.sessions.update("a.md", { conflict: true }));

    await act(async () => {
      await result.current.requestExit();
    });
    expect(invokeMock).toHaveBeenCalledWith("finish_exit", { proceed: false });

    // The user resolves the blocker and asks again.
    act(() => result.current.sessions.update("a.md", { conflict: false }));
    invokeMock.mockClear();
    await act(async () => {
      await result.current.requestExit();
    });

    expect(savedIds()).toEqual(["a.md", "b.md"]);
    expect(invokeMock).toHaveBeenCalledWith("finish_exit", { proceed: true });
  });

  it("cancels the quit when a write is blocked", async () => {
    const result = openTwoDirty();
    act(() => result.current.sessions.update("a.md", { conflict: true }));

    await act(async () => {
      await result.current.requestExit();
    });

    expect(savedIds()).toEqual([]);
    expect(invokeMock).toHaveBeenCalledWith("finish_exit", { proceed: false });
    // EDT-FR-33: the blocking tab is focused so the user can see what stopped it.
    expect(result.current.activeTab).toBe("art:a.md");
  });

  // OVW-FR-11, EDT-FR-28, EDT-FR-33: the gate the project switcher consults before it touches the
  // backend — true once everything is written, false (and focused) when blocked.
  it("gates a project switch on the outgoing project's writes", async () => {
    const result = openTwoDirty();

    let proceed = false;
    await act(async () => {
      proceed = await result.current.flushBeforeTeardown();
    });
    expect(proceed).toBe(true);
    expect(savedIds()).toEqual(["a.md", "b.md"]);

    seedDirty(result.current.sessions, "a.md", "v2");
    act(() => result.current.sessions.update("a.md", { conflict: true }));
    act(() => result.current.activateTab("dashboard"));
    await act(async () => {
      proceed = await result.current.flushBeforeTeardown();
    });
    expect(proceed).toBe(false);
    expect(result.current.activeTab).toBe("art:a.md");
  });

  // EDT-FR-28: leaving for the picker discards the project's retained state —
  // after it has been written.
  it("writes and then discards retained state when leaving for the picker", async () => {
    const result = openTwoDirty();

    await act(async () => {
      await result.current.openAnother();
    });

    expect(savedIds()).toEqual(["a.md", "b.md"]);
    expect(result.current.screen).toBe("picker");
    expect(result.current.sessions.get("a.md")).toBeUndefined();
  });
});


// SNV-FR-28..31 / EDT-FR-34..36 / `FLO-flow.md` FLO-FR-27: the File menu's Save and Save
// All. Enablement is computed here and pushed to the native menu, and the two
// actions differ in scope — Save writes the active tab alone, Save All sweeps
// everything unsaved.
describe("File menu Save / Save All (SNV-FR-28..31)", () => {
  /** The most recent enablement pushed to the native menu. */
  const lastMenuState = () => {
    const calls = invokeMock.mock.calls.filter(
      (c) => c[0] === "set_save_menu_state",
    );
    return calls.length
      ? (calls[calls.length - 1][1] as { save: boolean; saveAll: boolean })
      : null;
  };

  /** A dirty Flow on a Flow tab, as a canvas edit would have left it. */
  const seedDirtyFlow = async (
    result: { current: ReturnType<typeof useShellSession> },
    id: string,
  ) => {
    await act(async () => {
      result.current.openArtifact({ id, name: id, artifactType: "flow" });
      await result.current.flows.get(id)?.pendingLoad;
    });
    act(() =>
      result.current.flows.applyEdit(id, (d) => addNode(d, { x: 40, y: 40 })),
    );
  };

  const openIde = () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.loadProject(handle("acme", "~/dev/acme")));
    return result;
  };

  // SNV-FR-28: a clean Editor tab greys Save out; editing it enables Save.
  it("enables Save only once the active Editor tab holds unsaved changes", () => {
    const result = openIde();
    act(() => result.current.openArtifact({ id: "a.md", name: "a.md" }));
    expect(result.current.saveEnabled).toBe(false);

    act(() => seedDirty(result.current.sessions, "a.md", "v1"));

    expect(result.current.saveEnabled).toBe(true);
    expect(lastMenuState()?.save).toBe(true);
  });

  // DFV-FR-05 / SNV-FR-28: every tab type that owns no savable content greys
  // Save out. Neither settings surface is among them: each is a native child
  // window rather than a tab (SWN-FR-01), and what one holds is written by the
  // window itself (SWN-FR-08) — the case below asserts that separately, because
  // a settings window does not even make a tab active to test against.
  it("greys Save out on every tab that owns no savable content", () => {
    const result = openIde();
    act(() => result.current.openArtifact({ id: "a.md", name: "a.md" }));
    act(() => seedDirty(result.current.sessions, "a.md", "v1"));
    expect(result.current.saveEnabled).toBe(true);

    act(() => result.current.openSearchResults("q", "literal_insensitive"));
    expect(result.current.saveEnabled).toBe(false);

    act(() => result.current.activateTab("dashboard"));
    expect(result.current.saveEnabled).toBe(false);

    // …while Save All stays enabled, because something is still unsaved.
    expect(result.current.saveAllEnabled).toBe(true);
  });

  // SNV-FR-28: "A settings window is no tab and is never what Save acts on."
  // Opening one changes neither the active tab nor Save's state — and while one
  // is open the menu takes no interaction at all (SWN-FR-02), which is the
  // backend's guard rather than this hook's.
  it("SNV-FR-28: a settings window leaves Save exactly as it found it", () => {
    const result = openIde();
    act(() => result.current.openArtifact({ id: "a.md", name: "a.md" }));
    act(() => seedDirty(result.current.sessions, "a.md", "v1"));
    expect(result.current.saveEnabled).toBe(true);
    const activeBefore = result.current.activeTab;

    act(() => result.current.openGlobalSettings());
    act(() => result.current.openSettings());

    expect(result.current.activeTab).toBe(activeBefore);
    expect(result.current.saveEnabled).toBe(true);
  });

  // SNV-FR-28, SNV-FR-29, FLO-FR-27 / FLO-FR-21, FLO-FR-26: a dirty Flow tab enables Save, and Save writes the
  // serialized document through `"save artifact contents"` (FLO-FR-27).
  it("enables Save for a dirty Flow tab and writes that Flow", async () => {
    const result = openIde();
    await seedDirtyFlow(result, FLOW_ID);

    expect(result.current.saveEnabled).toBe(true);
    await act(async () => {
      await result.current.requestSave();
    });

    expect(result.current.flows.get(FLOW_ID)?.dirty).toBe(false);
    // FLO introduces no backend operation of its own: a Flow is written by the
    // same command an artifact is.
    expect(savedIds()).toEqual([FLOW_ID]);
  });

  // SNV-FR-28, SNV-FR-29: Save writes exactly the active tab's artifact.
  it("Save writes only the active tab, leaving other unsaved artifacts alone", async () => {
    const result = openIde();
    act(() => result.current.openArtifact({ id: "a.md", name: "a.md" }));
    act(() => result.current.openArtifact({ id: "b.md", name: "b.md" }));
    act(() => seedDirty(result.current.sessions, "a.md", "v1"));
    act(() => seedDirty(result.current.sessions, "b.md", "w1"));
    act(() => result.current.activateTab("art:b.md"));

    await act(async () => {
      await result.current.requestSave();
    });

    expect(savedIds()).toEqual(["b.md"]);
    // The written tab clears; the untouched one keeps its unsaved state, so
    // Save All is still on offer for it (SNV-FR-30).
    expect(result.current.sessions.get("b.md")?.dirty).toBe(false);
    expect(result.current.sessions.get("a.md")?.dirty).toBe(true);
    expect(result.current.saveEnabled).toBe(false);
    expect(result.current.saveAllEnabled).toBe(true);
  });

  // The gate behind SNV-FR-28: a Save that arrives while the item is greyed
  // must not write. Without it a desynced menu could write a clean buffer and
  // promote the artifact in the recently-edited MRU for no reason.
  it("Save writes nothing when it is not enabled", async () => {
    const result = openIde();
    act(() => result.current.openArtifact({ id: "a.md", name: "a.md" }));

    await act(async () => {
      await result.current.requestSave();
    });

    expect(savedIds()).toEqual([]);
  });

  // SNV-FR-30: Save All tracks the whole session, not the active tab.
  it("enables Save All whenever anything is unsaved, whichever tab is active", () => {
    const result = openIde();
    expect(result.current.saveAllEnabled).toBe(false);
    expect(lastMenuState()).toEqual({ save: false, saveAll: false });

    act(() => result.current.openArtifact({ id: "a.md", name: "a.md" }));
    act(() => seedDirty(result.current.sessions, "a.md", "v1"));
    act(() => result.current.activateTab("dashboard"));

    expect(result.current.saveAllEnabled).toBe(true);
    expect(lastMenuState()).toEqual({ save: false, saveAll: true });
  });

  // SNV-FR-30, SNV-FR-31 / EDT-FR-35: Save All writes every dirty artifact and Flow,
  // skips the clean ones, and greys itself out afterwards.
  it("Save All writes every unsaved artifact and Flow, and nothing clean", async () => {
    const result = openIde();
    act(() => result.current.openArtifact({ id: "a.md", name: "a.md" }));
    act(() => result.current.openArtifact({ id: "b.md", name: "b.md" }));
    act(() => result.current.openArtifact({ id: "clean.md", name: "clean.md" }));
    act(() => result.current.sessions.adoptLoad("clean.md", "x", "ck1"));
    act(() => seedDirty(result.current.sessions, "a.md", "v1"));
    act(() => seedDirty(result.current.sessions, "b.md", "w1"));
    await seedDirtyFlow(result, FLOW_ID);

    await act(async () => {
      await result.current.requestSaveAll();
    });

    expect(savedIds()).toEqual(["a.md", "b.md", FLOW_ID]);
    expect(result.current.flows.get(FLOW_ID)?.dirty).toBe(false);
    expect(result.current.saveAllEnabled).toBe(false);
    expect(lastMenuState()?.saveAll).toBe(false);
  });

  // SNV-FR-31, EDT-FR-36 / EDT-FR-32: a blocked write does not abort the sweep, and the
  // blocker is focused so the user can resolve it.
  it("Save All finishes the other writes and focuses the blocked artifact", async () => {
    const result = openIde();
    act(() => result.current.openArtifact({ id: "a.md", name: "a.md" }));
    act(() => result.current.openArtifact({ id: "b.md", name: "b.md" }));
    act(() => result.current.openArtifact({ id: "c.md", name: "c.md" }));
    act(() => seedDirty(result.current.sessions, "a.md", "v1"));
    act(() => seedDirty(result.current.sessions, "b.md", "w1"));
    act(() => seedDirty(result.current.sessions, "c.md", "z1"));
    // b.md is sitting on an unresolved external-change modal (EDT-FR-32).
    act(() => result.current.sessions.update("b.md", { conflict: true }));
    act(() => result.current.activateTab("dashboard"));

    await act(async () => {
      await result.current.requestSaveAll();
    });

    // The unblocked pair is written — the sweep did not stop at b.md…
    expect(savedIds()).toEqual(["a.md", "c.md"]);
    expect(result.current.sessions.get("b.md")?.dirty).toBe(true);
    // …and the blocked artifact's tab is focused with its modal visible.
    expect(result.current.activeTab).toBe("art:b.md");
    // Save All stays available so the user can invoke it again once they have
    // resolved the blocker (EDT-FR-36).
    expect(result.current.saveAllEnabled).toBe(true);
    expect(lastMenuState()?.saveAll).toBe(true);

    // Resolving it and invoking Save All again writes it (EDT-FR-36).
    act(() => result.current.sessions.update("b.md", { conflict: false }));
    await act(async () => {
      await result.current.requestSaveAll();
    });
    expect(savedIds()).toEqual(["a.md", "c.md", "b.md"]);
  });

  // EDT-FR-36: a blocker nobody is looking at is one nobody can resolve, so a
  // tab is opened for it.
  it("Save All opens a tab for a blocked artifact that has none", async () => {
    const result = openIde();
    act(() => seedDirty(result.current.sessions, "deep/x.md", "v1"));
    act(() => result.current.sessions.update("deep/x.md", { conflict: true }));

    await act(async () => {
      await result.current.requestSaveAll();
    });

    expect(result.current.activeTab).toBe("art:deep/x.md");
    expect(
      result.current.tabs.find((t) => t.id === "art:deep/x.md")?.label,
    ).toBe("x.md");
  });
});


/**
 * A close that has to write first is not instantaneous, so the user can act on
 * the strip while it is in flight. Focus has to be decided against the strip as
 * it stands when the write lands, not as it stood when the close was requested.
 */
describe("closeTab focus under a slow write", () => {
  /** Hold `save_artifact_contents` open until the returned `release` is called. */
  const gateWrites = () => {
    let release!: (v: unknown) => void;
    const gate = new Promise((r) => (release = r));
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "save_artifact_contents") {
        await gate;
        return { checksum: "ck-saved" };
      }
      return undefined;
    });
    return () => release(null);
  };

  it("leaves no dangling activeTab when the user re-activates the closing tab mid-flush", async () => {
    const release = gateWrites();

    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));
    seedDirty(result.current.sessions, "a/x.md", "v1");
    // Focus the Dashboard, so the close of the Editor tab starts with
    // activeTab === "dashboard" captured in the closure.
    act(() => result.current.activateTab("dashboard"));

    let closing!: Promise<boolean>;
    act(() => {
      closing = result.current.closeTab("art:a/x.md");
    });
    // While the write is in flight the tab is still in the strip, so the user
    // can click it — which is what makes the captured activeTab stale.
    act(() => result.current.activateTab("art:a/x.md"));

    await act(async () => {
      release();
      await closing;
    });

    expect(result.current.tabs.map((t) => t.id)).toEqual(["dashboard"]);
    expect(
      result.current.tabs.some((t) => t.id === result.current.activeTab),
    ).toBe(true);
  });

  // The other half of the same race: the tab being closed *was* the active one,
  // and the user moves focus elsewhere while its write runs. That choice stands
  // — the completing close must not drag focus back off it.
  it("keeps a focus the user moved elsewhere while the write was in flight", async () => {
    const release = gateWrites();

    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openArtifact({ id: "a/x.md", name: "x.md" }));
    act(() => result.current.openArtifact({ id: "a/y.md", name: "y.md" }));
    seedDirty(result.current.sessions, "a/x.md", "v1");
    act(() => result.current.activateTab("art:a/x.md"));

    let closing!: Promise<boolean>;
    act(() => {
      closing = result.current.closeTab("art:a/x.md");
    });
    act(() => result.current.activateTab("dashboard"));

    await act(async () => {
      release();
      await closing;
    });

    expect(result.current.tabs.map((t) => t.id)).toEqual([
      "dashboard",
      "art:a/y.md",
    ]);
    expect(result.current.activeTab).toBe("dashboard");
  });
});
