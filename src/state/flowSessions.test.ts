import { beforeEach, describe, expect, it, vi } from "vitest";

import { FlowSessionStore } from "./flowSessions";
import { AUTOSAVE_DELAY_MS } from "./writeSchedule";
import {
  addNode,
  moveNode,
  renameNode,
  serializeFlowDocument,
} from "./flowDocument";

const invokeMock = vi.fn();
const unlistenMock = vi.fn();
/** Every `"artifact changed externally"` handler the store registered. */
let externalHandlers: Array<(e: { payload: unknown }) => void> = [];
let listenCalls = 0;

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (_name: string, cb: (e: { payload: unknown }) => void) => {
    listenCalls += 1;
    externalHandlers.push(cb);
    return unlistenMock;
  }),
}));

/** What `load_artifact_contents_by_id` serves, per id. */
let disk: Record<string, { body: string; checksum: string }> = {};
/** Writes, in order, as `save_artifact_contents` received them. */
let writes: Array<{ id: string; body: string }> = [];
/** Ids whose write should be rejected. */
let failWrites = new Set<string>();
/**
 * A gate a test can install to hold a write open, so two of them can be shown
 * not to overlap. Returns a promise the write awaits before it records itself.
 */
let holdWrites: (() => Promise<void>) | null = null;
/**
 * FGV-FR-02: what `"validate flow document"` answers. Valid by default — the
 * rule set itself is the Rust suite's business (FGV-FR-05..14); what these tests
 * care about is that the store asks, and what it does with a `No`.
 */
let validation: { valid: boolean; violations: Array<Record<string, string>> } = {
  valid: true,
  violations: [],
};

beforeEach(() => {
  invokeMock.mockReset();
  unlistenMock.mockReset();
  externalHandlers = [];
  listenCalls = 0;
  disk = {};
  holdWrites = null;
  writes = [];
  failWrites = new Set();
  validation = { valid: true, violations: [] };
  invokeMock.mockImplementation(
    async (cmd: string, args: Record<string, string>) => {
      if (cmd === "validate_flow_document") return validation;
      if (cmd === "load_artifact_contents_by_id") {
        const entry = disk[args.id];
        if (!entry) throw new Error(`no such file: ${args.id}`);
        return entry;
      }
      if (cmd === "save_artifact_contents") {
        if (failWrites.has(args.id)) throw new Error("disk full");
        if (holdWrites) await holdWrites();
        writes.push({ id: args.id, body: args.body });
        disk[args.id] = { body: args.body, checksum: `ck-${writes.length}` };
        return { checksum: `ck-${writes.length}` };
      }
      throw new Error(`unexpected invoke ${cmd}`);
    },
  );
});


/** Spin the microtask queue until `ready` holds, so a queued write can reach the gate. */
async function until(ready: () => boolean, label: string) {
  for (let i = 0; i < 200; i++) {
    if (ready()) return;
    await Promise.resolve();
  }
  throw new Error(`timed out waiting for ${label}`);
}

/** A two-node Flow on disk, as `n1 -> n2`. */
const TWO_NODES = `{
  "version": 1,
  "nodes": [
    { "id": "n1", "name": "A", "position": { "x": 0, "y": 0 } },
    { "id": "n2", "name": "B", "position": { "x": 200, "y": 0 } }
  ],
  "edges": [{ "id": "e1", "from": "n1", "to": "n2" }]
}`;

/** Open `id` on a Flow tab and wait for its first load to settle. */
async function opened(store: FlowSessionStore, id: string) {
  store.openTab(id);
  await store.get(id)?.pendingLoad;
  return store.get(id)!;
}

