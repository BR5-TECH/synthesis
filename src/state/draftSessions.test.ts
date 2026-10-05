import { describe, expect, it, vi, beforeEach } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
const listenMock = vi.fn(async () => () => {});
vi.mock("@tauri-apps/api/event", () => ({
  listen: (...args: unknown[]) => listenMock(...(args as [])),
}));

import { DraftSessionStore } from "./draftSessions";
import { AUTOSAVE_DELAY_MS } from "./writeSchedule";
import { EditSessionStore } from "./editSessions";
import {
  artifactTransport,
  draftDocKey,
  draftTransport,
  parseDraftDocKey,
} from "./documentTransport";

/** Put an unsaved buffer into one draft file's session. */
function edit(store: DraftSessionStore, draftId: string, path: string, body: string) {
  const key = store.key(draftId, path);
  store.docs.ensure(key).buffer = body;
  store.docs.update(key, { dirty: true });
  return key;
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async () => ({ checksum: "c" }));
  listenMock.mockClear();
});

describe("draft document keys (documentTransport)", () => {
  it("round-trips a root file, a nested file, and a deeply nested one", () => {
    for (const [draftId, path] of [
      ["d1", "spec.md"],
      ["d1", "ui/NAW.md"],
      ["abc-123_XYZ", "a/b/c/d.md"],
    ] as const) {
      const key = draftDocKey(draftId, path);
      expect(parseDraftDocKey(key)).toEqual({ draftId, path });
    }
  });

  it("splits at the FIRST separator, so a nested path stays whole", () => {
    // A draft id is a bare token of [A-Za-z0-9_-] (`is_valid_draft_id` in
    // drafts.rs), so it holds no separator of its own — which is exactly what
    // makes the first `/` the boundary however deep the file sits. Splitting at
    // the last one would name the draft `d1/ui` and the file `NAW.md`.
    expect(parseDraftDocKey("d1/ui/core/NAW.md")).toEqual({
      draftId: "d1",
      path: "ui/core/NAW.md",
    });
  });

  it("a key with no separator names no file, and does not throw", () => {
    // The backend's own validation is what should report this; taking down a
    // render instead would turn a bad id into a blank tab.
    expect(parseDraftDocKey("d1")).toEqual({ draftId: "d1", path: "" });
  });

  it("the draft transport invokes the draft operations, not the artifact ones", async () => {
    invokeMock.mockImplementation(async () => ({ body: "x", checksum: "c" }));
    await draftTransport.load(draftDocKey("d1", "ui/NAW.md"));
    expect(invokeMock).toHaveBeenCalledWith("load_draft_file_contents", {
      id: "d1",
      path: "ui/NAW.md",
    });

    invokeMock.mockClear();
    await draftTransport.save(draftDocKey("d1", "ui/NAW.md"), "body");
    expect(invokeMock).toHaveBeenCalledWith("save_draft_file_contents", {
      id: "d1",
      path: "ui/NAW.md",
      body: "body",
    });
  });

  it("only the artifact transport claims the external-change channel", () => {
    // EXC-FR-LKHZ watches artifacts; nothing watches `.synthesis/drafts/`, and the
    // ids in that event are project-relative paths.
    expect(artifactTransport.watchesExternalChanges).toBe(true);
    expect(draftTransport.watchesExternalChanges).toBe(false);
  });
});

describe("EditSessionStore over a non-watching transport", () => {
  it("registers no external-change listener at all", () => {
    const store = new EditSessionStore(draftTransport);

    const stop = store.watchExternalChanges();
    stop();

    // Delete the guard in `watchExternalChanges` and this fails: the draft store
    // would subscribe, and an artifact's change carrying a matching id would
    // raise a blocking divergence modal over a draft file.
    expect(listenMock).not.toHaveBeenCalled();
  });

  it("writes through its own transport", async () => {
    const store = new EditSessionStore(draftTransport);
    const key = draftDocKey("d1", "spec.md");
    store.ensure(key).buffer = "written";
    store.update(key, { dirty: true });

    await store.flush(key);

    expect(invokeMock).toHaveBeenCalledWith("save_draft_file_contents", {
      id: "d1",
      path: "spec.md",
      body: "written",
    });
  });
});

