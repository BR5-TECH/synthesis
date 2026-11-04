/**
 * HTML comments on the WYSIWYG surface: the parse into breaks, alignment, and
 * chips, the serialisation back, the rendered elements, the toolbar commands,
 * and the chip tooltip. These drive the extension set directly; the toolbar
 * itself is in `Editor.blockMenus.test.tsx`.
 */
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { Editor } from "@tiptap/react";
import { NodeSelection, TextSelection } from "@tiptap/pm/state";
import type { Node as PMNode } from "@tiptap/pm/model";
import { getMarkdown, markdownExtensions } from "./markdownFidelity";
import { setBlockAlignment, setVerticalBreak } from "./markdownMarkup";
import { press } from "../test/richEditorFixtures";
import { resetLogBufferForTest } from "../logging";
import { blocksFor, decl, sheet } from "../test/cssRules";

let editors: Editor[] = [];

beforeEach(() => {
  resetLogBufferForTest();
});

function mount(content: string, editable = true): Editor {
  const element = document.createElement("div");
  document.body.appendChild(element);
  const editor = new Editor({ element, extensions: markdownExtensions(), content, editable });
  editors.push(editor);
  return editor;
}

afterEach(() => {
  for (const editor of editors) {
    const element = editor.view.dom.parentElement;
    editor.destroy();
    element?.remove();
  }
  editors = [];
  document.body.innerHTML = "";
});

function roundTrip(src: string): string {
  const editor = new Editor({ extensions: markdownExtensions(), content: src });
  const out = getMarkdown(editor);
  editor.destroy();
  return out;
}

/** The top-level nodes, as type names with the attributes that matter here. */
function outline(src: string): string[] {
  const editor = new Editor({ extensions: markdownExtensions(), content: src });
  const out: string[] = [];
  editor.state.doc.forEach((node) => {
    const name = node.type.name;
    if (name === "verticalBreak") out.push(`break:${node.attrs.level}`);
    else if (name === "htmlComment") out.push(`comment:${node.attrs.raw}`);
    else out.push(node.attrs.align ? `${name}@${node.attrs.align}` : name);
  });
  editor.destroy();
  return out;
}

/** The position of the first top-level node of `type`, and the node. */
function find(editor: Editor, test: (node: PMNode) => boolean): { pos: number; node: PMNode } {
  let found: { pos: number; node: PMNode } | null = null;
  editor.state.doc.descendants((node, pos) => {
    if (!found && test(node)) found = { pos, node };
    return !found;
  });
  if (!found) throw new Error("node not found");
  return found;
}

function caretIn(editor: Editor, text: string): void {
  const { pos } = find(editor, (node) => node.isTextblock && node.textContent === text);
  editor.view.dispatch(editor.state.tr.setSelection(TextSelection.create(editor.state.doc, pos + 1)));
}

function selectNode(editor: Editor, test: (node: PMNode) => boolean): void {
  const { pos } = find(editor, test);
  editor.view.dispatch(editor.state.tr.setSelection(NodeSelection.create(editor.state.doc, pos)));
}

