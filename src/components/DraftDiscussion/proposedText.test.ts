/**
 * A change's proposed text, rendered and edited as the Markdown it is
 * (`../../../specifications/ui/DCR-draft-change-review.md` DCR-FR-09,
 * DCR-FR-24, DCR-FR-KDSV, DCR-FR-10).
 *
 * The defect these cases exist for: the text a change removes was drawn rich
 * where it stands, and the text it proposes was drawn as Markdown source beside
 * it, so the author compared `## Requirements` with a heading.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Editor } from "@tiptap/react";

import { mountProposedText, type ProposedTextOptions } from "./proposedText";

const teardowns: Array<() => void> = [];

afterEach(() => {
  while (teardowns.length > 0) teardowns.pop()?.();
});

function mount(
  markdown: string,
  over: Partial<ProposedTextOptions> = {},
): { host: HTMLElement; doc: HTMLElement; editor: Editor; teardown: () => void } {
  const host = document.createElement("div");
  document.body.append(host);
  const teardown = mountProposedText(host, markdown, {
    editable: true,
    label: "Proposed text of change 1 of 1",
    onChange: () => {},
    ...over,
  });
  teardowns.push(() => {
    teardown();
    host.remove();
  });
  const doc = host.querySelector<HTMLElement>(".hunk__doc");
  if (!doc) throw new Error("no proposed text surface was mounted");
  // Tiptap keeps its instance on the element it mounts, which is how a case
  // reaches it without the module exporting one.
  const editor = (doc as HTMLElement & { editor?: Editor }).editor;
  if (!editor) throw new Error("no editor on the proposed text surface");
  return { host, doc, editor, teardown };
}

describe("the proposed text reads as the page reads (DCR-FR-09)", () => {
  it("DCR-FR-09: a heading, a list, and emphasis render as themselves", () => {
    const { doc } = mount("## Head\n\n- one\n- two\n\nSome *em* text.");
    expect(doc.querySelector("h2")?.textContent).toBe("Head");
    expect(doc.querySelectorAll("ul > li")).toHaveLength(2);
    expect(doc.querySelector("em")?.textContent).toBe("em");
    expect(doc.textContent).not.toContain("##");
    expect(doc.textContent).not.toContain("*em*");
  });

  it("DCR-FR-09: the surface carries the page's own typesetting class", () => {
    const { doc } = mount("Body.");
    expect(doc.classList.contains("doc")).toBe(true);
  });

  it("DCR-FR-09: raw HTML in the proposed text stays text and is never markup", () => {
    const { doc } = mount("a <b>x</b> and <script>y</script>");
    expect(doc.querySelector("b, script")).toBeNull();
    expect(doc.textContent).toContain("<b>x</b>");
    expect(doc.textContent).toContain("<script>y</script>");
  });

  it("DCR-FR-09: the blank lines around the text are not drawn", () => {
    const { doc } = mount("\n\nOnly this.\n\n");
    expect(doc.querySelectorAll("p")).toHaveLength(1);
    expect(doc.textContent).toBe("Only this.");
  });

  it.each(["", "\n\n"])("DCR-FR-09: text with nothing in it (%j) mounts", (markdown) => {
    expect(() => mount(markdown)).not.toThrow();
  });
});

describe("the proposed text is edited in place (DCR-FR-24)", () => {
  it("DCR-FR-24: an edit is reported as Markdown, with its separators put back", () => {
    const onChange = vi.fn();
    const { editor } = mount("\n\n## Old head\n\n", { onChange });
    editor.commands.setTextSelection(editor.state.doc.content.size - 1);
    editor.commands.insertContent(" now");
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenLastCalledWith("\n\n## Old head now\n\n");
  });

  it("DCR-FR-24: CRLF separators are kept as they were written", () => {
    const onChange = vi.fn();
    const { editor, doc } = mount("\r\n\r\nWord\r\n", { onChange });
    expect(doc.textContent).toBe("Word");
    editor.commands.setTextSelection(editor.state.doc.content.size - 1);
    editor.commands.insertContent("s");
    expect(onChange).toHaveBeenLastCalledWith("\r\n\r\nWords\r\n");
  });

  it("DCR-FR-24: a text written with CRLF gets its edit back in CRLF throughout", () => {
    const onChange = vi.fn();
    const { editor } = mount("One\r\n\r\nTwo", { onChange });
    editor.commands.setTextSelection(editor.state.doc.content.size - 1);
    editor.commands.insertContent("!");
    expect(onChange).toHaveBeenLastCalledWith("One\r\n\r\nTwo!");
  });

  it("DCR-FR-24: text that is all blank lines keeps one separator, not two", () => {
    const onChange = vi.fn();
    const { editor } = mount("\n\n", { onChange });
    editor.commands.insertContent("New");
    expect(onChange).toHaveBeenLastCalledWith("\n\nNew");
  });

  it("DCR-FR-24: a move of the caret is not an edit", () => {
    const onChange = vi.fn();
    const { editor } = mount("One two three.", { onChange });
    editor.commands.setTextSelection(3);
    // Turning the surface editable emits an update that changes no text, which
    // is the case the guard exists for.
    editor.setEditable(true);
    expect(onChange).not.toHaveBeenCalled();
  });

  it("DCR-FR-24, DCR-FR-10: an editable text is a named, writable textbox", () => {
    const { doc } = mount("Body.");
    expect(doc.getAttribute("contenteditable")).toBe("true");
    expect(doc.getAttribute("role")).toBe("textbox");
    expect(doc.getAttribute("aria-multiline")).toBe("true");
    expect(doc.getAttribute("aria-label")).toBe("Proposed text of change 1 of 1");
    expect(doc.hasAttribute("aria-readonly")).toBe(false);
  });

  it.each([
    ["a legacy change", { editable: false, onChange: () => {} }],
    ["no one to report an edit to", { editable: true, onChange: undefined }],
  ])("DCR-FR-KDSV, DCR-FR-10: with %s the text is read-only and still named", (_, over) => {
    const { doc } = mount("Body.", over);
    expect(doc.getAttribute("contenteditable")).toBe("false");
    expect(doc.getAttribute("aria-readonly")).toBe("true");
    expect(doc.getAttribute("aria-label")).toBe("Proposed text of change 1 of 1");
  });

  it("DCR-FR-24: an undo in the proposed text reverses the edit made to it", () => {
    const onChange = vi.fn();
    const { editor, doc } = mount("Word", { onChange });
    editor.commands.setTextSelection(editor.state.doc.content.size - 1);
    editor.commands.insertContent("s");
    expect(doc.textContent).toBe("Words");
    // jsdom reports no Apple platform, so the keymap reads `Mod` as Control.
    editor.view.dom.dispatchEvent(
      new KeyboardEvent("keydown", { key: "z", ctrlKey: true, bubbles: true, cancelable: true }),
    );
    expect(doc.textContent).toBe("Word");
    expect(onChange).toHaveBeenLastCalledWith("Word");
  });

  it("DCR-FR-09: an edit to a text that ends in a list adds no empty block at its foot", () => {
    const { editor, doc } = mount("Intro.\n\n- one\n- two");
    editor.commands.setTextSelection(editor.state.doc.content.size - 3);
    editor.commands.insertContent("!");
    expect(doc.lastElementChild?.tagName).toBe("UL");
  });

  it("DCR-FR-09: the surface keeps the rest of the Editor's extension set as it is", () => {
    // Configured again only to drop the trailing paragraph. The Editor's own
    // text node and the tab-wide history choice must hold here too.
    const { editor } = mount("Body.");
    const names = editor.extensionManager.extensions.map((e) => e.name);
    expect(names.filter((n) => n === "text")).toHaveLength(1);
    expect(names).not.toContain("undoRedo");
    expect(names).not.toContain("trailingNode");
    expect(names).toContain("proposedTextHistory");
  });

  it("DCR-FR-24: the teardown frees the surface", () => {
    const { editor, teardown } = mount("Body.");
    teardown();
    expect(editor.isDestroyed).toBe(true);
  });
});
