import { beforeEach, describe, expect, it, vi } from "vitest";

import { EditSessionStore } from "./editSessions";
import { hasEdits, recordEdit } from "./editHistory";

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

/** Deliver one payload to every subscriber the store currently has. */
function externalChange(payload: { artifactId: string; checksum: string }) {
  for (const handler of [...externalListeners]) handler({ payload });
}

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

// ---------------------------------------------------------------------------
// Rollback: quiesce and the two resets (EDT-FR-81, EDT-FR-72 – EXC-FR-UPGM, EXC-FR-VNLZ, EDT-FR-24, EFR-FR-GBJT / `EXC-editor-external-change.md` EXC-FR-ZXWI, EXC-FR-WTTZ)
// ---------------------------------------------------------------------------

describe("quiescing a session before a rollback (EDT-FR-81)", () => {
  it("cancels a scheduled write rather than performing it (EDT-FR-81, EDT-FR-72)", async () => {
    const store = new EditSessionStore();
    edited(store, "a.md", "body", "edited");
    store.scheduleWrite("a.md");
    expect(store.hasPendingWrite("a.md")).toBe(true);

    await store.quiesce(["a.md"]);

    expect(store.hasPendingWrite("a.md")).toBe(false);
    expect(
      invokeMock.mock.calls.some((c) => c[0] === "save_artifact_contents"),
    ).toBe(false);
  });

  it("refuses every later route to a write while quiesced (EDT-FR-81, EDT-FR-72)", async () => {
    const store = new EditSessionStore();
    edited(store, "a.md", "body", "edited");
    await store.quiesce(["a.md"]);
    invokeMock.mockClear();

    // The schedule, an explicit Save, a forced Save, and a Save All sweep.
    store.noteEdit("a.md");
    expect(store.hasPendingWrite("a.md")).toBe(false);
    await store.flush("a.md");
    await store.flush("a.md", { force: true });
    await store.flushAll();

    expect(
      invokeMock.mock.calls.some((c) => c[0] === "save_artifact_contents"),
    ).toBe(false);
    // The edit is still held in memory, unwritten and unlost.
    expect(store.get("a.md")?.dirty).toBe(true);
  });

  it("waits for an in-flight write to settle and lets it complete (EDT-FR-81, EDT-FR-72)", async () => {
    const store = new EditSessionStore();
    edited(store, "a.md", "body", "edited");

    let release!: (v: { checksum: string }) => void;
    invokeMock.mockImplementation(
      async (cmd: string) =>
        cmd === "save_artifact_contents"
          ? new Promise((resolve) => {
              release = resolve;
            })
          : undefined,
    );

    const writing = store.flush("a.md");
    await until(() => release != null, "the write to reach the transport");

    let quiesced = false;
    const waiting = store.quiesce(["a.md"]).then(() => {
      quiesced = true;
    });
    await Promise.resolve();
    // EDT-FR-81: the quiesce does not return while the write is outstanding.
    expect(quiesced).toBe(false);

    release({ checksum: "ck-in-flight" });
    await writing;
    await waiting;
    expect(quiesced).toBe(true);
    // The write that was already running completed normally and updated the
    // baseline, exactly as EDT-FR-81 requires.
    expect(store.get("a.md")?.baseline).toBe("ck-in-flight");
    expect(store.get("a.md")?.dirty).toBe(false);
  });

  it("awaits one artifact once however many views it has (EDT-FR-81, EDT-FR-72)", async () => {
    const store = new EditSessionStore();
    edited(store, "a.md", "body", "edited");
    // Several tabs are several views onto ONE session and one write chain
    // (EDT-FR-72), so the id appears once in the affected set either way.
    store.openTab("a.md");
    store.openTab("a.md");

    let saves = 0;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "save_artifact_contents") {
        saves += 1;
        return { checksum: "ck-saved" };
      }
      return undefined;
    });

    await store.quiesce(["a.md", "a.md"]);
    expect(saves).toBe(0);
  });

  it("reports a write that failed as it settled, without retrying (EDT-FR-81, EDT-FR-71)", async () => {
    const store = new EditSessionStore();
    edited(store, "a.md", "body", "edited");

    let release!: (e: unknown) => void;
    invokeMock.mockImplementation(
      async (cmd: string) =>
        cmd === "save_artifact_contents"
          ? new Promise((_resolve, reject) => {
              release = reject;
            })
          : undefined,
    );

    const writing = store.flush("a.md");
    await until(() => release != null, "the write to reach the transport");
    const waiting = store.quiesce(["a.md"]);
    release(new Error("disk full"));
    await writing;
    const failures = await waiting;

    // EDT-FR-81: reported to the caller that asked for the quiesce…
    expect(failures).toHaveLength(1);
    expect(failures[0].artifactId).toBe("a.md");
    expect(failures[0].reason).toMatch(/disk full/);
    // …and the quiesce completed rather than stalling, with the buffer intact
    // and nothing retried.
    expect(store.get("a.md")?.dirty).toBe(true);
    expect(hasEdits(store.get("a.md")!.history)).toBe(true);
    expect(store.hasPendingWrite("a.md")).toBe(false);
  });

  it("does not report a failure left over from an earlier write", async () => {
    const store = new EditSessionStore();
    edited(store, "a.md", "body", "edited");
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "save_artifact_contents") throw new Error("earlier failure");
      return undefined;
    });
    await store.flush("a.md");
    expect(store.get("a.md")?.error).toMatch(/earlier failure/);

    // The quiesce reports only writes that settled during its own wait.
    const failures = await store.quiesce(["a.md"]);
    expect(failures).toEqual([]);
  });
});

