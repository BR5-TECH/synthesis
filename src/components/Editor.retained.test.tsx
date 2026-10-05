import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
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

const hasTag = (tag: string) => !!pmEl().querySelector(tag);

// EDT-FR-28–EDT-FR-30: the artifact's edit state belongs to the session store,
// not to this component. A tab switch and a tab close both unmount the Editor
// (the Viewport keys it by artifact), so the test for "the state survived" is
// literally: unmount, mount again against the same store, and look.
describe("Editor (retained edit state)", () => {
  const loadCalls = () =>
    invokeMock.mock.calls.filter((c) => c[0] === "load_artifact_contents_by_id")
      .length;

  const mount = (sessions: EditSessionStore) =>
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        sessions={sessions}
      />,
    );

  /**
   * EDT-FR-30 / EDT-FR-70: activating another tab unmounts the Editor (the
   * Viewport keys the surface by artifact), so this is the case the rest lives
   * in the *store* for. A timer the component owned would be torn down here and
   * the edit the author just made would never reach disk.
   */
  it("EDT-FR-30, EDT-FR-70: writes while the tab sits in the background, reloading nothing", async () => {
    const backend: Backend = { load: { body: "# Title\n\nhello", checksum: "ck1" } };
    wireBackend(backend);
    const sessions = new EditSessionStore();
    const { unmount } = mount(sessions);
    await screen.findByRole("heading", { name: "Title" });

    bodyEdit("Heading 2");
    expect(screen.getByText("● unsaved")).toBeInTheDocument();

    unmount(); // the author activates a different tab
    await letWriteLand();

    // The write landed with nothing on screen to have triggered it.
    expect(backend.lastSavedBody).toBeTruthy();
    expect(sessions.get("a.md")?.dirty).toBe(false);

    // …and coming back reloads nothing and reverses nothing (EDT-FR-30).
    mount(sessions);
    await screen.findByRole("heading", { name: "Title" });
    expect(loadCalls()).toBe(1);
    expect(screen.queryByText("● unsaved")).not.toBeInTheDocument();
    pressUndo();
    expect(hasTag("h1")).toBe(true);
  });

  /**
   * EXC-FR-NMXQ: the write held behind the external-change modal is
   * performed the moment the author keeps their version — through the real
   * modal, so the resolution is actually wired to the held write rather than
   * merely reachable from the store.
   */
  it("EXC-FR-NMXQ: performs the held write when the author keeps their version", async () => {
    const backend: Backend = { load: { body: "# Title\n\nhello", checksum: "ck1" } };
    wireBackend(backend);
    const sessions = new EditSessionStore();
    mount(sessions);
    await screen.findByRole("heading", { name: "Title" });

    bodyEdit("Heading 2");
    listeners[ARTIFACT_CHANGED_EXTERNALLY]?.({
      payload: { artifactId: "a.md", checksum: "ck2" },
    });
    await screen.findByRole("dialog");

    // The rest elapses with the modal unresolved: nothing is written, and the
    // write is held rather than dropped.
    await letWriteLand();
    expect(backend.lastSavedBody).toBeUndefined();
    expect(sessions.get("a.md")?.dirty).toBe(true);

    fireEvent.click(screen.getByRole("button", { name: "Keep my version" }));

    await waitFor(() => expect(backend.lastSavedBody).toBeTruthy());
    expect(sessions.get("a.md")?.baseline).toBe("ck-saved");
    expect(sessions.get("a.md")?.dirty).toBe(false);
  });

  /**
   * EXC-FR-WDAV (the other resolution) / EXC-FR-WDAV: "Load from filesystem"
   * discards the held write along with the buffer it would have carried.
   */
  it("EXC-FR-WDAV: drops the held write when the author loads from the filesystem", async () => {
    const backend: Backend = { load: { body: "# Title\n\nhello", checksum: "ck1" } };
    wireBackend(backend);
    const sessions = new EditSessionStore();
    mount(sessions);
    await screen.findByRole("heading", { name: "Title" });

    bodyEdit("Heading 2");
    backend.load = { body: "# Theirs", checksum: "ck2" };
    listeners[ARTIFACT_CHANGED_EXTERNALLY]?.({
      payload: { artifactId: "a.md", checksum: "ck2" },
    });
    await screen.findByRole("dialog");
    await letWriteLand();
    expect(backend.lastSavedBody).toBeUndefined();

    fireEvent.click(screen.getByRole("button", { name: "Load from filesystem" }));
    await screen.findByRole("heading", { name: "Theirs" });

    // Nothing is written, then or later: the buffer that write carried is gone.
    await letWriteLand();
    expect(backend.lastSavedBody).toBeUndefined();
    expect(sessions.get("a.md")?.dirty).toBe(false);
  });

  // EDT-FR-30 / TAB-FR-09 / EDT-FR-30: activating another tab and coming back
  // re-mounts the Editor; nothing is reloaded and nothing is lost.
  it("resumes an unsaved edit after a tab switch without reloading", async () => {
    wireLoad({ body: "# Title\n\nhello", checksum: "ck1" });
    const sessions = new EditSessionStore();
    const { unmount } = mount(sessions);
    await screen.findByRole("heading", { name: "Title" });

    bodyEdit("Heading 2");
    expect(hasTag("h2")).toBe(true);
    expect(screen.getByText("● unsaved")).toBeInTheDocument();
    expect(loadCalls()).toBe(1);

    unmount(); // switch to another tab
    mount(sessions); // …and back
    await screen.findByRole("heading", { name: "Title" });

    expect(hasTag("h2")).toBe(true);
    expect(screen.getByText("● unsaved")).toBeInTheDocument();
    // EDT-FR-30: no reload, and the edit is still the next thing undo reverses.
    expect(loadCalls()).toBe(1);
    pressUndo();
    expect(hasTag("h1")).toBe(true);
  });

  // EDT-FR-24, EDT-FR-29, EDT-FR-31 / TAB-FR-10: closing the tab writes the changes; reopening the
  // artifact resumes them with the history still traversable down to the floor.
  it("resumes the history after the tab is closed and the artifact reopened", async () => {
    const backend: Backend = { load: { body: "# Title\n\nhello", checksum: "ck1" } };
    wireBackend(backend);
    const sessions = new EditSessionStore();
    const { unmount } = mount(sessions);
    await screen.findByRole("heading", { name: "Title" });

    bodyEdit("Heading 2");
    // What the shell does on close: flush, then release the tab (EDT-FR-31).
    await act(async () => {
      await sessions.flush("a.md");
    });
    sessions.closeTab("a.md");
    unmount();
    expect(backend.lastSavedBody).toContain("## Title");

    // Reopening: the on-disk content now matches what was flushed.
    backend.load = { body: "## Title\n\nhello", checksum: "ck-saved" };
    mount(sessions);
    await screen.findByRole("heading", { name: "Title" });

    expect(hasTag("h2")).toBe(true);
    // EDT-FR-24/EDT-FR-29: the floor did not move — the pre-close edit is still
    // reversible, back to the artifact as it first opened.
    pressUndo();
    expect(hasTag("h1")).toBe(true);
    pressUndo(); // no-op at the floor
    expect(hasTag("h1")).toBe(true);
  });

  // EDT-FR-29 / EDT-FR-17: the mode is part of the retained state, so a reopened
  // tab does not snap back to the WYSIWYG default.
  it("reopens in the editing mode the artifact was left in", async () => {
    wireLoad({ body: "# Title\n\nhello", checksum: "ck1" });
    const sessions = new EditSessionStore();
    const { unmount } = mount(sessions);
    await screen.findByRole("heading", { name: "Title" });

    fireEvent.click(screen.getByRole("button", { name: "Edit as Markdown source" }));
    const source = screen.getByLabelText("Markdown source") as HTMLTextAreaElement;
    fireEvent.change(source, { target: { value: "# Title\n\nedited in source" } });
    await act(async () => {
      await sessions.flush("a.md");
    });
    sessions.closeTab("a.md");
    unmount();

    wireLoad({ body: "# Title\n\nedited in source", checksum: "ck-saved" });
    mount(sessions);

    const reopened = (await screen.findByLabelText(
      "Markdown source",
    )) as HTMLTextAreaElement;
    expect(reopened.value).toBe("# Title\n\nedited in source");
  });

  // EDT-FR-24 / EDT-FR-28: an artifact that was only read carries nothing
  // forward — reopening it is a plain first load with an empty history.
  it("reloads from scratch when the artifact was never edited", async () => {
    const backend: Backend = { load: { body: "# Title\n\nhello", checksum: "ck1" } };
    wireBackend(backend);
    const sessions = new EditSessionStore();
    const { unmount } = mount(sessions);
    await screen.findByRole("heading", { name: "Title" });

    sessions.closeTab("a.md");
    unmount();
    expect(sessions.get("a.md")).toBeUndefined();

    backend.load = { body: "# Changed on disk", checksum: "ck2" };
    mount(sessions);
    await screen.findByRole("heading", { name: "Changed on disk" });

    expect(loadCalls()).toBe(2);
    pressUndo(); // nothing to undo — and certainly not back to the old content
    expect(pmEl().textContent).toContain("Changed on disk");
  });

  // EDT-FR-24 / EDT-FR-29: a file that moved while no tab was showing it is a
  // divergence like any other — neither version is adopted silently.
  it("raises the external-change modal when the file moved while the tab was closed", async () => {
    const backend: Backend = { load: { body: "# Title\n\nhello", checksum: "ck1" } };
    wireBackend(backend);
    const sessions = new EditSessionStore();
    const { unmount } = mount(sessions);
    await screen.findByRole("heading", { name: "Title" });

    bodyEdit("Heading 2");
    await act(async () => {
      await sessions.flush("a.md");
    });
    sessions.closeTab("a.md");
    unmount();

    // A third party rewrote the file in the meantime.
    backend.load = { body: "# Someone else\n\nwrote this", checksum: "ck-other" };
    mount(sessions);

    expect(await screen.findByRole("dialog")).toBeInTheDocument();
    // The retained version is what is on screen until the user chooses.
    expect(hasTag("h2")).toBe(true);

    fireEvent.click(screen.getByRole("button", { name: "Keep my version" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(hasTag("h2")).toBe(true);
  });

  // EDT-FR-29 (second half) / EDT-FR-24: resolving that divergence the other way
  // moves the floor — undo cannot reach back past the reload.
  it("drops the retained history when the reopen divergence is resolved by loading", async () => {
    const backend: Backend = { load: { body: "# Title\n\nhello", checksum: "ck1" } };
    wireBackend(backend);
    const sessions = new EditSessionStore();
    const { unmount } = mount(sessions);
    await screen.findByRole("heading", { name: "Title" });

    bodyEdit("Heading 2");
    await act(async () => {
      await sessions.flush("a.md");
    });
    sessions.closeTab("a.md");
    unmount();

    backend.load = { body: "# Someone else\n\nwrote this", checksum: "ck-other" };
    mount(sessions);
    await screen.findByRole("dialog");

    fireEvent.click(screen.getByRole("button", { name: "Load from filesystem" }));
    await screen.findByRole("heading", { name: "Someone else" });

    for (let i = 0; i < 5; i++) pressUndo();
    expect(pmEl().textContent).toContain("Someone else");
    expect(pmEl().textContent).not.toContain("hello");
  });

  // EDT-FR-70 / EDT-FR-24: a close-time flush writes an emptied buffer like any
  // other, with nothing asked of the author and the history left intact — undo
  // is what brings the content back, and it is still there to do it.
  it("writes an emptied buffer on a close-time flush, history intact", async () => {
    const backend: Backend = { load: { body: "# Title\n\nhello", checksum: "ck1" } };
    wireBackend(backend);
    const sessions = new EditSessionStore();
    mount(sessions);
    await screen.findByRole("heading", { name: "Title" });

    // Empty the document, then have the shell flush it on close.
    fireEvent.click(screen.getByRole("button", { name: "Edit as Markdown source" }));
    fireEvent.change(screen.getByLabelText("Markdown source"), {
      target: { value: "" },
    });
    let res!: Awaited<ReturnType<EditSessionStore["flush"]>>;
    await act(async () => {
      res = await sessions.flush("a.md");
    });

    expect(res.ok).toBe(true);
    expect(backend.lastSavedBody).toBe("");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.queryByText("● unsaved")).not.toBeInTheDocument();
    // EDT-FR-24, EDT-FR-70: the emptying edit is still the next thing undo reverses.
    pressUndo();
    expect(
      (screen.getByLabelText("Markdown source") as HTMLTextAreaElement).value,
    ).toContain("hello");
  });

  // EDT-FR-30 as written: raw-text mode, two unsaved edits, a tab switch, and
  // one undo reverses the second edit.
  it("resumes raw-text mode and a two-edit history across a tab switch", async () => {
    wireLoad({ body: "# Title\n\nhello", checksum: "ck1" });
    const sessions = new EditSessionStore();
    const { unmount } = mount(sessions);
    await screen.findByRole("heading", { name: "Title" });

    fireEvent.click(screen.getByRole("button", { name: "Edit as Markdown source" }));
    vi.useFakeTimers({ shouldAdvanceTime: true });
    fireEvent.change(screen.getByLabelText("Markdown source"), {
      target: { value: "# Title\n\nfirst" },
    });
    vi.advanceTimersByTime(HISTORY_COALESCE_MS + 50);
    fireEvent.change(screen.getByLabelText("Markdown source"), {
      target: { value: "# Title\n\nsecond" },
    });
    vi.useRealTimers();

    unmount();
    mount(sessions);

    const source = (await screen.findByLabelText(
      "Markdown source",
    )) as HTMLTextAreaElement;
    expect(source.value).toBe("# Title\n\nsecond");
    expect(screen.getByText("● unsaved")).toBeInTheDocument();
    expect(loadCalls()).toBe(1);

    pressUndo();
    expect(
      (screen.getByLabelText("Markdown source") as HTMLTextAreaElement).value,
    ).toBe("# Title\n\nfirst");
  });

  // Every route writes the artifact's own buffer, so the rest of EDT-FR-70
  // elapsing and a Save brought forward must put identical bytes on disk. Tested
  // after an undo, where the buffer is a historical snapshot rather than a fresh
  // serialization — the case where the two could most easily diverge.
  it("writes the same bytes whichever route performs the write, after an undo", async () => {
    const body = "# Title\n\n*emphasis* and a list:\n\n- one\n- two";
    const backend: Backend = { load: { body, checksum: "ck1" } };
    wireBackend(backend);
    const sessions = new EditSessionStore();
    mount(sessions);
    await screen.findByRole("heading", { name: "Title" });

    bodyEdit("Heading 2");
    pressUndo();

    // The artifact writes itself.
    await letWriteLand();
    await waitFor(() => expect(backend.lastSavedBody).toBeTruthy());
    const viaSchedule = backend.lastSavedBody;

    // And a Save brought forward writes the same thing.
    backend.lastSavedBody = undefined;
    await act(async () => {
      await sessions.flush("a.md", { force: true });
    });

    expect(backend.lastSavedBody).toBe(viaSchedule);
  });
});
