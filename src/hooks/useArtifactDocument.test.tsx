import { useEffect, useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";

import { useArtifactDocument } from "./useArtifactDocument";
import { useExternalChanges } from "./useExternalChanges";
import { EditSessionStore } from "../state/editSessions";
import type {
  ArtifactChangedPayload,
  ArtifactContents,
  SaveResult,
} from "../types";
import { ARTIFACT_CHANGED_EXTERNALLY } from "../events";

// Mirror the repo's invoke/listen mocking convention (see Library.test.tsx).
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

interface Backend {
  load: ArtifactContents;
  saveChecksum: string;
  lastSavedBody?: string;
}

function wireBackend(b: Backend) {
  invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
    if (cmd === "load_artifact_contents_by_id") return b.load;
    if (cmd === "save_artifact_contents") {
      b.lastSavedBody = (args as { body: string }).body;
      return { checksum: b.saveChecksum } satisfies SaveResult;
    }
    throw new Error(`unexpected invoke ${cmd}`);
  });
}

function fireExternalChange(artifactId: string, checksum: string) {
  listeners[ARTIFACT_CHANGED_EXTERNALLY]?.({
    payload: { artifactId, checksum },
  });
}

/**
 * A minimal editing surface over the hook — it stands in for the WYSIWYG editor
 * so the surface-agnostic lifecycle (`EXC-editor-external-change.md`) can be driven and
 * asserted without Tiptap's jsdom quirks. It does exactly what `LiveEditor`
 * does: seed its draft from the session buffer on each `seedToken`, write edits
 * back to that buffer and mark dirty, save through the store's guarded write
 * path, and surface the conflict modal.
 */
