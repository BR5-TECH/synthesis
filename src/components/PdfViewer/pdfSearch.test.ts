// The pure search functions of the PDF viewer (`PDV-pdf-viewer.md` PDV-FR-XMRL,
// PDV-FR-BPXG, PDV-FR-TIFB).
import { describe, expect, it } from "vitest";
import {
  buildPageText,
  countLabel,
  findPageMatches,
  firstMatchFrom,
  foldCase,
  matchSegments,
  normalizeQuery,
} from "./pdfSearch";

const text = buildPageText({
  items: [{ str: "Hello wor" }, { str: "ld and ", hasEOL: true }, { str: "HELLO" }, {}],
});

describe("PDV-FR-XMRL: matching", () => {
  it("PDV-FR-XMRL: joins the items of a page, with a space after a line end, and ignores items without a string", () => {
    expect(text.text).toBe("Hello world and  HELLO");
    expect(text.starts).toEqual([0, 9, 17]);
  });

  it("PDV-FR-XMRL: matching ignores case and finds every non-overlapping match of a page", () => {
    const matches = findPageMatches(3, text, "HeLLo");
    expect(matches).toEqual([
      { page: 3, start: 0, end: 5 },
      { page: 3, start: 17, end: 22 },
    ]);
    expect(findPageMatches(1, buildPageText({ items: [{ str: "aaaa" }] }), "aa")).toHaveLength(2);
  });

  it("PDV-FR-XMRL: an empty or blank query finds nothing, and spaces at its ends do not count", () => {
    expect(findPageMatches(1, text, "")).toEqual([]);
    expect(findPageMatches(1, text, "   ")).toEqual([]);
    expect(normalizeQuery("  Hello ")).toBe("hello");
    expect(findPageMatches(1, text, "  hello  ")).toHaveLength(2);
  });

  it("PDV-FR-XMRL: folding keeps the length of the text, so offsets stay valid", () => {
    for (const sample of ["İstanbul", "ΣΑΣ", "straße", "ǅ"]) {
      expect(foldCase(sample).length).toBe(sample.length);
    }
  });

  it("PDV-FR-XMRL: the count reads 3 of 12, or No matches", () => {
    expect(countLabel(12, 2)).toBe("3 of 12");
    expect(countLabel(1, 0)).toBe("1 of 1");
    expect(countLabel(0, 0)).toBe("No matches");
  });
});

describe("PDV-FR-BPXG, PDV-FR-TIFB: where a match lies", () => {
  it("PDV-FR-BPXG: a match that spans two text items is cut into one slice for each", () => {
    const [match] = findPageMatches(1, text, "wor");
    expect(matchSegments(text, match)).toEqual([{ item: 0, from: 6, to: 9 }]);
    const [across] = findPageMatches(1, text, "world");
    expect(matchSegments(text, across)).toEqual([
      { item: 0, from: 6, to: 9 },
      { item: 1, from: 0, to: 2 },
    ]);
  });

  it("PDV-FR-TIFB: a new search starts at the first match at or after the current page, and wraps to the first when none follows", () => {
    const matches = [
      { page: 1, start: 0, end: 1 },
      { page: 3, start: 0, end: 1 },
      { page: 3, start: 5, end: 6 },
      { page: 6, start: 0, end: 1 },
    ];
    expect(firstMatchFrom(matches, 1)).toBe(0);
    expect(firstMatchFrom(matches, 2)).toBe(1);
    expect(firstMatchFrom(matches, 3)).toBe(1);
    expect(firstMatchFrom(matches, 4)).toBe(3);
    expect(firstMatchFrom(matches, 7)).toBe(0);
    expect(firstMatchFrom([], 1)).toBe(0);
  });
});
