import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { Editor as TiptapEditor } from "@tiptap/react";

vi.mock("mermaid", async () => (await import("../test/mermaidMock")).mermaidModule);

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { Editor } from "./Editor";
import { EditSessionStore } from "../state/editSessions";
import { resetLogBufferForTest } from "../logging";
import { mermaidApi, resetMermaidMock } from "../test/mermaidMock";
import { SHOW_DIAGRAM_LABEL, SHOW_SOURCE_LABEL, DIAGRAM_LABEL } from "./markdownMermaid";
import { resetMermaidForTest } from "./markdownMermaidRender";

const BODY = "# Title\n\n```mermaid\ngraph TD\nA-->B\n```\n\nAfter the diagram.\n";

interface Backend {
  body: string;
  saves: string[];
}

/** A strict backend: it answers what the Editor asks for and rejects the rest. */
function wireBackend(body = BODY): Backend {
  const backend: Backend = { body, saves: [] };
  invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
    if (cmd === "load_artifact_contents_by_id") return { body: backend.body, checksum: "ck1" };
    if (cmd === "save_artifact_contents") {
      backend.saves.push((args as { body: string }).body);
      return { checksum: `ck-saved-${backend.saves.length}` };
    }
    if (cmd === "append_log_records") return undefined;
    throw new Error(`unexpected invoke ${cmd}`);
  });
  return backend;
}

let sessions: EditSessionStore;

const lastSave = (backend: Backend) => backend.saves[backend.saves.length - 1];

const pmEl = () => document.querySelector(".ProseMirror") as HTMLElement;
const tiptap = () => (pmEl() as unknown as { editor: TiptapEditor }).editor;
const fence = () => pmEl().querySelector("pre") as HTMLElement;
const viewOf = () => fence().getAttribute("data-diagram-view");
const pressUndo = () => fireEvent.keyDown(pmEl(), { key: "z", metaKey: true });
const pressRedo = () => fireEvent.keyDown(pmEl(), { key: "z", metaKey: true, shiftKey: true });

async function mount() {
  render(<Editor artifactId="a.md" artifactName="a.md" sessions={sessions} />);
  await screen.findByRole("button", { name: DIAGRAM_LABEL });
}

async function saveNow() {
  await act(async () => {
    await sessions.flush("a.md", { force: true });
  });
}

/** Type at the end of the fence's source, as one ordinary WYSIWYG edit. */
function typeInFence(text: string) {
  const ed = tiptap();
  let end = -1;
  ed.state.doc.descendants((node, pos) => {
    if (node.type.name === "codeBlock") end = pos + node.nodeSize - 1;
  });
  act(() => {
    ed.view.dispatch(ed.state.tr.insertText(text, end));
  });
}

beforeEach(() => {
  invokeMock.mockReset();
  resetLogBufferForTest();
  resetMermaidForTest();
  resetMermaidMock();
  sessions = new EditSessionStore();
});

afterEach(() => {
  cleanup();
});

