import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { useShellSession } from "./useShellSession";
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


// ---------------------------------------------------------------------------
// Rollback orchestration (CHG-FR-60, EDT-FR-81, EDT-FR-72 / CHG-FR-63, EXC-FR-JAWT, EXC-FR-ZXWI, TAB-FR-41, TAB-FR-42 / EXC-FR-CWUC / TAB-FR-22, DFV-FR-58)
// ---------------------------------------------------------------------------

/** A `rollback_paths` outcome entry (GTC-FR-25). */
const rollbackEntry = (
  path: string,
  over: Record<string, unknown> = {},
) => ({
  id: path,
  path,
  previousPath: null,
  outcome: "restored",
  restoredPaths: [path],
  removedPaths: [],
  failures: [],
  ...over,
});

describe("useShellSession.performRollback (CHG-FR-60 – CHG-FR-63)", () => {
  it("quiesces and waits for an in-flight save before reaching the backend", async () => {
    const order: string[] = [];
    let releaseSave!: () => void;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "save_artifact_contents") {
        order.push("save-started");
        return new Promise((resolve) => {
          releaseSave = () => {
            order.push("save-settled");
            resolve({ checksum: "ck-saved" });
          };
        });
      }
      if (cmd === "rollback_paths") {
        order.push("rollback");
        return { entries: [rollbackEntry("a.md")] };
      }
      if (cmd === "load_artifact_contents_by_id")
        return { body: "restored", checksum: "ck-head" };
      return undefined;
    });

    const { result } = renderHook(() => useShellSession());
    seedDirty(result.current.sessions, "a.md", "body");

    // A write is already in flight when the rollback begins.
    let writing!: Promise<unknown>;
    act(() => {
      writing = result.current.sessions.flush("a.md");
    });
    await waitFor(() => expect(order).toContain("save-started"));

    let rolling!: Promise<unknown>;
    act(() => {
      rolling = result.current.performRollback(["a.md"]);
    });
    // CHG-FR-60: the backend has not been reached while the save is
    // outstanding.
    await Promise.resolve();
    expect(order).not.toContain("rollback");

    await act(async () => {
      releaseSave();
      await writing;
      await rolling;
    });

    // The save settled first, and only then did the rollback run — nothing can
    // land on a file the rollback is about to write.
    expect(order).toEqual(["save-started", "save-settled", "rollback"]);
  });

  it("logs one ERROR per save that failed while preparing (CHG-FR-61)", async () => {
    let releaseSave!: () => void;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "save_artifact_contents") {
        return new Promise((_resolve, reject) => {
          releaseSave = () => reject(new Error("disk full"));
        });
      }
      if (cmd === "rollback_paths") return { entries: [rollbackEntry("a.md")] };
      if (cmd === "load_artifact_contents_by_id")
        return { body: "restored", checksum: "ck-head" };
      return undefined;
    });

    const { result } = renderHook(() => useShellSession());
    seedDirty(result.current.sessions, "a.md", "body");

    // A write is in flight and about to fail when the rollback begins.
    let writing!: Promise<unknown>;
    act(() => {
      writing = result.current.sessions.flush("a.md");
    });
    await waitFor(() => expect(releaseSave).toBeDefined());

    let rolling!: Promise<unknown>;
    act(() => {
      rolling = result.current.performRollback(["a.md"]);
    });
    await act(async () => {
      releaseSave();
      await writing;
      await rolling;
    });

    // CHG-FR-61: exactly one ERROR record, under the `frontend` domain, naming
    // the artifact and the typed reason in flat fields.
    const records = logErrorMock.mock.calls.filter(
      (c) => typeof c[1] === "string" && /preparing a rollback/i.test(c[1]),
    );
    expect(records).toHaveLength(1);
    expect(records[0][0]).toEqual(["frontend"]);
    expect(records[0][2]).toMatchObject({ artifact: "a.md" });
    expect(String(records[0][2].reason)).toMatch(/disk full/);
    // The failure neither blocked the rollback nor was retried.
    expect(
      invokeMock.mock.calls.some((c) => c[0] === "rollback_paths"),
    ).toBe(true);
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "save_artifact_contents"),
    ).toHaveLength(1);
  });

  it("resets a restored path and leaves a failed one untouched (CHG-FR-63)", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "save_artifact_contents") return { checksum: "ck-saved" };
      if (cmd === "load_artifact_contents_by_id")
        return { body: "restored", checksum: "ck-head" };
      if (cmd === "rollback_paths") {
        return {
          entries: [
            rollbackEntry("a.md"),
            rollbackEntry("b.md", {
              outcome: "failed",
              restoredPaths: [],
              failures: [{ path: "b.md", kind: "permission_denied" }],
            }),
          ],
        };
      }
      return undefined;
    });

    const { result } = renderHook(() => useShellSession());
    seedDirty(result.current.sessions, "a.md", "body");
    seedDirty(result.current.sessions, "b.md", "body");

    await act(async () => {
      await result.current.performRollback(["a.md", "b.md"]);
    });

    // EXC-FR-JAWT: the confirmed path is reset to what the filesystem holds.
    const a = result.current.sessions.get("a.md")!;
    expect(a.buffer).toBe("restored");
    expect(a.dirty).toBe(false);
    expect(a.quiesced).toBe(false);

    // EXC-FR-UOJF: the path the backend did not confirm keeps everything.
    const b = result.current.sessions.get("b.md")!;
    expect(b.buffer).toBe("body edited");
    expect(b.dirty).toBe(true);
    expect(b.quiesced).toBe(false);
  });

  it("closes tabs for a removed path without writing it (TAB-FR-41, EXC-FR-CWUC)", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "save_artifact_contents") return { checksum: "ck-saved" };
      if (cmd === "rollback_paths") {
        return {
          entries: [
            rollbackEntry("scratch.md", {
              outcome: "removed",
              restoredPaths: [],
              removedPaths: ["scratch.md"],
            }),
          ],
        };
      }
      return undefined;
    });

    const { result } = renderHook(() => useShellSession());
    act(() =>
      result.current.openArtifact({ id: "scratch.md", name: "scratch.md" }),
    );
    seedDirty(result.current.sessions, "scratch.md", "body");
    expect(
      result.current.tabs.some((t) => t.artifactId === "scratch.md"),
    ).toBe(true);

    await act(async () => {
      await result.current.performRollback(["scratch.md"]);
    });

    // TAB-FR-41: every tab bound to it is gone…
    expect(
      result.current.tabs.some((t) => t.artifactId === "scratch.md"),
    ).toBe(false);
    // …its session with them (EXC-FR-ZXWI)…
    expect(result.current.sessions.get("scratch.md")).toBeUndefined();
    // …and no write was made, which would have recreated the file.
    expect(savedIds()).not.toContain("scratch.md");
  });

  it("closes nothing for a restored path (TAB-FR-42, TAB-FR-22, DFV-FR-58)", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "save_artifact_contents") return { checksum: "ck-saved" };
      if (cmd === "load_artifact_contents_by_id")
        return { body: "restored", checksum: "ck-head" };
      if (cmd === "rollback_paths")
        return { entries: [rollbackEntry("a.md")] };
      return undefined;
    });

    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openArtifact({ id: "a.md", name: "a.md" }));
    seedDirty(result.current.sessions, "a.md", "body");

    await act(async () => {
      await result.current.performRollback(["a.md"]);
    });

    // TAB-FR-42: the tab stays open showing the restored content — the
    // opposite of what a commit does to a Diff tab.
    expect(result.current.tabs.some((t) => t.artifactId === "a.md")).toBe(true);
    expect(result.current.sessions.get("a.md")?.buffer).toBe("restored");
  });

  it("lifts every quiesce when the call itself is refused", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "save_artifact_contents") return { checksum: "ck-saved" };
      if (cmd === "rollback_paths") throw new Error("not a git repository");
      return undefined;
    });

    const { result } = renderHook(() => useShellSession());
    seedDirty(result.current.sessions, "a.md", "body");

    await act(async () => {
      await expect(
        result.current.performRollback(["a.md"]),
      ).rejects.toThrow(/not a git repository/);
    });

    // A session left quiesced would never write again.
    const s = result.current.sessions.get("a.md")!;
    expect(s.quiesced).toBe(false);
    expect(s.dirty).toBe(true);
    expect(s.buffer).toBe("body edited");
  });
});