describe("loading (FLO-FR-03 / FLO-FR-04 / FLO-FR-05)", () => {
  it("deserializes the file's body into the graph on the tab's first open", async () => {
    disk["f.flow"] = { body: TWO_NODES, checksum: "ck1" };
    const store = new FlowSessionStore();

    const s = await opened(store, "f.flow");

    expect(invokeMock).toHaveBeenCalledWith("load_artifact_contents_by_id", {
      id: "f.flow",
    });
    expect(s.doc?.nodes.map((n) => n.name)).toEqual(["A", "B"]);
    expect(s.doc?.edges).toHaveLength(1);
    expect(s.dirty).toBe(false);
    expect(s.parseError).toBeNull();
    expect(s.baseline).toBe("ck1");
  });

  // FLO-FR-04, FLO-FR-05, FLO-FR-46, first half: a Flow created through New Artifact / New File is
  // zero bytes on disk, and must open ready to edit.
  it("opens a zero-byte file as an empty graph with no error state", async () => {
    disk["new.flow"] = { body: "", checksum: "ck0" };
    const store = new FlowSessionStore();

    const s = await opened(store, "new.flow");

    expect(s.doc).toEqual({ version: 1, name: "", loops: [], nodes: [], edges: [] });
    expect(s.parseError).toBeNull();
    expect(s.error).toBeNull();
  });

  // FLO-FR-04, FLO-FR-46, second half / FLO-FR-05: a file the editor cannot read is never
  // replaced by an empty graph.
  it("puts a malformed body in the error state and writes nothing", async () => {
    disk["bad.flow"] = { body: "{ not json", checksum: "ck1" };
    const store = new FlowSessionStore();

    const s = await opened(store, "bad.flow");
    expect(s.doc).toBeNull();
    expect(s.parseError).toMatch(/not valid JSON/);

    // Neither an edit nor an explicit Save can reach the file in this state.
    store.applyEdit("bad.flow", (d) => addNode(d, { x: 0, y: 0 }));
    expect(store.get("bad.flow")?.dirty).toBe(false);
    await store.flush("bad.flow", { force: true });
    expect(writes).toEqual([]);
  });

  it("reports a load that could not reach the file as an error, not a parse failure", async () => {
    const store = new FlowSessionStore();

    const s = await opened(store, "gone.flow");

    expect(s.error).toContain("no such file");
    expect(s.parseError).toBeNull();
    expect(s.loaded).toBe(true);
  });
});