describe("diagram fences in an Editor tab", () => {
  it("EDT-FR-CQBG, EDT-FR-MOYS: rendering and toggling leave the buffer, the dirty state and the bytes alone", async () => {
    const backend = wireBackend();
    await mount();

    fireEvent.click(screen.getByRole("button", { name: SHOW_SOURCE_LABEL }));
    expect(viewOf()).toBe("source");
    fireEvent.click(screen.getByRole("button", { name: SHOW_DIAGRAM_LABEL }));
    expect(viewOf()).toBe("diagram");
    fireEvent.click(screen.getByRole("button", { name: DIAGRAM_LABEL }));
    expect(viewOf()).toBe("source");

    expect(screen.queryByText("● unsaved")).not.toBeInTheDocument();
    await saveNow();
    expect(backend.saves).toEqual([BODY]);
  });

  it("EDT-FR-CQBG: an undo after an edit and a change of view reverses the edit, not the view", async () => {
    wireBackend();
    await mount();
    fireEvent.click(screen.getByRole("button", { name: SHOW_SOURCE_LABEL }));
    typeInFence("X");
    await screen.findByText("● unsaved");
    fireEvent.click(screen.getByRole("button", { name: SHOW_DIAGRAM_LABEL }));
    expect(viewOf()).not.toBe("source");

    pressUndo();
    expect(tiptap().getText()).not.toContain("A-->BX");
    expect(tiptap().getText()).toContain("A-->B");
    await waitFor(() => expect(viewOf()).toBe("diagram"));
    // One undo took the edit back; a second has nothing left to take.
    pressUndo();
    expect(tiptap().getText()).toContain("A-->B");
  });

  it("EDT-FR-MIQE: a source edit is an ordinary edit, saved as a plain fence and undone as one step", async () => {
    const backend = wireBackend();
    await mount();
    fireEvent.click(screen.getByRole("button", { name: SHOW_SOURCE_LABEL }));

    typeInFence("\nB-->C");
    await screen.findByText("● unsaved");
    await waitFor(() =>
      expect(mermaidApi.render).toHaveBeenLastCalledWith(
        expect.any(String),
        "graph TD\nA-->B\nB-->C",
      ),
    );

    await saveNow();
    // The first WYSIWYG edit writes the surface's serialisation (EDT-FR-66).
    expect(lastSave(backend)).toBe(
      "# Title\n\n```mermaid\ngraph TD\nA-->B\nB-->C\n```\n\nAfter the diagram.",
    );

    const renders = mermaidApi.render.mock.calls.length;
    pressUndo();
    expect(tiptap().getText()).not.toContain("B-->C");
    // The undo puts the whole restored document back, so the fence starts
    // again in its default view.
    await waitFor(() => expect(viewOf()).toBe("diagram"));
    // EDT-FR-MIQE: the restored source draws its diagram again, from what is known.
    expect(fence().querySelector("svg text")?.textContent).toBe("graph TD\nA-->B");
    expect(mermaidApi.render.mock.calls.length).toBe(renders);

    pressRedo();
    expect(tiptap().getText()).toContain("B-->C");
    await waitFor(() =>
      expect(fence().querySelector("svg text")?.textContent).toBe("graph TD\nA-->B\nB-->C"),
    );
  });

  it("EDT-FR-XTEB: an invalid diagram shows its error and never blocks the save", async () => {
    const backend = wireBackend();
    await mount();
    fireEvent.click(screen.getByRole("button", { name: SHOW_SOURCE_LABEL }));
    typeInFence(" INVALID");
    expect(
      await screen.findByText("Diagram error: The Mermaid source is not valid."),
    ).toBeInTheDocument();
    expect(screen.getByText(/Parse error on line 1: INVALID/)).toBeInTheDocument();
    // EDT-FR-70: the write the edit scheduled lands by itself.
    await waitFor(() => expect(backend.saves.length).toBeGreaterThan(0), { timeout: 5000 });
    expect(lastSave(backend)).toContain("A-->B INVALID\n```");
  });

  it("EFR-FR-DGBG, EDT-FR-UYEN: the Find panel shows the source of a fence that holds the current match, until it closes", async () => {
    wireBackend();
    await mount();
    act(() => {
      sessions.setFind("a.md", { form: "find" });
    });
    fireEvent.change(screen.getByLabelText("Find"), { target: { value: "A-->B" } });
    await waitFor(() => expect(viewOf()).toBe("source"));
    expect(fence().querySelector(".find-match--current")?.textContent).toBe("A-->B");

    act(() => {
      sessions.setFind("a.md", { form: null });
    });
    await waitFor(() => expect(viewOf()).toBe("diagram"));
    expect(screen.queryByText("● unsaved")).not.toBeInTheDocument();
  });

  it("EDT-FR-UYEN, EDT-FR-MIQE: Find & Replace in a fence's source is an edit that renders again", async () => {
    const backend = wireBackend();
    await mount();
    act(() => {
      sessions.setFind("a.md", { form: "replace" });
    });
    fireEvent.change(screen.getByLabelText("Find"), { target: { value: "A-->B" } });
    fireEvent.change(screen.getByLabelText("Replace with"), { target: { value: "A-->Z" } });
    await waitFor(() => expect(viewOf()).toBe("source"));
    fireEvent.click(screen.getByRole("button", { name: "Replace All" }));
    await waitFor(() =>
      expect(mermaidApi.render).toHaveBeenLastCalledWith(expect.any(String), "graph TD\nA-->Z"),
    );
    await saveNow();
    expect(lastSave(backend)).toContain("```mermaid\ngraph TD\nA-->Z\n```");
  });

  it("EDT-FR-MOYS: the raw-text surface shows the fence as plain Markdown with no diagram", async () => {
    wireBackend();
    await mount();
    fireEvent.click(screen.getByRole("button", { name: "Edit as Markdown source" }));
    const source = (await screen.findByLabelText("Markdown source")) as HTMLTextAreaElement;
    expect(source.value).toContain("```mermaid\ngraph TD\nA-->B\n```");
    expect(source.closest(".editor__source")?.querySelector("svg")).toBeFalsy();
  });

  it("EDT-FR-UYEN: a discussion anchored in a fence marks the whole diagram", async () => {
    const backend = wireBackend();
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
      if (cmd === "list_discussions") {
        return [
          {
            id: "t1",
            target: { kind: "artifact", artifactId: "a.md" },
            fragmentTarget: {
              owner: { kind: "artifact", artifactId: "a.md" },
              path: "a.md",
              start: BODY.indexOf("A-->B"),
              end: BODY.indexOf("A-->B") + 5,
              quote: "A-->B",
            },
            comments: [],
            resolved: false,
            createdAt: "2026-01-01T00:00:00Z",
            updatedAt: "2026-01-01T00:00:00Z",
          },
        ];
      }
      return base(cmd, args);
    });
    sessions.update("a.md", { mode: "wysiwyg", railOpen: true });
    await mount();
    await waitFor(() => expect(fence().getAttribute("data-diagram-commented")).toBe("true"));
    expect(viewOf()).toBe("diagram");
    expect(backend.saves).toEqual([]);
  });

  it("ESH-FR-RKHL, EDT-FR-MOYS: a source file that holds a mermaid fence shows it as text, with no diagram", async () => {
    wireBackend("notes\n```mermaid\ngraph TD\nA-->B\n```\n");
    render(<Editor artifactId="notes.txt" artifactName="notes.txt" sessions={sessions} />);
    const source = (await screen.findByRole("textbox")) as HTMLTextAreaElement;
    await waitFor(() => expect(source.value).toContain("```mermaid"));
    expect(document.querySelector(".mermaid-block, .mermaid-diagram")).toBeNull();
    expect(mermaidApi.render).not.toHaveBeenCalled();
  });

  it("EDT-FR-CQBG: a new surface on the file shows each fence as its diagram", async () => {
    wireBackend();
    await mount();
    fireEvent.click(screen.getByRole("button", { name: SHOW_SOURCE_LABEL }));
    expect(viewOf()).toBe("source");
    cleanup();
    await mount();
    expect(viewOf()).toBe("diagram");
  });
});
