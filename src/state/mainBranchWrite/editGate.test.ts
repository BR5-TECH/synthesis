import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Editor } from "@tiptap/react";

import { markdownExtensions } from "../../components/markdownFidelity";
import { MainBranchEditGate } from "../../components/Editor/mainBranchEditGate";
import { FlowSessionStore } from "../flowSessions";
import { EditSessionStore } from "../editSessions";
import {
  currentMainBranchWrite,
  holdEdit,
  isEditGateReading,
  isEditHeld,
  mainBranchSaveRefusal,
  noteEditContext,
  primeEditGate,
  rememberConfirmation,
  settleMainBranchWrite,
} from ".";
import { mainGuard, scriptedReads } from "../../test/mainBranchWriteFixtures";
import { otherCommand } from "../../test/invokeMocks";
import { resetLogBufferForTest } from "../../logging";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string) => otherCommand(cmd),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

/** Prime the gate and wait for its read to land. */
async function primed() {
  primeEditGate();
  await vi.waitFor(() => expect(isEditGateReading()).toBe(false));
  expect(isEditHeld()).toBe(true);
}

let editor: Editor | null = null;
afterEach(() => {
  editor?.destroy();
  editor = null;
});

describe("the hold of the first change (EDT-FR-QXEK, EDT-FR-UPNU)", () => {
  beforeEach(() => {
    resetLogBufferForTest();
    scriptedReads(mainGuard());
  });

  it("EDT-FR-QXEK: the guard is read when a file opens, and the first change on an unconfirmed main branch is held and asks for the confirmation", async () => {
    await primed();
    expect(holdEdit()).toBe(true);
    const request = currentMainBranchWrite();
    expect(request?.write).toBe("edit_file");
    expect(request?.guard?.branch).toBe("main");
    // A second change while the confirmation is open asks nothing more.
    expect(holdEdit()).toBe(true);
    expect(currentMainBranchWrite()?.id).toBe(request?.id);
  });

  it("EDT-FR-QXEK: after a cancel the next change asks again; after a confirm changes apply", async () => {
    await primed();
    holdEdit();
    const first = currentMainBranchWrite()?.id;
    settleMainBranchWrite(false);
    // The next change asks again, as a new request, once the first has ended.
    await vi.waitFor(() => {
      expect(holdEdit()).toBe(true);
      expect(currentMainBranchWrite()?.id).not.toBe(first);
    });
    settleMainBranchWrite(false);
    rememberConfirmation(mainGuard({ confirmed: true }));
    expect(holdEdit()).toBe(false);
  });

  it("EDT-FR-QXEK: a branch that is not a main branch holds nothing", async () => {
    await primed();
    scriptedReads(mainGuard({ isMain: false, isDefault: false, branch: "feature" }));
    primeEditGate();
    await vi.waitFor(() => expect(isEditHeld()).toBe(false));
    expect(holdEdit()).toBe(false);
    expect(currentMainBranchWrite()).toBeNull();
  });

  it("EDT-FR-QXEK, MBW-FR-ILFN: a confirmation holds for its project only; a file of another project with the same branch name asks again", async () => {
    await primed();
    rememberConfirmation(mainGuard({ confirmed: true }));
    expect(isEditHeld()).toBe(false);
    // Another project opens a file: its backend holds no confirmation of `main`.
    primeEditGate();
    await vi.waitFor(() => expect(isEditHeld()).toBe(true));
    expect(holdEdit()).toBe(true);
    settleMainBranchWrite(false);
  });

  it("EDT-FR-QXEK, MBW-FR-UNAN: a guard read that fails holds the change and shows the confirmation with the error", async () => {
    const reads = scriptedReads(new Error("no project open"));
    await primed();
    expect(holdEdit()).toBe(true);
    expect(currentMainBranchWrite()).toMatchObject({
      write: "edit_file",
      guard: null,
      readError: "Error: no project open",
    });
    // The dialog reads again, finds a branch that needs nothing, and lets the
    // change go; the gate then reads again too.
    reads.answer = mainGuard({ isMain: false, isDefault: false, branch: "feature" });
    settleMainBranchWrite(true);
    await vi.waitFor(() => expect(isEditHeld()).toBe(false));
  });

  it("EDT-FR-QXEK, EDT-FR-UPNU, MBW-FR-KDCM: the rich surface does not apply a held change, and a load written by the editor itself asks nothing", async () => {
    editor = new Editor({
      extensions: markdownExtensions([MainBranchEditGate]),
      content: "first",
    });
    await primed();

    editor.commands.insertContent(" typed");
    expect(editor.getText()).toBe("first");
    expect(currentMainBranchWrite()?.write).toBe("edit_file");
    settleMainBranchWrite(false);

    editor.commands.setContent("loaded", { emitUpdate: false });
    expect(editor.getText()).toBe("loaded");
    expect(currentMainBranchWrite()).toBeNull();
  });

  it("EDT-FR-UPNU: the Flow canvas, which saves through save artifact contents, holds its first change too", async () => {
    const store = new FlowSessionStore();
    const session = store.ensure("f.flow");
    session.doc = { version: 1, name: "", loops: [], nodes: [], edges: [] };
    await primed();
    store.applyEdit("f.flow", (doc) => ({ ...doc, name: "changed" }));
    expect(session.doc?.name).toBe("");
    expect(session.dirty).toBe(false);
    expect(currentMainBranchWrite()?.write).toBe("edit_file");
    settleMainBranchWrite(false);
  });

  it("MBW-FR-KDCM: an autosave that the backend refuses for the check shows no confirmation", async () => {
    const store = new EditSessionStore(
      {
        load: async () => ({ body: "", checksum: "ck" }),
        save: async () => {
          throw `main_branch_write_unconfirmed: ${JSON.stringify(mainGuard())}`;
        },
        watchesExternalChanges: false,
      },
      false,
    );
    store.adoptLoad("a.md", "first", "ck");
    const session = store.ensure("a.md");
    session.buffer = "changed";
    store.noteEdit("a.md");
    await store.flush("a.md").catch(() => undefined);
    expect(currentMainBranchWrite()).toBeNull();
  });

  it("EDT-FR-QXEK: a change while the read is under way is held and shows nothing; the read then shows the confirmation on an unconfirmed main branch", async () => {
    const reads = scriptedReads(mainGuard());
    let land!: () => void;
    const answer = reads.read.bind(reads);
    reads.read = (branch, refresh) =>
      new Promise((resolve) => {
        land = () => resolve(answer(branch, refresh));
      });
    primeEditGate();
    expect(holdEdit()).toBe(true);
    expect(currentMainBranchWrite()).toBeNull();
    land();
    await vi.waitFor(() => expect(currentMainBranchWrite()?.write).toBe("edit_file"));
    expect(currentMainBranchWrite()?.guard?.branch).toBe("main");
    settleMainBranchWrite(false);
  });

  it("EDT-FR-QXEK: a change while the read is under way is held; a read that finds no main branch shows nothing, and the next change applies", async () => {
    const reads = scriptedReads(mainGuard({ isMain: false, isDefault: false, branch: "feature" }));
    let land!: () => void;
    const answer = reads.read.bind(reads);
    reads.read = (branch, refresh) =>
      new Promise((resolve) => {
        land = () => resolve(answer(branch, refresh));
      });
    primeEditGate();
    expect(holdEdit()).toBe(true);
    land();
    await vi.waitFor(() => expect(isEditGateReading()).toBe(false));
    expect(currentMainBranchWrite()).toBeNull();
    expect(holdEdit()).toBe(false);
  });

  it("EDT-FR-QXEK: a new worktree or branch under an open file reads the guard again", async () => {
    const reads = scriptedReads(mainGuard({ isMain: false, isDefault: false, branch: "feature" }));
    primeEditGate();
    await vi.waitFor(() => expect(isEditGateReading()).toBe(false));
    expect(holdEdit()).toBe(false);
    noteEditContext("/repo::feature");
    expect(reads.reads).toHaveLength(1);
    // The author checks out main in a terminal, and the file stays open.
    reads.answer = mainGuard();
    noteEditContext("/repo::main");
    expect(reads.reads).toHaveLength(2);
    await vi.waitFor(() => expect(isEditGateReading()).toBe(false));
    expect(holdEdit()).toBe(true);
    expect(currentMainBranchWrite()?.guard?.branch).toBe("main");
    settleMainBranchWrite(false);
  });

  it("MBW-FR-YSMC, MBW-FR-KDCM: a save refused for the check reads as words with no guard data, and the next change asks for the confirmation", async () => {
    const reads = scriptedReads(mainGuard({ isMain: false, isDefault: false, branch: "feature" }));
    primeEditGate();
    await vi.waitFor(() => expect(isEditGateReading()).toBe(false));
    const store = new EditSessionStore(
      {
        load: async () => ({ body: "", checksum: "ck" }),
        save: async () => {
          throw `main_branch_write_unconfirmed: ${JSON.stringify(mainGuard())}`;
        },
        watchesExternalChanges: false,
      },
      false,
    );
    store.adoptLoad("a.md", "first", "ck");
    const session = store.ensure("a.md");
    session.buffer = "changed";
    store.noteEdit("a.md");
    reads.answer = mainGuard();
    const before = reads.reads.length;
    await store.flush("a.md").catch(() => undefined);

    expect(session.error).toBe(
      "Not saved: main is a main branch, and the write to it is not confirmed. Change the file again to confirm the write.",
    );
    expect(session.error).not.toMatch(/[{}]/);
    expect(currentMainBranchWrite()).toBeNull();
    expect(reads.reads.length).toBe(before + 1);
    await vi.waitFor(() => expect(isEditGateReading()).toBe(false));
    expect(holdEdit()).toBe(true);
    expect(currentMainBranchWrite()?.write).toBe("edit_file");
    settleMainBranchWrite(false);
  });

  it("MBW-FR-JZME: a save refused because GitHub protects the branch says so, and any other failure is shown as it came", () => {
    expect(
      mainBranchSaveRefusal(
        `main_branch_protected: ${JSON.stringify(mainGuard({ protection: "protected" }))}`,
      ),
    ).toBe(
      "Not saved: GitHub protects main. Work reaches it through a pull request. Start a new branch for this work.",
    );
    expect(mainBranchSaveRefusal("disk full")).toBeNull();
  });
});