describe("parse (EDT-FR-QALB, EDT-FR-GJWR, EDT-FR-XPMC, EDT-FR-ANTD, EDT-FR-UYPS)", () => {
  it("EDT-FR-QALB: a break line is a spacer of its level, and five or more % is V4", () => {
    expect(outline("<!-- % -->\n# A")).toEqual(["break:1", "heading"]);
    expect(outline("<!-- %%%% -->\nA")).toEqual(["break:4", "paragraph"]);
    expect(outline("<!-- %%%%%%% -->\nA")).toEqual(["break:4", "paragraph"]);
  });

  it("EDT-FR-QALB: a malformed or multi-line % comment is an ordinary comment", () => {
    expect(outline("<!-- %x -->\nA")).toEqual(["comment:<!-- %x -->", "paragraph"]);
    expect(outline("<!--\n%%\n-->\n\nA")).toEqual(["comment:<!--\n%%\n-->", "paragraph"]);
  });

  it("EDT-FR-GJWR: a break line is a break above a blank line, above a comment, and at the end of the file", () => {
    expect(outline("A\n\n<!-- %% -->\n\nB")).toEqual(["paragraph", "break:2", "paragraph"]);
    expect(outline("<!-- %% -->\n<!-- note -->")).toEqual(["break:2", "comment:<!-- note -->"]);
    expect(outline("A\n\n<!-- %%% -->")).toEqual(["paragraph", "break:3"]);
    expect(outline("A\n<!-- %%% -->\nB")).toEqual(["paragraph", "break:3", "paragraph"]);
  });

  it("EDT-FR-XPMC: a marker aligns the block on the very next line, and only that block", () => {
    expect(outline("<!-- center -->\n# A\n\nB")).toEqual(["heading@center", "paragraph"]);
    expect(outline("<!-- right -->\nA\n\nB")).toEqual(["paragraph@right", "paragraph"]);
    expect(outline("<!--  center  -->\nA")).toEqual(["paragraph@center"]);
  });

  it("EDT-FR-XPMC, EDT-FR-GVJM: every top-level block type can be aligned", () => {
    expect(
      outline(
        [
          "<!-- center -->\n- a\n- b",
          "<!-- right -->\n1. a",
          "<!-- center -->\n- [ ] task",
          "<!-- right -->\n> quote",
          "<!-- center -->\n```js\nx\n```",
          "<!-- center -->\n```mermaid\npie\n```",
          "<!-- right -->\n| a |\n| --- |\n| 1 |",
          "<!-- center -->\n![pic](p.png)",
        ].join("\n\n"),
      ),
    ).toEqual([
      "bulletList@center",
      "orderedList@right",
      "taskList@center",
      "blockquote@right",
      "codeBlock@center",
      "codeBlock@center",
      "table@right",
      "paragraph@center",
    ]);
  });

  it("EDT-FR-ANTD: a marker before a blank line, a comment, or the end of the file is an ordinary comment", () => {
    expect(outline("<!-- center -->\n\nA")).toEqual(["comment:<!-- center -->", "paragraph"]);
    expect(outline("<!-- right -->\n<!-- note -->\nA")).toEqual([
      "comment:<!-- right -->",
      "comment:<!-- note -->",
      "paragraph",
    ]);
    expect(outline("A\n\n<!-- center -->")).toEqual(["paragraph", "comment:<!-- center -->"]);
  });

  it("EDT-FR-ANTD: a break or a marker inside a list item or a quote is an ordinary comment", () => {
    const editor = new Editor({
      extensions: markdownExtensions(),
      content: "- item\n\n  <!-- %% -->\n  <!-- center -->\n  more\n\n> <!-- %% -->\n> q",
    });
    const names: string[] = [];
    editor.state.doc.descendants((node) => {
      names.push(node.type.name);
    });
    editor.destroy();
    expect(names).not.toContain("verticalBreak");
    expect(names.filter((name) => name === "htmlComment")).toHaveLength(3);
  });

  it("EDT-FR-UYPS: a comment inside a paragraph is an inline chip, and one inside code is code text", () => {
    const editor = new Editor({
      extensions: markdownExtensions(),
      content: "Before <!-- a note --> after, and `<!-- code -->`.",
    });
    const inline: string[] = [];
    editor.state.doc.descendants((node) => {
      if (node.type.name === "htmlCommentInline") inline.push(node.attrs.raw);
    });
    expect(inline).toEqual(["<!-- a note -->"]);
    expect(editor.state.doc.textContent).toContain("<!-- code -->");
    editor.destroy();
  });

  it("EDT-FR-UYPS: a multi-line comment, blank lines included, is one chip", () => {
    expect(outline("<!--\n  one\n\n  two\n-->\n\nA")).toEqual([
      "comment:<!--\n  one\n\n  two\n-->",
      "paragraph",
    ]);
  });
});

