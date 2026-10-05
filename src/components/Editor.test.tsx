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
import { EditSessionStore } from "../state/editSessions";
import type { ArtifactChangedPayload, ArtifactContents } from "../types";
import { ARTIFACT_CHANGED_EXTERNALLY } from "../events";

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

describe("Editor (WYSIWYG integration)", () => {
  // EDT-FR-02: Markdown loads and renders as rich WYSIWYG content — a Markdown
  // heading becomes an actual <h1>, not literal "# ..." text.
  it("renders Markdown as WYSIWYG rich content", async () => {
    wireLoad({ body: "# Heading one\n\nA paragraph.", checksum: "ck1" });
    render(<Editor artifactId="a.md" artifactName="a.md" />);

    const heading = await screen.findByRole("heading", { name: "Heading one" });
    expect(heading.tagName).toBe("H1");
    expect(screen.getByText("A paragraph.")).toBeInTheDocument();
  });

  // EXC-FR-QCNO/EXC-FR-VTUH: an external change to the open artifact raises the
  // blocking modal with the two resolutions, through the real component.
  it("raises the blocking modal on an external change", async () => {
    wireLoad({ body: "# Title", checksum: "ck1" });
    render(<Editor artifactId="a.md" artifactName="a.md" />);
    await screen.findByRole("heading", { name: "Title" });

    listeners[ARTIFACT_CHANGED_EXTERNALLY]?.({
      payload: { artifactId: "a.md", checksum: "ck2" },
    });

    expect(await screen.findByRole("dialog")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Load from filesystem" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Keep my version" }),
    ).toBeInTheDocument();
  });

  // EDT-FR-04: a freshly loaded artifact is NOT dirty. Guards the bridge between
  // Tiptap and the hook — setContent/setEditable on mount must not fire spurious
  // update events that would mark the untouched doc dirty.
  it("does not mark a freshly loaded artifact dirty", async () => {
    wireLoad({ body: "# Title\n\nbody", checksum: "ck1" });
    render(<Editor artifactId="a.md" artifactName="a.md" />);
    await screen.findByRole("heading", { name: "Title" });
    expect(screen.queryByText("● unsaved")).not.toBeInTheDocument();
  });

  // EDT-FR-02: saving serializes the WYSIWYG document back to plain Markdown.
  // Round-trips a heading + bullet list through parse → serialize and asserts
  // the on-disk body the backend receives is equivalent Markdown.
  it("saves the document back as plain Markdown", async () => {
    const backend: Backend = {
      load: { body: "# Title\n\n- one\n- two", checksum: "ck1" },
    };
    wireBackend(backend);
    render(
      <Editor artifactId="a.md" artifactName="a.md" sessions={sessions} />,
    );
    await screen.findByRole("heading", { name: "Title" });

    await saveNow();

    await waitFor(() => expect(backend.lastSavedBody).toBeTruthy());
    const md = backend.lastSavedBody as string;
    expect(md).toContain("# Title");
    expect(md).toContain("one");
    expect(md).toContain("two");
    // It is Markdown, not HTML — no tags leak into the file (EDT-FR-02).
    expect(md).not.toContain("<h1");
    expect(md).not.toContain("<li");
  });

  // EXC-FR-WDAV: "Load from filesystem" re-renders the editor with the on-disk
  // content, and a subsequent save serializes that CURRENT content (not stale).
  it("reloads on-disk content and saves the current document", async () => {
    const backend: Backend = { load: { body: "# One", checksum: "ck1" } };
    wireBackend(backend);
    render(<Editor artifactId="a.md" artifactName="a.md" sessions={sessions} />);
    await screen.findByRole("heading", { name: "One" });

    // External change -> reload to new on-disk content.
    backend.load = { body: "# Two", checksum: "ck2" };
    listeners[ARTIFACT_CHANGED_EXTERNALLY]?.({
      payload: { artifactId: "a.md", checksum: "ck2" },
    });
    await screen.findByRole("dialog");
    fireEvent.click(screen.getByRole("button", { name: "Load from filesystem" }));

    // The rendered document is now the reloaded content.
    expect(await screen.findByRole("heading", { name: "Two" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "One" })).not.toBeInTheDocument();

    // Saving serializes the CURRENT (reloaded) document.
    await saveNow();
    await waitFor(() => expect(backend.lastSavedBody).toContain("# Two"));
  });

  // EXC-FR-VTUH: while the modal is up, the WYSIWYG surface is not editable.
  it("disables the editor while the conflict modal is showing", async () => {
    wireLoad({ body: "# Title", checksum: "ck1" });
    const { container } = render(
      <Editor artifactId="a.md" artifactName="a.md" />,
    );
    await screen.findByRole("heading", { name: "Title" });
    const prose = container.querySelector(".editor__prose");
    expect(prose?.getAttribute("contenteditable")).toBe("true");

    listeners[ARTIFACT_CHANGED_EXTERNALLY]?.({
      payload: { artifactId: "a.md", checksum: "ck2" },
    });
    await screen.findByRole("dialog");
    await waitFor(() =>
      expect(prose?.getAttribute("contenteditable")).toBe("false"),
    );
  });

  // EDT-FR-16: the mode toggle and the comments control render as
  // graphical icon controls (svg, no text label) grouped in one right-aligned
  // cluster — and there is no Save control anywhere in the tab, because an
  // artifact writes itself (EDT-FR-70).
  it("renders the primary actions as graphical icon controls", async () => {
    wireLoad({ body: "# Title", checksum: "ck1" });
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    await screen.findByRole("heading", { name: "Title" });
    const toggle = screen.getByRole("button", {
      name: "Edit as Markdown source",
    });
    expect(toggle.querySelector("svg")).toBeInTheDocument(); // graphical
    expect(toggle.textContent?.trim()).toBe(""); // not a text label
    expect(toggle.closest(".editor__actions")).not.toBeNull(); // one cluster

    // No Save control, and no Inject control, anywhere in the tab.
    expect(screen.queryByRole("button", { name: "Save" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Inject" })).toBeNull();
  });

  // EDT-FR-17: WYSIWYG is the default; toggling to source shows raw
  // Markdown; an edit there survives toggling back, and the dirty state persists.
  it("defaults to WYSIWYG and toggles to source and back, preserving edits", async () => {
    wireLoad({ body: "# Title\n\nhello", checksum: "ck1" });
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    // Default mode is WYSIWYG: rich heading, no source textarea, not dirty.
    await screen.findByRole("heading", { name: "Title" });
    expect(screen.queryByLabelText("Markdown source")).not.toBeInTheDocument();
    expect(screen.queryByText("● unsaved")).not.toBeInTheDocument();

    // Toggle to source mode -> the raw Markdown is shown.
    fireEvent.click(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    );
    const source = (await screen.findByLabelText(
      "Markdown source",
    )) as HTMLTextAreaElement;
    expect(source.value).toContain("# Title");
    expect(source.value).toContain("hello");

    // Edit in source -> dirty.
    fireEvent.change(source, { target: { value: "# Title\n\nhello world" } });
    expect(screen.getByText("● unsaved")).toBeInTheDocument();

    // Toggle back to WYSIWYG -> content preserved and still dirty.
    fireEvent.click(screen.getByRole("button", { name: "Edit as rich text" }));
    expect(
      await screen.findByRole("heading", { name: "Title" }),
    ).toBeInTheDocument();
    expect(screen.getByText("● unsaved")).toBeInTheDocument();
  });

  // EDT-FR-17: source mode is byte-exact — saving from it writes the
  // Markdown verbatim, with no WYSIWYG round-trip normalization.
  it("saves byte-exact Markdown from source mode", async () => {
    const original = "# Title\n\n- a\n- b\n";
    const backend: Backend = { load: { body: original, checksum: "ck1" } };
    wireBackend(backend);
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
        sessions={sessions}
      />,
    );
    await screen.findByRole("heading", { name: "Title" });

    fireEvent.click(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    );
    const source = (await screen.findByLabelText(
      "Markdown source",
    )) as HTMLTextAreaElement;
    expect(source.value).toBe(original); // byte-exact in source mode
    // A file without frontmatter shows no region in either mode.
    expect(screen.queryByLabelText("Frontmatter")).not.toBeInTheDocument();

    await saveNow();
    await waitFor(() => expect(backend.lastSavedBody).toBe(original));
  });
});