function Harness({ id, sessions }: { id: string; sessions: EditSessionStore }) {
  // The divergence watch is shell-level (it must see artifacts whose tab is not
  // active), so the harness mounts it alongside the document hook exactly as the
  // real app does.
  useExternalChanges(sessions);
  const doc = useArtifactDocument(id, sessions);
  const [draft, setDraft] = useState("");
  useEffect(() => {
    setDraft(doc.session.buffer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [doc.session, doc.session.seedToken]);
  return (
    <div>
      <textarea
        aria-label="body"
        value={draft}
        disabled={doc.conflict}
        onChange={(e) => {
          setDraft(e.target.value);
          doc.session.buffer = e.target.value;
          doc.markDirty();
        }}
      />
      {doc.dirty && <span>DIRTY</span>}
      {doc.error && <span>{doc.error}</span>}
      {/* EDT-FR-34: Save reaches the store directly, because it also writes
          artifacts no Editor is mounted on. The harness does the same. */}
      <button onClick={() => void sessions.flush(id)}>save</button>
      {doc.conflict && (
        <div role="dialog">
          <button onClick={() => void doc.loadFromFilesystem()}>load</button>
          <button onClick={doc.keepMine}>keep</button>
        </div>
      )}
    </div>
  );
}

function renderHarness(id = "a.md", sessions = new EditSessionStore()) {
  render(<Harness id={id} sessions={sessions} />);
  return sessions;
}

beforeEach(() => {
  invokeMock.mockReset();
  unlistenMock.mockReset();
  listeners = {};
});

afterEach(() => {
  cleanup();
});

describe("useArtifactDocument lifecycle", () => {
  // EXC-FR-WCOM: load body + baseline.
  it("loads the body from the backend", async () => {
    wireBackend({ load: { body: "v1", checksum: "ck1" }, saveChecksum: "ck-x" });
    renderHarness();
    const ta = (await screen.findByLabelText("body")) as HTMLTextAreaElement;
    await waitFor(() => expect(ta.value).toBe("v1"));
    expect(invokeMock).toHaveBeenCalledWith("load_artifact_contents_by_id", {
      id: "a.md",
    });
  });

  // EXC-FR-QCNO, EXC-FR-TYKX: external change with C' != C raises the modal.
  it("raises the modal on an external change", async () => {
    wireBackend({ load: { body: "v1", checksum: "ck1" }, saveChecksum: "ck-x" });
    renderHarness();
    await screen.findByLabelText("body");
    fireExternalChange("a.md", "ck2");
    expect(await screen.findByRole("dialog")).toBeInTheDocument();
  });

  // EXC-FR-QCNO: an event for a different artifact is ignored.
  it("ignores external changes for other artifacts", async () => {
    wireBackend({ load: { body: "v1", checksum: "ck1" }, saveChecksum: "ck-x" });
    renderHarness("a.md");
    await screen.findByLabelText("body");
    fireExternalChange("other.md", "ck2");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  // EXC-FR-ALXA: an event equal to the LOAD baseline is a no-op echo.
  it("suppresses the modal for an event equal to the load baseline", async () => {
    wireBackend({ load: { body: "v1", checksum: "ck1" }, saveChecksum: "ck-x" });
    renderHarness();
    await screen.findByLabelText("body");
    fireExternalChange("a.md", "ck1");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  // EXC-FR-WDAV: Load from filesystem replaces the buffer, clears dirty + modal.
  it("loads from filesystem, replacing the buffer and clearing dirty", async () => {
    const backend: Backend = {
      load: { body: "v1", checksum: "ck1" },
      saveChecksum: "ck-x",
    };
    wireBackend(backend);
    renderHarness();
    const ta = (await screen.findByLabelText("body")) as HTMLTextAreaElement;
    await waitFor(() => expect(ta.value).toBe("v1"));

    // Local edit -> dirty; then an external change arrives.
    fireEvent.change(ta, { target: { value: "local-edit" } });
    expect(screen.getByText("DIRTY")).toBeInTheDocument();
    backend.load = { body: "v2-from-disk", checksum: "ck2" };
    fireExternalChange("a.md", "ck2");
    await screen.findByRole("dialog");

    fireEvent.click(screen.getByRole("button", { name: "load" }));
    await waitFor(() => expect(ta.value).toBe("v2-from-disk"));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.queryByText("DIRTY")).not.toBeInTheDocument();
  });

  // EXC-FR-WDEJ, EXC-FR-NMXQ: Keep my version retains buffer + dirty; next save overwrites disk.
  it("keeps the in-memory version and overwrites on the next save", async () => {
    const backend: Backend = {
      load: { body: "v1", checksum: "ck1" },
      saveChecksum: "ck3",
    };
    wireBackend(backend);
    renderHarness();
    const ta = (await screen.findByLabelText("body")) as HTMLTextAreaElement;
    await waitFor(() => expect(ta.value).toBe("v1"));

    fireEvent.change(ta, { target: { value: "my-edit" } });
    fireExternalChange("a.md", "ck2");
    await screen.findByRole("dialog");
    fireEvent.click(screen.getByRole("button", { name: "keep" }));

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(ta.value).toBe("my-edit");
    expect(screen.getByText("DIRTY")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "save" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("save_artifact_contents", {
        id: "a.md",
        body: "my-edit",
      }),
    );
    expect(backend.lastSavedBody).toBe("my-edit");
  });

  // EXC-FR-ALXA: the editor's own save does not raise the modal (self-write).
  it("suppresses the modal for its own save", async () => {
    wireBackend({ load: { body: "v1", checksum: "ck1" }, saveChecksum: "ck3" });
    renderHarness();
    const ta = await screen.findByLabelText("body");

    fireEvent.change(ta, { target: { value: "edited" } });
    fireEvent.click(screen.getByRole("button", { name: "save" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "save_artifact_contents",
        expect.anything(),
      ),
    );

    // The watcher echoes our own write with the saved checksum -> no modal.
    fireExternalChange("a.md", "ck3");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  // EXC-FR-UWYK: after keep-mine, a FRESH change re-prompts but the
  // SAME change stays quiet (the acknowledged checksum is the new baseline).
  it("re-prompts on a fresh change but not the same one after keep-mine", async () => {
    wireBackend({ load: { body: "v1", checksum: "ck1" }, saveChecksum: "ck-x" });
    renderHarness();
    await screen.findByLabelText("body");

    fireExternalChange("a.md", "ck2");
    await screen.findByRole("dialog");
    fireEvent.click(screen.getByRole("button", { name: "keep" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    // Same change again -> quiet.
    fireExternalChange("a.md", "ck2");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    // A distinct change -> re-prompt.
    fireExternalChange("a.md", "ck4");
    expect(await screen.findByRole("dialog")).toBeInTheDocument();
  });

  // EXC-FR-WDEJ: a failed save surfaces the error and does NOT clear dirty, so the
  // user's unsaved work is not silently lost.
  it("surfaces a save error and keeps the document dirty", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_artifact_contents_by_id")
        return { body: "v1", checksum: "ck1" };
      if (cmd === "save_artifact_contents") throw new Error("disk full");
      throw new Error(`unexpected invoke ${cmd}`);
    });
    renderHarness();
    const ta = (await screen.findByLabelText("body")) as HTMLTextAreaElement;
    await waitFor(() => expect(ta.value).toBe("v1"));

    fireEvent.change(ta, { target: { value: "edited" } });
    expect(screen.getByText("DIRTY")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "save" }));

    await screen.findByText(/disk full/);
    // Still dirty — the failed save must not clear unsaved state.
    expect(screen.getByText("DIRTY")).toBeInTheDocument();
  });

  // EDT-FR-29: reopening an artifact with retained state resumes that state and
  // only *checks* the on-disk checksum — it does not re-adopt the file.
  it("resumes a retained buffer on reopen without adopting the loaded body", async () => {
    wireBackend({ load: { body: "v1", checksum: "ck1" }, saveChecksum: "ck-x" });
    const sessions = new EditSessionStore();
    sessions.adoptLoad("a.md", "v1", "ck1");
    const s = sessions.ensure("a.md");
    s.buffer = "my retained edit";
    sessions.update("a.md", { dirty: true, revalidate: true });

    renderHarness("a.md", sessions);
    const ta = (await screen.findByLabelText("body")) as HTMLTextAreaElement;

    await waitFor(() => expect(ta.value).toBe("my retained edit"));
    // Checksums agree, so no modal — and the retained buffer was not replaced by
    // the loaded body.
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.getByText("DIRTY")).toBeInTheDocument();
  });

  // EDT-FR-29: if that revalidating load fails, the retained state must survive
  // — the error is surfaced, nothing is adopted, and nothing is discarded.
  it("keeps the retained buffer when the revalidating load fails", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_artifact_contents_by_id") throw new Error("gone");
      throw new Error(`unexpected invoke ${cmd}`);
    });
    const sessions = new EditSessionStore();
    sessions.adoptLoad("a.md", "v1", "ck1");
    sessions.ensure("a.md").buffer = "my retained edit";
    sessions.update("a.md", { dirty: true, revalidate: true });

    renderHarness("a.md", sessions);

    expect(await screen.findByText(/gone/)).toBeInTheDocument();
    const ta = screen.getByLabelText("body") as HTMLTextAreaElement;
    expect(ta.value).toBe("my retained edit");
    expect(screen.getByText("DIRTY")).toBeInTheDocument();
    expect(sessions.get("a.md")?.history.steps).toHaveLength(1);
  });

  // The load error path surfaces without crashing.
  it("surfaces a load error", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_artifact_contents_by_id")
        throw new Error("not a synthesis project");
      throw new Error(`unexpected invoke ${cmd}`);
    });
    renderHarness();
    expect(await screen.findByText(/not a synthesis project/)).toBeInTheDocument();
  });
});

