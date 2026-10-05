import { beforeEach, describe, expect, it, vi } from "vitest";

import { AUTOSAVE_DELAY_MS, EditSessionStore } from "./editSessions";
import { artifactTransport } from "./documentTransport";
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
// Re-reading a document rewritten under the author (DCR-FR-13)
// ---------------------------------------------------------------------------

describe("EditSessionStore.reload", () => {
  it("adopts the new bytes as the buffer, the baseline and the history floor", async () => {
    const store = new EditSessionStore(artifactTransport);
    store.adoptLoad("a.md", "v1", "ck1");
    recordEdit(store.ensure("a.md").history, "v1 plus mine", "wysiwyg", "body");
    store.update("a.md", { dirty: true });

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_artifact_contents_by_id")
        return { body: "v2", checksum: "ck2" };
      throw new Error(`unexpected invoke ${cmd}`);
    });
    await store.reload("a.md");

    const s = store.get("a.md")!;
    expect(s.buffer).toBe("v2");
    expect(s.baseline).toBe("ck2");
    expect(s.dirty).toBe(false);
    // The history floor moved with the bytes: an undo must not be able to
    // reinstate the superseded text as an edit to be written back.
    expect(hasEdits(s.history)).toBe(false);
    expect(s.history.steps[0]?.doc).toBe("v2");
  });

  it("leaves the record alone when the read fails, and says so", async () => {
    const store = new EditSessionStore(artifactTransport);
    // An unsaved edit resting on its write timer when the acceptance lands.
    store.adoptLoad("a.md", "v1", "ck1");
    store.ensure("a.md").buffer = "the author's private draft";
    store.noteEdit("a.md");
    invokeMock.mockImplementation(async () => {
      throw new Error("read denied");
    });

    await store.reload("a.md");

    // The read is what failed; the author's buffer is not the casualty. Losing
    // the dirty flag or the scheduled write here would strand an unsaved edit
    // with no write coming for it.
    const s = store.get("a.md")!;
    expect(s.buffer).toBe("the author's private draft");
    expect(s.dirty).toBe(true);
    expect(store.hasPendingWrite("a.md")).toBe(true);
    expect(s.error).toContain("read denied");
    expect(logErrorMock).toHaveBeenCalledTimes(1);
    const [, message, fields] = logErrorMock.mock.calls[0] as [
      unknown,
      string,
      Record<string, unknown>,
    ];
    expect(message).toMatch(/read again/i);
    expect(fields.artifact).toBe("a.md");
    // Nothing downstream redacts anything, so the body must never be a field —
    // and the retained buffer is still the author's text at this point.
    expect(JSON.stringify(fields)).not.toContain("private draft");
  });

  it("drops the write the superseded buffer had scheduled", async () => {
    // The rest is real here: a write that fires after the reload would put the
    // pre-acceptance text back on top of the file the author just accepted.
    const store = new EditSessionStore(artifactTransport);
    store.adoptLoad("a.md", "v1", "ck1");
    store.ensure("a.md").buffer = "v1 plus mine";
    store.noteEdit("a.md");
    expect(store.hasPendingWrite("a.md")).toBe(true);

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_artifact_contents_by_id")
        return { body: "v2", checksum: "ck2" };
      if (cmd === "save_artifact_contents") return { checksum: "ck-saved" };
      throw new Error(`unexpected invoke ${cmd}`);
    });
    await store.reload("a.md");

    expect(store.hasPendingWrite("a.md")).toBe(false);
    await new Promise((r) => setTimeout(r, AUTOSAVE_DELAY_MS + 30));
    expect(
      invokeMock.mock.calls.some((c) => c[0] === "save_artifact_contents"),
    ).toBe(false);
    expect(store.get("a.md")?.buffer).toBe("v2");
  });

  it("takes the newest bytes when a write is still in flight", async () => {
    // The write path waits on `pendingLoad`, so publishing the read there is
    // what stops the save's answer from being applied over it.
    const store = new EditSessionStore(artifactTransport);
    store.adoptLoad("a.md", "v1", "ck1");
    store.ensure("a.md").buffer = "v1 plus mine";
    store.noteEdit("a.md");
    let releaseSave!: (v: { checksum: string }) => void;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "save_artifact_contents")
        return new Promise<{ checksum: string }>((resolve) => {
          releaseSave = resolve;
        });
      if (cmd === "load_artifact_contents_by_id")
        return { body: "the accepted text", checksum: "ck-accepted" };
      throw new Error(`unexpected invoke ${cmd}`);
    });

    const writing = store.flush("a.md");
    await until(() => releaseSave !== undefined, "the save to be outstanding");
    const reloading = store.reload("a.md");
    releaseSave({ checksum: "ck-written" });
    await Promise.all([writing, reloading]);

    expect(store.get("a.md")?.buffer).toBe("the accepted text");
    expect(store.get("a.md")?.baseline).toBe("ck-accepted");
    expect(store.get("a.md")?.dirty).toBe(false);
  });

  it("lets the newest read win when two overlap", async () => {
    // The store is public API; a caller that reloads twice must not have the
    // slower first answer land on top of the newer one.
    const store = new EditSessionStore(artifactTransport);
    store.adoptLoad("a.md", "v1", "ck1");
    const releases: ((v: { body: string; checksum: string }) => void)[] = [];
    invokeMock.mockImplementation(
      async () =>
        new Promise<{ body: string; checksum: string }>((resolve) =>
          releases.push(resolve),
        ),
    );

    const first = store.reload("a.md");
    const second = store.reload("a.md");
    await until(() => releases.length === 2, "both reads to be outstanding");
    // The newer read answers first, the older one straggles in behind it.
    releases[1]({ body: "newest", checksum: "ck-new" });
    releases[0]({ body: "stale", checksum: "ck-stale" });
    await Promise.all([first, second]);

    expect(store.get("a.md")?.buffer).toBe("newest");
    expect(store.get("a.md")?.baseline).toBe("ck-new");
  });
});