describe("editing and saving (FLO-FR-26 / FLO-FR-27)", () => {
  it("is clean until the first edit and unsaved from it until the next write", async () => {
    disk["f.flow"] = { body: TWO_NODES, checksum: "ck1" };
    const store = new FlowSessionStore();
    await opened(store, "f.flow");
    expect(store.get("f.flow")?.dirty).toBe(false);

    store.applyEdit("f.flow", (d) => renameNode(d, "n1", "Collect context"));

    expect(store.get("f.flow")?.dirty).toBe(true);
    expect(store.dirtyIds()).toEqual(["f.flow"]);

    await store.flush("f.flow");

    expect(store.get("f.flow")?.dirty).toBe(false);
    expect(store.dirtyIds()).toEqual([]);
  });

  // FLO-FR-26: an action that leaves the graph exactly as it was is not an
  // edit. Every mutator returns a new document when it changes anything, so an
  // identical one back is the signal that nothing happened — without this a
  // re-selected artifact or an out-and-back drag would leave the Flow unsaved
  // and make the next tab close write a graph nobody touched.
  it("raises nothing for an edit that returns the same document", async () => {
    disk["f.flow"] = { body: TWO_NODES, checksum: "ck1" };
    const store = new FlowSessionStore();
    await opened(store, "f.flow");
    const before = store.getVersion();
    const notified = vi.fn();
    const stop = store.subscribe(notified);

    store.applyEdit("f.flow", (d) => d);

    expect(store.get("f.flow")?.dirty).toBe(false);
    expect(store.getVersion()).toBe(before);
    expect(notified).not.toHaveBeenCalled();

    // …and the same through a real mutator asked for something already true.
    store.applyEdit("f.flow", (d) =>
      moveNode(d, "n1", d.nodes[0].position),
    );
    expect(store.get("f.flow")?.dirty).toBe(false);

    stop();
  });

  it("writes the serialized document through save artifact contents", async () => {
    disk["f.flow"] = { body: TWO_NODES, checksum: "ck1" };
    const store = new FlowSessionStore();
    await opened(store, "f.flow");
    store.applyEdit("f.flow", (d) => renameNode(d, "n2", "Draft review"));

    await store.flush("f.flow");

    expect(writes).toHaveLength(1);
    expect(writes[0].id).toBe("f.flow");
    expect(writes[0].body).toBe(
      serializeFlowDocument(store.get("f.flow")!.doc!),
    );
    expect(JSON.parse(writes[0].body).nodes[1].name).toBe("Draft review");
    // PST-FR-16: the written checksum becomes the baseline, so the watcher's
    // echo of this write is not read as an external change (FLO-FR-31).
    expect(store.get("f.flow")?.baseline).toBe("ck-1");
  });

  it("writes nothing for a clean Flow unless the save is explicit", async () => {
    disk["f.flow"] = { body: TWO_NODES, checksum: "ck1" };
    const store = new FlowSessionStore();
    await opened(store, "f.flow");

    await store.flush("f.flow");
    expect(writes).toEqual([]);

    // SNV-FR-29: an explicit Save writes whatever the graph holds.
    await store.flush("f.flow", { force: true });
    expect(writes).toHaveLength(1);
  });

  it("keeps the Flow unsaved and reports the error when the write fails", async () => {
    disk["f.flow"] = { body: TWO_NODES, checksum: "ck1" };
    failWrites.add("f.flow");
    const store = new FlowSessionStore();
    await opened(store, "f.flow");
    store.applyEdit("f.flow", (d) => addNode(d, { x: 0, y: 0 }));

    const res = await store.flush("f.flow");

    expect(res).toEqual({
      ok: false,
      blocked: "error",
      artifactId: "f.flow",
    });
    expect(store.get("f.flow")?.dirty).toBe(true);
    expect(store.get("f.flow")?.error).toContain("disk full");
  });

  // FLO-FR-29: the teardown sweep. It stops at the first blocked write so the
  // caller can focus that tab and cancel rather than tearing down over it.
  it("flushAll writes every unsaved Flow and stops at the first failure", async () => {
    disk["a.flow"] = { body: TWO_NODES, checksum: "ck1" };
    disk["b.flow"] = { body: TWO_NODES, checksum: "ck1" };
    const store = new FlowSessionStore();
    await opened(store, "a.flow");
    await opened(store, "b.flow");
    store.applyEdit("a.flow", (d) => addNode(d, { x: 0, y: 0 }));
    store.applyEdit("b.flow", (d) => addNode(d, { x: 0, y: 0 }));

    expect(await store.flushAll()).toEqual({ ok: true });
    expect(writes.map((w) => w.id)).toEqual(["a.flow", "b.flow"]);

    failWrites.add("a.flow");
    store.applyEdit("a.flow", (d) => addNode(d, { x: 0, y: 0 }));
    store.applyEdit("b.flow", (d) => addNode(d, { x: 0, y: 0 }));
    const res = await store.flushAll();

    expect(res.ok).toBe(false);
    expect(res.artifactId).toBe("a.flow");
    // Stopped at the blocker: b was not reached by this sweep.
    expect(store.get("b.flow")?.dirty).toBe(true);
  });

  it("notifies subscribers so a mounted canvas re-renders on every edit", async () => {
    disk["f.flow"] = { body: TWO_NODES, checksum: "ck1" };
    const store = new FlowSessionStore();
    await opened(store, "f.flow");
    const seen: number[] = [];
    store.subscribe(() => seen.push(store.getVersion()));

    store.applyEdit("f.flow", (d) => addNode(d, { x: 0, y: 0 }));
    store.applyEdit("f.flow", (d) => addNode(d, { x: 0, y: 0 }));

    expect(seen).toHaveLength(2);
    expect(new Set(seen).size).toBe(2);
  });
});

