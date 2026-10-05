import { beforeEach, describe, expect, it, vi } from "vitest";

import { EditSessionStore } from "./editSessions";
import { recordEdit, hasEdits } from "./editHistory";

// EDT-FR-37–EDT-FR-41 — the two editing conventions the status bar surfaces:
// an artifact's indentation (client-side, per-artifact, part of retained edit
// state) and the project's line endings (backend-normalised, whose change marks
// open tabs dirty).

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "save_artifact_contents") return { checksum: "ck-saved" };
    throw new Error(`unexpected invoke ${cmd}`);
  });
});

/** An artifact loaded with `body`, then edited to `next`. */
function edit(store: EditSessionStore, id: string, body: string, next: string) {
  store.adoptLoad(id, body, "ck1");
  const s = store.ensure(id);
  s.buffer = next;
  recordEdit(s.history, next, "text", "source");
  store.update(id, { dirty: true });
  return s;
}

describe("indentation on the edit session (EDT-FR-37 / EDT-FR-39)", () => {
  it("detects the convention when a load is adopted", () => {
    const store = new EditSessionStore();
    store.adoptLoad("a.md", "- one\n\t- nested\n", "ck1");
    expect(store.get("a.md")?.indentation).toEqual({ kind: "tabs" });
    expect(store.get("a.md")?.indentationOverridden).toBe(false);

    store.adoptLoad("b.md", "# no indents here\n", "ck1");
    expect(store.get("b.md")?.indentation).toEqual({
      kind: "spaces",
      width: 2,
    });
  });

  it("an override outranks detection on a later load", () => {
    // EXC-FR-WDAV's Load-from-filesystem re-adopts the body. Re-detecting there
    // would silently undo a choice the user made earlier in the session.
    const store = new EditSessionStore();
    store.adoptLoad("a.md", "- one\n  - nested\n", "ck1");
    store.setIndentation("a.md", { kind: "tabs" });
    store.adoptLoad("a.md", "- one\n  - nested\n", "ck2");
    expect(store.get("a.md")?.indentation).toEqual({ kind: "tabs" });
  });

  it("setting a convention marks nothing dirty and adds no history step", () => {
    // EDT-FR-38 / STB-FR-23: choosing a convention rewrites no line, alters no
    // byte, does not mark the artifact dirty, and occupies no position in the
    // undo history.
    const store = new EditSessionStore();
    store.adoptLoad("a.md", "- one\n  - nested\n", "ck1");
    const before = store.get("a.md")!.buffer;

    store.setIndentation("a.md", { kind: "tabs" });

    const s = store.get("a.md")!;
    expect(s.dirty).toBe(false);
    expect(s.buffer).toBe(before);
    expect(hasEdits(s.history)).toBe(false);
  });

  it("an overridden convention survives the tab closing and reopening", () => {
    // EDT-FR-39, EDT-FR-28 / STB-FR-24. The override is something the user authored, so
    // — like an edit — it keeps the record alive past the close.
    const store = new EditSessionStore();
    store.openTab("a.md");
    store.adoptLoad("a.md", "- one\n  - nested\n", "ck1");
    store.setIndentation("a.md", { kind: "tabs" });
    store.closeTab("a.md");

    expect(store.get("a.md")?.indentation).toEqual({ kind: "tabs" });
  });

  it("a merely detected convention is forgotten with the record", () => {
    // The other side of the same rule: an artifact opened, read, and closed
    // without an edit or an override carries nothing forward, so its next open
    // is a plain first load described by detection alone (EDT-FR-28).
    const store = new EditSessionStore();
    store.openTab("a.md");
    store.adoptLoad("a.md", "- one\n\t- nested\n", "ck1");
    store.closeTab("a.md");
    expect(store.get("a.md")).toBeUndefined();
  });

  it("discards the override with the retained state on a project close", () => {
    // EDT-FR-39, EDT-FR-28, second half: reopening after the project closed re-detects.
    const store = new EditSessionStore();
    store.openTab("a.md");
    store.adoptLoad("a.md", "- one\n  - nested\n", "ck1");
    store.setIndentation("a.md", { kind: "tabs" });

    store.clear();

    store.adoptLoad("a.md", "- one\n  - nested\n", "ck1");
    expect(store.get("a.md")?.indentation).toEqual({
      kind: "spaces",
      width: 2,
    });
  });
});

describe("markOpenTabsDirty (EDT-FR-41 / STB-FR-18)", () => {
  it("dirties every open tab without touching a buffer or a history", () => {
    // EDT-FR-41 / EDT-FR-70, STB-FR-18: two clean open tabs go dirty, their buffers are
    // byte-identical to before, and undo still reverses a user edit rather than
    // the convention change.
    const store = new EditSessionStore();
    store.openTab("a.md");
    store.adoptLoad("a.md", "alpha\n", "ck1");
    store.openTab("b.md");
    const b = edit(store, "b.md", "beta\n", "beta edited\n");
    store.update("b.md", { dirty: false }); // saved, so both start clean
    const aBuffer = store.get("a.md")!.buffer;
    const bBuffer = b.buffer;
    const bHistoryLength = b.history.steps.length;

    store.markOpenTabsDirty();

    expect(store.get("a.md")?.dirty).toBe(true);
    expect(store.get("b.md")?.dirty).toBe(true);
    expect(store.get("a.md")?.buffer).toBe(aBuffer);
    expect(store.get("b.md")?.buffer).toBe(bBuffer);
    expect(store.get("b.md")?.history.steps.length).toBe(bHistoryLength);
  });

  it("leaves an artifact with no open tab alone", () => {
    // EDT-FR-41 / STB-FR-18: it gains no dirty marker and is converted whenever
    // it is next written.
    const store = new EditSessionStore();
    store.openTab("open.md");
    store.adoptLoad("open.md", "alpha\n", "ck1");
    // A closed artifact retained because it was edited then saved.
    store.openTab("closed.md");
    edit(store, "closed.md", "beta\n", "beta edited\n");
    store.update("closed.md", { dirty: false });
    store.closeTab("closed.md");

    store.markOpenTabsDirty();

    expect(store.get("open.md")?.dirty).toBe(true);
    expect(store.get("closed.md")?.dirty).toBe(false);
  });

  it("notifies subscribers exactly once for a batch", () => {
    // The shell re-renders off the store's version; a per-artifact notify would
    // make a convention change an O(tabs) render storm.
    const store = new EditSessionStore();
    store.openTab("a.md");
    store.adoptLoad("a.md", "alpha\n", "ck1");
    store.openTab("b.md");
    store.adoptLoad("b.md", "beta\n", "ck1");

    const before = store.getVersion();
    store.markOpenTabsDirty();
    expect(store.getVersion()).toBe(before + 1);

    // And a second call with nothing left to dirty notifies not at all.
    const after = store.getVersion();
    store.markOpenTabsDirty();
    expect(store.getVersion()).toBe(after);
  });
});
