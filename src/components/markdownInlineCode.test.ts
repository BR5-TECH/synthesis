/**
 * EDT-FR-VCOH, EDT-FR-LLBU, EDT-FR-CCBX, EDT-FR-APAL, EDT-FR-EKEZ: inline code
 * in the WYSIWYG surface is auto-paired. A backtick opens a span at once, a
 * virtual closing backtick shows where it ends, and a backtick or the forward
 * arrow closes it.
 */
import { afterEach, describe, expect, it } from "vitest";
import { getMarkdown } from "./markdownFidelity";
import {
  codeMarkedText,
  cursorAtEnd,
  destroyRichEditor,
  ghost,
  mountRichEditor as mount,
  press,
  typeText,
  type KeyOptions,
} from "../test/richEditorFixtures";

afterEach(() => {
  destroyRichEditor();
});

function storedCode(ed: ReturnType<typeof mount>): boolean {
  return !!ed.state.storedMarks?.some((mark) => mark.type.name === "code");
}

describe("a backtick opens an inline-code span (EDT-FR-VCOH)", () => {
  it("EDT-FR-VCOH: a backtick inserts no character and opens a span", () => {
    const ed = mount("x");
    cursorAtEnd(ed);
    typeText(ed, " ");
    const before = ed.state.doc.toJSON();
    expect(press(ed)).toBe(true);
    expect(ed.state.doc.toJSON()).toEqual(before);
    expect(storedCode(ed)).toBe(true);
  });

  it("EDT-FR-VCOH: the text typed next is inline code", () => {
    const ed = mount("x");
    cursorAtEnd(ed);
    typeText(ed, " ");
    press(ed);
    typeText(ed, "abc");
    expect(codeMarkedText(ed)).toBe("abc");
    expect(ed.state.doc.textContent).toBe("x abc");
  });

  it("EDT-FR-VCOH: a span opens at the start of an empty paragraph", () => {
    const ed = mount("");
    press(ed);
    typeText(ed, "a");
    expect(codeMarkedText(ed)).toBe("a");
  });

  it("EDT-FR-VCOH: moving the cursor away from an empty span drops it", () => {
    const ed = mount("hello");
    cursorAtEnd(ed);
    press(ed);
    expect(storedCode(ed)).toBe(true);
    ed.commands.setTextSelection(2);
    expect(storedCode(ed)).toBe(false);
    expect(ghost(ed)).toBeNull();
    expect(ed.state.doc.textContent).toBe("hello");
    typeText(ed, "z");
    expect(codeMarkedText(ed)).toBe("");
  });
});

describe("a span next to inline code (EDT-FR-VCOH, EDT-FR-XTGK)", () => {
  it("EDT-FR-VCOH: a backtick directly after a closed span opens a span that joins that code", () => {
    const ed = mount("x");
    cursorAtEnd(ed);
    press(ed);
    typeText(ed, "abc");
    press(ed);
    expect(ghost(ed)).toBeNull();
    press(ed);
    expect(ghost(ed)).not.toBeNull();
    typeText(ed, "d");
    expect(codeMarkedText(ed)).toBe("abcd");
    press(ed);
    typeText(ed, "!");
    expect(getMarkdown(ed)).toBe("x`abcd`!");
  });

  it("EDT-FR-VCOH, EDT-FR-XTGK: two backticks directly before a span give two literal backticks", () => {
    const ed = mount("a `bc`");
    ed.commands.setTextSelection(3);
    press(ed);
    press(ed);
    expect(ed.state.doc.textContent).toBe("a ``bc");
    expect(codeMarkedText(ed)).toBe("bc");
  });

  it("EDT-FR-VCOH: a backtick after a closed span whose code ends in a backtick opens a span", () => {
    const ed = mount("x");
    cursorAtEnd(ed);
    press(ed);
    typeText(ed, "a");
    // A literal backtick inside the code, then close.
    ed.view.dispatch(ed.state.tr.insertText("`"));
    press(ed);
    expect(codeMarkedText(ed)).toBe("a`");
    press(ed);
    expect(ghost(ed)).not.toBeNull();
    expect(ed.state.doc.textContent).toBe("xa`");
  });
});

