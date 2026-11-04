import { describe, expect, it } from "vitest";

import { parseInline, parseMarkdownBlocks } from "./markdown";

const kinds = (text: string) => parseMarkdownBlocks(text).map((b) => b.kind);

describe("parseMarkdownBlocks (DFV-FR-19)", () => {
  it("splits a list into one block per item, not one block per list", () => {
    // The granularity is what change marking is applied at: a whole list would
    // mark every item when one word of one item changed.
    expect(kinds("- one\n- two\n- three\n")).toEqual([
      "list-item",
      "list-item",
      "list-item",
    ]);
  });

  it("splits a table into one block per row", () => {
    const blocks = parseMarkdownBlocks("| a | b |\n|---|---|\n| 1 | 2 |\n");
    expect(blocks.map((b) => b.kind)).toEqual([
      "table-row",
      "table-row",
      "table-row",
    ]);
    expect(blocks[0].cells).toEqual(["a", "b"]);
    // The `|---|---|` row is structure rather than content.
    expect(blocks[1].isDivider).toBe(true);
    expect(blocks[2].isDivider).toBe(false);
  });

  it("keeps a paragraph whole across its wrapped lines", () => {
    const blocks = parseMarkdownBlocks("one line\nand its continuation\n\nnext\n");
    expect(blocks).toHaveLength(2);
    expect(blocks[0].text).toBe("one line\nand its continuation");
    expect(blocks[1].text).toBe("next");
  });

  it("reads a heading's level and strips its syntax", () => {
    const [h] = parseMarkdownBlocks("### Deep heading\n");
    expect(h.kind).toBe("heading");
    expect(h.level).toBe(3);
    expect(h.text).toBe("Deep heading");
  });

  it("keeps a fenced code block whole, syntax inside it included", () => {
    const [block] = parseMarkdownBlocks(
      "```ts\n# not a heading\n- not a list\n```\n",
    );
    expect(block.kind).toBe("code");
    expect(block.language).toBe("ts");
    expect(block.text).toBe("# not a heading\n- not a list");
  });

  it("keeps an unterminated fence from swallowing the parse into nothing", () => {
    const blocks = parseMarkdownBlocks("```\nstill open\n");
    expect(blocks).toHaveLength(1);
    expect(blocks[0].kind).toBe("code");
    expect(blocks[0].text).toBe("still open");
  });

  it("folds consecutive quote lines into one block quote", () => {
    const blocks = parseMarkdownBlocks("> first\n> second\n\nafter\n");
    expect(blocks.map((b) => b.kind)).toEqual(["quote", "paragraph"]);
    expect(blocks[0].text).toBe("first\nsecond");
  });

  it("records a list item's nesting and whether it is ordered", () => {
    const blocks = parseMarkdownBlocks("- top\n  - nested\n1. first\n");
    expect(blocks[0].depth).toBe(0);
    expect(blocks[0].ordered).toBe(false);
    expect(blocks[1].depth).toBe(1);
    expect(blocks[2].ordered).toBe(true);
    expect(blocks[2].marker).toBe("1.");
  });

  it("gives every block its literal source, which is its identity when aligning", () => {
    const blocks = parseMarkdownBlocks("# Title\n\n- item\n");
    expect(blocks.map((b) => b.source)).toEqual(["# Title", "- item"]);
  });

  it("recognises a thematic break, and does not mistake a list for one", () => {
    expect(kinds("---\n")).toEqual(["rule"]);
    expect(kinds("- item\n")).toEqual(["list-item"]);
  });

  it("has no blocks for a revision the comparison does not have (DFV-FR-15)", () => {
    expect(parseMarkdownBlocks(null)).toEqual([]);
    expect(parseMarkdownBlocks("")).toEqual([]);
    expect(parseMarkdownBlocks("\n\n  \n")).toEqual([]);
  });
});

describe("parseInline (DFV-FR-17)", () => {
  it("reads emphasis, code, and links as formatting rather than syntax", () => {
    expect(parseInline("a **bold** and *soft* and `code` word")).toEqual([
      { kind: "text", text: "a " },
      { kind: "strong", text: "bold" },
      { kind: "text", text: " and " },
      { kind: "em", text: "soft" },
      { kind: "text", text: " and " },
      { kind: "code", text: "code" },
      { kind: "text", text: " word" },
    ]);
  });

  it("reads a link's text and its target separately", () => {
    expect(parseInline("see [the spec](./DFV.md) now")).toEqual([
      { kind: "text", text: "see " },
      { kind: "link", text: "the spec", href: "./DFV.md" },
      { kind: "text", text: " now" },
    ]);
  });

  it("leaves code spans literal — their content is not Markdown", () => {
    expect(parseInline("`**not bold**`")).toEqual([
      { kind: "code", text: "**not bold**" },
    ]);
  });

  it("leaves unmatched syntax as the text it is", () => {
    expect(parseInline("2 * 3 * 4")).toEqual([{ kind: "text", text: "2 * 3 * 4" }]);
    expect(parseInline("")).toEqual([]);
  });
});

