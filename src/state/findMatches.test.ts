import { describe, expect, it } from "vitest";
import {
  applyReplacements,
  compileQuery,
  findMatches,
  reindexCurrent,
  smartCaseIsSensitive,
  stepIndex,
} from "./findMatches";

/** Convenience: the matched substrings, which read better than offset pairs. */
const hits = (text: string, query: string, mode: Parameters<typeof findMatches>[2]) =>
  findMatches(text, query, mode).matches.map((m) => text.slice(m.start, m.end));

describe("findMatches — the three query modes (EFR-FR-CLZF, per SCC-FR-05)", () => {
  it("literal_insensitive matches the query as plain text, ignoring case", () => {
    const text = "Widget widget WIDGET";
    expect(hits(text, "widget", "literal_insensitive")).toEqual([
      "Widget",
      "widget",
      "WIDGET",
    ]);
  });

  it("smart_case ignores case until the query carries a capital (SCC-FR-05)", () => {
    const text = "Widget widget";
    expect(hits(text, "widget", "smart_case")).toEqual(["Widget", "widget"]);
    expect(hits(text, "Widget", "smart_case")).toEqual(["Widget"]);
  });

  it("literal modes treat regex metacharacters literally (SCC-FR-05)", () => {
    // The whole point of a literal mode: `a.c` is three characters, not "any
    // character between a and c". A missed escape here silently turns every
    // punctuation-bearing query into a pattern.
    expect(hits("abc", "a.c", "literal_insensitive")).toEqual([]);
    expect(hits("a.c", "a.c", "literal_insensitive")).toEqual(["a.c"]);
    expect(hits("abc", "a.c", "smart_case")).toEqual([]);
    // …and regex mode does assign it meaning.
    expect(hits("abc", "a.c", "regex")).toEqual(["abc"]);
  });

  it("regex mode is case-sensitive unless the pattern says otherwise", () => {
    expect(hits("Widget widget", "widget", "regex")).toEqual(["widget"]);
    expect(hits("Widget widget", "[Ww]idget", "regex")).toEqual([
      "Widget",
      "widget",
    ]);
  });

  it("escapes every regex metacharacter in a literal query", () => {
    // One un-escaped metacharacter is enough to make a literal query throw or
    // silently mis-match, so the whole set is pinned rather than a sample.
    for (const ch of ".*+?^${}()|[]\\") {
      const text = `x${ch}y`;
      expect(hits(text, text, "literal_insensitive")).toEqual([text]);
    }
  });
});

describe("findMatches — offsets and ordering (EFR-FR-DOQR, EFR-FR-DYMA)", () => {
  it("returns every match in document order with half-open ranges", () => {
    expect(findMatches("ab ab ab", "ab", "literal_insensitive")).toEqual({
      valid: true,
      matches: [
        { start: 0, end: 2 },
        { start: 3, end: 5 },
        { start: 6, end: 8 },
      ],
    });
  });

  it("finds adjacent, non-overlapping matches", () => {
    expect(hits("aaaa", "aa", "literal_insensitive")).toEqual(["aa", "aa"]);
  });

  it("an empty query matches nothing rather than everything", () => {
    expect(findMatches("anything", "", "literal_insensitive")).toEqual({
      valid: true,
      matches: [],
    });
    expect(findMatches("anything", "", "regex")).toEqual({
      valid: true,
      matches: [],
    });
  });
});

describe("findMatches — invalid patterns (EFR-FR-ENKY)", () => {
  it("reports an uncompilable regex as invalid rather than as no matches", () => {
    // The panel renders these two states differently — an invalid-pattern
    // indication vs. a plain zero count — so they must not collapse.
    expect(findMatches("foo(bar", "foo(", "regex")).toEqual({
      valid: false,
      matches: [],
    });
    expect(findMatches("x", "[unclosed", "regex")).toEqual({
      valid: false,
      matches: [],
    });
  });

  it("never reports invalid for a literal mode, whatever the query", () => {
    // A literal query is escaped before compiling, so no user input can make it
    // uncompilable — `[unclosed` is a plain string to find.
    expect(findMatches("a [unclosed b", "[unclosed", "literal_insensitive")).toEqual({
      valid: true,
      matches: [{ start: 2, end: 11 }],
    });
    expect(findMatches("x", "foo(", "smart_case")).toEqual({
      valid: true,
      matches: [],
    });
  });

  it("compileQuery yields null exactly for an empty or uncompilable query", () => {
    expect(compileQuery("", "literal_insensitive")).toBeNull();
    expect(compileQuery("foo(", "regex")).toBeNull();
    expect(compileQuery("foo(", "literal_insensitive")).not.toBeNull();
  });
});

describe("findMatches — zero-length matches", () => {
  it("terminates and records nothing for a pattern that matches emptily", () => {
    // `a*` matches the empty string at every position. Recording those would
    // make the counter meaningless and give Replace nothing to rewrite; not
    // advancing past them would hang the scan outright.
    expect(hits("bbb", "a*", "regex")).toEqual([]);
    expect(hits("baab", "a*", "regex")).toEqual(["aa"]);
    expect(hits("abc", "^", "regex")).toEqual([]);
    expect(hits("abc\ndef", "$", "regex")).toEqual([]);
  });
});

