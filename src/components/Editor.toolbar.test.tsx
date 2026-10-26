import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
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
import { resetMermaidMock } from "../test/mermaidMock";
import { resetMermaidForTest } from "./markdownMermaidRender";
import { DIAGRAM_TEMPLATES } from "./Editor/diagramTemplates";

const BODY = "# Title\n\nSome text.\n";

interface Backend {
  saves: string[];
}

/** A strict backend: it answers what the Editor asks for and rejects the rest. */
function wireBackend(body = BODY): Backend {
  const backend: Backend = { saves: [] };
  invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
    if (cmd === "load_artifact_contents_by_id") return { body, checksum: "ck1" };
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

const pmEl = () => document.querySelector(".ProseMirror") as HTMLElement;
const tiptap = () => (pmEl() as unknown as { editor: TiptapEditor }).editor;
const pressUndo = () => fireEvent.keyDown(pmEl(), { key: "z", metaKey: true });
const pressRedo = () => fireEvent.keyDown(pmEl(), { key: "z", metaKey: true, shiftKey: true });
const diagramButton = () => screen.getByRole("button", { name: "Insert diagram" });

async function mount(props: { readOnly?: boolean } = {}) {
  const view = render(
    <Editor artifactId="a.md" artifactName="a.md" sessions={sessions} {...props} />,
  );
  await screen.findByRole("heading", { name: "Title" });
  return view;
}

async function saveNow() {
  await act(async () => {
    await sessions.flush("a.md", { force: true });
  });
}

/** Put the caret in the textblock whose text is `text`. */
function caretIn(text: string) {
  const ed = tiptap();
  let at = -1;
  ed.state.doc.descendants((node, pos) => {
    if (at === -1 && node.isTextblock && node.textContent === text) at = pos + 1;
  });
  act(() => {
    ed.commands.setTextSelection(at);
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

describe("heading buttons (EDT-FR-NTHC)", () => {
  it.each([1, 2, 3, 4])(
    "EDT-FR-NTHC: Heading %i makes the block a heading of its level, shows it active, and makes it a paragraph again",
    async (level) => {
      wireBackend("Some text.\n");
      render(<Editor artifactId="a.md" artifactName="a.md" sessions={sessions} />);
      await screen.findByText("Some text.");
      caretIn("Some text.");
      const button = screen.getByRole("button", { name: `Heading ${level}` });
      expect(button).toHaveAttribute("title", `Heading ${level}`);
      expect(button).toHaveAttribute("data-active", "false");

      fireEvent.mouseDown(button);
      expect(pmEl().querySelector(`h${level}`)).toHaveTextContent("Some text.");
      expect(button).toHaveAttribute("data-active", "true");

      fireEvent.mouseDown(button);
      expect(pmEl().querySelector(`h${level}`)).toBeNull();
      expect(pmEl().querySelector("p")).toHaveTextContent("Some text.");
      expect(button).toHaveAttribute("data-active", "false");
    },
  );

  it("EDT-FR-NTHC: the heading buttons are H1 to H4, side by side, as text glyphs", async () => {
    wireBackend();
    await mount();
    const labels = [1, 2, 3, 4].map(
      (n) => screen.getByRole("button", { name: `Heading ${n}` }).textContent,
    );
    expect(labels).toEqual(["H1", "H2", "H3", "H4"]);
    expect(screen.queryByRole("button", { name: "Heading 5" })).toBeNull();
  });

  it("EDT-FR-NTHC, EFR-FR-ABVQ: the toolbar shows in WYSIWYG mode only", async () => {
    wireBackend();
    await mount();
    fireEvent.click(screen.getByRole("button", { name: "Edit as Markdown source" }));
    await screen.findByLabelText("Markdown source");
    expect(screen.queryByRole("button", { name: "Heading 1" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Insert diagram" })).toBeNull();

    // EFR-FR-ABVQ: closing a find panel in raw-text mode brings no toolbar.
    act(() => sessions.setFind("a.md", { form: "find" }));
    await screen.findByRole("textbox", { name: /find/i });
    act(() => sessions.setFind("a.md", { form: null }));
    await waitFor(() => expect(screen.queryByRole("textbox", { name: /find/i })).toBeNull());
    expect(screen.queryByRole("button", { name: "Heading 1" })).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Edit as rich text" }));
    await screen.findByRole("button", { name: "Heading 4" });
    expect(diagramButton()).toBeInTheDocument();
  });
});

describe("Insert diagram (EDT-FR-VUGQ, EDT-FR-OWNT, EDT-FR-IPMX, EDT-FR-HAGB)", () => {
  it("EDT-FR-VUGQ: the button sits after the quote control, shows an icon and no text, and has its name and tooltip", async () => {
    wireBackend();
    await mount();
    const button = diagramButton();
    expect(button).toHaveAttribute("title", "Insert diagram");
    expect(button.textContent).toBe("");
    expect(button.querySelector("svg")).not.toBeNull();
    const quote = screen.getByTitle("Quote");
    const band = quote.closest(".editor__toolbar") as HTMLElement;
    const buttons: HTMLElement[] = Array.from(band.querySelectorAll("button"));
    expect(buttons.indexOf(button)).toBe(buttons.indexOf(quote) + 1);
    expect(button).not.toBeDisabled();
  });

  it("EDT-FR-VUGQ: the button is disabled while the surface accepts no input, and an open menu closes", async () => {
    wireBackend();
    const view = await mount();
    fireEvent.click(diagramButton());
    expect(screen.getByRole("menu", { name: "Diagram type" })).toBeInTheDocument();

    view.rerender(<Editor artifactId="a.md" artifactName="a.md" sessions={sessions} readOnly />);
    await waitFor(() => {
      expect(diagramButton()).toBeDisabled();
      expect(screen.queryByRole("menu", { name: "Diagram type" })).toBeNull();
    });
  });

  it("EDT-FR-SCSP, EDT-FR-OWNT: the menu lists every template, with focus on the first one", async () => {
    wireBackend();
    await mount();
    fireEvent.click(diagramButton());
    const menu = screen.getByRole("menu", { name: "Diagram type" });
    const items = within(menu).getAllByRole("menuitem");
    expect(items.map((i) => i.textContent)).toEqual(DIAGRAM_TEMPLATES.map((t) => t.name));
    expect(diagramButton()).toHaveAttribute("aria-expanded", "true");
    expect(items[0]).toHaveFocus();
  });

  it("EDT-FR-OWNT: the arrow keys, Home, and End move between types and stop at both ends", async () => {
    wireBackend();
    await mount();
    fireEvent.click(diagramButton());
    const items = within(screen.getByRole("menu")).getAllByRole("menuitem");
    fireEvent.keyDown(items[0], { key: "ArrowUp" });
    expect(items[0]).toHaveFocus();
    fireEvent.keyDown(items[0], { key: "ArrowDown" });
    expect(items[1]).toHaveFocus();
    fireEvent.keyDown(items[1], { key: "End" });
    expect(items[items.length - 1]).toHaveFocus();
    fireEvent.keyDown(items[items.length - 1], { key: "ArrowDown" });
    expect(items[items.length - 1]).toHaveFocus();
    fireEvent.keyDown(items[items.length - 1], { key: "Home" });
    expect(items[0]).toHaveFocus();
  });

  it("EDT-FR-OWNT: Escape closes the menu and returns focus to the button", async () => {
    wireBackend();
    await mount();
    fireEvent.click(diagramButton());
    const items = within(screen.getByRole("menu")).getAllByRole("menuitem");
    fireEvent.keyDown(items[2], { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(diagramButton()).toHaveFocus();
    expect(diagramButton()).toHaveAttribute("aria-expanded", "false");
  });

  it("EDT-FR-OWNT: a pointer-down outside the menu and a second click on the button close it", async () => {
    wireBackend();
    await mount();
    fireEvent.click(diagramButton());
    fireEvent.mouseDown(document.body);
    expect(screen.queryByRole("menu")).toBeNull();

    fireEvent.click(diagramButton());
    expect(screen.getByRole("menu")).toBeInTheDocument();
    fireEvent.click(diagramButton());
    expect(screen.queryByRole("menu")).toBeNull();
    expect(tiptap().getText()).not.toContain("graph");
  });

  it("EDT-FR-IPMX, EDT-FR-OWNT: selecting a type inserts its example after the block with the caret, and closes the menu", async () => {
    wireBackend();
    await mount();
    caretIn("Title");
    fireEvent.click(diagramButton());
    fireEvent.click(screen.getByRole("menuitem", { name: "Sequence diagram" }));

    expect(screen.queryByRole("menu")).toBeNull();
    const sequence = DIAGRAM_TEMPLATES.find((t) => t.key === "sequence");
    const kinds: string[] = [];
    tiptap().state.doc.forEach((node) => kinds.push(node.type.name));
    expect(kinds.slice(0, 3)).toEqual(["heading", "codeBlock", "paragraph"]);
    const fence = tiptap().state.doc.child(1);
    expect(fence.attrs.language).toBe("mermaid");
    expect(fence.textContent).toBe(sequence?.example);
    // EDT-FR-IPMX: the fence shows its source with the caret in it.
    const { from } = tiptap().state.selection;
    const fenceStart = tiptap().state.doc.child(0).nodeSize;
    expect(from).toBeGreaterThan(fenceStart);
    expect(from).toBeLessThan(fenceStart + fence.nodeSize);
    expect(pmEl().querySelector("pre")?.getAttribute("data-diagram-view")).toBe("source");
    // The surface has focus again, so what the author types goes into the fence.
    expect(pmEl().contains(document.activeElement)).toBe(true);
  });

  it("EDT-FR-IPMX: in an empty document the fence replaces the one empty paragraph", async () => {
    wireBackend("");
    render(<Editor artifactId="a.md" artifactName="a.md" sessions={sessions} />);
    await waitFor(() => expect(pmEl()).not.toBeNull());
    fireEvent.click(diagramButton());
    fireEvent.click(screen.getByRole("menuitem", { name: "Pie chart" }));
    const first = tiptap().state.doc.child(0);
    expect(first.type.name).toBe("codeBlock");
    expect(first.attrs.language).toBe("mermaid");
  });

  it("EDT-FR-HAGB: the insert marks the artifact dirty, writes a plain mermaid fence, and is one undo step", async () => {
    const backend = wireBackend();
    await mount();
    caretIn("Some text.");
    fireEvent.click(diagramButton());
    fireEvent.click(screen.getByRole("menuitem", { name: "Flowchart" }));
    await screen.findByText("● unsaved");

    const flowchart = DIAGRAM_TEMPLATES[0];
    await saveNow();
    expect(backend.saves[backend.saves.length - 1]).toBe(
      "# Title\n\nSome text.\n\n```mermaid\n" + flowchart.example + "\n```",
    );

    pressUndo();
    await waitFor(() => expect(pmEl().querySelector("pre")).toBeNull());
    expect(tiptap().getText()).toContain("Some text.");
    pressRedo();
    await waitFor(() => expect(pmEl().querySelector("pre")).not.toBeNull());
  });

  it("EDT-FR-HAGB: the insert is a step of its own, which joins neither the edit before it nor the edit after it", async () => {
    wireBackend("Some text.\n");
    render(<Editor artifactId="a.md" artifactName="a.md" sessions={sessions} />);
    await screen.findByText("Some text.");
    caretIn("Some text.");
    // Three edits at one instant, so the coalescing window joins any two of
    // them that nothing seals apart, however slow the machine.
    const now = vi.spyOn(Date, "now").mockReturnValue(1_000_000);
    try {
      fireEvent.mouseDown(screen.getByRole("button", { name: "Heading 2" }));
      fireEvent.click(diagramButton());
      fireEvent.click(screen.getByRole("menuitem", { name: "Pie chart" }));
      caretIn("Some text.");
      fireEvent.mouseDown(screen.getByRole("button", { name: "Heading 2" }));
    } finally {
      now.mockRestore();
    }
    expect(pmEl().querySelector("h2")).toBeNull();

    pressUndo();
    await waitFor(() => expect(pmEl().querySelector("h2")).not.toBeNull());
    expect(pmEl().querySelector("pre")).not.toBeNull();
    pressUndo();
    await waitFor(() => expect(pmEl().querySelector("pre")).toBeNull());
    expect(pmEl().querySelector("h2")).not.toBeNull();
    pressUndo();
    await waitFor(() => expect(pmEl().querySelector("h2")).toBeNull());
  });
});
