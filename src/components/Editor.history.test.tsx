import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  createEvent,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";

import { Editor } from "./Editor";
import { HISTORY_COALESCE_MS } from "../state/editHistory";
import { EditSessionStore } from "../state/editSessions";
import type { ArtifactChangedPayload, ArtifactContents } from "../types";
import { ARTIFACT_CHANGED_EXTERNALLY } from "../events";
import { letWriteLand } from "../test/autosave";

// Same invoke/listen mocking convention as the rest of the suite. The detailed
// lifecycle (load/save/external-change resolution) is covered against the hook
// in useArtifactDocument.test.tsx; here we verify the integrated Editor mounts
// the WYSIWYG surface, renders Markdown as rich content (EDT-FR-02), and shows
// the blocking modal on an external change.
const invokeMock = vi.fn();
const unlistenMock = vi.fn();
let listeners: Record<
  string,
  (event: { payload: ArtifactChangedPayload }) => void
> = {};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (
      event: string,
      cb: (event: { payload: ArtifactChangedPayload }) => void,
    ) => {
      listeners[event] = cb;
      return unlistenMock;
    },
  ),
}));

// A mutable backend so a test can change the on-disk body before a reload and
// capture what `save` was handed.
interface Backend {
  load: ArtifactContents;
  lastSavedBody?: string;
}

function wireBackend(b: Backend) {
  invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
    if (cmd === "load_artifact_contents_by_id") return b.load;
    if (cmd === "save_artifact_contents") {
      b.lastSavedBody = (args as { body: string }).body;
      return { checksum: "ck-saved" };
    }
    throw new Error(`unexpected invoke ${cmd}`);
  });
}

function wireLoad(load: ArtifactContents) {
  wireBackend({ load });
}

/**
 * The store the tests that need a handle on the write path mount their Editor
 * over. Most tests leave the Editor to its own private store; one that has to
 * bring a write forward on demand (EDT-FR-34) — the only route that writes a
 * file nobody edited — passes this and calls `saveNow`.
 */
let sessions: EditSessionStore;

/** EDT-FR-34: File → Save, brought forward for an artifact mounted on `sessions`. */
async function saveNow(id = "a.md") {
  await act(async () => {
    await sessions.flush(id, { force: true });
  });
}

beforeEach(() => {
  invokeMock.mockReset();
  unlistenMock.mockReset();
  listeners = {};
  sessions = new EditSessionStore();
});

afterEach(() => {
  cleanup();
});

// EDT-FR-22–EDT-FR-25 / `EXC-editor-external-change.md` EXC-FR-QDMC: the tab's single undo/redo history.
//
// Driving a *real* WYSIWYG body edit under jsdom: ProseMirror handles plain
// typing through DOM mutation observation, which jsdom does not produce, but it
// handles keymap commands synchronously. So a body edit here is "select the whole
// document (Mod-A, prosemirror-keymap) then apply a toolbar command" — one
// genuine user transaction through Tiptap's onUpdate, and each command leaves
// visibly different Markdown.
const pmEl = () => document.querySelector(".ProseMirror") as HTMLElement;
/** The surface an accelerator would be delivered to (source when in text mode). */
const editSurface = (): HTMLElement =>
  (screen.queryByLabelText("Markdown source") as HTMLElement | null) ?? pmEl();

const bodyEdit = (toolbarTitle: string) => {
  fireEvent.keyDown(pmEl(), { key: "a", ctrlKey: true }); // selectAll
  fireEvent.mouseDown(screen.getByTitle(toolbarTitle));
};
const pressUndo = () =>
  fireEvent.keyDown(editSurface(), { key: "z", metaKey: true });
const pressRedo = () =>
  fireEvent.keyDown(editSurface(), { key: "z", metaKey: true, shiftKey: true });

const hasTag = (tag: string) => !!pmEl().querySelector(tag);

