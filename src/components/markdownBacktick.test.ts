/**
 * EDT-FR-FDGH: one press of the backtick key is one backtick in the WYSIWYG
 * surface, and the Markdown input rules still apply to it.
 *
 * The editor is mounted with the same extension set as every rich surface
 * (`markdownExtensions`), and key presses are sent to it as DOM key events. A
 * character that is not a backtick goes in through the text-input chain, as
 * the platform's own typing does.
 */
import { afterEach, describe, expect, it } from "vitest";
import { Editor } from "@tiptap/react";
import { NodeSelection, TextSelection } from "@tiptap/pm/state";
import { CellSelection } from "@tiptap/pm/tables";
import { getMarkdown, markdownExtensions } from "./markdownFidelity";

let editor: Editor | null = null;

afterEach(() => {
  editor?.destroy();
  editor = null;
});

function mount(content: string, editable = true): Editor {
  editor = new Editor({ extensions: markdownExtensions(), content, editable });
  return editor;
}

/** Put the cursor at the end of the document's last textblock. */
function cursorAtEnd(ed: Editor): void {
  ed.commands.setTextSelection(ed.state.doc.content.size - 1);
}

interface KeyOptions {
  key?: string;
  code?: string;
  shiftKey?: boolean;
  metaKey?: boolean;
  ctrlKey?: boolean;
  altKey?: boolean;
  isComposing?: boolean;
  keyCode?: number;
}

/** Send one keydown to the surface. Returns true when its default was prevented. */
function press(ed: Editor, options: KeyOptions = {}): boolean {
  const { keyCode, ...init } = options;
  const event = new KeyboardEvent("keydown", {
    key: "`",
    bubbles: true,
    cancelable: true,
    ...init,
  });
  if (keyCode !== undefined) {
    Object.defineProperty(event, "keyCode", { value: keyCode });
  }
  return !ed.view.dom.dispatchEvent(event);
}

/** Type text the way the platform's text input puts it in. */
function typeText(ed: Editor, text: string): void {
  for (const ch of text) {
    const view = ed.view;
    const { from, to } = view.state.selection;
    const insert = () => view.state.tr.insertText(ch, from, to);
    const handled = view.someProp("handleTextInput", (handle) =>
      handle(view, from, to, ch, insert),
    );
    if (!handled) view.dispatch(insert());
  }
}

function pressBackticks(ed: Editor, count: number): void {
  for (let i = 0; i < count; i += 1) press(ed);
}

/**
 * Press a key as the platform does: when the surface does not take the key,
 * the platform types `platformText` for it.
 */
function pressAsPlatform(ed: Editor, platformText: string, options: KeyOptions = {}): void {
  if (!press(ed, options)) typeText(ed, platformText);
}

/** The text that carries the inline-code mark, in document order. */
function codeMarkedText(ed: Editor): string {
  const codeType = ed.schema.marks.code;
  let marked = "";
  ed.state.doc.descendants((node) => {
    if (node.isText && codeType.isInSet(node.marks)) marked += node.text;
  });
  return marked;
}

describe("one backtick per key press (EDT-FR-FDGH)", () => {
  it("EDT-FR-FDGH: one press inserts exactly one backtick and takes the key from the platform", () => {
    const ed = mount("a");
    cursorAtEnd(ed);
    expect(press(ed)).toBe(true);
    expect(ed.state.doc.textContent).toBe("a`");
  });

  it("EDT-FR-FDGH: three presses give exactly three backticks", () => {
    const ed = mount("");
    pressBackticks(ed, 3);
    expect(ed.state.doc.textContent).toBe("```");
    expect(ed.state.doc.firstChild?.type.name).toBe("paragraph");
  });

  it("EDT-FR-FDGH: a platform that types two backticks per press still opens a code block", () => {
    const ed = mount("");
    for (let i = 0; i < 3; i += 1) pressAsPlatform(ed, "``");
    pressAsPlatform(ed, " ", { key: " " });
    expect(ed.state.doc.firstChild?.type.name).toBe("codeBlock");
  });

  it.each<[string, KeyOptions]>([
    ["with Shift", { shiftKey: true, code: "IntlBackslash" }],
    ["with Option", { altKey: true }],
    ["with AltGr (Control and Alt)", { ctrlKey: true, altKey: true }],
  ])("EDT-FR-FDGH: a backtick a layout types %s is one backtick", (_name, options) => {
    const ed = mount("a");
    cursorAtEnd(ed);
    expect(press(ed, options)).toBe(true);
    expect(ed.state.doc.textContent).toBe("a`");
  });

  it("EDT-FR-FDGH: a press replaces a non-empty selection", () => {
    const ed = mount("hello world");
    ed.commands.setTextSelection({ from: 7, to: 12 });
    press(ed);
    expect(ed.state.doc.textContent).toBe("hello `");
    expect(ed.state.selection.empty).toBe(true);
    expect(ed.state.selection.from).toBe(8);
  });
});