describe("EditSessionStore.rekey / forget / allIds", () => {
  it("rekey carries the whole record to the new id", () => {
    const store = new EditSessionStore();
    store.adoptLoad("a.md", "loaded", "sum");
    const s = store.ensure("a.md");
    s.buffer = "unsaved";
    store.update("a.md", { dirty: true, mode: "text" });

    store.rekey("a.md", "b.md");

    expect(store.get("a.md")).toBeUndefined();
    const moved = store.get("b.md")!;
    expect(moved.artifactId).toBe("b.md");
    expect(moved.buffer).toBe("unsaved");
    expect(moved.dirty).toBe(true);
    expect(moved.mode).toBe("text");
    // The bytes on disk did not change, only where they are.
    expect(moved.baseline).toBe("sum");
    expect(store.dirtyIds()).toEqual(["b.md"]);
  });

  it("rekey onto itself changes nothing", () => {
    const store = new EditSessionStore();
    store.ensure("a.md").buffer = "kept";
    store.rekey("a.md", "a.md");
    expect(store.get("a.md")?.buffer).toBe("kept");
  });

  it("rekey of an id the store does not hold is a no-op", () => {
    const store = new EditSessionStore();
    store.rekey("missing.md", "b.md");
    expect(store.allIds()).toEqual([]);
  });

  it("forget drops a dirty record so no teardown writes it back", async () => {
    const store = new EditSessionStore();
    store.ensure("gone.md").buffer = "content";
    store.update("gone.md", { dirty: true });

    store.forget("gone.md");

    expect(store.get("gone.md")).toBeUndefined();
    expect(store.dirtyIds()).toEqual([]);
    await store.flushAll();
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("allIds reports every record, dirty or not", () => {
    const store = new EditSessionStore();
    store.ensure("a.md");
    store.ensure("b.md");
    expect(store.allIds().sort()).toEqual(["a.md", "b.md"]);
  });
});

describe("DraftSessionStore", () => {
  it("keeps the selection and the rail state per draft", () => {
    const store = new DraftSessionStore();

    // NAW-FR-08: hidden is the default; NAW-FR-10: nothing is selected until a
    // file is.
    expect(store.ensure("d1").railShown).toBe(false);
    expect(store.ensure("d1").selected).toBeNull();

    store.setRailShown("d1", true);
    store.select("d1", "ui/NAW.md");

    expect(store.get("d1")).toMatchObject({
      railShown: true,
      selected: "ui/NAW.md",
    });
    // Another draft is unaffected — the state belongs to the draft, not to the
    // workspace component that happened to set it.
    expect(store.ensure("d2")).toMatchObject({
      railShown: false,
      selected: null,
    });
  });

  it("scopes dirtiness to the draft, including one whose id is a prefix of another", () => {
    const store = new DraftSessionStore();
    edit(store, "d10", "a.md", "one");

    // Drop the trailing `/` from the prefix in `isDirty` and this reports d1 as
    // dirty on the strength of d10's buffer.
    expect(store.isDirty("d10")).toBe(true);
    expect(store.isDirty("d1")).toBe(false);
    expect(store.dirtyIds()).toEqual(["d10"]);
  });

  it("flushes only the named draft's files", async () => {
    const store = new DraftSessionStore();
    edit(store, "d1", "a.md", "mine");
    edit(store, "d2", "b.md", "theirs");

    await store.flush("d1");

    const saved = invokeMock.mock.calls.filter(
      (c) => c[0] === "save_draft_file_contents",
    );
    expect(saved).toHaveLength(1);
    expect(saved[0][1]).toMatchObject({ id: "d1", path: "a.md", body: "mine" });
    expect(store.isDirty("d2")).toBe(true);
  });

  it("writes every dirty file of a draft, not just the selected one", async () => {
    const store = new DraftSessionStore();
    edit(store, "d1", "a.md", "one");
    edit(store, "d1", "ui/b.md", "two");
    store.select("d1", "a.md");

    await store.flush("d1");

    const paths = invokeMock.mock.calls
      .filter((c) => c[0] === "save_draft_file_contents")
      .map((c) => (c[1] as { path: string }).path)
      .sort();
    expect(paths).toEqual(["a.md", "ui/b.md"]);
    expect(store.isDirty("d1")).toBe(false);
  });

  it("reports a failed write against the draft, so the shell can focus its tab", async () => {
    const store = new DraftSessionStore();
    edit(store, "d1", "a.md", "unwritable");
    invokeMock.mockImplementation(async () => {
      throw "disk full";
    });

    const res = await store.flush("d1");

    expect(res.ok).toBe(false);
    // The draft, not the file key — a tab is opened on a draft.
    expect(res.artifactId).toBe("d1");
    expect(store.isDirty("d1")).toBe(true);
  });

  it("NAW-FR-13: an emptied draft file is written rather than blocked", async () => {
    // A draft is working material and is routinely emptied on the way to being
    // rewritten. The Editor's empty-save confirmation guards a *published*
    // artifact; raising it against an autosave would put a modal in front of an
    // author who did nothing but select all and start over — and the workspace
    // deliberately renders no Save control to answer it with.
    const store = new DraftSessionStore();
    const key = store.key("d1", "a.md");
    store.docs.adoptLoad(key, "had content", "sum");
    store.docs.ensure(key).buffer = "";
    store.docs.update(key, { dirty: true });

    const res = await store.flush("d1");

    expect(res.ok).toBe(true);
    expect(invokeMock).toHaveBeenCalledWith("save_draft_file_contents", {
      id: "d1",
      path: "a.md",
      body: "",
    });
  });

  /**
   * NAW-FR-13: a draft file's rest belongs to its New Artifact tab, which also
   * reports the write and refreshes the draft list. If the document store armed
   * one of its own, one buffer would carry two schedules and be written twice.
   */
  it("NAW-FR-13: the document store arms no schedule of its own", async () => {
    const store = new DraftSessionStore();
    const key = store.key("d1", "a.md");
    edit(store, "d1", "a.md", "unsaved words");

    expect(store.docs.writes.enabled).toBe(false);
    expect(store.docs.hasPendingWrite(key)).toBe(false);
    await new Promise((r) => setTimeout(r, AUTOSAVE_DELAY_MS + 200));
    expect(invokeMock).not.toHaveBeenCalledWith(
      "save_draft_file_contents",
      expect.anything(),
    );
    // The edit itself registered — only the schedule is the tab's.
    expect(store.docs.get(key)?.dirty).toBe(true);
  });

  describe("renamePath (NAW-FR-09)", () => {
    it("carries a renamed file's session and moves the selection with it", () => {
      const store = new DraftSessionStore();
      edit(store, "d1", "ui/NAW.md", "unsaved words");
      store.select("d1", "ui/NAW.md");

      store.renamePath("d1", "ui/NAW.md", "ui/NAW-new-artifact.md");

      expect(store.docs.get(store.key("d1", "ui/NAW.md"))).toBeUndefined();
      expect(store.docs.get(store.key("d1", "ui/NAW-new-artifact.md"))?.buffer).toBe(
        "unsaved words",
      );
      expect(store.get("d1")?.selected).toBe("ui/NAW-new-artifact.md");
    });

    it("moves a renamed folder's whole subtree", () => {
      const store = new DraftSessionStore();
      edit(store, "d1", "ui/a.md", "one");
      edit(store, "d1", "ui/nested/b.md", "two");
      store.select("d1", "ui/nested/b.md");

      store.renamePath("d1", "ui", "surface");

      expect(store.docs.get(store.key("d1", "surface/a.md"))?.buffer).toBe("one");
      expect(store.docs.get(store.key("d1", "surface/nested/b.md"))?.buffer).toBe(
        "two",
      );
      expect(store.get("d1")?.selected).toBe("surface/nested/b.md");
    });

    it("leaves a same-prefixed sibling alone", () => {
      // Match on the path or a `/`-terminated prefix of it, never a bare
      // `startsWith`: replace the match with `path.startsWith(from)` and
      // `DRS.md.bak` is dragged along and loses its own buffer.
      const store = new DraftSessionStore();
      edit(store, "d1", "DRS.md", "renamed one");
      edit(store, "d1", "DRS.md.bak", "keep me");
      store.select("d1", "DRS.md.bak");

      store.renamePath("d1", "DRS.md", "DRS-draft-storage.md");

      expect(store.docs.get(store.key("d1", "DRS.md.bak"))?.buffer).toBe("keep me");
      expect(store.get("d1")?.selected).toBe("DRS.md.bak");
    });

    it("touches no other draft's sessions", () => {
      const store = new DraftSessionStore();
      edit(store, "d1", "a.md", "mine");
      edit(store, "d2", "a.md", "theirs");

      store.renamePath("d1", "a.md", "b.md");

      expect(store.docs.get(store.key("d2", "a.md"))?.buffer).toBe("theirs");
      expect(store.docs.get(store.key("d2", "b.md"))).toBeUndefined();
    });
  });

  describe("drop and clear", () => {
    it("drop removes the draft's own state and every one of its file sessions", () => {
      const store = new DraftSessionStore();
      edit(store, "d1", "a.md", "one");
      edit(store, "d10", "a.md", "prefix twin");
      store.select("d1", "a.md");

      store.drop("d1");

      expect(store.get("d1")).toBeUndefined();
      expect(store.docs.get(store.key("d1", "a.md"))).toBeUndefined();
      // Drop the trailing `/` from the prefix and d10's buffer goes with it.
      expect(store.docs.get(store.key("d10", "a.md"))?.buffer).toBe("prefix twin");
    });

    it("clear discards everything, drafts and documents alike", () => {
      const store = new DraftSessionStore();
      edit(store, "d1", "a.md", "one");
      store.setRailShown("d1", true);

      store.clear();

      expect(store.get("d1")).toBeUndefined();
      expect(store.docs.allIds()).toEqual([]);
    });
  });

  it("notifies subscribers when a document goes dirty, not only on its own writes", () => {
    // The workspace and the shell both re-render on this store; the dirty
    // transition is recorded on `docs`, so it has to reach here.
    const store = new DraftSessionStore();
    const listener = vi.fn();
    store.subscribe(listener);

    edit(store, "d1", "a.md", "typed");

    expect(listener).toHaveBeenCalled();
  });
});

describe("the composer's text belongs to the draft (NAW-FR-15)", () => {
  // Held here rather than in `NewArtifactWorkspace`, which unmounts on every
  // tab switch. The requirement is that dismissing the composer and reopening
  // it returns what was typed — and that closing the tab and reopening the
  // draft within the session does too.
  it("survives the tab, is per draft, and is never a draft file", () => {
    const store = new DraftSessionStore();

    expect(store.ensure("d1").composer).toBe("");
    store.setComposer("d1", "rework the graduation part");
    store.setComposer("d2", "something else");

    expect(store.get("d1")?.composer).toBe("rework the graduation part");
    expect(store.get("d2")?.composer).toBe("something else");
    // It is not a document: nothing about it makes a file dirty or gives the
    // draft something to flush.
    expect(store.docs.allIds()).toEqual([]);
    expect(store.isDirty("d1")).toBe(false);
    expect(store.dirtyIds()).toEqual([]);
  });

  it("notifies on a change and stays quiet on a rewrite of the same text", () => {
    const store = new DraftSessionStore();
    store.setComposer("d1", "typed");
    const listener = vi.fn();
    store.subscribe(listener);

    store.setComposer("d1", "typed");
    expect(listener).not.toHaveBeenCalled();

    store.setComposer("d1", "typed more");
    expect(listener).toHaveBeenCalled();
  });

  it("goes with the draft, and with the project", () => {
    const store = new DraftSessionStore();
    store.setComposer("d1", "held for the session");

    store.drop("d1");
    expect(store.ensure("d1").composer).toBe("");

    store.setComposer("d2", "held for the session");
    store.clear();
    expect(store.ensure("d2").composer).toBe("");
  });

  it("holds the composer's pending attachments on exactly the same terms (NAW-FR-31)", () => {
    // CMT-FR-46: nothing has been sent anywhere while these are held, so this is
    // a queue rather than a record — and it belongs to the draft, so dismissing
    // the composer and reopening it returns the strip.
    const store = new DraftSessionStore();
    const shot = {
      input: {
        kind: "inline" as const,
        mediaType: "image/png",
        filename: "chooser.png",
        data: "aGk=",
      },
      name: "chooser.png",
    };

    expect(store.ensure("d1").composerAttachments).toEqual([]);
    const listener = vi.fn();
    store.subscribe(listener);
    store.setComposerAttachments("d1", [shot]);
    expect(listener).toHaveBeenCalled();

    expect(store.get("d1")?.composerAttachments).toEqual([shot]);
    // Per draft, and no more a document than the typed body is.
    expect(store.ensure("d2").composerAttachments).toEqual([]);
    expect(store.docs.allIds()).toEqual([]);
    expect(store.isDirty("d1")).toBe(false);

    // It goes with the draft, and with the project.
    store.drop("d1");
    expect(store.ensure("d1").composerAttachments).toEqual([]);
    store.setComposerAttachments("d2", [shot]);
    store.clear();
    expect(store.ensure("d2").composerAttachments).toEqual([]);
  });
});

describe("the search target of a draft (NAW-FR-57, NAW-FR-59)", () => {
  it("names the live prompt's session only while the prompt is editable", () => {
    const store = new DraftSessionStore();

    // Nothing is searchable before the record has landed: there is no prompt
    // to search and no surface showing one.
    expect(store.searchTarget("d1")).toBeNull();

    store.select("d1", "Untitled.md");
    expect(store.searchTarget("d1")).toBeNull();

    // The tab reports the live prompt is showing and takes edits.
    store.setPromptEditable("d1", true);
    expect(store.searchTarget("d1")).toBe(store.key("d1", "Untitled.md"));

    // NAW-FR-09 / NAW-FR-44: a version being read, or a draft its graduation
    // holds, is not editable — so the shell is given no target at all.
    store.setPromptEditable("d1", false);
    expect(store.searchTarget("d1")).toBeNull();
  });

  it("is per draft, notifies on a change alone, and goes with the draft", () => {
    const store = new DraftSessionStore();
    store.select("d1", "Untitled.md");
    store.select("d2", "Other.md");
    store.setPromptEditable("d1", true);

    // One draft's answer says nothing about another's.
    expect(store.searchTarget("d2")).toBeNull();

    const listener = vi.fn();
    store.subscribe(listener);
    // Republishing the same answer costs the shell no re-render.
    store.setPromptEditable("d1", true);
    expect(listener).not.toHaveBeenCalled();
    store.setPromptEditable("d1", false);
    expect(listener).toHaveBeenCalledTimes(1);

    store.setPromptEditable("d1", true);
    store.drop("d1");
    expect(store.searchTarget("d1")).toBeNull();
    store.select("d2", "Other.md");
    store.setPromptEditable("d2", true);
    store.clear();
    expect(store.searchTarget("d2")).toBeNull();
  });
});