describe("Editor (undo/redo history)", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  // EDT-FR-23 + EDT-FR-24 (the regression this history exists for):
  // the load itself is not an undoable step, so undo on an untouched document can
  // never walk back into the empty editor Tiptap starts out as.
  it("never empties a freshly loaded artifact, however many undos", async () => {
    wireLoad({ body: "# Title\n\nhello", checksum: "ck1" });
    render(<Editor artifactId="a.md" artifactName="a.md" />);
    await screen.findByRole("heading", { name: "Title" });

    for (let i = 0; i < 10; i++) pressUndo();

    expect(
      screen.getByRole("heading", { name: "Title" }).tagName,
    ).toBe("H1");
    expect(screen.getByText("hello")).toBeInTheDocument();
    expect(pmEl().textContent).toContain("Title");
    // EDT-FR-23, EDT-FR-24: a no-op undo does not fabricate a dirty state either.
    expect(screen.queryByText("● unsaved")).not.toBeInTheDocument();
  });

  // EDT-FR-24: three edits undo in reverse order; the fourth undo is
  // a no-op that leaves the artifact exactly as it loaded.
  it("undoes edits in reverse order and stops at the loaded document", async () => {
    wireLoad({ body: "# Title\n\nhello", checksum: "ck1" });
    render(<Editor artifactId="a.md" artifactName="a.md" />);
    await screen.findByRole("heading", { name: "Title" });

    // Space the edits past the coalescing window so each is its own step.
    vi.useFakeTimers({ shouldAdvanceTime: true });
    bodyEdit("Heading 2");
    vi.advanceTimersByTime(HISTORY_COALESCE_MS + 50);
    bodyEdit("Bold");
    vi.advanceTimersByTime(HISTORY_COALESCE_MS + 50);
    bodyEdit("Quote");
    expect(hasTag("blockquote")).toBe(true);

    pressUndo(); // reverses the quote
    expect(hasTag("blockquote")).toBe(false);
    expect(hasTag("strong")).toBe(true);

    pressUndo(); // reverses the bold
    expect(hasTag("strong")).toBe(false);
    expect(hasTag("h2")).toBe(true);

    pressUndo(); // reverses the heading change — back to the loaded document
    expect(hasTag("h2")).toBe(false);
    expect(hasTag("h1")).toBe(true);

    pressUndo(); // EDT-FR-24: no-op at the floor
    expect(hasTag("h1")).toBe(true);
    expect(pmEl().textContent).toContain("hello");
  });

  // EDT-FR-22: redo replays the traversed steps forward again.
  it("redoes an undone edit", async () => {
    wireLoad({ body: "# Title\n\nhello", checksum: "ck1" });
    render(<Editor artifactId="a.md" artifactName="a.md" />);
    await screen.findByRole("heading", { name: "Title" });

    bodyEdit("Heading 2");
    expect(hasTag("h2")).toBe(true);
    pressUndo();
    expect(hasTag("h1")).toBe(true);
    pressRedo();
    expect(hasTag("h2")).toBe(true);
  });

  // EDT-FR-22 + EDT-FR-25: one history spans both modes, and undo
  // switches back to the mode an edit was made in so the reversal is visible.
  it("undoes across the mode toggle, switching back to the editing mode", async () => {
    wireLoad({ body: "# Title\n\nhello", checksum: "ck1" });
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    await screen.findByRole("heading", { name: "Title" });

    bodyEdit("Heading 2"); // edit 1 — WYSIWYG body
    expect(hasTag("h2")).toBe(true);

    fireEvent.click(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    );
    const source = (await screen.findByLabelText(
      "Markdown source",
    )) as HTMLTextAreaElement;
    fireEvent.change(source, { target: { value: "## Title\n\n## goodbye" } }); // edit 2 — source

    pressUndo(); // reverses the source edit, staying in text mode
    expect(
      (screen.getByLabelText("Markdown source") as HTMLTextAreaElement).value,
    ).toBe("## Title\n\n## hello"); // the whole document, not just its body

    pressUndo(); // reverses the body edit — made in WYSIWYG, so the mode follows
    expect(screen.queryByLabelText("Markdown source")).not.toBeInTheDocument();
    expect(hasTag("h1")).toBe(true);
    expect(hasTag("h2")).toBe(false);
  });

  // EDT-FR-23 + EDT-FR-25: a mode toggle is not an edit, so it never
  // occupies a position in the history and undo has nothing to reverse.
  it("does not make the mode toggle undoable", async () => {
    wireLoad({ body: "# Title\n\nhello", checksum: "ck1" });
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    await screen.findByRole("heading", { name: "Title" });

    for (let i = 0; i < 3; i++) {
      fireEvent.click(
        screen.getByRole("button", { name: "Edit as Markdown source" }),
      );
      fireEvent.click(screen.getByRole("button", { name: "Edit as rich text" }));
    }

    pressUndo();

    // Still WYSIWYG, still the loaded document, still clean.
    expect(screen.queryByLabelText("Markdown source")).not.toBeInTheDocument();
    expect(hasTag("h1")).toBe(true);
    expect(screen.queryByText("● unsaved")).not.toBeInTheDocument();
  });

  // EDT-FR-22 + EDT-FR-25: frontmatter edits share the one history,
  // and undoing one re-expands a collapsed region so the reversal is visible.
  it("undoes frontmatter and body edits in one stack, revealing the region", async () => {
    wireLoad({
      body: "---\ntitle: Old\n---\n\n# Heading\n\nbody",
      checksum: "ck1",
    });
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    const fm = (await screen.findByLabelText(
      "Frontmatter",
    )) as HTMLTextAreaElement;

    fireEvent.change(fm, { target: { value: "title: New" } }); // edit 1
    bodyEdit("Heading 2"); // edit 2 — different surface, so its own step
    expect(hasTag("h2")).toBe(true);

    // Collapse the region by scrolling the body down (EDT-FR-20).
    const scroller = document.querySelector(".editor") as HTMLElement;
    Object.defineProperty(scroller, "scrollTop", { value: 400, writable: true });
    fireEvent.scroll(scroller);
    expect(screen.queryByLabelText("Frontmatter")).not.toBeInTheDocument();

    pressUndo(); // reverses the body edit first — a body step, so no peek
    expect(hasTag("h2")).toBe(false);
    expect(hasTag("h1")).toBe(true);
    expect(screen.queryByLabelText("Frontmatter")).not.toBeInTheDocument();
    expect(screen.getByText("title: New")).toBeInTheDocument(); // collapsed preview

    pressUndo(); // then the frontmatter edit — the region opens to reveal it
    expect(
      (screen.getByLabelText("Frontmatter") as HTMLTextAreaElement).value,
    ).toBe("title: Old");
  });

  // EXC-FR-WDAV / EDT-FR-24: "Load from filesystem" discards the buffer AND the
  // history above it, so undo cannot resurrect the discarded edits.
  it("cannot undo across a reload", async () => {
    const backend: Backend = { load: { body: "# One\n\nalpha", checksum: "ck1" } };
    wireBackend(backend);
    render(<Editor artifactId="a.md" artifactName="a.md" />);
    await screen.findByRole("heading", { name: "One" });

    bodyEdit("Heading 2");
    expect(hasTag("h2")).toBe(true);

    backend.load = { body: "# Two\n\nbeta", checksum: "ck2" };
    listeners[ARTIFACT_CHANGED_EXTERNALLY]?.({
      payload: { artifactId: "a.md", checksum: "ck2" },
    });
    fireEvent.click(
      await screen.findByRole("button", { name: "Load from filesystem" }),
    );
    await screen.findByRole("heading", { name: "Two" });

    for (let i = 0; i < 5; i++) pressUndo();

    expect(screen.getByRole("heading", { name: "Two" }).tagName).toBe("H1");
    expect(screen.queryByText("alpha")).not.toBeInTheDocument();
  });

  // EXC-FR-QDMC: undo/redo are inert while the modal blocks the tab,
  // and the history survives "Keep my version" intact.
  it("is inert while the external-change modal blocks, and survives Keep my version", async () => {
    wireLoad({ body: "# Title\n\nhello", checksum: "ck1" });
    render(<Editor artifactId="a.md" artifactName="a.md" />);
    await screen.findByRole("heading", { name: "Title" });

    bodyEdit("Heading 2");
    expect(hasTag("h2")).toBe(true);

    listeners[ARTIFACT_CHANGED_EXTERNALLY]?.({
      payload: { artifactId: "a.md", checksum: "ck2" },
    });
    await screen.findByRole("dialog");

    pressUndo();
    pressUndo();
    expect(hasTag("h2")).toBe(true); // blocked — the buffer is untouched

    fireEvent.click(screen.getByRole("button", { name: "Keep my version" }));
    pressUndo();
    expect(hasTag("h2")).toBe(false);
    expect(hasTag("h1")).toBe(true);
  });

  // EDT-FR-15: undo acts on the edit history, not on save points — a
  // save neither clears the history nor floors it.
  it("keeps pre-save edits reversible after a save", async () => {
    const backend: Backend = { load: { body: "# Title\n\nhello", checksum: "ck1" } };
    wireBackend(backend);
    render(
      <Editor artifactId="a.md" artifactName="a.md" sessions={sessions} />,
    );
    await screen.findByRole("heading", { name: "Title" });

    vi.useFakeTimers({ shouldAdvanceTime: true });
    bodyEdit("Heading 2");
    vi.advanceTimersByTime(HISTORY_COALESCE_MS + 50);
    bodyEdit("Bold");
    vi.useRealTimers();

    // Written through Save rather than by waiting the rest out: swapping the
    // clock for the coalescing window above discards whatever timer the store
    // had armed, so the rest would never elapse here.
    await saveNow();
    await waitFor(() => expect(backend.lastSavedBody).toBeTruthy());

    pressUndo();
    expect(hasTag("strong")).toBe(false);
    pressUndo();
    expect(hasTag("h1")).toBe(true);
  });

  // EDT-FR-15 / SNV-FR-14: the native Edit menu's Undo role reaches the webview
  // as a beforeinput, not a keystroke. The Editor claims that route too, so the
  // menu item and the accelerator drive the same history.
  it("serves the native Edit menu's Undo/Redo roles from the same history", async () => {
    wireLoad({ body: "# Title\n\nhello", checksum: "ck1" });
    render(<Editor artifactId="a.md" artifactName="a.md" />);
    await screen.findByRole("heading", { name: "Title" });

    bodyEdit("Heading 2");
    expect(hasTag("h2")).toBe(true);

    fireEvent(
      pmEl(),
      new InputEvent("beforeinput", {
        inputType: "historyUndo",
        bubbles: true,
        cancelable: true,
      }),
    );
    expect(hasTag("h1")).toBe(true);

    fireEvent(
      pmEl(),
      new InputEvent("beforeinput", {
        inputType: "historyRedo",
        bubbles: true,
        cancelable: true,
      }),
    );
    expect(hasTag("h2")).toBe(true);
  });

  // EDT-FR-22 (step granularity): a rapid run of edits on one surface folds into
  // a single step, so undo reverses the burst rather than one character of it.
  it("folds a rapid burst on one surface into a single step", async () => {
    wireLoad({ body: "# Title\n\nhello", checksum: "ck1" });
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    await screen.findByRole("heading", { name: "Title" });
    fireEvent.click(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    );
    const source = (await screen.findByLabelText(
      "Markdown source",
    )) as HTMLTextAreaElement;

    // Fake timers so the burst is provably inside the window, not racing it.
    vi.useFakeTimers({ shouldAdvanceTime: true });
    fireEvent.change(source, { target: { value: "# Title\n\nhello w" } });
    fireEvent.change(source, { target: { value: "# Title\n\nhello wo" } });
    fireEvent.change(source, { target: { value: "# Title\n\nhello wor" } });
    vi.useRealTimers();

    pressUndo();

    expect(
      (screen.getByLabelText("Markdown source") as HTMLTextAreaElement).value,
    ).toBe("# Title\n\nhello");
  });

  // EDT-FR-24 / EDT-FR-70: an emptied buffer is written like any other edit —
  // no confirmation is asked for and nothing blocks the write. What recovers a
  // file emptied by accident is undo, in the same session the write happened in.
  it("writes an emptied file without asking, and undo restores it", async () => {
    const backend: Backend = { load: { body: "# Title\n\nhello", checksum: "ck1" } };
    wireBackend(backend);
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    await screen.findByRole("heading", { name: "Title" });

    // Select the whole document and delete it (prosemirror-keymap: Backspace ->
    // deleteSelection), leaving an empty editor.
    fireEvent.keyDown(pmEl(), { key: "a", ctrlKey: true });
    fireEvent.keyDown(pmEl(), { key: "Backspace" });
    expect(pmEl().textContent).toBe("");

    await letWriteLand();

    expect(screen.queryByRole("dialog")).toBeNull();
    await waitFor(() => expect(backend.lastSavedBody).toBe(""));
    expect(screen.queryByText("● unsaved")).not.toBeInTheDocument();

    // And the restored content is written in turn.
    pressUndo();
    await letWriteLand();
    await waitFor(() => expect(backend.lastSavedBody).toContain("Title"));
  });

  // The negative case for the same rule: an artifact that loaded empty is on
  // exactly the same terms as one that was emptied.
  it("writes an artifact that loaded empty on the same terms", async () => {
    const backend: Backend = { load: { body: "", checksum: "ck1" } };
    wireBackend(backend);
    render(<Editor artifactId="a.md" artifactName="a.md" sessions={sessions} />);
    await waitFor(() => expect(pmEl()).toBeInTheDocument());

    await saveNow();

    expect(screen.queryByRole("dialog")).toBeNull();
    await waitFor(() => expect(backend.lastSavedBody).toBe(""));
  });

  // SNV-FR-14 / EDT-FR-15, EDT-FR-22: the Editor CLAIMS the accelerator — it prevents the
  // default so the focused control's own native history never runs. Without the
  // preventDefault the textarea/contenteditable would undo behind the tab's back.
  it("claims the undo accelerator instead of letting the surface handle it", async () => {
    wireLoad({ body: "# Title\n\nhello", checksum: "ck1" });
    render(<Editor artifactId="a.md" artifactName="a.md" />);
    await screen.findByRole("heading", { name: "Title" });

    const undoEvent = createEvent.keyDown(pmEl(), { key: "z", metaKey: true });
    fireEvent(pmEl(), undoEvent);
    expect(undoEvent.defaultPrevented).toBe(true);

    const redoEvent = createEvent.keyDown(pmEl(), {
      key: "z",
      metaKey: true,
      shiftKey: true,
    });
    fireEvent(pmEl(), redoEvent);
    expect(redoEvent.defaultPrevented).toBe(true);
  });

  // EXC-FR-QDMC: the default is prevented even while blocked, so a surface-local
  // stack cannot quietly diverge from the tab history behind the modal.
  it("claims — and ignores — the accelerator while the modal blocks", async () => {
    wireLoad({ body: "# Title\n\nhello", checksum: "ck1" });
    render(<Editor artifactId="a.md" artifactName="a.md" />);
    await screen.findByRole("heading", { name: "Title" });
    bodyEdit("Heading 2");

    listeners[ARTIFACT_CHANGED_EXTERNALLY]?.({
      payload: { artifactId: "a.md", checksum: "ck2" },
    });
    await screen.findByRole("dialog");

    const blockedUndo = createEvent.keyDown(pmEl(), { key: "z", metaKey: true });
    fireEvent(pmEl(), blockedUndo);
    expect(blockedUndo.defaultPrevented).toBe(true);
    expect(hasTag("h2")).toBe(true); // …and the buffer is untouched
  });

  // EDT-FR-15 / SNV-FR-14: only undo/redo are claimed. Copy, cut, paste and
  // select-all must fall through to the focused surface untouched.
  it("leaves the other edit accelerators to the focused surface", async () => {
    wireLoad({ body: "# Title\n\nhello", checksum: "ck1" });
    render(<Editor artifactId="a.md" artifactName="a.md" />);
    await screen.findByRole("heading", { name: "Title" });

    for (const key of ["c", "x", "v", "a"]) {
      const e = createEvent.keyDown(pmEl(), { key, metaKey: true });
      fireEvent(pmEl(), e);
      expect(e.defaultPrevented).toBe(false);
    }
  });

  // EDT-FR-15: the Windows/Linux bindings drive the same history.
  it("accepts Ctrl+Z and Ctrl+Y as undo and redo", async () => {
    wireLoad({ body: "# Title\n\nhello", checksum: "ck1" });
    render(<Editor artifactId="a.md" artifactName="a.md" />);
    await screen.findByRole("heading", { name: "Title" });

    bodyEdit("Heading 2");
    fireEvent.keyDown(pmEl(), { key: "z", ctrlKey: true });
    expect(hasTag("h1")).toBe(true);
    fireEvent.keyDown(pmEl(), { key: "y", ctrlKey: true });
    expect(hasTag("h2")).toBe(true);
  });

  // EDT-FR-22: the frontmatter region is a first-class surface of the same
  // history — an accelerator delivered there drives the tab stack, not the
  // textarea's own.
  it("serves undo from the frontmatter surface too", async () => {
    wireLoad({
      body: "---\ntitle: Old\n---\n\n# H\n\nbody",
      checksum: "ck1",
    });
    render(<Editor artifactId="a.md" artifactName="a.md" />);
    const fm = (await screen.findByLabelText(
      "Frontmatter",
    )) as HTMLTextAreaElement;
    fireEvent.change(fm, { target: { value: "title: New" } });

    const e = createEvent.keyDown(fm, { key: "z", metaKey: true });
    fireEvent(fm, e);

    expect(e.defaultPrevented).toBe(true);
    expect(
      (screen.getByLabelText("Frontmatter") as HTMLTextAreaElement).value,
    ).toBe("title: Old");
  });

  // EXC-FR-QDMC (the redo half of EXC-FR-QDMC).
  it("holds redo inert while blocked and restores it after Keep my version", async () => {
    wireLoad({ body: "# Title\n\nhello", checksum: "ck1" });
    render(<Editor artifactId="a.md" artifactName="a.md" />);
    await screen.findByRole("heading", { name: "Title" });

    bodyEdit("Heading 2");
    pressUndo();
    expect(hasTag("h1")).toBe(true);

    listeners[ARTIFACT_CHANGED_EXTERNALLY]?.({
      payload: { artifactId: "a.md", checksum: "ck2" },
    });
    await screen.findByRole("dialog");

    pressRedo();
    expect(hasTag("h1")).toBe(true); // blocked

    fireEvent.click(screen.getByRole("button", { name: "Keep my version" }));
    pressRedo();
    expect(hasTag("h2")).toBe(true);
  });

  // EDT-FR-25: redo switches modes too — the direction the undo test does not
  // exercise (text -> wysiwyg on the way forward).
  it("switches mode on redo, back into the mode the step was made in", async () => {
    wireLoad({ body: "# Title\n\nhello", checksum: "ck1" });
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    await screen.findByRole("heading", { name: "Title" });

    fireEvent.click(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    );
    const source = (await screen.findByLabelText(
      "Markdown source",
    )) as HTMLTextAreaElement;
    fireEvent.change(source, { target: { value: "# Title\n\ngoodbye" } });

    fireEvent.click(screen.getByRole("button", { name: "Edit as rich text" }));
    await screen.findByRole("heading", { name: "Title" });
    bodyEdit("Heading 2"); // a WYSIWYG step on top of a text step

    pressUndo(); // reverses the body step, stays in WYSIWYG
    pressUndo(); // reverses the source step — switches to text mode
    expect(screen.getByLabelText("Markdown source")).toBeInTheDocument();

    pressRedo(); // re-applies the source step, still text mode
    expect(
      (screen.getByLabelText("Markdown source") as HTMLTextAreaElement).value,
    ).toBe("# Title\n\ngoodbye");

    pressRedo(); // re-applies the body step — made in WYSIWYG, so mode follows
    expect(screen.queryByLabelText("Markdown source")).not.toBeInTheDocument();
    expect(hasTag("h2")).toBe(true);
  });

  // EDT-FR-18 + EDT-FR-22: a traversal restores the document's bytes, not a
  // re-derivation of them — in text mode the save after an undo is byte-exact.
  it("writes byte-exact Markdown after an undo in text mode", async () => {
    const original = "---\ntitle: Doc\n---\n\n# H\n\n- a\n- b\n";
    const backend: Backend = { load: { body: original, checksum: "ck1" } };
    wireBackend(backend);
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    await screen.findByRole("heading", { name: "H" });

    fireEvent.click(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    );
    const source = (await screen.findByLabelText(
      "Markdown source",
    )) as HTMLTextAreaElement;
    fireEvent.change(source, { target: { value: "wrecked" } });

    pressUndo();
    await letWriteLand();

    await waitFor(() => expect(backend.lastSavedBody).toBe(original));
  });

  // EDT-FR-18: undoing a frontmatter edit puts the block back byte-for-byte —
  // the restore re-seeds the round-trip baseline, so the save reuses the
  // verbatim original block rather than reconstructing a canonical one.
  it("preserves the frontmatter block byte-for-byte after undoing its edit", async () => {
    const original = "---\ntitle:    Spaced\n---\n\n# H\n\nbody";
    const backend: Backend = { load: { body: original, checksum: "ck1" } };
    wireBackend(backend);
    render(<Editor artifactId="a.md" artifactName="a.md" />);
    const fm = (await screen.findByLabelText(
      "Frontmatter",
    )) as HTMLTextAreaElement;

    fireEvent.change(fm, { target: { value: "title: Rewritten" } });
    pressUndo();
    await letWriteLand();

    await waitFor(() => expect(backend.lastSavedBody).toBeTruthy());
    // The odd spacing survives, so this is the verbatim block, not a rebuild.
    expect(backend.lastSavedBody).toContain("---\ntitle:    Spaced\n---\n");
  });

  // EDT-FR-70: the raw-text surface writes itself on exactly the same terms, and
  // a whitespace-only buffer is an ordinary edit rather than a special case.
  it("writes a whitespace-only buffer from raw-text mode", async () => {
    const backend: Backend = { load: { body: "# Title\n\nhello", checksum: "ck1" } };
    wireBackend(backend);
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    await screen.findByRole("heading", { name: "Title" });
    fireEvent.click(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    );
    const source = (await screen.findByLabelText(
      "Markdown source",
    )) as HTMLTextAreaElement;

    fireEvent.change(source, { target: { value: "   \n\n  " } });
    await letWriteLand();

    expect(screen.queryByRole("dialog")).toBeNull();
    await waitFor(() => expect(backend.lastSavedBody).toBe("   \n\n  "));
  });

  // EDT-FR-70: emptying the BODY writes the file with its frontmatter intact —
  // the write carries whatever the buffer holds, whichever surface emptied.
  it("writes the frontmatter back when the body alone is emptied", async () => {
    const backend: Backend = {
      load: { body: "---\ntitle: Doc\n---\n\n# H\n\nbody", checksum: "ck1" },
    };
    wireBackend(backend);
    render(<Editor artifactId="a.md" artifactName="a.md" />);
    await screen.findByLabelText("Frontmatter");

    fireEvent.keyDown(pmEl(), { key: "a", ctrlKey: true });
    fireEvent.keyDown(pmEl(), { key: "Backspace" });
    await letWriteLand();

    await waitFor(() =>
      expect(backend.lastSavedBody).toContain("---\ntitle: Doc\n---\n"),
    );
  });

  // EXC-FR-WCOM guard: a save fired before the load settles must not write the
  // still-empty buffer over the file.
  it("does not save before the artifact has loaded", async () => {
    const backend: Backend = { load: { body: "# Title", checksum: "ck1" } };
    let release: (() => void) | undefined;
    const gate = new Promise<void>((r) => {
      release = r;
    });
    invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
      if (cmd === "load_artifact_contents_by_id") {
        await gate;
        return backend.load;
      }
      backend.lastSavedBody = (args as { body: string }).body;
      return { checksum: "ck-saved" };
    });
    render(
      <Editor artifactId="a.md" artifactName="a.md" sessions={sessions} />,
    );

    // A write asked for while the load is still in flight — the case the store's
    // `pendingLoad` await guards (EDT-FR-29). The buffer is empty until the load
    // lands, so a write that did not wait would blank the file.
    const save = saveNow();
    await letWriteLand();
    expect(backend.lastSavedBody).toBeUndefined();

    release?.();
    await screen.findByRole("heading", { name: "Title" });
    await save;

    // It waited, and wrote what the load returned rather than the empty buffer
    // it would have found had it gone ahead.
    expect(backend.lastSavedBody).toBe("# Title");
  });

  /**
   * EDT-FR-71 / EDT-FR-32, EDT-FR-34: a write nobody triggered can fail, so the tab has to
   * say so — and keep the buffer and the history it could not write.
   */
  it("EDT-FR-71: attaches a failed scheduled write to the tab, buffer intact", async () => {
    let fail = true;
    const saved: string[] = [];
    invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
      if (cmd === "load_artifact_contents_by_id")
        return { body: "# Title\n\nhello", checksum: "ck1" };
      if (cmd === "save_artifact_contents") {
        if (fail) throw new Error("disk full");
        saved.push((args as { body: string }).body);
        return { checksum: "ck-saved" };
      }
      return undefined;
    });
    render(
      <Editor artifactId="a.md" artifactName="a.md" sessions={sessions} />,
    );
    await screen.findByRole("heading", { name: "Title" });

    bodyEdit("Heading 2");
    await letWriteLand();

    // The tab announces it, and everything the write carried is still here.
    expect(await screen.findByRole("alert")).toHaveTextContent("disk full");
    expect(screen.getByText("● unsaved")).toBeInTheDocument();
    expect(hasTag("h2")).toBe(true);
    expect(saved).toHaveLength(0);

    // EDT-FR-34: Save retries on demand, and the error clears on the success.
    fail = false;
    await saveNow();
    await waitFor(() => expect(saved).toHaveLength(1));
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.queryByText("● unsaved")).not.toBeInTheDocument();
  });

  // EDT-FR-66 / EDT-FR-70: an artifact the author only opened and read schedules
  // no write, so a file nobody edited is never rewritten behind their back.
  it("writes nothing at all for an artifact that was only read", async () => {
    const backend: Backend = { load: { body: "# Title\n\nhello", checksum: "ck1" } };
    wireBackend(backend);
    render(<Editor artifactId="a.md" artifactName="a.md" />);
    await screen.findByRole("heading", { name: "Title" });

    await letWriteLand();

    expect(backend.lastSavedBody).toBeUndefined();
    expect(screen.queryByText("● unsaved")).not.toBeInTheDocument();
  });
});