describe("backslash escapes", () => {
  /**
   * DFV-FR-17 / ADQ-FR-NUEB: a character a backslash protects is text rather
   * than syntax, and the backslash itself is not shown.
   *
   * This is the reading half of the contract the backend's `escape_markdown`
   * writes (`src-tauri/src/comments/question_submit.rs`): an option value or a
   * note carrying an asterisk must read in the comment exactly as the author
   * chose it, which needs both halves to agree.
   */
  it("DFV-FR-17: an escaped character is literal and the backslash is dropped", () => {
    expect(parseInline("use \\*bold\\* please")).toEqual([
      { kind: "text", text: "use *bold* please" },
    ]);
    expect(parseInline("a \\`code\\` span")).toEqual([
      { kind: "text", text: "a `code` span" },
    ]);
    expect(parseInline("an \\_under\\_ score")).toEqual([
      { kind: "text", text: "an _under_ score" },
    ]);
    expect(parseInline("\\[not a link\\](x)")).toEqual([
      { kind: "text", text: "[not a link](x)" },
    ]);
  });

  it("DFV-FR-17: an escaped backslash is one backslash", () => {
    expect(parseInline("C:\\\\Users\\\\demo")).toEqual([
      { kind: "text", text: "C:\\Users\\demo" },
    ]);
  });

  it("DFV-FR-17: a backslash before anything else stays as it was written", () => {
    // Not every punctuation mark Markdown defines an escape for — this renderer
    // reads emphasis, code and links and nothing else, so a Windows path pasted
    // into a comment reads as it was pasted.
    expect(parseInline("C:\\Users\\demo")).toEqual([
      { kind: "text", text: "C:\\Users\\demo" },
    ]);
    expect(parseInline("one\\. two")).toEqual([
      { kind: "text", text: "one\\. two" },
    ]);
  });

  it("DFV-FR-17: unescaped emphasis still renders as emphasis", () => {
    // The escape suppresses formatting; its absence must not.
    expect(parseInline("use *bold* please")).toEqual([
      { kind: "text", text: "use " },
      { kind: "em", text: "bold" },
      { kind: "text", text: " please" },
    ]);
  });

  it("DFV-FR-17: an escape inside a real span is restored too", () => {
    expect(parseInline("**a \\* b**")).toEqual([
      { kind: "strong", text: "a * b" },
    ]);
  });

  it("DFV-FR-17: text carrying no backslash is untouched", () => {
    expect(parseInline("plain words, with punctuation: 1.5 (a+b) ~ok~ | x")).toEqual([
      { kind: "text", text: "plain words, with punctuation: 1.5 (a+b) ~ok~ | x" },
    ]);
  });
});