describe("resetting a restored artifact (EXC-FR-UVJY)", () => {
  it("reloads, clears dirty, drops the history, and raises no modal (EXC-FR-UPGM, EXC-FR-VNLZ, EDT-FR-24, EFR-FR-GBJT)", async () => {
    const store = new EditSessionStore();
    edited(store, "a.md", "body", "edited");
    store.setFind("a.md", { form: "find", query: "session" });
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_artifact_contents_by_id")
        return { body: "restored", checksum: "ck-head" };
      if (cmd === "save_artifact_contents") return { checksum: "ck-saved" };
      return undefined;
    });

    await store.quiesce(["a.md"]);
    await store.resetRestored("a.md");

    const s = store.get("a.md")!;
    expect(s.buffer).toBe("restored");
    expect(s.dirty).toBe(false);
    expect(s.baseline).toBe("ck-head");
    // EDT-FR-24: the reloaded content is the oldest state undo can reach.
    expect(hasEdits(s.history)).toBe(false);
    // EXC-FR-NPFL: never answered as an external change.
    expect(s.conflict).toBe(false);
    // EXC-FR-WXXS: the find panel survives, describing no discarded text.
    expect(s.find.form).toBe("find");
    expect(s.find.query).toBe("session");
    // The quiesce is lifted, so the next edit writes again.
    store.noteEdit("a.md");
    expect(store.hasPendingWrite("a.md")).toBe(true);
  });

  it("dismisses a standing external-change modal with neither resolution (EXC-FR-UPGM, EXC-FR-VNLZ, EDT-FR-24, EFR-FR-GBJT)", async () => {
    const store = new EditSessionStore();
    edited(store, "a.md", "body", "edited");
    store.update("a.md", { conflict: true, pending: "ck-external" });
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_artifact_contents_by_id")
        return { body: "restored", checksum: "ck-head" };
      return undefined;
    });

    await store.quiesce(["a.md"]);
    await store.resetRestored("a.md");

    const s = store.get("a.md")!;
    expect(s.conflict).toBe(false);
    expect(s.pending).toBeNull();
    expect(s.buffer).toBe("restored");
  });

  it("lifts the quiesce even when the reload fails", async () => {
    const store = new EditSessionStore();
    edited(store, "a.md", "body", "edited");
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_artifact_contents_by_id") throw new Error("unreadable");
      if (cmd === "save_artifact_contents") return { checksum: "ck-saved" };
      return undefined;
    });

    await store.quiesce(["a.md"]);
    await store.resetRestored("a.md");

    // A session left quiesced would never write again, silently costing the
    // author every edit from then on.
    expect(store.get("a.md")?.quiesced).toBe(false);
    store.noteEdit("a.md");
    expect(store.hasPendingWrite("a.md")).toBe(true);
  });
});