describe("serialisation (EDT-FR-68, EDT-FR-OZPO, EDT-FR-66, EDT-FR-69)", () => {
  it.each([
    ["a break above a heading", "<!-- %% -->\n# Title"],
    ["a break at the end", "Text\n\n<!-- %%%% -->\n"],
    ["a break then a marker then a block", "<!-- %%% -->\n<!-- right -->\nPara"],
    ["an aligned table", "<!-- center -->\n| a | b |\n| --- | --- |\n| 1 | 2 |\n"],
    ["an aligned list", "<!-- right -->\n- a\n- b"],
    ["an aligned fence", "<!-- center -->\n```mermaid\npie\n```"],
    ["an inline comment", "Text <!-- note --> more"],
    ["a multi-line comment", "<!--\n  multi\n\n  line\n-->\n\nPara"],
    ["a marker that is a comment", "<!-- center -->\n\nPara"],
    ["a marker spelled with extra spaces", "<!--   center   -->\nPara"],
    ["six %", "<!-- %%%%%% -->\nPara"],
    ["a tag-shaped prose beside a comment", "<artifact> and <!-- c -->"],
    ["Unicode in a comment", "<!-- заметка — ünïcödé ✓ -->\n\nPara"],
  ])("EDT-FR-68, EDT-FR-OZPO: %s round-trips unchanged", (_name, src) => {
    expect(roundTrip(src)).toBe(src);
  });

  it("EDT-FR-69: a break that interrupts a paragraph settles on one spelling and stays there", () => {
    const once = roundTrip("Para\n<!-- % -->\nNext");
    expect(once).toBe("Para\n\n<!-- % -->\nNext");
    expect(roundTrip(once)).toBe(once);
  });

  it("EDT-FR-68: every comment of the input reaches the output", () => {
    const src = "<!-- a -->\n\n# T <!-- b -->\n\n- x <!-- c -->\n\n> q <!-- d -->\n\n| h <!-- e --> |\n| --- |\n| v |\n";
    const out = roundTrip(src);
    for (const raw of ["<!-- a -->", "<!-- b -->", "<!-- c -->", "<!-- d -->", "<!-- e -->"]) {
      expect(out).toContain(raw);
    }
    expect(roundTrip(out)).toBe(out);
  });
});

