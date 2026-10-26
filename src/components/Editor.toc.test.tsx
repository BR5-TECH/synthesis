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

const BODY = [
  "---",
  "title: Doc",
  "# a yaml comment",
  "---",
  "# Intro",
  "",
  "Some text.",
  "",
  "## **Prerequisites**",
  "",
  "```",
  "# not a heading",
  "```",
  "",
  "#### Details",
  "",
].join("\n");

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
    if (cmd === "list_comment_threads") return [];
    if (cmd === "append_log_records") return undefined;
    return Promise.reject(new Error(`unexpected invoke ${cmd}`));
  });
  return backend;
}

let sessions: EditSessionStore;
let scrollIntoView: ReturnType<typeof vi.fn>;
let scrollTo: ReturnType<typeof vi.fn>;
/**
 * jsdom has neither method, so each test sets a spy on the prototype and the
 * restore removes the spy again rather than leaving an own `undefined`.
 */
const original = {
  scrollIntoView: Object.getOwnPropertyDescriptor(HTMLElement.prototype, "scrollIntoView"),
  scrollTo: Object.getOwnPropertyDescriptor(HTMLElement.prototype, "scrollTo"),
};
function restore(name: keyof typeof original) {
  const descriptor = original[name];
  if (descriptor) Object.defineProperty(HTMLElement.prototype, name, descriptor);
  else delete (HTMLElement.prototype as unknown as Record<string, unknown>)[name];
}

const pmEl = () => document.querySelector(".ProseMirror") as HTMLElement;
const tiptap = () => (pmEl() as unknown as { editor: TiptapEditor }).editor;
const tocButton = () => screen.getByRole("button", { name: "Table of contents" });
const entries = () =>
  within(screen.getByRole("listbox", { name: "Table of contents" })).getAllByRole("option");
const entryTexts = () => entries().map((e) => `${e.getAttribute("data-level")}:${e.textContent}`);
const tocFilter = () => screen.getByTestId("toc-filter");
const pressUndo = () => fireEvent.keyDown(pmEl(), { key: "z", metaKey: true });
const pressRedo = () => fireEvent.keyDown(pmEl(), { key: "z", metaKey: true, shiftKey: true });

/** The position just inside the textblock whose text is `text`. */
function inside(ed: TiptapEditor, text: string): number {
  let at = -1;
  ed.state.doc.descendants((node, pos) => {
    if (at === -1 && node.isTextblock && node.textContent === text) at = pos + 1;
  });
  return at;
}

/** One ordinary WYSIWYG edit: `text` typed at the end of the textblock `after`. */
function typeAfter(after: string, text: string) {
  const ed = tiptap();
  const at = inside(ed, after) + after.length;
  act(() => {
    ed.view.dispatch(ed.state.tr.insertText(text, at));
  });
}

async function mount(props: { toc?: boolean; artifactId?: string } = {}) {
  const { toc = true, artifactId = "a.md" } = props;
  const view = render(
    <Editor artifactId={artifactId} artifactName={artifactId} sessions={sessions} toc={toc} />,
  );
  await waitFor(() => expect(document.querySelector(".editor__toolbar")).not.toBeNull());
  return view;
}

async function mountMarkdown() {
  await mount();
  await screen.findByRole("heading", { name: "Intro" });
}

async function toRaw() {
  fireEvent.click(screen.getByRole("button", { name: "Edit as Markdown source" }));
  return (await screen.findByLabelText("Markdown source")) as HTMLTextAreaElement;
}

async function saveNow() {
  await act(async () => {
    await sessions.flush("a.md", { force: true });
  });
}

beforeEach(() => {
  invokeMock.mockReset();
  resetLogBufferForTest();
  resetMermaidForTest();
  resetMermaidMock();
  sessions = new EditSessionStore();
  scrollIntoView = vi.fn();
  scrollTo = vi.fn();
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
    configurable: true,
    writable: true,
    value: scrollIntoView,
  });
  Object.defineProperty(HTMLElement.prototype, "scrollTo", {
    configurable: true,
    writable: true,
    value: scrollTo,
  });
});

afterEach(() => {
  cleanup();
  restore("scrollIntoView");
  restore("scrollTo");
});