describe("HTML comments as blocks (DFV-FR-SZZX, EDT-FR-QALB, EDT-FR-XPMC, EDT-FR-ANTD)", () => {
  it("DFV-FR-SZZX, EDT-FR-QALB: a break line is a block of its own with its level", () => {
    const blocks = parseMarkdownBlocks("Intro\n<!-- %%% -->\n# Title");
    expect(blocks.map((b) => b.kind)).toEqual(["paragraph", "break", "heading"]);
    expect(blocks[1]).toMatchObject({ level: 3, source: "<!-- %%% -->", line: 1 });
    expect(parseMarkdownBlocks("<!-- %%%%%% -->")[0]).toMatchObject({ kind: "break", level: 4 });
  });

  it("DFV-FR-SZZX, EDT-FR-UYPS: a comment on lines of its own is one comment block", () => {
    const blocks = parseMarkdownBlocks("<!--\n  one\n\n  two\n-->\nAfter");
    expect(blocks.map((b) => b.kind)).toEqual(["comment", "paragraph"]);
    expect(blocks[0]).toMatchObject({ text: "<!--\n  one\n\n  two\n-->", line: 0 });
    expect(blocks[1].line).toBe(5);
  });

  it("DFV-FR-SZZX, EDT-FR-XPMC: a marker is part of the block under it, source and line included", () => {
    const [block] = parseMarkdownBlocks("<!-- center -->\n# Title");
    expect(block).toMatchObject({
      kind: "heading",
      align: "center",
      alignMarker: true,
      source: "<!-- center -->\n# Title",
      line: 0,
      text: "Title",
    });
  });

  it("DFV-FR-SZZX: a changed marker line changes the identity of its block", () => {
    const [left] = parseMarkdownBlocks("<!-- center -->\nText");
    const [right] = parseMarkdownBlocks("<!-- right -->\nText");
    expect(left.source).not.toBe(right.source);
  });

  it("EDT-FR-ANTD: a marker before a blank line, a comment, or the end is a comment block", () => {
    expect(kinds("<!-- center -->\n\nText")).toEqual(["comment", "paragraph"]);
    expect(kinds("<!-- right -->\n<!-- x -->\nText")).toEqual(["comment", "comment", "paragraph"]);
    expect(kinds("Text\n\n<!-- center -->")).toEqual(["paragraph", "comment"]);
  });

  it("EDT-FR-ANTD: a comment indented to the content of a list item is part of that item", () => {
    const blocks = parseMarkdownBlocks("- item\n  <!-- %% -->\n- next\n\n  <!--\n  two\n  -->\n\n1. n\n   <!-- c -->");
    expect(blocks.map((b) => b.kind)).toEqual(["list-item", "list-item", "list-item"]);
    expect(blocks[0]).toMatchObject({ source: "- item\n  <!-- %% -->", text: "item <!-- %% -->" });
    expect(blocks[1].source).toBe("- next\n\n  <!--\n  two\n  -->");
    expect(blocks[2].text).toBe("n <!-- c -->");
  });

  it("EDT-FR-ANTD: a comment indented less than the content of the item above is top level", () => {
    expect(kinds("- a\n\n <!-- % -->\nText")).toEqual(["list-item", "break", "paragraph"]);
  });

  it("EDT-FR-GVJM: the items of an aligned list and the rows of an aligned table share its alignment", () => {
    const list = parseMarkdownBlocks("<!-- right -->\n- a\n- b\n  - c");
    expect(list.map((b) => b.align)).toEqual(["right", "right", "right"]);
    const table = parseMarkdownBlocks("<!-- center -->\n| a |\n| --- |\n| 1 |");
    expect(table.map((b) => b.align)).toEqual(["center", "center", "center"]);
  });

  it("EDT-FR-GVJM: the alignment does not reach a second table or a list of another marker", () => {
    expect(parseMarkdownBlocks("<!-- center -->\n| a |\n\n| b |").map((b) => b.align)).toEqual([
      "center",
      undefined,
    ]);
    expect(parseMarkdownBlocks("<!-- center -->\n- a\n\n* b").map((b) => b.align)).toEqual([
      "center",
      undefined,
    ]);
    expect(parseMarkdownBlocks("<!-- center -->\n- a\n\n- b").map((b) => b.align)).toEqual([
      "center",
      "center",
    ]);
  });

  it("EDT-FR-UYPS, EDT-FR-QALB: marker and comment lines inside a fence are code", () => {
    expect(kinds("```md\n<!-- % -->\n<!-- center -->\n<!-- note -->\n```\nAfter")).toEqual([
      "code",
      "paragraph",
    ]);
  });

  it("EDT-FR-UYPS, EDT-FR-KMAR: a comment closed by --!> ends there, on both surfaces", () => {
    expect(kinds("<!-- x --!>\nText")).toEqual(["comment", "paragraph"]);
    expect(parseInline("a <!-- x --!> b -->")).toEqual([
      { kind: "text", text: "a " },
      { kind: "comment", text: "", raw: "<!-- x --!>" },
      { kind: "text", text: " b -->" },
    ]);
  });

  it("EDT-FR-QALB: a CRLF line is read as its LF form", () => {
    expect(kinds("<!-- %% -->\r\nText\r\n")).toEqual(["break", "paragraph"]);
  });

  it("EDT-FR-UYPS: an inline comment is a comment span with no text, and keeps its raw spelling", () => {
    expect(parseInline("a <!-- b \\* c --> d")).toEqual([
      { kind: "text", text: "a " },
      { kind: "comment", text: "", raw: "<!-- b \\* c -->" },
      { kind: "text", text: " d" },
    ]);
    expect(parseInline("`<!-- code -->`")).toEqual([{ kind: "code", text: "<!-- code -->" }]);
  });
});