// EXC-FR-LKHZ / EXC-FR-VNLZ: only the ACTIVE tab's Editor is mounted (EDT-FR-30
// keeps the others alive without rendering them), so divergence detection lives
// with the session store. A change to an artifact the user is not looking at
// must still be caught — nothing else would notice it, since re-activating that
// tab performs no reload.
describe("useExternalChanges", () => {
  function Watcher({ sessions }: { sessions: EditSessionStore }) {
    useExternalChanges(sessions);
    return null;
  }

  it("raises the divergence on an artifact whose tab is not the active one", async () => {
    const sessions = new EditSessionStore();
    sessions.adoptLoad("background.md", "v1", "ck1");
    sessions.ensure("background.md").buffer = "edited but not on screen";
    sessions.update("background.md", { dirty: true });
    render(<Watcher sessions={sessions} />);
    await waitFor(() =>
      expect(listeners[ARTIFACT_CHANGED_EXTERNALLY]).toBeTypeOf("function"),
    );

    fireExternalChange("background.md", "ck-other");

    expect(sessions.get("background.md")?.conflict).toBe(true);
    expect(sessions.get("background.md")?.pending).toBe("ck-other");
    // …and that unresolved divergence now refuses the write a close would make.
    expect(await sessions.flush("background.md")).toEqual({
      ok: false,
      blocked: "conflict",
      artifactId: "background.md",
    });
  });

  it("ignores an event for an artifact it holds no state for", async () => {
    const sessions = new EditSessionStore();
    render(<Watcher sessions={sessions} />);
    await waitFor(() =>
      expect(listeners[ARTIFACT_CHANGED_EXTERNALLY]).toBeTypeOf("function"),
    );

    fireExternalChange("never-opened.md", "ck2");

    expect(sessions.get("never-opened.md")).toBeUndefined();
  });

  it("ignores the echo of our own write", async () => {
    const sessions = new EditSessionStore();
    sessions.adoptLoad("a.md", "v1", "ck1");
    render(<Watcher sessions={sessions} />);
    await waitFor(() =>
      expect(listeners[ARTIFACT_CHANGED_EXTERNALLY]).toBeTypeOf("function"),
    );

    fireExternalChange("a.md", "ck1");

    expect(sessions.get("a.md")?.conflict).toBe(false);
  });
});