describe("edges of the grammar (EDT-FR-ANTD, EDT-FR-OZPO, EDT-FR-67)", () => {
  it("EDT-FR-ANTD, EDT-FR-68: a marker above a line that makes no block is a comment, and nothing is lost", () => {
    const src = "<!-- center -->\n[ref]: https://example.com\n\nPara [ref]";
    expect(outline(src)[0]).toBe("comment:<!-- center -->");
    const out = roundTrip(src);
    expect(out).toContain("<!-- center -->");
    expect(roundTrip(out)).toBe(out);
  });

  it("EDT-FR-OZPO: a marker the author did not change keeps its spelling after an edit elsewhere", () => {
    const editor = mount("<!--   center   -->\nA\n\n<!--%%%-->\nB");
    editor.commands.insertContentAt(editor.state.doc.content.size - 1, " more");
    expect(getMarkdown(editor)).toBe("<!--   center   -->\nA\n\n<!--%%%-->\nB more");
  });

  it("EDT-FR-UYPS, EDT-FR-QALB: marker and comment lines inside a fence are code", () => {
    const src = "```md\n<!-- % -->\n<!-- center -->\n<!-- note -->\n```";
    expect(outline(src)).toEqual(["codeBlock"]);
    expect(roundTrip(src)).toBe(src);
  });

  it("EDT-FR-UYPS, EDT-FR-KMAR: a comment closed by --!> ends there, and the text after it is prose", () => {
    const editor = new Editor({
      extensions: markdownExtensions(),
      content: "A <!-- x --!> shown -->\n\n<!-- block --!>\n\nB",
    });
    const raws: string[] = [];
    editor.state.doc.descendants((node) => {
      if (node.type.name.startsWith("htmlComment")) raws.push(node.attrs.raw);
    });
    expect(raws).toEqual(["<!-- x --!>", "<!-- block --!>"]);
    expect(editor.state.doc.textContent).toContain(" shown -->");
    editor.destroy();
    const src = "A <!-- x --!> shown -->\n\n<!-- block --!>\n\nB";
    expect(roundTrip(src)).toBe(src);
  });

  it("EDT-FR-OZPO: an indented marker keeps its indent", () => {
    expect(roundTrip("   <!-- %% -->\nA")).toBe("   <!-- %% -->\nA");
    expect(roundTrip("  <!-- right -->\nA")).toBe("  <!-- right -->\nA");
  });

  it("EDT-FR-68: a comment in a table cell that holds a pipe keeps the cell", () => {
    const src = "| a <!-- x \\| y --> | b |\n| --- | --- |\n| 1 | 2 |\n";
    expect(roundTrip(src)).toBe(src);
  });

  it("EDT-FR-UYPS, EDT-FR-68: a comment inside a tight list keeps the list tight", () => {
    expect(roundTrip("- a\n  <!-- c -->\n- b")).toBe("- a\n  <!-- c -->\n- b");
  });

  it("EDT-FR-67: escaped prose that looks like a comment stays prose", () => {
    expect(outline("\\<!-- not a comment -->")).toEqual(["paragraph"]);
    expect(roundTrip("\\<!-- not a comment -->")).toBe("\\<!-- not a comment -->");
  });

  it("EDT-FR-67: prose typed as `<!--` is written escaped, so it reads back as prose", () => {
    const editor = mount("Text");
    editor.commands.insertContentAt(editor.state.doc.content.size - 1, {
      type: "text",
      text: " <!-- typed -->",
    });
    const out = getMarkdown(editor);
    expect(out).toBe("Text \\<!-- typed -->");
    expect(outline(out)).toEqual(["paragraph"]);
  });

  it("EDT-FR-XPMC: Enter at the end of an aligned block starts a left-aligned block", () => {
    const editor = mount("<!-- center -->\nTitle");
    editor.commands.setTextSelection(editor.state.doc.content.size - 1);
    editor.commands.splitBlock();
    editor.commands.insertContent("Body");
    expect(getMarkdown(editor)).toBe("<!-- center -->\nTitle\n\nBody");
  });

  it("EDT-FR-69: a break directly above a marker, and a break above a comment, are stable over two passes", () => {
    for (const src of ["<!-- % -->\n<!-- center -->\nA", "<!-- % -->\n<!-- note -->\n\nA"]) {
      const once = roundTrip(src);
      expect(roundTrip(once)).toBe(once);
      expect(once).toBe(src);
    }
  });
});

describe("rendering (EDT-FR-CSKH, EDT-FR-XKTS, EDT-FR-UYPS, EDT-FR-GVJM, EDT-FR-KMAR)", () => {
  it("EDT-FR-CSKH, EDT-FR-XKTS: a spacer is an empty separator named for its level", () => {
    const editor = mount("<!-- %%% -->\nText");
    const spacer = editor.view.dom.querySelector(".md-break") as HTMLElement;
    expect(spacer).toHaveAttribute("role", "separator");
    expect(spacer).toHaveAttribute("aria-label", "Vertical break V3");
    expect(spacer).toHaveAttribute("data-level", "3");
    expect(spacer.style.getPropertyValue("--md-break-lines").trim()).toBe("5");
    expect(spacer.textContent).toBe("");
  });

  it("EDT-FR-UYPS: a chip shows no comment text and has the name HTML comment", () => {
    const editor = mount("<!-- block -->\n\nText <!-- inline --> more");
    const chips = editor.view.dom.querySelectorAll("[data-md-chip]");
    expect(chips).toHaveLength(2);
    for (const chip of chips) {
      expect(chip).toHaveAttribute("role", "img");
      expect(chip).toHaveAttribute("aria-label", "HTML comment");
      // The raw text is there for selection and copy, visually hidden.
      expect(chip.querySelector(".md-chip__raw")).not.toBeNull();
    }
  });

  it("EDT-FR-GVJM: an aligned block carries its alignment, and a marker shows nothing", () => {
    const editor = mount("<!-- center -->\n# Title\n\n<!-- right -->\n| a |\n| --- |\n| 1 |");
    expect(editor.view.dom.querySelector("h1")).toHaveAttribute("data-align", "center");
    expect(editor.view.dom.querySelector("table")).toHaveAttribute("data-align", "right");
    expect(editor.view.dom.textContent).not.toContain("center");
  });

  it("EDT-FR-KMAR: comment text never becomes an element", () => {
    const editor = mount('<!-- <img src="x" onerror="alert(1)"><script>bad()</script> -->\n\nA <!-- <b>bold</b> --> z');
    expect(editor.view.dom.querySelector("img")).toBeNull();
    expect(editor.view.dom.querySelector("script")).toBeNull();
    expect(editor.view.dom.querySelector("b")).toBeNull();
    expect(getMarkdown(editor)).toContain('<img src="x" onerror="alert(1)">');
  });
});