describe("lifecycle (FLO-FR-28 / FLO-FR-29 / FLO-FR-30)", () => {
  // FLO-FR-27, FLO-FR-28 / TAB-FR-12: nothing survives the close, so reopening the Flow
  // deserializes the on-disk document afresh.
  it("retains nothing when the tab closes, so a reopen loads from disk", async () => {
    disk["f.flow"] = { body: TWO_NODES, checksum: "ck1" };
    const store = new FlowSessionStore();
    await opened(store, "f.flow");
    store.applyEdit("f.flow", (d) => renameNode(d, "n1", "edited"));

    await store.flush("f.flow");
    store.closeTab("f.flow");
    expect(store.get("f.flow")).toBeUndefined();

    const reopened = await opened(store, "f.flow");
    expect(reopened.doc?.nodes[0].name).toBe("edited");
    expect(reopened.dirty).toBe(false);
    // Two loads: the first open and this one. Nothing was carried over.
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "load_artifact_contents_by_id"),
    ).toHaveLength(2);
  });

  // FLO-FR-30: switching tabs unmounts the canvas but must neither reload the
  // Flow nor discard its edits.
  it("does not reload on a focus jump back to an already-open tab", async () => {
    disk["f.flow"] = { body: TWO_NODES, checksum: "ck1" };
    const store = new FlowSessionStore();
    await opened(store, "f.flow");
    store.applyEdit("f.flow", (d) => renameNode(d, "n1", "unsaved"));

    store.openTab("f.flow");

    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "load_artifact_contents_by_id"),
    ).toHaveLength(1);
    expect(store.get("f.flow")?.doc?.nodes[0].name).toBe("unsaved");
    expect(store.get("f.flow")?.dirty).toBe(true);
  });

  it("clear discards every Flow (project close / switch / worktree change)", async () => {
    disk["a.flow"] = { body: TWO_NODES, checksum: "ck1" };
    disk["b.flow"] = { body: TWO_NODES, checksum: "ck1" };
    const store = new FlowSessionStore();
    await opened(store, "a.flow");
    await opened(store, "b.flow");

    store.clear();

    expect(store.get("a.flow")).toBeUndefined();
    expect(store.get("b.flow")).toBeUndefined();
    expect(store.dirtyIds()).toEqual([]);
  });

  it("hands each Flow its own graph, so editing one does not move another", async () => {
    disk["a.flow"] = { body: TWO_NODES, checksum: "ck1" };
    disk["b.flow"] = { body: TWO_NODES, checksum: "ck1" };
    const store = new FlowSessionStore();
    await opened(store, "a.flow");
    await opened(store, "b.flow");

    store.applyEdit("a.flow", (d) => renameNode(d, "n1", "only-a"));

    expect(store.get("a.flow")?.doc?.nodes[0].name).toBe("only-a");
    expect(store.get("b.flow")?.doc?.nodes[0].name).toBe("A");
    expect(store.get("b.flow")?.dirty).toBe(false);
  });
});