// The session outlives the component (EDT-FR-28), so a load in flight when the
// Editor unmounts must still commit its answer — a write parked on `pendingLoad`
// is waiting for exactly that answer, and dropping it would let the write
// overwrite the divergence the load just found.
describe("a load racing an unmount", () => {
  it("still records a divergence found after the Editor unmounted", async () => {
    let release!: (v: ArtifactContents) => void;
    invokeMock.mockImplementation(
      async (cmd: string) =>
        cmd === "load_artifact_contents_by_id"
          ? new Promise<ArtifactContents>((resolve) => {
              release = resolve;
            })
          : { checksum: "ck-saved" },
    );
    const sessions = new EditSessionStore();
    sessions.adoptLoad("a.md", "v1", "ck1");
    sessions.ensure("a.md").buffer = "my retained edit";
    sessions.update("a.md", { dirty: true, revalidate: true });

    const { unmount } = render(<Harness id="a.md" sessions={sessions} />);
    // A write starts while the revalidating load is still outstanding…
    const writing = sessions.flush("a.md");
    // …and the tab closes before the answer lands.
    unmount();
    release({ body: "someone else wrote this", checksum: "ck-other" });

    expect(await writing).toEqual({
      ok: false,
      blocked: "conflict",
      artifactId: "a.md",
    });
    // Nothing was written over the third party's version.
    expect(
      invokeMock.mock.calls.some((c) => c[0] === "save_artifact_contents"),
    ).toBe(false);
    expect(sessions.get("a.md")?.conflict).toBe(true);
  });

  // EDT-FR-29: the revalidation is authoritative. A divergence the watch raised
  // while the tab was closed is cleared when the file turns out to match again.
  it("clears a stale divergence when the reopen finds the file unchanged", async () => {
    wireBackend({ load: { body: "v1", checksum: "ck1" }, saveChecksum: "ck-x" });
    const sessions = new EditSessionStore();
    sessions.adoptLoad("a.md", "v1", "ck1");
    sessions.ensure("a.md").buffer = "my retained edit";
    sessions.update("a.md", { dirty: true, revalidate: true });
    // Touched while closed, then returned to the bytes we already hold.
    sessions.update("a.md", { conflict: true, pending: "ck-transient" });

    renderHarness("a.md", sessions);

    await waitFor(() => expect(sessions.get("a.md")?.revalidate).toBe(false));
    expect(sessions.get("a.md")?.conflict).toBe(false);
    expect(sessions.get("a.md")?.pending).toBeNull();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// A document rewritten under a mounted Editor (DCR-FR-13, NAW-FR-09)
// ---------------------------------------------------------------------------

// An accepted draft-change proposal replaces a file's bytes while the tab
// showing that file stays mounted. `reload` is what the acceptance calls, and
// it has to reach the surface without the Editor doing anything: nothing
// watches a draft file, and the mounted Editor has no reason to load again.
describe("a document rewritten while its Editor is mounted", () => {
  it("shows the new text without the tab being reopened (DCR-FR-13)", async () => {
    const backend: Backend = {
      load: { body: "v1", checksum: "ck1" },
      saveChecksum: "ck-x",
    };
    wireBackend(backend);
    const sessions = renderHarness();
    const ta = (await screen.findByLabelText("body")) as HTMLTextAreaElement;
    await waitFor(() => expect(ta.value).toBe("v1"));

    backend.load = { body: "v2 — the accepted text", checksum: "ck2" };
    await act(async () => {
      await sessions.reload("a.md");
    });

    await waitFor(() => expect(ta.value).toBe("v2 — the accepted text"));
    // The baseline moved with it, or the next write reports a divergence
    // against bytes nobody edited.
    expect(sessions.get("a.md")?.baseline).toBe("ck2");
    // And the undo history was refloored: an undo must not be able to reinstate
    // the superseded text as an edit to be written back. `dirty` would prove
    // nothing here — this document was never edited.
    expect(sessions.get("a.md")?.history.steps).toHaveLength(1);
    expect(sessions.get("a.md")?.history.steps[0]?.doc).toBe(
      "v2 — the accepted text",
    );
  });

  it("leaves a discarded document discarded (NAW-FR-09)", async () => {
    // `forget` is the other half of the pair and means the file is GONE — a
    // deleted draft file. Reading it again would ask the backend for a path the
    // author just removed, so the reload must not be folded into `forget`.
    wireBackend({ load: { body: "v1", checksum: "ck1" }, saveChecksum: "ck-x" });
    const sessions = renderHarness();
    await waitFor(() =>
      expect((screen.getByLabelText("body") as HTMLTextAreaElement).value).toBe(
        "v1",
      ),
    );
    const loads = () =>
      invokeMock.mock.calls.filter((c) => c[0] === "load_artifact_contents_by_id")
        .length;
    expect(loads()).toBe(1);

    await act(async () => {
      sessions.forget("a.md");
    });

    expect(loads()).toBe(1);
  });
});
