/**
 * EDT-FR-FDGH, EDT-FR-XTGK, EDT-FR-FKJP: the surface acts on the backtick key
 * itself, literal backticks still go in where a literal one is meant, and the
 * code-fence and inline-code input rules still apply.
 *
 * The open, close, wrap and pair behavior of inline code is in
 * `markdownInlineCode.test.ts`.
 */
import { afterEach, describe, expect, it } from "vitest";
import { NodeSelection, TextSelection } from "@tiptap/pm/state";
import { CellSelection } from "@tiptap/pm/tables";
import { getMarkdown } from "./markdownFidelity";
import {
  codeMarkedText,
  cursorAtEnd,
  destroyRichEditor,
  ghost,
  mountRichEditor as mount,
  press,
  pressAsPlatform,
  pressBackticks,
  typeText,
  type KeyOptions,
} from "../test/richEditorFixtures";

afterEach(() => {
  destroyRichEditor();
});

describe("the surface acts on the backtick key (EDT-FR-FDGH)", () => {
  it("EDT-FR-FDGH: a backtick press is taken from the platform", () => {
    const ed = mount("a");
    cursorAtEnd(ed);
    expect(press(ed)).toBe(true);
  });

  it("EDT-FR-FDGH, EDT-FR-XTGK: a platform that types two backticks per press still opens a code block", () => {
    const ed = mount("");
    for (let i = 0; i < 3; i += 1) pressAsPlatform(ed, "``");
    pressAsPlatform(ed, " ", { key: " " });
    expect(ed.state.doc.firstChild?.type.name).toBe("codeBlock");
  });

  it.each<[string, KeyOptions]>([
    ["with Shift", { shiftKey: true, code: "IntlBackslash" }],
    ["with Option", { altKey: true }],
    ["with AltGr (Control and Alt)", { ctrlKey: true, altKey: true }],
  ])("EDT-FR-FDGH: a backtick a layout types %s is taken", (_name, options) => {
    const ed = mount("a``");
    cursorAtEnd(ed);
    expect(press(ed, options)).toBe(true);
    expect(ed.state.doc.textContent).toBe("a```");
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
    expect(ed.state.storedMarks).toBeNull();
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

describe("literal backticks and the code fence (EDT-FR-XTGK)", () => {
  it("EDT-FR-XTGK: a backtick in an empty open span gives two literal backticks and no span", () => {
    const ed = mount("");
    pressBackticks(ed, 2);
    expect(ed.state.doc.textContent).toBe("``");
    expect(codeMarkedText(ed)).toBe("");
    typeText(ed, "x");
    expect(codeMarkedText(ed)).toBe("");
  });

  it("EDT-FR-XTGK: the third backtick is literal and opens no span", () => {
    const ed = mount("");
    pressBackticks(ed, 3);
    expect(ed.state.doc.textContent).toBe("```");
    expect(ed.state.storedMarks?.some((m) => m.type.name === "code") ?? false).toBe(false);
    expect(ghost(ed)).toBeNull();
  });

  it("EDT-FR-XTGK: ``` that is not at the start of a paragraph stays literal", () => {
    const ed = mount("a");
    cursorAtEnd(ed);
    typeText(ed, " ");
    pressBackticks(ed, 3);
    typeText(ed, " ");
    expect(ed.state.doc.firstChild?.type.name).toBe("paragraph");
    expect(ed.state.doc.textContent).toBe("a ``` ");
  });

  it("EDT-FR-XTGK: a backtick right after a literal backtick is literal", () => {
    const ed = mount("");
    pressBackticks(ed, 4);
    expect(ed.state.doc.textContent).toBe("````");
    expect(ed.state.doc.firstChild?.type.name).toBe("paragraph");
  });

  it("EDT-FR-XTGK: ``` then Space opens a code block", () => {
    const ed = mount("");
    pressBackticks(ed, 3);
    typeText(ed, " ");
    const block = ed.state.doc.firstChild;
    expect(block?.type.name).toBe("codeBlock");
    expect(block?.textContent).toBe("");
  });

  it("EDT-FR-XTGK: ``` then Enter opens a code block", () => {
    const ed = mount("");
    pressBackticks(ed, 3);
    press(ed, { key: "Enter" });
    expect(ed.state.doc.firstChild?.type.name).toBe("codeBlock");
  });

  it.each([" ", "Enter"])(
    "EDT-FR-XTGK: ```lang then %s opens a code block with that language",
    (finish) => {
      const ed = mount("");
      pressBackticks(ed, 3);
      typeText(ed, "ts");
      if (finish === "Enter") press(ed, { key: "Enter" });
      else typeText(ed, finish);
      const block = ed.state.doc.firstChild;
      expect(block?.type.name).toBe("codeBlock");
      expect(block?.attrs.language).toBe("ts");
    },
  );

  it("EDT-FR-XTGK: the opened code block serialises as a fenced block", () => {
    const ed = mount("");
    pressBackticks(ed, 3);
    press(ed, { key: "Enter" });
    typeText(ed, "let a");
    expect(getMarkdown(ed)).toBe("```\nlet a\n```");
  });
});

describe("a literal backtick in code, and the inline-code rule (EDT-FR-FKJP)", () => {
  it("EDT-FR-FKJP: in a code block, each press is one literal backtick", () => {
    const ed = mount("```\nx\n```");
    cursorAtEnd(ed);
    pressBackticks(ed, 3);
    typeText(ed, " ");
    const block = ed.state.doc.firstChild;
    expect(block?.type.name).toBe("codeBlock");
    expect(block?.textContent).toBe("x``` ");
  });

  it("EDT-FR-FKJP: a selection in a code block is replaced by one literal backtick", () => {
    const ed = mount("```\nabc\n```");
    ed.commands.setTextSelection({ from: 2, to: 3 });
    expect(press(ed)).toBe(true);
    expect(ed.state.doc.firstChild?.textContent).toBe("a`c");
  });

  it("EDT-FR-FKJP: inside an inline-code span, before its end, a backtick is literal code", () => {
    const ed = mount("a `bc` d");
    // Between "b" and "c", inside the code mark.
    ed.view.dispatch(
      ed.state.tr.setSelection(TextSelection.create(ed.state.doc, 4)),
    );
    expect(press(ed)).toBe(true);
    expect(ed.state.doc.textContent).toBe("a b`c d");
    expect(codeMarkedText(ed)).toBe("b`c");
  });

  it("EDT-FR-FKJP: a backtick after a literal opening backtick and text closes them into inline code", () => {
    const ed = mount("x \\`foo");
    cursorAtEnd(ed);
    expect(ed.state.doc.textContent).toBe("x `foo");
    press(ed);
    expect(codeMarkedText(ed)).toBe("foo");
    expect(ed.state.doc.textContent).toBe("x foo");
    typeText(ed, "z");
    expect(codeMarkedText(ed)).toBe("foo");
    expect(getMarkdown(ed)).toBe("x `foo`z");
  });
});