describe("rollback edge cases the union across entries depends on", () => {
  it("applies each half of a half-failed rename on its own (CHG-FR-63, GTC-FR-26)", async () => {
    // GTC-FR-26: the previous path could not be restored while the current one
    // WAS removed. The removed half must still be applied — its file really is
    // gone from disk — while the restored half's session is left untouched.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "save_artifact_contents") return { checksum: "ck-saved" };
      if (cmd === "load_artifact_contents_by_id")
        return { body: "restored", checksum: "ck-head" };
      if (cmd === "rollback_paths") {
        return {
          entries: [
            {
              id: "lib.rs",
              path: "lib.rs",
              previousPath: "main.rs",
              outcome: "failed",
              restoredPaths: [],
              removedPaths: ["lib.rs"],
              failures: [{ path: "main.rs", kind: "permission_denied" }],
            },
          ],
        };
      }
      return undefined;
    });

    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openArtifact({ id: "lib.rs", name: "lib.rs" }));
    seedDirty(result.current.sessions, "lib.rs", "body");
    seedDirty(result.current.sessions, "main.rs", "body");

    await act(async () => {
      await result.current.performRollback(["lib.rs"]);
    });

    // The confirmed removal is applied: session discarded, tab closed, and no
    // write made that would recreate the file.
    expect(result.current.sessions.get("lib.rs")).toBeUndefined();
    expect(result.current.tabs.some((t) => t.artifactId === "lib.rs")).toBe(
      false,
    );
    expect(savedIds()).not.toContain("lib.rs");

    // The half that failed keeps everything, quiesce lifted.
    const main = result.current.sessions.get("main.rs")!;
    expect(main.buffer).toBe("body edited");
    expect(main.dirty).toBe(true);
    expect(main.quiesced).toBe(false);
  });

  it("closes a Diff tab bound to a removed path (TAB-FR-41, EXC-FR-CWUC)", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "save_artifact_contents") return { checksum: "ck-saved" };
      if (cmd === "get_file_revisions") return { old: "a", new: "b", isBinary: false };
      if (cmd === "rollback_paths") {
        return {
          entries: [
            rollbackEntry("scratch.md", {
              outcome: "removed",
              restoredPaths: [],
              removedPaths: ["scratch.md"],
            }),
          ],
        };
      }
      return undefined;
    });

    const { result } = renderHook(() => useShellSession());
    // One Editor tab and one Diff tab on the same file, as TAB-FR-41, EXC-FR-CWUC specifies.
    act(() =>
      result.current.openArtifact({ id: "scratch.md", name: "scratch.md" }),
    );
    act(() =>
      result.current.openDiff({
        path: "scratch.md",
        name: "scratch.md",
        scope: { kind: "path", path: "scratch.md" },
        comparisonLabel: "uncommitted",
      }),
    );
    expect(
      result.current.tabs.filter(
        (t) => t.artifactId === "scratch.md" || t.diff?.path === "scratch.md",
      ).length,
    ).toBeGreaterThanOrEqual(2);

    await act(async () => {
      await result.current.performRollback(["scratch.md"]);
    });

    // TAB-FR-41: both go, the Diff tab included — a Diff tab left open on a
    // removed file has its own write path and would recreate it.
    expect(
      result.current.tabs.some(
        (t) => t.artifactId === "scratch.md" || t.diff?.path === "scratch.md",
      ),
    ).toBe(false);
    expect(savedIds()).not.toContain("scratch.md");
  });

  it("keeps a Diff tab open for a restored path (TAB-FR-42, TAB-FR-22, DFV-FR-58)", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "save_artifact_contents") return { checksum: "ck-saved" };
      if (cmd === "get_file_revisions") return { old: "a", new: "b", isBinary: false };
      if (cmd === "load_artifact_contents_by_id")
        return { body: "restored", checksum: "ck-head" };
      if (cmd === "rollback_paths")
        return { entries: [rollbackEntry("a.md")] };
      return undefined;
    });

    const { result } = renderHook(() => useShellSession());
    act(() =>
      result.current.openDiff({
        path: "a.md",
        name: "a.md",
        scope: { kind: "path", path: "a.md" },
        comparisonLabel: "uncommitted",
      }),
    );
    seedDirty(result.current.sessions, "a.md", "body");

    await act(async () => {
      await result.current.performRollback(["a.md"]);
    });

    // TAB-FR-42: a rollback that restored closes nothing — the opposite of what
    // a commit does to a Diff tab.
    expect(result.current.tabs.some((t) => t.diff?.path === "a.md")).toBe(true);
    expect(result.current.sessions.get("a.md")?.buffer).toBe("restored");
  });

  it("does not discard a tab opened while the rollback was in flight", async () => {
    // `performRollback` awaits three times before closing tabs; a wholesale
    // setTabs built on the pre-rollback array would silently drop this one.
    let releaseRollback!: () => void;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "save_artifact_contents") return { checksum: "ck-saved" };
      if (cmd === "rollback_paths") {
        return new Promise((resolve) => {
          releaseRollback = () =>
            resolve({
              entries: [
                rollbackEntry("gone.md", {
                  outcome: "removed",
                  restoredPaths: [],
                  removedPaths: ["gone.md"],
                }),
              ],
            });
        });
      }
      return undefined;
    });

    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openArtifact({ id: "gone.md", name: "gone.md" }));
    seedDirty(result.current.sessions, "gone.md", "body");

    let rolling!: Promise<unknown>;
    act(() => {
      rolling = result.current.performRollback(["gone.md"]);
    });
    await waitFor(() => expect(releaseRollback).toBeDefined());
    // A tab opened by another route mid-rollback.
    act(() => result.current.openArtifact({ id: "later.md", name: "later.md" }));

    await act(async () => {
      releaseRollback();
      await rolling;
    });

    expect(result.current.tabs.some((t) => t.artifactId === "gone.md")).toBe(
      false,
    );
    expect(result.current.tabs.some((t) => t.artifactId === "later.md")).toBe(
      true,
    );
  });
});