describe("commands (EDT-FR-UXCK, EDT-FR-MAVJ, EDT-FR-XKTS, EDT-FR-FXAG)", () => {
  it("EDT-FR-UXCK, EDT-FR-OZPO: a break goes on its own line directly above the caret block", () => {
    const editor = mount("# Title\n\nBody");
    caretIn(editor, "Title");
    expect(setVerticalBreak(editor, 1)).toBe(true);
    expect(getMarkdown(editor)).toBe("<!-- % -->\n# Title\n\nBody");
  });

  it("EDT-FR-UXCK: over several blocks, one break goes above the first", () => {
    const editor = mount("One\n\nTwo\n\nThree");
    const one = find(editor, (n) => n.isTextblock && n.textContent === "One").pos;
    const three = find(editor, (n) => n.isTextblock && n.textContent === "Three").pos;
    editor.view.dispatch(
      editor.state.tr.setSelection(TextSelection.create(editor.state.doc, one + 2, three + 2)),
    );
    setVerticalBreak(editor, 2);
    expect(getMarkdown(editor)).toBe("<!-- %% -->\nOne\n\nTwo\n\nThree");
  });

  it("EDT-FR-UXCK: above an aligned block, the break goes above its marker", () => {
    const editor = mount("<!-- center -->\nTitle");
    caretIn(editor, "Title");
    setVerticalBreak(editor, 4);
    expect(getMarkdown(editor)).toBe("<!-- %%%% -->\n<!-- center -->\nTitle");
  });

  it("EDT-FR-UXCK, EDT-FR-OZPO: with a spacer selected, a choice sets its level and inserts nothing", () => {
    const editor = mount("<!--%%-->\nText");
    selectNode(editor, (n) => n.type.name === "verticalBreak");
    setVerticalBreak(editor, 3);
    expect(getMarkdown(editor)).toBe("<!-- %%% -->\nText");
    expect(editor.state.selection).toBeInstanceOf(NodeSelection);
  });

  it("EDT-FR-UXCK: a caret inside a list item puts the break above the list", () => {
    const editor = mount("Intro\n\n- a\n- b");
    caretIn(editor, "b");
    setVerticalBreak(editor, 2);
    expect(getMarkdown(editor)).toBe("Intro\n\n<!-- %% -->\n- a\n- b");
  });

  it("EDT-FR-MAVJ, EDT-FR-OZPO: Center and Right write one marker above each touched block, and replace one there", () => {
    const editor = mount("<!--   right   -->\nOne\n\nTwo\n\nThree");
    const one = find(editor, (n) => n.isTextblock && n.textContent === "One").pos;
    const two = find(editor, (n) => n.isTextblock && n.textContent === "Two").pos;
    editor.view.dispatch(
      editor.state.tr.setSelection(TextSelection.create(editor.state.doc, one + 1, two + 2)),
    );
    setBlockAlignment(editor, "center");
    expect(getMarkdown(editor)).toBe("<!-- center -->\nOne\n\n<!-- center -->\nTwo\n\nThree");
  });

  it("EDT-FR-MAVJ: Left removes the marker", () => {
    const editor = mount("<!-- center -->\nOne");
    caretIn(editor, "One");
    expect(setBlockAlignment(editor, null)).toBe(true);
    expect(getMarkdown(editor)).toBe("One");
  });

  it("EDT-FR-MAVJ: a block inside a list item aligns its top-level list", () => {
    const editor = mount("- a\n  - nested");
    caretIn(editor, "nested");
    setBlockAlignment(editor, "right");
    expect(getMarkdown(editor)).toBe("<!-- right -->\n- a\n  - nested");
  });

  it("EDT-FR-MAVJ: an alignment over a spacer only changes nothing", () => {
    const editor = mount("<!-- % -->\nText");
    selectNode(editor, (n) => n.type.name === "verticalBreak");
    expect(setBlockAlignment(editor, "center")).toBe(false);
    expect(getMarkdown(editor)).toBe("<!-- % -->\nText");
  });

  it("EDT-FR-XKTS: Backspace and Delete with a spacer selected remove its line", () => {
    for (const key of ["Backspace", "Delete"]) {
      const editor = mount("A\n\n<!-- %% -->\nB");
      selectNode(editor, (n) => n.type.name === "verticalBreak");
      press(editor, { key });
      expect(getMarkdown(editor)).toBe("A\n\nB");
    }
  });

  it("EDT-FR-FXAG: Backspace and Delete with a chip selected remove the whole comment", () => {
    for (const key of ["Backspace", "Delete"]) {
      const editor = mount("A <!-- x --> B\n\n<!--\nmulti\n-->\n\nC");
      selectNode(editor, (n) => n.type.name === "htmlComment");
      press(editor, { key });
      selectNode(editor, (n) => n.type.name === "htmlCommentInline");
      press(editor, { key });
      expect(getMarkdown(editor)).toBe("A  B\n\nC");
    }
  });

  it("EDT-FR-FXAG: a step forward from the block before a chip selects the chip as one unit", () => {
    // The arrow keys of ProseMirror need a layout that jsdom does not have, so
    // this drives the command the arrow key runs at the end of a textblock.
    // The browser verification checks the key itself.
    const editor = mount("A\n\n<!-- note -->\n\nB");
    editor.view.dispatch(editor.state.tr.setSelection(TextSelection.create(editor.state.doc, 2)));
    expect(editor.commands.selectNodeForward()).toBe(true);
    expect(editor.state.selection).toBeInstanceOf(NodeSelection);
    expect((editor.state.selection as NodeSelection).node.type.name).toBe("htmlComment");
  });

  it("EDT-FR-PCCA: a surface that accepts no input renders the markup and removes nothing", () => {
    const editor = mount("<!-- %% -->\n<!-- center -->\nA <!-- x -->", false);
    expect(editor.view.dom.querySelector(".md-break")).not.toBeNull();
    expect(editor.view.dom.querySelector("p")).toHaveAttribute("data-align", "center");
    expect(editor.view.dom.querySelector("[data-md-chip]")).not.toBeNull();
    caretIn(editor, "A ");
    expect(setVerticalBreak(editor, 1)).toBe(false);
    expect(setBlockAlignment(editor, "right")).toBe(false);
    // EDT-FR-IGZL: the chip keeps its tooltip there.
    const chip = editor.view.dom.querySelector("[data-md-chip]") as HTMLElement;
    chip.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    expect((document.querySelector(".md-chip-tooltip") as HTMLElement).hidden).toBe(false);
  });
});