describe("smartCaseIsSensitive", () => {
  it("is sensitive exactly when the query carries an uppercase character", () => {
    expect(smartCaseIsSensitive("widget")).toBe(false);
    expect(smartCaseIsSensitive("wid-get_1")).toBe(false);
    expect(smartCaseIsSensitive("Widget")).toBe(true);
    expect(smartCaseIsSensitive("widgeT")).toBe(true);
  });
});

describe("applyReplacements (EFR-FR-EXHA)", () => {
  it("rewrites every given range and leaves the rest byte-identical", () => {
    const text = "one two one two";
    const ranges = findMatches(text, "one", "literal_insensitive").matches;
    expect(applyReplacements(text, ranges, "1")).toBe("1 two 1 two");
  });

  it("rewrites a single range without touching the others", () => {
    const text = "one two one";
    const all = findMatches(text, "one", "literal_insensitive").matches;
    expect(applyReplacements(text, [all[1]], "1")).toBe("one two 1");
  });

  it("inserts the replacement literally — no capture-group substitution (EFR-FR-FWOU)", () => {
    // A regex query with a group and a `$1` replacement writes the characters
    // `$1`, not the captured text. This is the deliberate v1 boundary.
    const text = "smart-case and upper-case";
    const ranges = findMatches(text, "(\\w+)-case", "regex").matches;
    expect(applyReplacements(text, ranges, "$1")).toBe("$1 and $1");
    expect(applyReplacements(text, ranges, "$&")).toBe("$& and $&");
  });

  it("replacing with the empty string deletes the matches", () => {
    const text = "a-b-c";
    const ranges = findMatches(text, "-", "literal_insensitive").matches;
    expect(applyReplacements(text, ranges, "")).toBe("abc");
  });

  it("is a no-op for an empty range list", () => {
    expect(applyReplacements("untouched", [], "x")).toBe("untouched");
  });

  it("handles a replacement that contains the query", () => {
    // Naive repeated string replacement would loop forever here; range-based
    // rewriting is done in one pass over the original offsets.
    const text = "cat cat";
    const ranges = findMatches(text, "cat", "literal_insensitive").matches;
    expect(applyReplacements(text, ranges, "cats")).toBe("cats cats");
  });
});

describe("reindexCurrent (EFR-FR-EVHJ)", () => {
  const matches = [
    { start: 10, end: 13 },
    { start: 20, end: 23 },
    { start: 30, end: 33 },
  ];

  it("keeps the current match where it is when it survives", () => {
    expect(reindexCurrent(matches, 20)).toBe(1);
  });

  it("moves to the nearest following match when the current one is gone", () => {
    // The user deleted the text of the match at 20; the next one takes over.
    expect(reindexCurrent(matches, 15)).toBe(1);
    expect(reindexCurrent(matches, 25)).toBe(2);
  });

  it("wraps to the first match when nothing follows the old position", () => {
    expect(reindexCurrent(matches, 99)).toBe(0);
  });

  it("returns 0 for an empty match set or a missing anchor", () => {
    expect(reindexCurrent([], 20)).toBe(0);
    expect(reindexCurrent(matches, null)).toBe(0);
  });
});

describe("stepIndex (EFR-FR-DYMA)", () => {
  it("wraps forward past the last match to the first", () => {
    expect(stepIndex(0, 3, 1)).toBe(1);
    expect(stepIndex(2, 3, 1)).toBe(0);
  });

  it("wraps backward past the first match to the last", () => {
    expect(stepIndex(2, 3, -1)).toBe(1);
    expect(stepIndex(0, 3, -1)).toBe(2);
  });

  it("stays at 0 with no matches", () => {
    expect(stepIndex(0, 0, 1)).toBe(0);
    expect(stepIndex(0, 0, -1)).toBe(0);
  });

  it("is a no-op cycle with exactly one match", () => {
    expect(stepIndex(0, 1, 1)).toBe(0);
    expect(stepIndex(0, 1, -1)).toBe(0);
  });
});

describe("regex mode and astral characters", () => {
  it("matches a whole emoji rather than half a surrogate pair", () => {
    // Without the `u` flag `.` matches one UTF-16 code unit, so a replacement
    // over the match would write a lone surrogate into the buffer — a corrupt
    // document. With it, the astral character is one match.
    const text = "a😀b";
    const result = findMatches(text, ".", "regex");
    expect(result.matches.map((m) => text.slice(m.start, m.end))).toEqual([
      "a",
      "😀",
      "b",
    ]);
  });

  it("replaces an emoji without splitting it", () => {
    const text = "hi 😀 there";
    const ranges = findMatches(text, "😀", "regex").matches;
    expect(applyReplacements(text, ranges, ":)")).toBe("hi :) there");
  });

  it("reports offsets after an emoji consistently with the source string", () => {
    const text = "😀 needle";
    const [m] = findMatches(text, "needle", "literal_insensitive").matches;
    expect(text.slice(m.start, m.end)).toBe("needle");
  });

  it("still accepts a pattern that only compiles without the u flag", () => {
    // `u` rejects a bare `{`, which is a legal literal unrestricted — and a
    // perfectly ordinary thing to search for in code. A query the user could
    // previously run must not start reading as invalid because of the flag.
    expect(() => new RegExp("{", "gu")).toThrow();
    expect(findMatches("fn() { body }", "{", "regex")).toEqual({
      valid: true,
      matches: [{ start: 5, end: 6 }],
    });
  });

  it("still reports a genuinely uncompilable pattern as invalid", () => {
    expect(findMatches("x", "foo(", "regex").valid).toBe(false);
  });
});