/**
 * ESH-FR-SSDV: the file's name decides which surface it is edited on, and the
 * record is where that answer is kept — so the rail's placement (CMT-FR-02),
 * where discussions are read (ACT-FR-19) and which surface a history step names
 * (EDT-FR-25) all read one answer rather than each deriving their own.
 */
describe("EditSessionStore starting mode (ESH-FR-SSDV)", () => {
  it("starts a Markdown file in WYSIWYG and a source file on its one surface", () => {
    const store = new EditSessionStore();
    expect(store.ensure("notes.md").mode).toBe("wysiwyg");
    expect(store.ensure("docs/deep/SKILL.md").mode).toBe("wysiwyg");
    // Case is not part of the answer.
    expect(store.ensure("README.MD").mode).toBe("wysiwyg");

    expect(store.ensure("src/main.rs").mode).toBe("text");
    expect(store.ensure("notes.txt").mode).toBe("text");
    expect(store.ensure("Makefile").mode).toBe("text");
    // Markdown-like, but not `.md` — the rule is the final extension alone.
    expect(store.ensure("README.markdown").mode).toBe("text");
  });

  it("keeps a source file on its surface across a load and a reopen", () => {
    const store = new EditSessionStore();
    store.openTab("src/main.rs");
    store.adoptLoad("src/main.rs", "fn main() {}\n", "ck1");
    expect(store.get("src/main.rs")?.mode).toBe("text");

    recordEdit(store.get("src/main.rs")!.history, "fn main() { }\n", "text", "source");
    store.get("src/main.rs")!.dirty = true;
    store.closeTab("src/main.rs");
    // Retained rather than dropped, and still on the one surface it has.
    expect(store.get("src/main.rs")?.mode).toBe("text");
  });
});