describe("the virtual closing backtick (EDT-FR-LLBU)", () => {
  it("EDT-FR-LLBU: an empty open span shows the virtual backtick at the cursor", () => {
    const ed = mount("x");
    cursorAtEnd(ed);
    expect(ghost(ed)).toBeNull();
    press(ed);
    const shown = ghost(ed);
    expect(shown).not.toBeNull();
    expect(shown?.textContent).toBe("`");
    expect(shown?.getAttribute("aria-hidden")).toBe("true");
    expect(shown?.contentEditable).toBe("false");
    expect(shown?.closest("code")).not.toBeNull();
  });

  it("EDT-FR-LLBU: the virtual backtick follows the span's text", () => {
    const ed = mount("x");
    cursorAtEnd(ed);
    press(ed);
    typeText(ed, "abc");
    const shown = ghost(ed);
    expect(shown).not.toBeNull();
    expect(shown?.closest("code")?.textContent).toBe("abc`");
  });

  it("EDT-FR-LLBU: with the cursor in the middle of a span, the virtual backtick is at its end", () => {
    const ed = mount("a `bcd` e");
    // Between "b" and "c".
    ed.commands.setTextSelection(4);
    const shown = ghost(ed);
    expect(shown).not.toBeNull();
    expect(shown?.previousSibling?.textContent).toBe("bcd");
  });

  it("EDT-FR-LLBU: the virtual backtick is not content and is not saved", () => {
    const ed = mount("x");
    cursorAtEnd(ed);
    typeText(ed, " ");
    press(ed);
    typeText(ed, "abc");
    expect(ghost(ed)).not.toBeNull();
    expect(ed.state.doc.textContent).toBe("x abc");
    expect(getMarkdown(ed)).toBe("x `abc`");
  });

  it("EDT-FR-LLBU: no virtual backtick shows in plain text or in a closed span", () => {
    const ed = mount("a `bc` d");
    cursorAtEnd(ed);
    expect(ghost(ed)).toBeNull();
    // Before the span: text typed here is plain.
    ed.commands.setTextSelection(2);
    expect(ghost(ed)).toBeNull();
  });
});