describe("the tab's page and field are hooked up in the markup (EDT-FR-63)", () => {
  // The framing itself is CSS, asserted against the stylesheet in
  // `src/test/style-invariants.test.ts` — those rules select nothing unless
  // this markup is emitted, and deleting either hook here leaves that file
  // green while every Editor tab loses its field.
  it("names the tab as the field and the surface as the page, in both modes", async () => {
    wireBackend({ load: { body: "# Title\n\nbody\n", checksum: "ck1" } });
    const { container } = render(
      <Editor artifactId="a.md" artifactName="a.md" />,
    );
    await screen.findByRole("heading", { name: "Title" });

    expect(container.firstElementChild).toHaveClass("editor-tab");
    // The page is the WYSIWYG scroll container, and the band is its sibling
    // above rather than anything inside it.
    expect(document.querySelector(".editor-tab > .editor__toolbar")).not.toBeNull();
    expect(document.querySelector(".editor")).not.toBeNull();
    expect(document.querySelector(".editor .editor__toolbar")).toBeNull();

    // EDT-FR-17 / EDT-FR-63: the raw-text surface is the page the WYSIWYG
    // surface is, so it has a sheet of its own rather than filling the tab.
    fireEvent.click(screen.getByRole("button", { name: "Edit as Markdown source" }));
    await screen.findByLabelText("Markdown source");
    const sheet = document.querySelector(".editor__source-wrap");
    expect(sheet).not.toBeNull();
    expect(sheet!.querySelector(".editor__source")).not.toBeNull();
    expect(sheet!.closest(".editor-tab")).not.toBeNull();
  });
});
