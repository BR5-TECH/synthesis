import { describe, expect, it } from "vitest";

import { changedRanges, diffWords, splitByRanges, tokenize } from "./wordDiff";

/** The changed runs of one side, which is what carries the stronger mark. */
const changed = (segments: { text: string; changed: boolean }[] | undefined) =>
  (segments ?? []).filter((s) => s.changed).map((s) => s.text);

/** The whole side reassembled — segmenting must never lose or reorder text. */
const whole = (segments: { text: string }[] | undefined) =>
  (segments ?? []).map((s) => s.text).join("");

describe("tokenize", () => {
  it("splits into identifier-shaped runs, whitespace, and single punctuation", () => {
    expect(tokenize("const a = getFoo(1);")).toEqual([
      "const", " ", "a", " ", "=", " ", "getFoo", "(", "1", ")", ";",
    ]);
  });

  it("has no tokens for an empty line", () => {
    expect(tokenize("")).toEqual([]);
  });
});

describe("diffWords", () => {
  it("marks only the words that differ in a one-word edit", () => {
    const result = diffWords(
      "The quick brown fox jumps over the dog.",
      "The quick red fox jumps over the dog.",
    )!;
    expect(changed(result.old)).toEqual(["brown"]);
    expect(changed(result.new)).toEqual(["red"]);
  });

  it("keeps every character of both sides", () => {
    const oldLine = "  return compute(a, b) + 1;";
    const newLine = "  return compute(a, b, c) + 2;";
    const result = diffWords(oldLine, newLine)!;
    expect(whole(result.old)).toBe(oldLine);
    expect(whole(result.new)).toBe(newLine);
  });

  it("marks an insertion on the new side only", () => {
    const result = diffWords("call(a)", "call(a, b)")!;
    expect(changed(result.old)).toEqual([]);
    expect(changed(result.new)).toEqual([", b"]);
  });

  it("merges adjacent changed tokens into one run", () => {
    const result = diffWords("a one two b", "a three four b")!;
    // One highlight spanning "three four", not four separate ones.
    expect(changed(result.new)).toEqual(["three four"]);
  });

  it("declines two lines with too little in common", () => {
    // Segmenting these would speckle both lines with highlights and say less
    // than the whole-line marking they already carry.
    expect(
      diffWords("const alpha = 1;", "throw new Error('completely other');"),
    ).toBeNull();
  });

  it("declines a pair whose every word differs", () => {
    expect(diffWords("aaa bbb", "ccc ddd")).toBeNull();
  });

  it("does not count shared indentation as similarity", () => {
    // Every line of an indented file shares its indentation with every other,
    // so whitespace alone must not make two unrelated lines look like an edit.
    expect(diffWords("        alpha", "        beta")).toBeNull();
  });

  it("still segments when the shared part is real content rather than whitespace", () => {
    // `();` is genuinely common to both, so renaming the call is one edit and
    // marking just the name is exactly the point.
    const result = diffWords("    alpha();", "    omega_thing();")!;
    expect(changed(result.old)).toEqual(["alpha"]);
    expect(changed(result.new)).toEqual(["omega_thing"]);
  });

  it("declines when either side is empty", () => {
    expect(diffWords("", "something")).toBeNull();
    expect(diffWords("something", "")).toBeNull();
  });

  it("keeps an astral character whole", () => {
    // A surrogate half in its own DOM node renders as a replacement glyph, so
    // a segment boundary must never fall inside a pair.
    const result = diffWords("a 👍 b", "a 👎 b")!;
    for (const side of [result.old, result.new]) {
      for (const segment of side) {
        expect(segment.text).not.toMatch(/[\uD800-\uDBFF]$/);
        expect(segment.text).not.toMatch(/^[\uDC00-\uDFFF]/);
      }
    }
    expect(changed(result.old)).toEqual(["👍"]);
    expect(changed(result.new)).toEqual(["👎"]);
  });

  it("treats an accented word as one word", () => {
    expect(tokenize("café")).toEqual(["café"]);
    const result = diffWords("le café noir", "le café blanc")!;
    expect(changed(result.new)).toEqual(["blanc"]);
  });

  it("finds the changed word in a script that writes no spaces", () => {
    const result = diffWords(
      "これは日本語のテキストです",
      "これは日本語の文章です",
    )!;
    expect(changed(result.old)).toEqual(["テキスト"]);
    expect(changed(result.new)).toEqual(["文章"]);
  });

  it("marks an append that lengthens the line several times over", () => {
    // Measuring similarity against the longer side would decline exactly the
    // case the marking is most useful for.
    const result = diffWords(
      "short",
      "short but very much longer now with many extra words appended here",
    )!;
    expect(whole(result.new)).toBe(
      "short but very much longer now with many extra words appended here",
    );
    expect(changed(result.new).join("")).toContain("appended");
    expect(changed(result.new).join("")).not.toContain("short");
  });

  it("marks a reindent as a change to the whitespace itself", () => {
    // The only difference is the indentation, so that is what is marked. The
    // alternative — declining — would leave the reader with two identically
    // tinted lines and no clue what moved.
    const result = diffWords("  foo", "    foo")!;
    expect(changed(result.old)).toEqual(["  "]);
    expect(changed(result.new)).toEqual(["    "]);
  });

  it("declines a pair too big to compare cheaply", () => {
    // This runs once per replaced line and side-by-side renders the whole file,
    // so an unbounded per-line cost becomes an unbounded per-file one and the
    // tab stops painting.
    const shuffled = (seed: string, n: number) =>
      Array.from({ length: n }, (_, i) => `${seed}${(i * 7919) % n}`).join(" ");
    const started = performance.now();
    expect(diffWords(shuffled("a", 900), shuffled("b", 900))).toBeNull();
    expect(performance.now() - started).toBeLessThan(50);
  });

  it("still segments a long line whose edit is small", () => {
    // The budget is measured after the shared head and tail are discounted, so
    // an ordinary edit inside a long line still gets marked.
    const head = Array.from({ length: 900 }, (_, i) => `w${i}`).join(" ");
    const result = diffWords(`${head} alpha`, `${head} omega`)!;
    expect(changed(result.new)).toEqual(["omega"]);
  });

  it("handles a long line with a trailing edit without losing the head", () => {
    const head = "a".repeat(200);
    const result = diffWords(`${head} tail one`, `${head} tail two`)!;
    expect(changed(result.new)).toEqual(["two"]);
    expect(whole(result.new)).toBe(`${head} tail two`);
  });
});