describe("a backtick or the forward arrow closes the span (EDT-FR-CCBX)", () => {
  it("EDT-FR-CCBX: a backtick at the end of the span closes it", () => {
    const ed = mount("x");
    cursorAtEnd(ed);
    typeText(ed, " ");
    press(ed);
    typeText(ed, "abc");
    const at = ed.state.selection.from;
    expect(press(ed)).toBe(true);
    expect(ed.state.selection.from).toBe(at);
    expect(ed.state.doc.textContent).toBe("x abc");
    expect(ghost(ed)).toBeNull();
    typeText(ed, "z");
    expect(codeMarkedText(ed)).toBe("abc");
    expect(getMarkdown(ed)).toBe("x `abc`z");
  });

  it("EDT-FR-CCBX: the right arrow at the end of the span closes it in place, with no space", () => {
    const ed = mount("x");
    cursorAtEnd(ed);
    typeText(ed, " ");
    press(ed);
    typeText(ed, "abc");
    const at = ed.state.selection.from;
    expect(press(ed, { key: "ArrowRight" })).toBe(true);
    expect(ed.state.selection.from).toBe(at);
    expect(ed.state.doc.textContent).toBe("x abc");
    expect(ghost(ed)).toBeNull();
    typeText(ed, "z");
    expect(getMarkdown(ed)).toBe("x `abc`z");
  });

  it.each(["backtick", "ArrowRight"])(
    "EDT-FR-CCBX: a %s at the end of a span in the middle of a paragraph closes it in place",
    (key) => {
      const ed = mount("a `bc` d");
      ed.commands.setTextSelection(5);
      expect(ghost(ed)).not.toBeNull();
      expect(press(ed, key === "backtick" ? {} : { key })).toBe(true);
      expect(ed.state.selection.from).toBe(5);
      expect(ghost(ed)).toBeNull();
      typeText(ed, "z");
      expect(getMarkdown(ed)).toBe("a `bc`z d");
    },
  );

  it("EDT-FR-CCBX: the right arrow in the middle of a span is left to the platform", () => {
    const ed = mount("a `bcd` e");
    ed.commands.setTextSelection(4);
    expect(press(ed, { key: "ArrowRight" })).toBe(false);
    expect(ed.state.selection.from).toBe(4);
    expect(ghost(ed)).not.toBeNull();
  });

  it("EDT-FR-CCBX: in a table cell, the right arrow at the end of the span closes it in place", () => {
    const ed = mount("| `ab` | c |\n| --- | --- |\n| d | e |");
    let end = -1;
    ed.state.doc.descendants((node, pos) => {
      if (node.isText && node.text === "ab") end = pos + node.nodeSize;
    });
    ed.commands.setTextSelection(end);
    expect(ghost(ed)).not.toBeNull();
    expect(press(ed, { key: "ArrowRight" })).toBe(true);
    expect(ed.state.selection.from).toBe(end);
    expect(ghost(ed)).toBeNull();
    typeText(ed, "z");
    expect(codeMarkedText(ed)).toBe("ab");
  });

  it("EDT-FR-CCBX: the right arrow at the end of a span at a paragraph end closes it and inserts no space", () => {
    const ed = mount("x `abc`\n\nnext");
    // At the end of "abc", the end of the first paragraph.
    ed.commands.setTextSelection(6);
    expect(press(ed, { key: "ArrowRight" })).toBe(true);
    expect(ed.state.selection.from).toBe(6);
    // Closed now: the key goes on to the platform, and no space is inserted.
    expect(press(ed, { key: "ArrowRight" })).toBe(false);
    expect(ed.state.doc.firstChild?.textContent).toBe("x abc");
  });

  it("EDT-FR-CCBX: the right arrow drops an empty open span", () => {
    const ed = mount("x");
    cursorAtEnd(ed);
    press(ed);
    expect(press(ed, { key: "ArrowRight" })).toBe(true);
    expect(ghost(ed)).toBeNull();
    typeText(ed, "z");
    expect(codeMarkedText(ed)).toBe("");
  });

  it("EDT-FR-CCBX: in right-to-left text, the left arrow closes the span", () => {
    const ed = mount("x");
    // The direction of the cursor's own paragraph decides.
    (ed.view.dom.firstElementChild as HTMLElement).style.direction = "rtl";
    cursorAtEnd(ed);
    press(ed);
    typeText(ed, "abc");
    expect(press(ed, { key: "ArrowRight" })).toBe(false);
    expect(press(ed, { key: "ArrowLeft" })).toBe(true);
    expect(ghost(ed)).toBeNull();
    typeText(ed, "z");
    expect(codeMarkedText(ed)).toBe("abc");
  });

  it.each<[string, KeyOptions]>([
    ["Shift", { shiftKey: true }],
    ["Option", { altKey: true }],
    ["Control", { ctrlKey: true }],
    ["Command", { metaKey: true }],
  ])("EDT-FR-CCBX: the right arrow with %s is left to the platform", (_name, modifier) => {
    const ed = mount("x");
    cursorAtEnd(ed);
    press(ed);
    typeText(ed, "abc");
    expect(press(ed, { key: "ArrowRight", ...modifier })).toBe(false);
    expect(ghost(ed)).not.toBeNull();
  });

  it("EDT-FR-CCBX: the right arrow in plain text is left to the platform", () => {
    const plain = mount("plain text");
    plain.commands.setTextSelection(3);
    expect(press(plain, { key: "ArrowRight" })).toBe(false);
  });
});

describe("a backtick wraps the selection (EDT-FR-APAL)", () => {
  it("EDT-FR-APAL: a selection in one textblock becomes inline code and the span stays open", () => {
    const ed = mount("hello world");
    ed.commands.setTextSelection({ from: 7, to: 12 });
    expect(press(ed)).toBe(true);
    expect(codeMarkedText(ed)).toBe("world");
    expect(ed.state.doc.textContent).toBe("hello world");
    expect(ed.state.selection.empty).toBe(true);
    expect(ed.state.selection.from).toBe(12);
    expect(ghost(ed)).not.toBeNull();
    typeText(ed, "s");
    expect(codeMarkedText(ed)).toBe("worlds");
    press(ed);
    typeText(ed, "!");
    expect(getMarkdown(ed)).toBe("hello `worlds`!");
  });

  it("EDT-FR-APAL: a selection that spans textblocks is deleted, then a span opens", () => {
    const ed = mount("first\n\nsecond");
    // From inside "first" to inside "second".
    ed.commands.setTextSelection({ from: 3, to: 11 });
    expect(press(ed)).toBe(true);
    expect(ed.state.doc.childCount).toBe(1);
    expect(ed.state.doc.textContent).toBe("fiond");
    expect(storedCode(ed)).toBe(true);
    typeText(ed, "x");
    expect(codeMarkedText(ed)).toBe("x");
  });
});

