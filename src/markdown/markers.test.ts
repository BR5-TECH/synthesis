import { describe, expect, it } from "vitest";
import {
  BREAK_LINE_HEIGHTS,
  breakLabel,
  canonicalAlign,
  canonicalBreak,
  classifyMarkerLine,
  commentText,
  findCommentClose,
  scanBlockComment,
  splitInlineComments,
} from "./markers";

describe("vertical break grammar (EDT-FR-QALB)", () => {
  it.each([
    ["<!-- % -->", 1],
    ["<!-- %% -->", 2],
    ["<!-- %%% -->", 3],
    ["<!-- %%%% -->", 4],
    ["<!-- %%%%% -->", 4],
    ["<!-- %%%%%%%%%% -->", 4],
  ])("EDT-FR-QALB: %s is a break of level %i", (line, level) => {
    expect(classifyMarkerLine(line)).toEqual({ kind: "break", level });
  });

  it.each(["<!--%%-->", "<!--   %%\t-->", "   <!-- %% -->", "<!-- %% -->   "])(
    "EDT-FR-QALB: %j is a break, spaces inside and around the comment allowed",
    (line) => {
      expect(classifyMarkerLine(line)).toEqual({ kind: "break", level: 2 });
    },
  );

  it.each(["<!-- %x -->", "<!-- % % -->", "<!-- -->", "<!-- 50% -->", "    <!-- % -->", "<!-- % --> text", "text <!-- % -->"])(
    "EDT-FR-QALB: %j is not a break",
    (line) => {
      expect(classifyMarkerLine(line)).toBeNull();
    },
  );

  it("EDT-FR-CSKH: V1 is 12, V2 is 8, V3 is 5, and V4 is 3 body line heights", () => {
    expect(BREAK_LINE_HEIGHTS).toEqual({ 1: 12, 2: 8, 3: 5, 4: 3 });
  });

  it("EDT-FR-XKTS, EDT-FR-HDCZ: the accessible name gives the level", () => {
    expect(breakLabel(2)).toBe("Vertical break V2");
  });
});

describe("alignment marker grammar (EDT-FR-XPMC)", () => {
  it.each([
    ["<!-- center -->", "center"],
    ["<!-- right -->", "right"],
    ["<!--center-->", "center"],
    ["<!--   right   -->", "right"],
  ])("EDT-FR-XPMC: %j aligns %s", (line, value) => {
    expect(classifyMarkerLine(line)).toEqual({ kind: "align", value });
  });

  it.each(["<!-- left -->", "<!-- Center -->", "<!-- CENTER -->", "<!-- centre -->", "<!-- center right -->", "<!-- /center -->"])(
    "EDT-FR-XPMC: %j is no marker",
    (line) => {
      expect(classifyMarkerLine(line)).toBeNull();
    },
  );
});

describe("canonical form (EDT-FR-OZPO)", () => {
  it("EDT-FR-OZPO: lowercase, one space on each side of the content", () => {
    expect(canonicalBreak(1)).toBe("<!-- % -->");
    expect(canonicalBreak(4)).toBe("<!-- %%%% -->");
    expect(canonicalAlign("center")).toBe("<!-- center -->");
    expect(canonicalAlign("right")).toBe("<!-- right -->");
  });

  it("EDT-FR-OZPO: each canonical marker reads back as what it was written for", () => {
    for (const level of [1, 2, 3, 4] as const) {
      expect(classifyMarkerLine(canonicalBreak(level))).toEqual({ kind: "break", level });
    }
    expect(classifyMarkerLine(canonicalAlign("right"))).toEqual({ kind: "align", value: "right" });
  });
});

describe("comment scanning (EDT-FR-UYPS)", () => {
  it("EDT-FR-UYPS: a comment over several lines, blank lines included, is one block", () => {
    const lines = ["<!--", "  first", "", "  second", "-->", "After"];
    expect(scanBlockComment(lines, 0)).toEqual({
      end: 5,
      raw: "<!--\n  first\n\n  second\n-->",
    });
  });

  it("EDT-FR-UYPS: text after the close makes the comment inline, and an unclosed one is no comment", () => {
    expect(scanBlockComment(["<!-- a --> tail"], 0)).toBeNull();
    expect(scanBlockComment(["<!-- never", "closed"], 0)).toBeNull();
    expect(scanBlockComment(["text <!-- a -->"], 0)).toBeNull();
  });

  it("EDT-FR-UYPS: a close that only overlaps the opening does not end the comment", () => {
    expect(scanBlockComment(["<!-->", "-->"], 0)).toEqual({ end: 2, raw: "<!-->\n-->" });
  });

  it("EDT-FR-UYPS: inline comments split from the prose around them, in order", () => {
    expect(splitInlineComments("a <!-- b --> c <!-- d -->")).toEqual([
      { kind: "text", text: "a " },
      { kind: "comment", raw: "<!-- b -->" },
      { kind: "text", text: " c " },
      { kind: "comment", raw: "<!-- d -->" },
    ]);
    expect(splitInlineComments("a <!-- open")).toEqual([{ kind: "text", text: "a <!-- open" }]);
  });
});

describe("tooltip text (EDT-FR-IGZL)", () => {
  it("EDT-FR-IGZL: the text without its delimiters, line breaks kept, common indent removed", () => {
    expect(commentText("<!-- a note -->")).toBe("a note");
    expect(commentText("<!--\n  Naming: one\n    - nested\n  Two\n-->")).toBe(
      "Naming: one\n  - nested\nTwo",
    );
    expect(commentText("<!-- -->")).toBe("");
    expect(commentText("<!-- <script>alert(1)</script> -->")).toBe("<script>alert(1)</script>");
  });
});

describe("the close of a comment (EDT-FR-UYPS, EDT-FR-KMAR)", () => {
  it("EDT-FR-UYPS: a comment ends at the first --> or --!>, as HTML ends it", () => {
    expect(findCommentClose("<!-- a --> b", 4)).toEqual({ at: 7, end: 10 });
    expect(findCommentClose("<!-- a --!> b -->", 4)).toEqual({ at: 7, end: 11 });
    expect(findCommentClose("<!-- a --> b --!>", 4)).toEqual({ at: 7, end: 10 });
    expect(findCommentClose("<!-- open", 4)).toBeNull();
  });

  it("EDT-FR-KMAR: text after --!> is prose, not part of the chip", () => {
    expect(splitInlineComments("a <!-- x --!> b -->")).toEqual([
      { kind: "text", text: "a " },
      { kind: "comment", raw: "<!-- x --!>" },
      { kind: "text", text: " b -->" },
    ]);
    expect(scanBlockComment(["<!-- x --!>"], 0)).toEqual({ end: 1, raw: "<!-- x --!>" });
    expect(scanBlockComment(["<!-- x --!> shown -->"], 0)).toBeNull();
    expect(commentText("<!-- x --!>")).toBe("x");
  });
});