describe("external changes (FLO-FR-31)", () => {
  const fireExternal = (artifactId: string, checksum: string) =>
    externalHandlers.forEach((h) => h({ payload: { artifactId, checksum } }));

  it("reloads a Flow tab holding no unsaved changes", async () => {
    disk["f.flow"] = { body: TWO_NODES, checksum: "ck1" };
    const store = new FlowSessionStore();
    store.watchExternalChanges();
    await opened(store, "f.flow");

    disk["f.flow"] = {
      body: '{"version":1,"nodes":[{"id":"n9","name":"external","position":{"x":1,"y":2}}],"edges":[]}',
      checksum: "ck2",
    };
    fireExternal("f.flow", "ck2");
    await store.get("f.flow")?.pendingLoad;

    expect(store.get("f.flow")?.doc?.nodes.map((n) => n.name)).toEqual([
      "external",
    ]);
    expect(store.get("f.flow")?.dirty).toBe(false);
  });

  it("keeps a dirty Flow's in-memory graph, and its next write replaces the file", async () => {
    disk["f.flow"] = { body: TWO_NODES, checksum: "ck1" };
    const store = new FlowSessionStore();
    store.watchExternalChanges();
    await opened(store, "f.flow");
    store.applyEdit("f.flow", (d) => renameNode(d, "n1", "mine"));

    disk["f.flow"] = { body: '{"version":1,"nodes":[],"edges":[]}', checksum: "ck2" };
    fireExternal("f.flow", "ck2");
    await store.get("f.flow")?.pendingLoad;

    expect(store.get("f.flow")?.doc?.nodes[0].name).toBe("mine");
    expect(store.get("f.flow")?.dirty).toBe(true);

    await store.flush("f.flow");
    expect(JSON.parse(writes[0].body).nodes[0].name).toBe("mine");
  });

  // The race the reload path has to lose: the tab is clean when the event
  // arrives, so a reload starts — and the user edits before the file comes back.
  // Adopting it then would discard an edit made after the decision to reload.
  it("keeps an edit applied while a reload was still in flight", async () => {
    disk["f.flow"] = { body: TWO_NODES, checksum: "ck1" };
    const store = new FlowSessionStore();
    store.watchExternalChanges();
    await opened(store, "f.flow");

    disk["f.flow"] = {
      body: '{"version":1,"nodes":[{"id":"n9","name":"external","position":{"x":1,"y":2}}],"edges":[]}',
      checksum: "ck2",
    };
    fireExternal("f.flow", "ck2");
    // Before the load resolves — the tab was clean when it started.
    store.applyEdit("f.flow", (d) => renameNode(d, "n1", "mine"));
    await store.get("f.flow")?.pendingLoad;

    expect(store.get("f.flow")?.doc?.nodes.map((n) => n.name)).toEqual([
      "mine",
      "B",
    ]);
    expect(store.get("f.flow")?.dirty).toBe(true);

    // …and the next write replaces the file with the graph the user kept.
    await store.flush("f.flow");
    expect(JSON.parse(writes[0].body).nodes[0].name).toBe("mine");
  });

  it("ignores the echo of its own write and events for Flows it holds nothing for", async () => {
    disk["f.flow"] = { body: TWO_NODES, checksum: "ck1" };
    const store = new FlowSessionStore();
    store.watchExternalChanges();
    await opened(store, "f.flow");
    store.applyEdit("f.flow", (d) => renameNode(d, "n1", "mine"));
    await store.flush("f.flow");
    const loadsBefore = invokeMock.mock.calls.filter(
      (c) => c[0] === "load_artifact_contents_by_id",
    ).length;

    // The watcher reporting our own write, and an unrelated artifact.
    fireExternal("f.flow", "ck-1");
    fireExternal("other.flow", "ck9");
    await store.get("f.flow")?.pendingLoad;

    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "load_artifact_contents_by_id"),
    ).toHaveLength(loadsBefore);
    expect(store.get("other.flow")).toBeUndefined();
  });

  // Ref-counted: the shell and a mounted canvas both depend on the watch, but
  // exactly one subscription exists — and a stop always detaches the listener it
  // started, even one whose `listen()` resolved after that stop (which is what
  // StrictMode's mount/cleanup/mount produces).
  it("keeps exactly one subscription and detaches every listener it started", async () => {
    const store = new FlowSessionStore();

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

describe("backend validation (FLO-FR-46 / FLO-FR-47)", () => {
  // FLO-FR-46: the backend judges the body before the canvas renders anything.
  it("validates the loaded body and renders nothing when it is refused", async () => {
    disk["f.flow"] = { body: TWO_NODES, checksum: "ck1" };
    validation = {
      valid: false,
      violations: [
        { code: "cross_container_edge", message: "e1 crosses a boundary", edgeId: "e1" },
        { code: "duplicate_id", message: "two elements carry n1", elementId: "n1" },
      ],
    };
    const store = new FlowSessionStore();

    const s = await opened(store, "f.flow");

    expect(invokeMock).toHaveBeenCalledWith("validate_flow_document", {
      body: TWO_NODES,
    });
    expect(s.doc).toBeNull();
    // FLO-FR-05: every violation the report named, not the first.
    expect(s.violations).toHaveLength(2);
    expect(s.parseError).toContain("e1 crosses a boundary");
    expect(s.parseError).toContain("two elements carry n1");

    // …and no write can reach the file while it stands.
    await store.flush("f.flow", { force: true });
    expect(writes).toEqual([]);
  });

  it("clears the violations once a body validates again", async () => {
    disk["f.flow"] = { body: TWO_NODES, checksum: "ck1" };
    validation = {
      valid: false,
      violations: [{ code: "not_json", message: "nope" }],
    };
    const store = new FlowSessionStore();
    await opened(store, "f.flow");
    expect(store.get("f.flow")?.violations).toHaveLength(1);

    validation = { valid: true, violations: [] };
    await store.load("f.flow");

    expect(store.get("f.flow")?.violations).toEqual([]);
    expect(store.get("f.flow")?.parseError).toBeNull();
    expect(store.get("f.flow")?.doc?.nodes).toHaveLength(2);
  });

  // FLO-FR-27, FLO-FR-47, FLO-FR-28: nothing is written, the file is unchanged, the graph and its
  // dirty state are exactly as they were, and every violation is surfaced.
  it("refuses a write whose document does not validate", async () => {
    disk["f.flow"] = { body: TWO_NODES, checksum: "ck1" };
    const store = new FlowSessionStore();
    await opened(store, "f.flow");
    store.applyEdit("f.flow", (d) => renameNode(d, "n1", "Collect context"));

    validation = {
      valid: false,
      violations: [
        { code: "cross_container_edge", message: "e4 crosses a loop boundary", edgeId: "e4" },
      ],
    };
    const result = await store.flush("f.flow");

    expect(result).toEqual({ ok: false, blocked: "error", artifactId: "f.flow" });
    expect(writes).toEqual([]);
    expect(disk["f.flow"].body).toBe(TWO_NODES);
    const s = store.get("f.flow")!;
    expect(s.dirty).toBe(true);
    expect(s.doc?.nodes[0].name).toBe("Collect context");
    expect(s.violations.map((v) => v.edgeId)).toEqual(["e4"]);
    expect(s.error).toContain("e4 crosses a loop boundary");

    // FLO-FR-47: a refused write is a write that cannot proceed safely, so a
    // transition that flushes first is cancelled by it.
    expect(await store.flushAll()).toEqual({
      ok: false,
      blocked: "error",
      artifactId: "f.flow",
    });

    // Fixing it lets the same graph through, and clears the report.
    validation = { valid: true, violations: [] };
    expect(await store.flush("f.flow")).toEqual({ ok: true });
    expect(writes).toHaveLength(1);
    expect(store.get("f.flow")?.violations).toEqual([]);
    expect(store.get("f.flow")?.dirty).toBe(false);
  });

  // The one path where the backend and the deserializer disagree: the report
  // said Flow, the parser could not build one. It has to surface rather than
  // leave the tab on a blank canvas, and it must still write nothing.
  it("surfaces a body the backend accepted but the parser cannot deserialize", async () => {
    disk["f.flow"] = { body: '{"version":1,"nodes":"not an array","edges":[]}', checksum: "ck1" };
    validation = { valid: true, violations: [] };
    const store = new FlowSessionStore();

    const s = await opened(store, "f.flow");

    expect(s.doc).toBeNull();
    expect(s.parseError).toMatch(/`nodes` must be an array/);
    // No violation list to show, so the tab falls back to the parser's reason.
    expect(s.violations).toEqual([]);
    await store.flush("f.flow", { force: true });
    expect(writes).toEqual([]);
  });
});

/**
 * FLO-FR-27 / FLO-FR-47: the rest after the last canvas edit, and what becomes
 * of the write it schedules.
 *
 * Driven against the store because the schedule is the store's: only the active
 * tab's canvas is mounted (FLO-FR-30), and the requirement is precisely that a
 * write lands for a Flow nobody is looking at.
 */
describe("the rest after the last canvas edit (FLO-FR-27)", () => {
  /** Let a scheduled write's rest elapse. */
  const rest = async () => {
    await new Promise((r) => setTimeout(r, AUTOSAVE_DELAY_MS + 30));
  };

  it("FLO-FR-27: writes once after a drag rather than once per frame", async () => {
    disk["f.flow"] = { body: TWO_NODES, checksum: "ck1" };
    const store = new FlowSessionStore();
    await opened(store, "f.flow");

    // A drag arrives as one edit per animation frame.
    for (let x = 1; x <= 60; x++) {
      store.applyEdit("f.flow", (d) => moveNode(d, "n1", { x, y: x }));
    }
    expect(writes).toHaveLength(0);

    await rest();

    expect(writes).toHaveLength(1);
    // And it carries the node's final position, not an intermediate one.
    expect(writes[0].body).toContain('"x": 60');
  });

  it("FLO-FR-27, FLO-FR-30: lands while the Flow tab sits in the background", async () => {
    disk["f.flow"] = { body: TWO_NODES, checksum: "ck1" };
    const store = new FlowSessionStore();
    await opened(store, "f.flow");
    store.applyEdit("f.flow", (d) => renameNode(d, "n1", "Collect context"));

    await rest();

    expect(writes).toHaveLength(1);
    expect(store.get("f.flow")?.dirty).toBe(false);
  });

  it("spends the schedule when a Save brings the write forward (SNV-FR-29)", async () => {
    disk["f.flow"] = { body: TWO_NODES, checksum: "ck1" };
    const store = new FlowSessionStore();
    await opened(store, "f.flow");
    store.applyEdit("f.flow", (d) => renameNode(d, "n1", "Collect context"));

    await store.flush("f.flow", { force: true });
    expect(writes).toHaveLength(1);

    await rest();
    expect(writes).toHaveLength(1);
  });

  // FLO-FR-27, FLO-FR-28 / FLO-FR-47: a graph the author is midway through repairing must
  // not be re-refused every few seconds.
  it("FLO-FR-27, FLO-FR-47, FLO-FR-28: does not re-attempt a write the validation refused", async () => {
    disk["f.flow"] = { body: TWO_NODES, checksum: "ck1" };
    const store = new FlowSessionStore();
    await opened(store, "f.flow");
    validation = {
      valid: false,
      violations: [{ code: "FGV-DANGLING-EDGE", message: "edge to nowhere" }],
    };
    store.applyEdit("f.flow", (d) => renameNode(d, "n1", "Collect context"));

    await rest();
    expect(writes).toHaveLength(0);
    expect(store.get("f.flow")?.violations).toHaveLength(1);
    expect(store.get("f.flow")?.dirty).toBe(true);

    // Waiting without editing attempts nothing further.
    await rest();
    expect(writes).toHaveLength(0);

    // The author's next edit is what schedules the next attempt, and a graph
    // that now validates is written.
    validation = { valid: true, violations: [] };
    store.applyEdit("f.flow", (d) => renameNode(d, "n2", "Draft review"));
    await rest();

    expect(writes).toHaveLength(1);
    expect(store.get("f.flow")?.violations).toEqual([]);
  });

  it("FLO-FR-28: a closed tab writes nothing further", async () => {
    disk["f.flow"] = { body: TWO_NODES, checksum: "ck1" };
    const store = new FlowSessionStore();
    await opened(store, "f.flow");
    store.applyEdit("f.flow", (d) => renameNode(d, "n1", "Collect context"));

    // The tab close flushes first, then ends the session (FLO-FR-28).
    await store.flush("f.flow");
    store.closeTab("f.flow");
    await rest();

    expect(writes).toHaveLength(1);
  });

  it("FLO-FR-29: a teardown drops every write it did not already perform", async () => {
    disk["f.flow"] = { body: TWO_NODES, checksum: "ck1" };
    const store = new FlowSessionStore();
    await opened(store, "f.flow");
    store.applyEdit("f.flow", (d) => renameNode(d, "n1", "Collect context"));

    store.clear();

    expect(store.hasPendingWrite("f.flow")).toBe(false);
    await rest();
    expect(writes).toHaveLength(0);
  });

  it("FLO-FR-28: closing the tab drops a write the close did not perform", async () => {
    disk["f.flow"] = { body: TWO_NODES, checksum: "ck1" };
    const store = new FlowSessionStore();
    await opened(store, "f.flow");
    store.applyEdit("f.flow", (d) => renameNode(d, "n1", "Collect context"));
    expect(store.hasPendingWrite("f.flow")).toBe(true);

    store.closeTab("f.flow");

    expect(store.hasPendingWrite("f.flow")).toBe(false);
    await rest();
    expect(writes).toHaveLength(0);
  });

  /**
   * Two writes of one Flow must never be in flight together — see the artifact
   * store's test of the same name for what overlapping costs.
   */
  it("never overlaps two writes of the same Flow", async () => {
    disk["f.flow"] = { body: TWO_NODES, checksum: "ck1" };
    const store = new FlowSessionStore();
    await opened(store, "f.flow");

    const release: Array<() => void> = [];
    let concurrent = 0;
    let peak = 0;
    holdWrites = () =>
      new Promise<void>((resolve) => {
        concurrent += 1;
        peak = Math.max(peak, concurrent);
        release.push(() => {
          concurrent -= 1;
          resolve();
        });
      });

    store.applyEdit("f.flow", (d) => renameNode(d, "n1", "First"));
    const a = store.flush("f.flow");
    await until(() => release.length === 1, "the first write to start");
    store.applyEdit("f.flow", (d) => renameNode(d, "n1", "Second"));
    const b = store.flush("f.flow", { force: true });
    await until(() => concurrent === 1, "the second write to queue");

    // The second write is queued, not running alongside the first.
    expect(release).toHaveLength(1);

    release[0]();
    await a;
    await until(() => release.length === 2, "the second write to start");
    expect(peak).toBe(1);
    release[1]();
    await b;

    expect(writes).toHaveLength(2);
    expect(writes[1].body).toContain("Second");
    expect(store.get("f.flow")?.dirty).toBe(false);
  });
});