describe("changedRanges / splitByRanges", () => {
  it("turns runs into offsets over the same string", () => {
    const segments = [
      { text: "the ", changed: false },
      { text: "red", changed: true },
      { text: " fox", changed: false },
    ];
    expect(changedRanges(segments)).toEqual([[4, 7]]);
  });

  it("marks the part of a span that falls inside a range", () => {
    // Rich rendering splits a block into emphasis and code spans, so a range
    // can start in one span and end in another.
    const ranges = changedRanges([
      { text: "A ", changed: false },
      { text: "bold word", changed: true },
      { text: " tail", changed: false },
    ]);
    // The span "A bold" begins at offset 0; only "bold" is inside the range.
    expect(splitByRanges("A bold", 0, ranges)).toEqual([
      { text: "A ", changed: false },
      { text: "bold", changed: true },
    ]);
    // The span " word tail" begins at offset 6; its head is still inside it.
    expect(splitByRanges(" word tail", 6, ranges)).toEqual([
      { text: " word", changed: true },
      { text: " tail", changed: false },
    ]);
  });

  it("leaves a span wholly outside every range untouched", () => {
    expect(splitByRanges("plain", 100, [[0, 4]])).toEqual([
      { text: "plain", changed: false },
    ]);
  });

  it("marks a span that lies wholly inside a range", () => {
    // Both clamping arms at once: the range starts before the span and ends
    // after it.
    expect(splitByRanges("bold", 2, [[0, 10]])).toEqual([
      { text: "bold", changed: true },
    ]);
  });

  it("loses no text, whatever the ranges", () => {
    const text = "alpha beta gamma";
    const cases: Array<[number, Array<[number, number]>]> = [
      [0, []],
      [0, [[0, 5]]],
      [0, [[0, 5], [11, 16]]],
      [0, [[0, 5], [5, 11]]], // adjacent
      [0, [[0, 16]]], // the whole string
      [0, [[-3, 2]]], // clamped at the head
      [0, [[13, 99]]], // clamped at the tail
      [4, [[0, 20]]], // a non-zero offset
    ];
    for (const [offset, ranges] of cases) {
      const rebuilt = splitByRanges(text, offset, ranges)
        .map((run) => run.text)
        .join("");
      expect(rebuilt, `offset ${offset}, ranges ${JSON.stringify(ranges)}`).toBe(
        text,
      );
    }
  });

  it("returns the whole span unmarked when there are no ranges", () => {
    expect(splitByRanges("anything", 0, [])).toEqual([
      { text: "anything", changed: false },
    ]);
  });
});