describe("chip tooltip (EDT-FR-IGZL)", () => {
  const bubble = () => document.querySelector(".md-chip-tooltip") as HTMLElement;

  it("EDT-FR-IGZL: a selected chip shows its text as plain text, and Escape closes it", () => {
    const editor = mount("A\n\n<!--\n  first <b>line</b>\n  second\n-->\n\nB");
    expect(bubble()).toHaveAttribute("role", "tooltip");
    expect(bubble().hidden).toBe(true);
    editor.view.dom.focus();
    selectNode(editor, (n) => n.type.name === "htmlComment");
    expect(bubble().hidden).toBe(false);
    expect(bubble().textContent).toBe("first <b>line</b>\nsecond");
    expect(bubble().querySelector("b")).toBeNull();

    // Escape that closes the tooltip is consumed, so it closes nothing else.
    expect(press(editor, { key: "Escape" })).toBe(true);
    expect(bubble().hidden).toBe(true);
    expect(press(editor, { key: "Escape" })).toBe(false);
  });

  it("EDT-FR-IGZL: the tooltip closes when the selection moves and when the surface loses focus", async () => {
    const editor = mount("A\n\n<!-- note -->\n\nB");
    editor.view.dom.focus();
    selectNode(editor, (n) => n.type.name === "htmlComment");
    expect(bubble().hidden).toBe(false);
    caretIn(editor, "B");
    expect(bubble().hidden).toBe(true);

    selectNode(editor, (n) => n.type.name === "htmlComment");
    expect(bubble().hidden).toBe(false);
    // The tooltip reads the focus after the focus event, in a microtask.
    editor.view.dom.blur();
    await Promise.resolve();
    expect(bubble().hidden).toBe(true);
    editor.view.dom.focus();
    await Promise.resolve();
    expect(bubble().hidden).toBe(false);
  });

  it("EDT-FR-IGZL: a chip selected on a surface without focus shows no tooltip", () => {
    // An editable run of a Diff tab that holds only a chip starts with it selected.
    const editor = mount("<!-- only -->");
    selectNode(editor, (n) => n.type.name === "htmlComment");
    expect(bubble().hidden).toBe(true);
  });

  it("EDT-FR-IGZL: hover on a chip shows the tooltip, and leaving the surface hides it", () => {
    const editor = mount("Text <!-- hover me --> end");
    const chip = editor.view.dom.querySelector("[data-md-chip]") as HTMLElement;
    chip.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    expect(bubble().hidden).toBe(false);
    expect(bubble().textContent).toBe("hover me");
    // The pointer moves on to the text beside the chip.
    editor.view.dom.querySelector("p")!.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    expect(bubble().hidden).toBe(true);
    chip.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    editor.view.dom.dispatchEvent(new MouseEvent("mouseleave"));
    expect(bubble().hidden).toBe(true);
    expect(document.activeElement).not.toBe(bubble());
  });

  it("EDT-FR-IGZL: the tooltip goes away with its surface", () => {
    const editor = mount("Text <!-- x -->");
    expect(document.querySelectorAll(".md-chip-tooltip")).toHaveLength(1);
    editor.destroy();
    editors = [];
    expect(document.querySelectorAll(".md-chip-tooltip")).toHaveLength(0);
  });
});