describe("discarding a removed artifact (EXC-FR-ZXWI)", () => {
  it("drops the record without writing it (EXC-FR-ZXWI, EXC-FR-WTTZ)", async () => {
    const store = new EditSessionStore();
    edited(store, "scratch.md", "body", "edited");
    store.scheduleWrite("scratch.md");

    await store.quiesce(["scratch.md"]);
    store.discardRemoved("scratch.md");

    expect(store.get("scratch.md")).toBeUndefined();
    expect(store.has("scratch.md")).toBe(false);
    // The pending write is gone rather than performed: writing the buffer of a
    // file the author just deleted would recreate exactly what they deleted.
    expect(
      invokeMock.mock.calls.some((c) => c[0] === "save_artifact_contents"),
    ).toBe(false);
    // And a later teardown sweep finds nothing to write for it either.
    await store.flushAll();
    expect(
      invokeMock.mock.calls.some((c) => c[0] === "save_artifact_contents"),
    ).toBe(false);
  });

  it("leaves an unconfirmed artifact exactly as it stood (EXC-FR-ZXWI, EXC-FR-WTTZ)", async () => {
    const store = new EditSessionStore();
    const s = edited(store, "b.md", "body", "edited");
    store.update("b.md", { conflict: true });
    const historyBefore = s.history;

    await store.quiesce(["b.md"]);
    store.unquiesce("b.md");

    const after = store.get("b.md")!;
    expect(after.buffer).toBe("edited");
    expect(after.dirty).toBe(true);
    expect(after.history).toBe(historyBefore);
    expect(hasEdits(after.history)).toBe(true);
    // The external-change modal it was sitting on is untouched.
    expect(after.conflict).toBe(true);
    expect(after.quiesced).toBe(false);
    // A dirty session re-arms its rest, or the edit made while it was quiesced
    // would sit unwritten until the author happened to type again.
    expect(store.hasPendingWrite("b.md")).toBe(true);
  });
});

describe("the queued-write guard (EDT-FR-81)", () => {
  it("does not start a write that queued behind an in-flight one", async () => {
    // The distinction the requirement draws: a write already running completes,
    // one merely NEXT in the queue must not start. Without the guard that sits
    // after the `writeChain` await, the queued write reaches the transport the
    // moment the first settles — landing on the file the rollback is about to
    // write, which is the data-loss shape this whole mechanism exists to stop.
    const store = new EditSessionStore();
    edited(store, "a.md", "body", "edited");

    let saves = 0;
    let releaseFirst!: () => void;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd !== "save_artifact_contents") return undefined;
      saves += 1;
      if (saves === 1) {
        return new Promise((resolve) => {
          releaseFirst = () => resolve({ checksum: "ck-first" });
        });
      }
      return { checksum: `ck-${saves}` };
    });

    const first = store.flush("a.md");
    await until(() => releaseFirst != null, "the first write to start");
    // A second write queues behind it, before any quiesce.
    const queued = store.flush("a.md");
    await Promise.resolve();
    expect(saves).toBe(1);

    // The quiesce lands while the first is still outstanding.
    const quiescing = store.quiesce(["a.md"]);
    releaseFirst();
    await first;
    await queued;
    await quiescing;

    // The in-flight write completed and adopted its baseline; the queued one
    // never reached the transport.
    expect(saves).toBe(1);
    expect(store.get("a.md")?.baseline).toBe("ck-first");
  });

  it("keeps an external-change resolution from writing while quiesced", async () => {
    // CHG-FR-60: "no late save, no autosave retry, and no external-change
    // resolution writes an affected file" once preparation has begun.
    const store = new EditSessionStore();
    edited(store, "a.md", "body", "edited");
    store.update("a.md", { conflict: true, pending: "ck-external" });

    await store.quiesce(["a.md"]);
    invokeMock.mockClear();

    // "Keep my version" takes the held write and performs it.
    store.takeHeldWrite("a.md");
    store.update("a.md", { conflict: false });
    await store.flush("a.md");

    expect(
      invokeMock.mock.calls.some((c) => c[0] === "save_artifact_contents"),
    ).toBe(false);
  });
});