describe("a backtick on a selection that is not in one textblock (EDT-FR-APAL)", () => {
  it("EDT-FR-APAL, EDT-FR-FDGH: a backtick on select-all is taken, deletes the selection and opens a span", () => {
    const ed = mount("hello\n\nworld");
    ed.commands.selectAll();
    expect(press(ed)).toBe(true);
    expect(ed.state.doc.textContent).toBe("");
    expect(storedCode(ed)).toBe(true);
  });

  it("EDT-FR-APAL, EDT-FR-FKJP: a selection deleted into a code block leaves a literal backtick", () => {
    const ed = mount("xyz\n\n```\nabc\n```");
    // From the start of "xyz" to after "a" in the code block.
    ed.commands.setTextSelection({ from: 1, to: 7 });
    expect(press(ed)).toBe(true);
    const block = ed.state.doc.firstChild;
    expect(block?.type.name).toBe("codeBlock");
    expect(block?.textContent).toBe("`bc");
    expect(ed.state.storedMarks).toBeNull();
  });
});

describe("text typed between two literal backticks becomes code (EDT-FR-EKEZ)", () => {
  it("EDT-FR-EKEZ: a character between `` becomes code, the backticks go, the span stays open", () => {
    const ed = mount("a");
    cursorAtEnd(ed);
    typeText(ed, " ");
    // Two presses give two literal backticks (EDT-FR-XTGK).
    press(ed);
    press(ed);
    expect(ed.state.doc.textContent).toBe("a ``");
    ed.commands.setTextSelection(ed.state.selection.from - 1);
    typeText(ed, "x");
    expect(ed.state.doc.textContent).toBe("a x");
    expect(codeMarkedText(ed)).toBe("x");
    expect(ghost(ed)).not.toBeNull();
    typeText(ed, "yz");
    expect(codeMarkedText(ed)).toBe("xyz");
  });

  it("EDT-FR-EKEZ: a pair from the Markdown source works the same way", () => {
    const ed = mount("a \\`\\` b");
    expect(ed.state.doc.textContent).toBe("a `` b");
    ed.commands.setTextSelection(4);
    typeText(ed, "q");
    expect(ed.state.doc.textContent).toBe("a q b");
    expect(codeMarkedText(ed)).toBe("q");
  });

  it("EDT-FR-EKEZ: a backtick that the platform types between `` is literal", () => {
    const ed = mount("a \\`\\` b");
    ed.commands.setTextSelection(4);
    typeText(ed, "`");
    expect(ed.state.doc.textContent).toBe("a ``` b");
    expect(codeMarkedText(ed)).toBe("");
  });

  it("EDT-FR-EKEZ: text of several characters between `` becomes code as one", () => {
    const ed = mount("a \\`\\` b");
    ed.commands.setTextSelection(4);
    const view = ed.view;
    const handled = view.someProp("handleTextInput", (handle) =>
      handle(view, 4, 4, "qr", () => view.state.tr.insertText("qr", 4, 4)),
    );
    expect(handled).toBe(true);
    expect(ed.state.doc.textContent).toBe("a qr b");
    expect(codeMarkedText(ed)).toBe("qr");
    expect(ed.state.selection.from).toBe(5);
    expect(ghost(ed)).not.toBeNull();
  });

  it.each([
    ["three backticks on the left", "a ```|` b"],
    ["two backticks on the right", "a `|`` b"],
    ["no backtick on the right", "a `| b"],
  ])("EDT-FR-EKEZ: no code with %s", (_name, layout) => {
    const text = layout.replace("|", "");
    const ed = mount(text.replace(/`/g, "\\`"));
    ed.commands.setTextSelection(1 + layout.indexOf("|"));
    typeText(ed, "q");
    expect(codeMarkedText(ed)).toBe("");
    expect(ed.state.doc.textContent).toBe(layout.replace("|", "q"));
  });
});
