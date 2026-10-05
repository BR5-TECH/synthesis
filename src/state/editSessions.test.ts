import { beforeEach, describe, expect, it, vi } from "vitest";

import { AUTOSAVE_DELAY_MS, EditSessionStore } from "./editSessions";
import { artifactTransport } from "./documentTransport";
import { recordEdit } from "./editHistory";

const invokeMock = vi.fn();
const unlistenMock = vi.fn();
let listenCalls = 0;

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

// `reload` reports its own failure — the one error path here nobody watches
// happen. Mocked so a test can assert what it recorded, and that the document's
// body is not in it.
const logErrorMock = vi.fn();
vi.mock("../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: (...args: unknown[]) => logErrorMock(...args),
  flushLogs: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (_name: string, handler: (e: { payload: unknown }) => void) => {
    listenCalls += 1;
    // Captured as well as counted, so a test can deliver an
    // `"artifact changed externally"` the way the backend does rather than
    // reaching into the store's private divergence handling.
    externalListeners.push(handler);
    return unlistenMock;
  }),
}));

/** The store's live `"artifact changed externally"` subscribers. */
const externalListeners: Array<(e: { payload: unknown }) => void> = [];

beforeEach(() => {
  invokeMock.mockReset();
  unlistenMock.mockReset();
  logErrorMock.mockReset();
  listenCalls = 0;
  externalListeners.length = 0;
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "save_artifact_contents") return { checksum: "ck-saved" };
    throw new Error(`unexpected invoke ${cmd}`);
  });
});


/** Spin the microtask queue until `ready` holds, so a queued write can reach the gate. */
async function until(ready: () => boolean, label: string) {
  for (let i = 0; i < 200; i++) {
    if (ready()) return;
    await Promise.resolve();
  }
  throw new Error(`timed out waiting for ${label}`);
}

/** An artifact loaded with `body`, then edited to `edited`. */
function edited(store: EditSessionStore, id: string, body: string, next: string) {
  store.adoptLoad(id, body, "ck1");
  const s = store.ensure(id);
  s.buffer = next;
  recordEdit(s.history, next, "wysiwyg", "body");
  store.update(id, { dirty: true });
  return s;
}

describe("EditSessionStore retention (EDT-FR-28)", () => {
  // EDT-FR-24, EDT-FR-28: an artifact that was opened and read but never edited leaves
  // nothing behind, so reopening it is a plain first load.
  it("forgets an artifact whose tab closed without any edit", () => {
    const store = new EditSessionStore();
    store.adoptLoad("a.md", "body", "ck1");

    store.closeTab("a.md");

    expect(store.get("a.md")).toBeUndefined();
  });

  // EDT-FR-24, EDT-FR-29, EDT-FR-31: an edited artifact keeps its buffer and history past the tab.
  it("retains an edited artifact past its tab and marks it for revalidation", () => {
    const store = new EditSessionStore();
    edited(store, "a.md", "v1", "v2");

    store.closeTab("a.md");

    const s = store.get("a.md");
    expect(s?.buffer).toBe("v2");
    expect(s?.history.steps).toHaveLength(2);
    // EDT-FR-29: the next open re-checks the on-disk checksum.
    expect(s?.revalidate).toBe(true);
  });

  it("retains an artifact whose edits were saved before the close", async () => {
    const store = new EditSessionStore();
    edited(store, "a.md", "v1", "v2");
    await store.flush("a.md");

    store.closeTab("a.md");

    // Saving does not un-edit the artifact: the history is still traversable.
    expect(store.get("a.md")?.history.steps).toHaveLength(2);
    expect(store.get("a.md")?.dirty).toBe(false);
  });

  // EDT-FR-28: the store is scoped to the open project.
  it("discards everything on clear", () => {
    const store = new EditSessionStore();
    edited(store, "a.md", "v1", "v2");
    edited(store, "b.md", "w1", "w2");

    store.clear();

    expect(store.get("a.md")).toBeUndefined();
    expect(store.pendingIds()).toEqual([]);
  });

  // EDT-FR-28: state is keyed by the artifact's stable key — one artifact's
  // close must not touch another's buffer or history.
  it("keeps each artifact's session independent", () => {
    const store = new EditSessionStore();
    edited(store, "a.md", "v1", "v2");
    edited(store, "b.md", "w1", "w2");

    store.closeTab("a.md");

    expect(store.get("a.md")?.buffer).toBe("v2");
    expect(store.get("a.md")?.revalidate).toBe(true);
    expect(store.get("b.md")?.buffer).toBe("w2");
    expect(store.get("b.md")?.revalidate).toBe(false);
    expect(store.get("b.md")?.history.steps).toHaveLength(2);
  });

  it("notifies subscribers when observable state changes", () => {
    const store = new EditSessionStore();
    const seen = vi.fn();
    const unsubscribe = store.subscribe(seen);

    store.update("a.md", { dirty: true });
    expect(seen).toHaveBeenCalledTimes(1);

    unsubscribe();
    store.update("a.md", { dirty: false });
    expect(seen).toHaveBeenCalledTimes(1);
  });

  // Typing must not re-render every subscriber, so buffer/history mutation is
  // deliberately outside the notification path.
  it("does not notify for buffer or history mutation", () => {
    const store = new EditSessionStore();
    store.adoptLoad("a.md", "v1", "ck1");
    const seen = vi.fn();
    store.subscribe(seen);

    const s = store.ensure("a.md");
    s.buffer = "v2";
    recordEdit(s.history, "v2", "wysiwyg", "body");

    expect(seen).not.toHaveBeenCalled();
  });
});