/**
 * PCR-FR-11 / PCR-FR-24 / EDT-FR-84: the external-change modal is held back
 * while a prompt change review is showing over the artifact.
 *
 * The divergence is still recorded — it is what the review's standing line is
 * said from, and what is raised once the review's reason for passing it over
 * turns out not to have come true — but no modal is put up and no tab is made
 * inert, because the author is about to replace the whole file on purpose.
 */
describe("holding the external-change modal for a review (EDT-FR-84)", () => {
  /** Deliver an `"artifact changed externally"` for `id`. */
  async function diverge(store: EditSessionStore, id: string, checksum: string) {
    // The store's watch is what turns an event into a divergence, and it is
    // established asynchronously — so a test mounts one exactly as a surface
    // does and lets the subscription land before delivering.
    const release = store.watchExternalChanges();
    await Promise.resolve();
    await Promise.resolve();
    externalChange({ artifactId: id, checksum });
    release();
  }

  it("records the divergence without raising the modal", async () => {
    const store = new EditSessionStore();
    edited(store, "a.md", "body", "edited");
    const release = store.holdExternalChanges("a.md");

    await diverge(store, "a.md", "ck-external");

    expect(store.get("a.md")?.conflict).toBe(false);
    expect(store.get("a.md")?.pending).toBe("ck-external");
    release();
  });

  it("raises what it held once the hold is released", async () => {
    const store = new EditSessionStore();
    edited(store, "a.md", "body", "edited");
    const release = store.holdExternalChanges("a.md");
    await diverge(store, "a.md", "ck-external");

    release();

    // The review is gone and the replacement it was passed over for did not
    // happen, so the ordinary modal is raised for it.
    expect(store.get("a.md")?.conflict).toBe(true);
  });

  it("is ref-counted, so one holder cannot release another's", async () => {
    // React mounts an effect, cleans it up and mounts it again under
    // StrictMode; a release that took the hold off while another holder was
    // still relying on it would raise the modal the review exists to keep down.
    const store = new EditSessionStore();
    edited(store, "a.md", "body", "edited");
    const first = store.holdExternalChanges("a.md");
    const second = store.holdExternalChanges("a.md");
    await diverge(store, "a.md", "ck-external");

    first();
    expect(store.get("a.md")?.conflict).toBe(false);
    // …and a release used twice takes the hold off only once.
    first();
    expect(store.get("a.md")?.conflict).toBe(false);

    second();
    expect(store.get("a.md")?.conflict).toBe(true);
  });

  it("dismisses a standing modal with neither resolution taken, and keeps the divergence", async () => {
    // EXC-FR-UOJF / EDT-FR-84, EDT-FR-81, EXC-FR-QCNO: the buffer, the dirty indicator and the undo
    // history are left exactly as they stand — the acceptance's own reset is
    // what replaces them, and only on success. The **divergence** is kept, so a
    // failed acceptance can put the question back.
    const store = new EditSessionStore();
    const session = edited(store, "a.md", "body", "edited");
    await diverge(store, "a.md", "ck-external");
    expect(store.get("a.md")?.conflict).toBe(true);

    const hold = store.holdExternalChanges("a.md");
    store.dismissConflict("a.md");

    expect(store.get("a.md")?.conflict).toBe(false);
    expect(store.get("a.md")?.buffer).toBe("edited");
    expect(store.get("a.md")?.dirty).toBe(true);
    expect(session.history.steps.length).toBeGreaterThan(0);

    // The acceptance then fails, so the question is live again.
    store.raiseHeldExternalChange("a.md");
    expect(store.get("a.md")?.conflict).toBe(true);
    hold();
  });

  it("raises nothing where no divergence was held", async () => {
    const store = new EditSessionStore();
    edited(store, "a.md", "body", "edited");
    const release = store.holdExternalChanges("a.md");

    release();

    expect(store.get("a.md")?.conflict).toBe(false);
  });
});