describe("styles (EDT-FR-CSKH, EDT-FR-GVJM)", () => {
  const css = sheet("components.css");

  it("EDT-FR-CSKH: a spacer is its level's count of body line heights", () => {
    const [base] = blocksFor(css, ".doc .md-break");
    expect(decl(base, "height")).toBe(
      "calc(var(--md-break-lines, 3) * var(--fs-doc-md) * var(--font-rich-line-height))",
    );
    const lines = [1, 2, 3, 4].map((level) =>
      decl(blocksFor(css, `.doc .md-break[data-level="${level}"]`)[0], "--md-break-lines"),
    );
    expect(lines).toEqual(["12", "8", "5", "3"]);
  });

  it("EDT-FR-GVJM: text blocks align their text, and a table or a code block moves as a box", () => {
    expect(decl(blocksFor(css, '.doc [data-align="center"]')[0], "text-align")).toBe("center");
    expect(decl(blocksFor(css, '.doc [data-align="right"]')[0], "text-align")).toBe("right");
    const [box] = blocksFor(css, ".doc pre[data-align]");
    expect(blocksFor(css, ".doc table[data-align]")[0]).toBe(box);
    expect(decl(box, "width")).toBe("fit-content");
    expect(decl(box, "text-align")).toBe("left");
    const [center] = blocksFor(css, '.doc pre[data-align="center"]');
    expect([decl(center, "margin-left"), decl(center, "margin-right")]).toEqual(["auto", "auto"]);
    const [right] = blocksFor(css, '.doc pre[data-align="right"]');
    expect([decl(right, "margin-left"), decl(right, "margin-right")]).toEqual(["auto", "0"]);
  });
});