describe("EditSessionStore.flush (EDT-FR-31 / EDT-FR-32)", () => {
  it("writes the buffer and adopts the returned checksum", async () => {
    const store = new EditSessionStore();
    edited(store, "a.md", "v1", "v2");

    const res = await store.flush("a.md");

    expect(res.ok).toBe(true);
    expect(invokeMock).toHaveBeenCalledWith("save_artifact_contents", {
      id: "a.md",
      body: "v2",
    });
    const s = store.get("a.md");
    expect(s?.dirty).toBe(false);
    // EXC-FR-HKKG/EXC-FR-ALXA: the written checksum is the new baseline, so the
    // watcher's echo of this write raises no modal.
    expect(s?.baseline).toBe("ck-saved");
  });

  it("writes nothing for an artifact with no unsaved changes", async () => {
    const store = new EditSessionStore();
    store.adoptLoad("a.md", "v1", "ck1");

    expect((await store.flush("a.md")).ok).toBe(true);
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("writes a clean artifact anyway when the save is explicit", async () => {
    const store = new EditSessionStore();
    store.adoptLoad("a.md", "v1", "ck1");

    expect((await store.flush("a.md", { force: true })).ok).toBe(true);
    expect(invokeMock).toHaveBeenCalledWith("save_artifact_contents", {
      id: "a.md",
      body: "v1",
    });
  });

  // EDT-FR-32 / EXC-FR-VNLZ: a flush must never auto-resolve a divergence.
  it("refuses to write while an external change is unresolved", async () => {
    const store = new EditSessionStore();
    edited(store, "a.md", "v1", "v2");
    store.update("a.md", { conflict: true, pending: "ck2" });

    const res = await store.flush("a.md");

    expect(res).toEqual({ ok: false, blocked: "conflict", artifactId: "a.md" });
    expect(invokeMock).not.toHaveBeenCalled();
    expect(store.get("a.md")?.dirty).toBe(true);
  });

  // EDT-FR-24 / EDT-FR-70: an emptied buffer is written like any other edit.
  // Nothing is asked for and nothing blocks the write — undo is what recovers a
  // file the author emptied by accident, and it recovers it in the same session
  // the write happened in.
  it("writes an emptied buffer without asking", async () => {
    const store = new EditSessionStore();
    edited(store, "a.md", "v1", "   \n ");

    const res = await store.flush("a.md");

    expect(res.ok).toBe(true);
    expect(invokeMock).toHaveBeenCalledWith("save_artifact_contents", {
      id: "a.md",
      body: "   \n ",
    });
  });

  it("writes a fully emptied buffer too", async () => {
    const store = new EditSessionStore();
    edited(store, "a.md", "v1", "");

    expect((await store.flush("a.md")).ok).toBe(true);
    expect(invokeMock).toHaveBeenCalledWith("save_artifact_contents", {
      id: "a.md",
      body: "",
    });
  });

  // EXC-FR-VNLZ / EDT-FR-32: the divergence guard is not conditional on the user
  // having edited. An artifact they only READ can be sitting on the modal, and a
  // close that wrote nothing would still have dismissed the divergence and
  // silently adopted one of the two versions.
  it("refuses a clean artifact that is sitting on an unresolved divergence", async () => {
    const store = new EditSessionStore();
    store.adoptLoad("a.md", "v1", "ck1");
    store.update("a.md", { conflict: true, pending: "ck2" });

    const res = await store.flush("a.md");

    expect(res).toEqual({ ok: false, blocked: "conflict", artifactId: "a.md" });
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("counts a clean but diverged artifact as pending, so a teardown stops on it", async () => {
    const store = new EditSessionStore();
    store.openTab("a.md");
    store.adoptLoad("a.md", "v1", "ck1");
    store.update("a.md", { conflict: true, pending: "ck2" });

    expect(store.pendingIds()).toEqual(["a.md"]);
    expect(await store.flushAll()).toEqual({
      ok: false,
      blocked: "conflict",
      artifactId: "a.md",
    });
  });

  // EDT-FR-29: a reopen's revalidating load may be about to discover that the
  // file changed. A write that races it must wait for the answer rather than
  // overwrite the divergence.
  it("waits for an in-flight load before writing, and then honours what it found", async () => {
    const store = new EditSessionStore();
    edited(store, "a.md", "v1", "v2");
    let release!: () => void;
    const s = store.ensure("a.md");
    s.pendingLoad = new Promise<void>((resolve) => {
      release = () => {
        // What the revalidating load does when the checksums differ.
        store.update("a.md", { conflict: true, pending: "ck-other" });
        s.pendingLoad = null;
        resolve();
      };
    });

    const pending = store.flush("a.md");
    expect(invokeMock).not.toHaveBeenCalled(); // still waiting
    release();

    expect(await pending).toEqual({
      ok: false,
      blocked: "conflict",
      artifactId: "a.md",
    });
    expect(invokeMock).not.toHaveBeenCalled();
  });

  // EDT-FR-32: a failed write refuses too — the changes stay unsaved and
  // whichever tab shows the artifact surfaces the error.
  it("reports a failed save and keeps the artifact dirty", async () => {
    invokeMock.mockImplementation(async () => {
      throw new Error("disk full");
    });
    const store = new EditSessionStore();
    edited(store, "a.md", "v1", "v2");

    const res = await store.flush("a.md");

    expect(res).toEqual({ ok: false, blocked: "error", artifactId: "a.md" });
    expect(store.get("a.md")?.dirty).toBe(true);
    expect(store.get("a.md")?.error).toMatch(/disk full/);
  });
});

describe("EditSessionStore.flushAll (EDT-FR-33)", () => {
  it("writes every artifact holding unsaved changes", async () => {
    const store = new EditSessionStore();
    edited(store, "a.md", "v1", "v2");
    edited(store, "b.md", "w1", "w2");
    store.adoptLoad("c.md", "clean", "ck1"); // untouched -> not written

    const res = await store.flushAll();

    expect(res.ok).toBe(true);
    const saved = invokeMock.mock.calls
      .filter((c) => c[0] === "save_artifact_contents")
      .map((c) => (c[1] as { id: string }).id);
    expect(saved).toEqual(["a.md", "b.md"]);
    expect(store.pendingIds()).toEqual([]);
  });

  // EDT-FR-32: one blocker cancels the whole teardown and names itself, so the
  // caller can focus that tab.
  it("stops at the first blocked artifact and reports which one", async () => {
    const store = new EditSessionStore();
    edited(store, "a.md", "v1", "v2");
    edited(store, "b.md", "w1", "w2");
    store.update("b.md", { conflict: true });
    edited(store, "c.md", "x1", "x2");

    const res = await store.flushAll();

    expect(res).toEqual({ ok: false, blocked: "conflict", artifactId: "b.md" });
    // "a" was written before the blocker; "c" was never reached.
    const saved = invokeMock.mock.calls.map((c) => (c[1] as { id: string }).id);
    expect(saved).toEqual(["a.md"]);
    expect(store.get("c.md")?.dirty).toBe(true);
  });

  // A divergence on an artifact nobody has open must NOT block a teardown:
  // there is no tab to show the modal in, and nothing unsaved is at stake — a
  // closed artifact's changes were written as it closed (EDT-FR-31).
  it("does not block a teardown on a divergence with no tab open", async () => {
    const store = new EditSessionStore();
    store.openTab("a.md");
    edited(store, "a.md", "v1", "v2");
    await store.flush("a.md");
    store.closeTab("a.md");
    store.update("a.md", { conflict: true, pending: "ck-other" });

    expect(store.pendingIds()).toEqual([]);
    expect(await store.flushAll()).toEqual({ ok: true });
  });

  // Ref-counted: several surfaces depend on the watch, but exactly one
  // subscription exists — and a stop always detaches the listener it started,
  // even one whose `listen()` resolved after that stop (StrictMode's
  // mount/cleanup/mount).
  it("keeps exactly one subscription and detaches every listener it started", async () => {
    const store = new EditSessionStore();

    const stopA = store.watchExternalChanges();
    const stopB = store.watchExternalChanges();
    await Promise.resolve();
    expect(listenCalls).toBe(1);

    stopA();
    expect(unlistenMock).not.toHaveBeenCalled(); // B still needs it
    stopB();
    await Promise.resolve();
    expect(unlistenMock).toHaveBeenCalledTimes(1);

    // A stop that lands before its own subscribe resolves still detaches.
    const stopC = store.watchExternalChanges();
    stopC();
    await Promise.resolve();
    await Promise.resolve();
    expect(unlistenMock).toHaveBeenCalledTimes(2);
  });
});

// EDT-FR-35 / SNV-FR-30: the Save All scope. Deliberately narrower than
// `pendingIds` — an artifact that has nothing to write must not put Save All on
// offer, nor be reported as blocking it.
describe("EditSessionStore.dirtyIds (EDT-FR-35)", () => {
  it("lists only artifacts holding unsaved changes, in open order", () => {
    const store = new EditSessionStore();
    store.adoptLoad("a.md", "a", "ck1");
    store.adoptLoad("b.md", "b", "ck1");
    store.adoptLoad("c.md", "c", "ck1");
    expect(store.dirtyIds()).toEqual([]);

    store.update("a.md", { dirty: true });
    store.update("c.md", { dirty: true });

    expect(store.dirtyIds()).toEqual(["a.md", "c.md"]);
  });

  it("excludes an artifact sitting on a divergence it has no edits behind", () => {
    // Opened, read, never edited — then the file changed on disk. A teardown
    // must still deal with it (`pendingIds`), but Save All has nothing to write
    // for it: offering Save All here would light the menu item up for an
    // artifact whose flush can only ever come back blocked.
    const store = new EditSessionStore();
    store.adoptLoad("a.md", "a", "ck1");
    store.openTab("a.md");
    store.update("a.md", { conflict: true, pending: "ck2" });

    expect(store.pendingIds()).toEqual(["a.md"]);
    expect(store.dirtyIds()).toEqual([]);
  });

  it("includes a diverged artifact that also holds edits", () => {
    // Here Save All does have something to write, so it is offered — and the
    // flush then reports the divergence as the blocker (EDT-FR-36).
    const store = new EditSessionStore();
    store.adoptLoad("a.md", "a", "ck1");
    store.openTab("a.md");
    store.update("a.md", { dirty: true, conflict: true });

    expect(store.dirtyIds()).toEqual(["a.md"]);
  });

  it("drops an artifact once its changes are written", async () => {
    const store = new EditSessionStore();
    store.adoptLoad("a.md", "a", "ck1");
    store.ensure("a.md").buffer = "a edited";
    store.update("a.md", { dirty: true });

    await store.flush("a.md");

    expect(store.dirtyIds()).toEqual([]);
  });
});

/**
 * EDT-FR-70 / EDT-FR-71: the rest after the last keystroke, and what becomes of
 * the write it schedules.
 *
 * Driven against the store rather than through the Editor because the schedule
 * is the store's: only the active tab's Editor is mounted (EDT-FR-30), and the
 * requirement is precisely that a write lands for an artifact nobody is
 * looking at.
 */
describe("the rest after the last edit (EDT-FR-70)", () => {
  const saves = () =>
    invokeMock.mock.calls.filter((c) => c[0] === "save_artifact_contents");

  /** Let a scheduled write's rest elapse. */
  const rest = async () => {
    await new Promise((r) => setTimeout(r, AUTOSAVE_DELAY_MS + 30));
  };

  /** One edit through the path the Editor uses on every keystroke. */
  function type(store: EditSessionStore, id: string, next: string) {
    store.ensure(id).buffer = next;
    store.noteEdit(id);
  }

  it("EDT-FR-70: writes once for a burst of typing, carrying the settled text", async () => {
    const store = new EditSessionStore();
    store.adoptLoad("a.md", "", "ck1");

    // Forty keystrokes, none of them a pause long enough to end the burst.
    let text = "";
    for (const ch of "the quick brown fox jumped over the lazy dog") {
      text += ch;
      type(store, "a.md", text);
    }
    expect(saves()).toHaveLength(0);

    await rest();

    expect(saves()).toHaveLength(1);
    expect(saves()[0][1]).toEqual({ id: "a.md", body: text });
    expect(store.get("a.md")?.dirty).toBe(false);
  });

  it("EDT-FR-30, EDT-FR-70: lands with no tab open on the artifact, and clears the indicator", async () => {
    const store = new EditSessionStore();
    store.adoptLoad("a.md", "v1", "ck1");
    store.openTab("a.md");
    type(store, "a.md", "v2");

    // The author activates a different tab: the Editor unmounts, and the record
    // is all that is left. The write must still land.
    store.closeTab("a.md");
    await rest();

    expect(saves()).toHaveLength(1);
    expect(store.get("a.md")?.dirty).toBe(false);
    expect(store.get("a.md")?.baseline).toBe("ck-saved");
  });

  it("spends the schedule when a Save brings the write forward (SNV-FR-29)", async () => {
    const store = new EditSessionStore();
    store.adoptLoad("a.md", "v1", "ck1");
    type(store, "a.md", "v2");

    await store.flush("a.md", { force: true });
    expect(saves()).toHaveLength(1);

    // The rest would have elapsed by now; it must not write the same buffer
    // a second time.
    await rest();
    expect(saves()).toHaveLength(1);
  });

  it("EXC-FR-NMXQ: holds the write behind an unresolved divergence, then performs it", async () => {
    const store = new EditSessionStore();
    store.adoptLoad("a.md", "v1", "ck1");
    store.openTab("a.md");
    type(store, "a.md", "mine");
    store.update("a.md", { conflict: true, pending: "ck2" });

    await rest();

    // Nothing was written, and the write is held rather than dropped.
    expect(saves()).toHaveLength(0);
    expect(store.hasPendingWrite("a.md")).toBe(true);
    expect(store.get("a.md")?.dirty).toBe(true);

    // "Keep my version" takes it and performs it at once (EXC-FR-WDEJ).
    expect(store.takeHeldWrite("a.md")).toBe(true);
    store.update("a.md", { conflict: false, pending: null });
    await store.flush("a.md");

    expect(saves()).toHaveLength(1);
    expect(saves()[0][1]).toEqual({ id: "a.md", body: "mine" });
  });

  it("EXC-FR-WDAV: drops the held write when the buffer is loaded from the filesystem", async () => {
    const store = new EditSessionStore();
    store.adoptLoad("a.md", "v1", "ck1");
    store.openTab("a.md");
    type(store, "a.md", "mine");
    store.update("a.md", { conflict: true, pending: "ck2" });
    await rest();
    expect(store.hasPendingWrite("a.md")).toBe(true);

    // "Load from filesystem" replaces the buffer, and the write goes with it.
    store.adoptLoad("a.md", "theirs", "ck2");

    expect(store.hasPendingWrite("a.md")).toBe(false);
    expect(store.takeHeldWrite("a.md")).toBe(false);
    await rest();
    expect(saves()).toHaveLength(0);
    expect(store.get("a.md")?.buffer).toBe("theirs");
  });

  it("EDT-FR-71: does not retry a failed write on a timer, and retries on Save", async () => {
    invokeMock.mockImplementation(async () => {
      throw new Error("disk full");
    });
    const store = new EditSessionStore();
    store.adoptLoad("a.md", "v1", "ck1");
    type(store, "a.md", "v2");

    await rest();
    expect(saves()).toHaveLength(1);
    expect(store.get("a.md")?.error).toContain("disk full");
    // The buffer and the indicator are exactly as they were.
    expect(store.get("a.md")?.dirty).toBe(true);
    expect(store.get("a.md")?.buffer).toBe("v2");

    // No second attempt arrives on its own — the tab would otherwise re-raise
    // the same error every rest for as long as the disk stayed full.
    await rest();
    expect(saves()).toHaveLength(1);

    // Save retries on demand, and a success clears the error.
    invokeMock.mockImplementation(async () => ({ checksum: "ck-saved" }));
    await store.flush("a.md", { force: true });
    expect(saves()).toHaveLength(2);
    expect(store.get("a.md")?.error).toBeNull();
  });

  it("EDT-FR-71: a Save that fails does not leave the rest to retry behind it", async () => {
    const store = new EditSessionStore();
    store.adoptLoad("a.md", "v1", "ck1");
    // The edit arms the rest; the Save arrives before it elapses and fails.
    type(store, "a.md", "v2");
    invokeMock.mockImplementation(async () => {
      throw new Error("disk full");
    });

    await store.flush("a.md");
    expect(saves()).toHaveLength(1);
    expect(store.get("a.md")?.error).toContain("disk full");

    // The rest the edit armed must have been spent by the Save. Left running, it
    // would fire here and re-attempt the write the author was just told failed.
    expect(store.hasPendingWrite("a.md")).toBe(false);
    await rest();
    expect(saves()).toHaveLength(1);
  });

  it("EDT-FR-71: the next edit is what schedules the next write after a failure", async () => {
    invokeMock.mockImplementation(async () => {
      throw new Error("disk full");
    });
    const store = new EditSessionStore();
    store.adoptLoad("a.md", "v1", "ck1");
    type(store, "a.md", "v2");
    await rest();
    expect(saves()).toHaveLength(1);

    type(store, "a.md", "v3");
    await rest();

    expect(saves()).toHaveLength(2);
  });

  it("TAB-FR-20: a document that no longer exists writes nothing", async () => {
    const store = new EditSessionStore();
    store.adoptLoad("a.md", "v1", "ck1");
    type(store, "a.md", "v2");

    // The path was removed on disk and the tab closed automatically; writing
    // now would recreate the file the user just deleted.
    store.forget("a.md");

    // Asserted while the rest would still be running, so this proves the write
    // was *dropped* rather than merely made harmless by the record being gone.
    expect(store.hasPendingWrite("a.md")).toBe(false);
    await rest();
    expect(saves()).toHaveLength(0);
  });

  it("EDT-FR-28: a teardown drops every write it did not already flush", async () => {
    const store = new EditSessionStore();
    store.adoptLoad("a.md", "v1", "ck1");
    type(store, "a.md", "v2");

    store.clear();

    expect(store.hasPendingWrite("a.md")).toBe(false);
    await rest();
    expect(saves()).toHaveLength(0);
  });

  it("EXC-FR-WDAV: loading from the filesystem drops the resting write too", async () => {
    const store = new EditSessionStore();
    store.adoptLoad("a.md", "v1", "ck1");
    type(store, "a.md", "mine");
    expect(store.hasPendingWrite("a.md")).toBe(true);

    // The author chose "Load from filesystem" before the rest elapsed: the
    // buffer the write would have carried is gone, so the write goes with it.
    store.adoptLoad("a.md", "theirs", "ck2");

    expect(store.hasPendingWrite("a.md")).toBe(false);
    await rest();
    expect(saves()).toHaveLength(0);
  });

  it("NAW-FR-13: a store written on someone else's schedule arms nothing", async () => {
    const store = new EditSessionStore(artifactTransport, false);
    store.adoptLoad("a.md", "v1", "ck1");
    type(store, "a.md", "v2");

    await rest();

    expect(saves()).toHaveLength(0);
    expect(store.hasPendingWrite("a.md")).toBe(false);
    // The edit itself still registered — only the schedule is someone else's.
    expect(store.get("a.md")?.dirty).toBe(true);
  });

  /**
   * Two writes of one artifact must never be in flight together. Their
   * completions would race: the older buffer can land on disk after the newer
   * one, and its checksum then becomes the baseline with `dirty` cleared — so
   * the edit that was actually lost looks saved, raises no divergence, and is
   * gone for good.
   */
  it("never overlaps two writes of the same artifact", async () => {
    const inFlight: Array<(v: { checksum: string }) => void> = [];
    let concurrent = 0;
    let peak = 0;
    invokeMock.mockImplementation(
      (_cmd: string) =>
        new Promise((resolve) => {
          concurrent += 1;
          peak = Math.max(peak, concurrent);
          inFlight.push((v) => {
            concurrent -= 1;
            resolve(v);
          });
        }),
    );
    const store = new EditSessionStore();
    store.adoptLoad("a.md", "v1", "ck1");
    type(store, "a.md", "first");

    // The rest fires while the author keeps typing; a Save arrives on top.
    const a = store.flush("a.md");
    await until(() => inFlight.length === 1, "the first write to start");
    type(store, "a.md", "second");
    const b = store.flush("a.md", { force: true });
    await until(() => concurrent === 1, "the second write to queue");

    // The second write is queued, not running alongside the first.
    expect(inFlight).toHaveLength(1);

    // Let them drain in order and check the disk ends on the newer buffer.
    inFlight[0]({ checksum: "ck-a" });
    await a;
    await until(() => inFlight.length === 2, "the second write to start");
    expect(peak).toBe(1);
    inFlight[1]({ checksum: "ck-b" });
    await b;

    const bodies = saves().map((c) => (c[1] as { body: string }).body);
    expect(bodies).toEqual(["first", "second"]);
    expect(store.get("a.md")?.baseline).toBe("ck-b");
    expect(store.get("a.md")?.dirty).toBe(false);
  });

  it("lets a queued write see what the one ahead of it settled", async () => {
    const store = new EditSessionStore();
    store.adoptLoad("a.md", "v1", "ck1");
    type(store, "a.md", "v2");

    // Two teardown sweeps racing each other, as a quit landing on the heels of
    // a project close does. The second finds nothing left to write.
    await Promise.all([store.flush("a.md"), store.flush("a.md")]);

    expect(saves()).toHaveLength(1);
  });

  // STB-FR-18 / EDT-FR-70: a line-ending change marks every open tab dirty, and
  // each of them then writes itself on the ordinary terms of EDT-FR-70.
  it("EDT-FR-70, STB-FR-18: schedules a write for every tab a line-ending change dirtied", async () => {
    const store = new EditSessionStore();
    for (const id of ["a.md", "b.md"]) {
      store.adoptLoad(id, "v1", "ck1");
      store.openTab(id);
    }
    // A third artifact with no open tab gains nothing.
    store.adoptLoad("c.md", "v1", "ck1");

    store.markOpenTabsDirty();
    expect(store.get("c.md")?.dirty).toBe(false);

    await rest();

    expect(saves().map((c) => (c[1] as { id: string }).id).sort()).toEqual([
      "a.md",
      "b.md",
    ]);
    // EDT-FR-70, STB-FR-18's second clause: both indicators clear once the writes land.
    expect(store.get("a.md")?.dirty).toBe(false);
    expect(store.get("b.md")?.dirty).toBe(false);
    expect(store.get("c.md")?.dirty).toBe(false);
  });
});