describe("the table of contents control (EDT-FR-LGQA, EDT-FR-16)", () => {
  it("EDT-FR-LGQA, EDT-FR-16: an icon control with its name and tooltip", async () => {
    wireBackend();
    await mountMarkdown();
    const button = tocButton();
    expect(button).toHaveAttribute("title", "Table of contents");
    expect(button.textContent).toBe("");
    expect(button.querySelector("svg")).not.toBeNull();
    expect(button).toHaveAttribute("aria-expanded", "false");
  });

  /**
   * The pair rule: the control's wrapper is the element directly before the
   * mode toggle, so no control of any kind stands between them, and the
   * comments control stands before the pair.
   */
  function expectPairedWithToggle(toggleName: string) {
    const button = tocButton();
    const toggle = screen.getByRole("button", { name: toggleName });
    const cluster = button.closest(".editor__actions");
    expect(cluster).not.toBeNull();
    expect(toggle.closest(".editor__actions")).toBe(cluster);
    expect(button.closest(".toc")?.nextElementSibling).toBe(toggle);
    // EDT-FR-16: the cluster stands beside the band's clipping surface, not in
    // it, so a narrow band never clips the pair.
    expect(cluster!.closest(".editor__band-surface")).toBeNull();
    expect(cluster!.previousElementSibling).toHaveClass("editor__band-surface");
    const comments = cluster!.querySelector(".editor__comments-toggle");
    expect(comments).not.toBeNull();
    expect(comments!.compareDocumentPosition(button) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  }

  it("EDT-FR-LGQA, EDT-FR-16: in WYSIWYG with the formatting toolbar, directly before the mode toggle, after the comments control", async () => {
    wireBackend();
    await mountMarkdown();
    expect(screen.getByRole("button", { name: "Heading 1" })).toBeInTheDocument();
    expectPairedWithToggle("Edit as Markdown source");
  });

  it("EDT-FR-LGQA, EDT-FR-16: in raw-text mode, directly before the mode toggle, after the comments control", async () => {
    wireBackend();
    await mountMarkdown();
    await toRaw();
    await screen.findByRole("button", { name: "Edit as rich text" });
    expectPairedWithToggle("Edit as rich text");
  });

  it.each(["find", "replace"] as const)(
    "EDT-FR-LGQA, EDT-FR-16: with the %s panel in the band, directly before the mode toggle",
    async (form) => {
      wireBackend();
      await mountMarkdown();
      act(() => {
        sessions.setFind("a.md", { form });
      });
      await screen.findByLabelText("Find");
      expect(screen.queryByRole("button", { name: "Heading 1" })).toBeNull();
      expectPairedWithToggle("Edit as Markdown source");
    },
  );

  it("EDT-FR-LGQA, EDT-FR-16: a source file has no table of contents control", async () => {
    wireBackend("const a = 1;\n");
    await mount({ artifactId: "a.ts" });
    await screen.findByLabelText("Source");
    expect(screen.queryByRole("button", { name: "Table of contents" })).toBeNull();
  });

  it("EDT-FR-LGQA: a surface that does not ask for the control has none", async () => {
    wireBackend();
    await mount({ toc: false });
    await screen.findByRole("heading", { name: "Intro" });
    expect(screen.queryByRole("button", { name: "Table of contents" })).toBeNull();
  });
});

describe("the table of contents panel", () => {
  it("EDT-FR-PNGU, EDT-FR-JVXN, EDT-FR-AQCR: lists the headings of the rich body in order, as plain text, with their levels", async () => {
    wireBackend();
    await mountMarkdown();
    fireEvent.click(tocButton());
    expect(tocButton()).toHaveAttribute("aria-expanded", "true");
    expect(entryTexts()).toEqual(["1:Intro", "2:Prerequisites", "4:Details"]);
    const [first, second] = entries();
    expect(second).toHaveAttribute("title", "Prerequisites");
    expect(second).toHaveAccessibleName("Prerequisites, heading level 2");
    expect(first).toHaveAccessibleName("Intro, heading level 1");
    expect(tocFilter()).toHaveFocus();
  });

  it("EDT-FR-OCJG: in raw-text mode the panel lists the ATX headings of the source, without frontmatter or fenced lines", async () => {
    wireBackend();
    await mountMarkdown();
    await toRaw();
    fireEvent.click(tocButton());
    expect(entryTexts()).toEqual(["1:Intro", "2:Prerequisites", "4:Details"]);
  });

  it("EDT-FR-EBNU: a document with no heading shows \"No headings\", and the control stays enabled", async () => {
    wireBackend("Just text.\n");
    await mount();
    await screen.findByText("Just text.");
    expect(tocButton()).not.toBeDisabled();
    fireEvent.click(tocButton());
    expect(screen.getByText("No headings")).toBeInTheDocument();
    expect(screen.queryByRole("option")).toBeNull();
    fireEvent.keyDown(tocFilter(), { key: "Escape" });
    expect(screen.queryByText("No headings")).toBeNull();
  });

  it("EDT-FR-KJLQ: while open, the list follows an edit, an undo, and a redo", async () => {
    wireBackend();
    await mountMarkdown();
    fireEvent.click(tocButton());
    expect(entryTexts()).toHaveLength(3);

    // An edit: "Some text." becomes a level 3 heading.
    const ed = tiptap();
    act(() => {
      ed.chain().setTextSelection(inside(ed, "Some text.")).toggleHeading({ level: 3 }).run();
    });
    const withEdit = ["1:Intro", "3:Some text.", "2:Prerequisites", "4:Details"];
    await waitFor(() => expect(entryTexts()).toEqual(withEdit));

    pressUndo();
    await waitFor(() => expect(entryTexts()).toEqual(["1:Intro", "2:Prerequisites", "4:Details"]));
    pressRedo();
    await waitFor(() => expect(entryTexts()).toEqual(withEdit));
  });

  // The switch is driven programmatically: a pointer-down on the mode toggle is
  // outside the panel, so with a real pointer the panel closes first. The store
  // still has to give the list of the new surface to a panel that stays open.
  it("EDT-FR-KJLQ: while open, the list follows each switch of mode and the edits made on the source", async () => {
    wireBackend();
    await mountMarkdown();
    fireEvent.click(tocButton());
    expect(entryTexts()).toHaveLength(3);

    // To raw text: the targets are now source offsets, so a selection puts the
    // caret in the textarea rather than in the hidden rich body.
    const source = await toRaw();
    fireEvent.change(source, { target: { value: `${source.value}\n## Added\n` } });
    await waitFor(() =>
      expect(entryTexts()).toEqual(["1:Intro", "2:Prerequisites", "4:Details", "2:Added"]),
    );
    fireEvent.click(entries()[3]);
    expect(document.activeElement).toBe(source);
    expect(source.selectionStart).toBe(source.value.indexOf("## Added"));

    // Back to WYSIWYG, with the panel open: the heading typed on the source is
    // a heading of the rich body, and a selection moves the rich caret.
    fireEvent.click(tocButton());
    fireEvent.click(screen.getByRole("button", { name: "Edit as rich text" }));
    await screen.findByRole("heading", { name: "Added" });
    await waitFor(() =>
      expect(entryTexts()).toEqual(["1:Intro", "2:Prerequisites", "4:Details", "2:Added"]),
    );
    fireEvent.click(entries()[3]);
    expect(tiptap().state.selection.$from.parent.textContent).toBe("Added");
  });

  it("EDT-FR-YSJX, EDT-FR-NHDM: selecting an entry in WYSIWYG moves the caret to the heading, scrolls it, and changes no content", async () => {
    const backend = wireBackend();
    await mountMarkdown();
    fireEvent.click(tocButton());
    fireEvent.click(entries()[1]);

    expect(screen.queryByTestId("toc-menu")).toBeNull();
    const ed = tiptap();
    const { $from } = ed.state.selection;
    expect($from.parent.type.name).toBe("heading");
    expect($from.parent.textContent).toBe("Prerequisites");
    expect($from.parentOffset).toBe(0);
    const smooth = scrollIntoView.mock.calls.findIndex(
      ([options]) => (options as ScrollIntoViewOptions | undefined)?.behavior === "smooth",
    );
    expect(scrollIntoView.mock.calls[smooth]?.[0]).toEqual({ behavior: "smooth", block: "start" });
    expect((scrollIntoView.mock.contexts[smooth] as HTMLElement).textContent).toBe("Prerequisites");
    await waitFor(() => expect(pmEl().contains(document.activeElement)).toBe(true));

    // EDT-FR-NHDM: no dirty state and no write of changed bytes.
    expect(sessions.get("a.md")?.dirty ?? false).toBe(false);
    await saveNow();
    expect(backend.saves).toEqual([BODY]);
  });

  it("EDT-FR-NHDM: a selection adds no undo step — the next undo reverses the edit made before it", async () => {
    wireBackend();
    await mountMarkdown();
    typeAfter("Some text.", " X");
    await screen.findByText("● unsaved");

    fireEvent.click(tocButton());
    fireEvent.click(entries()[2]);
    expect(tiptap().state.selection.$from.parent.textContent).toBe("Details");

    pressUndo();
    await waitFor(() => expect(tiptap().getText()).not.toContain("Some text. X"));
    expect(tiptap().getText()).toContain("Some text.");
  });

  it("EDT-FR-YSJX, EDT-FR-AQCR: Enter on an entry selects it", async () => {
    wireBackend();
    await mountMarkdown();
    fireEvent.click(tocButton());
    fireEvent.keyDown(tocFilter(), { key: "ArrowDown" });
    expect(entries()[1]).toHaveAttribute("data-highlighted", "true");
    fireEvent.keyDown(tocFilter(), { key: "ArrowDown" });
    fireEvent.keyDown(tocFilter(), { key: "Enter" });
    expect(screen.queryByTestId("toc-menu")).toBeNull();
    expect(tiptap().state.selection.$from.parent.textContent).toBe("Details");
  });

  it("EDT-FR-YSJX, EDT-FR-NHDM: selecting an entry in raw-text mode puts the caret at the start of its line", async () => {
    wireBackend();
    await mountMarkdown();
    const source = await toRaw();
    fireEvent.click(tocButton());
    fireEvent.click(entries()[2]);

    const offset = BODY.indexOf("#### Details");
    expect(document.activeElement).toBe(source);
    expect(source.selectionStart).toBe(offset);
    expect(source.selectionEnd).toBe(offset);
    expect(scrollTo).toHaveBeenCalledWith(expect.objectContaining({ behavior: "smooth" }));
    expect(source.value).toBe(BODY);
    expect(sessions.get("a.md")?.dirty ?? false).toBe(false);
  });

  it("EDT-FR-AQCR, EDT-FR-KOUB: Escape closes the panel, returns focus to the control, and leaves the Find panel open", async () => {
    wireBackend();
    await mountMarkdown();
    act(() => {
      sessions.setFind("a.md", { form: "find" });
    });
    await screen.findByRole("textbox", { name: /find/i });

    // Nothing outside the panel hears its Escape.
    const heard = vi.fn();
    document.addEventListener("keydown", heard);
    fireEvent.click(tocButton());
    fireEvent.keyDown(tocFilter(), { key: "Escape" });
    document.removeEventListener("keydown", heard);
    expect(heard).not.toHaveBeenCalled();
    expect(screen.queryByTestId("toc-menu")).toBeNull();
    expect(tocButton()).toHaveFocus();
    expect(screen.getByRole("textbox", { name: /find/i })).toBeInTheDocument();
  });

  it("EDT-FR-KOUB: a pointer-down outside closes the panel", async () => {
    wireBackend();
    await mountMarkdown();
    fireEvent.click(tocButton());
    fireEvent.mouseDown(document.body);
    expect(screen.queryByTestId("toc-menu")).toBeNull();
  });

  it("EDT-FR-IZHD, EDT-FR-YSJX: a filtered entry still moves the caret to its own heading", async () => {
    wireBackend();
    await mountMarkdown();
    fireEvent.click(tocButton());
    fireEvent.change(tocFilter(), { target: { value: "DETAIL" } });
    expect(entryTexts()).toEqual(["4:Details"]);
    fireEvent.click(entries()[0]);
    expect(tiptap().state.selection.$from.parent.textContent).toBe("Details");
    await waitFor(() => expect(pmEl().contains(document.activeElement)).toBe(true));
  });

  it("EDT-FR-YSJX: the rich surface takes focus at once, before the smooth scroll starts", async () => {
    wireBackend();
    await mountMarkdown();
    let focusedAtScroll = false;
    scrollIntoView.mockImplementation(() => {
      focusedAtScroll = pmEl().contains(document.activeElement);
    });
    fireEvent.click(tocButton());
    fireEvent.click(entries()[1]);
    expect(scrollIntoView).toHaveBeenCalledWith({ behavior: "smooth", block: "start" });
    expect(focusedAtScroll).toBe(true);
  });

  it("EDT-FR-KOUB: opening and closing the panel leave the comment rail as it is", async () => {
    wireBackend();
    await mountMarkdown();
    const comments = document.querySelector(".editor__comments-toggle") as HTMLButtonElement;
    fireEvent.click(comments);
    await waitFor(() => expect(comments).toHaveAttribute("aria-expanded", "true"));
    fireEvent.click(tocButton());
    fireEvent.keyDown(tocFilter(), { key: "Escape" });
    fireEvent.click(tocButton());
    fireEvent.click(entries()[0]);
    expect(comments).toHaveAttribute("aria-expanded", "true");
  });
});