describe("the input rules apply to the backtick (EDT-FR-FDGH)", () => {
  it("EDT-FR-FDGH: ``` then Space opens a code block", () => {
    const ed = mount("");
    pressBackticks(ed, 3);
    typeText(ed, " ");
    const block = ed.state.doc.firstChild;
    expect(block?.type.name).toBe("codeBlock");
    expect(block?.textContent).toBe("");
  });

  it("EDT-FR-FDGH: ``` then Enter opens a code block", () => {
    const ed = mount("");
    pressBackticks(ed, 3);
    press(ed, { key: "Enter" });
    expect(ed.state.doc.firstChild?.type.name).toBe("codeBlock");
  });

  it("EDT-FR-FDGH: ```lang then Space opens a code block with that language", () => {
    const ed = mount("");
    pressBackticks(ed, 3);
    typeText(ed, "ts ");
    const block = ed.state.doc.firstChild;
    expect(block?.type.name).toBe("codeBlock");
    expect(block?.attrs.language).toBe("ts");
  });

  it("EDT-FR-FDGH: ```lang then Enter opens a code block with that language", () => {
    const ed = mount("");
    pressBackticks(ed, 3);
    typeText(ed, "ts");
    press(ed, { key: "Enter" });
    const block = ed.state.doc.firstChild;
    expect(block?.type.name).toBe("codeBlock");
    expect(block?.attrs.language).toBe("ts");
  });

  it("EDT-FR-FDGH: the opened code block serialises as a fenced block", () => {
    const ed = mount("");
    pressBackticks(ed, 3);
    press(ed, { key: "Enter" });
    typeText(ed, "let a");
    expect(getMarkdown(ed)).toBe("```\nlet a\n```");
  });

  // The fence rule fires on the Space or Enter, not on the backtick. This test
  // is the one that shows the backtick itself goes through the input rules.
  it("EDT-FR-FDGH: `text` becomes inline code", () => {
    const ed = mount("x");
    cursorAtEnd(ed);
    typeText(ed, " ");
    press(ed);
    typeText(ed, "code");
    press(ed);
    expect(codeMarkedText(ed)).toBe("code");
    expect(ed.state.doc.textContent).toBe("x code");
    typeText(ed, "z");
    expect(codeMarkedText(ed)).toBe("code");
    expect(getMarkdown(ed)).toBe("x `code`z");
  });

  it("EDT-FR-FDGH: inside a code block the backtick is literal text", () => {
    const ed = mount("```\nx\n```");
    cursorAtEnd(ed);
    pressBackticks(ed, 3);
    typeText(ed, " ");
    const block = ed.state.doc.firstChild;
    expect(block?.type.name).toBe("codeBlock");
    expect(block?.textContent).toBe("x``` ");
  });

  it("EDT-FR-FDGH: inside inline code the backtick is literal text", () => {
    const ed = mount("a `bc` d");
    // Between "b" and "c", inside the code mark.
    ed.view.dispatch(
      ed.state.tr.setSelection(TextSelection.create(ed.state.doc, 4)),
    );
    expect(press(ed)).toBe(true);
    expect(ed.state.doc.textContent).toBe("a b`c d");
    expect(codeMarkedText(ed)).toBe("b`c");
  });
});

describe("a backtick the platform keeps (EDT-FR-FDGH)", () => {
  it.each<[string, KeyOptions]>([
    ["with Command", { metaKey: true }],
    ["with Control", { ctrlKey: true }],
    ["during a composition", { isComposing: true }],
    ["from an input method (keyCode 229)", { keyCode: 229 }],
    ["from a dead key", { key: "Dead" }],
  ])("EDT-FR-FDGH: a backtick %s is not taken from the platform", (_name, options) => {
    const ed = mount("a");
    cursorAtEnd(ed);
    expect(press(ed, options)).toBe(false);
    expect(ed.state.doc.textContent).toBe("a");
  });

  it("EDT-FR-FDGH: a composed backtick goes in by the platform's own text input", () => {
    const ed = mount("a");
    cursorAtEnd(ed);
    pressAsPlatform(ed, "`", { isComposing: true });
    expect(ed.state.doc.textContent).toBe("a`");
  });

  it("EDT-FR-FDGH: a backtick on a node selection is not taken from the platform", () => {
    const ed = mount("a\n\n---\n\nb");
    let rulePos = -1;
    ed.state.doc.forEach((node, offset) => {
      if (node.type.name === "horizontalRule") rulePos = offset;
    });
    ed.view.dispatch(
      ed.state.tr.setSelection(NodeSelection.create(ed.state.doc, rulePos)),
    );
    const before = ed.state.doc.toJSON();
    expect(press(ed)).toBe(false);
    expect(ed.state.doc.toJSON()).toEqual(before);
  });

  it("EDT-FR-FDGH: a backtick on a cell selection is not taken from the platform", () => {
    const ed = mount("| a | b |\n| --- | --- |\n| c | d |");
    const cells: number[] = [];
    ed.state.doc.descendants((node, pos) => {
      if (node.type.name === "tableCell" || node.type.name === "tableHeader") {
        cells.push(pos);
      }
    });
    ed.view.dispatch(
      ed.state.tr.setSelection(CellSelection.create(ed.state.doc, cells[0], cells[1])),
    );
    const before = ed.state.doc.toJSON();
    expect(press(ed)).toBe(false);
    expect(ed.state.doc.toJSON()).toEqual(before);
  });

  it("EDT-FR-FDGH: a read-only surface leaves the key alone", () => {
    const ed = mount("a", false);
    cursorAtEnd(ed);
    expect(press(ed)).toBe(false);
    expect(ed.state.doc.textContent).toBe("a");
  });
});
