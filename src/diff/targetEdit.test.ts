import { describe, expect, it } from "vitest";

import { blockLineRange, replaceLineRange, targetLines } from "./targetEdit";

/**
 * Tests for `DFV-diff-viewer.md` DFV-FR-44: an edit made in one rendered row is
 * one edit to the whole target, and what is outside the row is carried through
 * untouched.
 *
 * The rendered rows are a *window* onto the target — Unified renders only the
 * hunks — so the invariant every case here is about is the same one: nothing
 * outside the range that was addressed may move.
 */

describe("targetLines", () => {
  it("reads a trailing newline as terminating the last line rather than starting one", () => {
    // The same reading the line aligner and the block parser take, so a row's
    // index here addresses the line those two named.
    expect(targetLines("a\nb\n")).toEqual(["a", "b"]);
    expect(targetLines("a\nb")).toEqual(["a", "b"]);
    expect(targetLines("")).toEqual([""]);
    // An empty last line the author actually typed is not a terminator.
    expect(targetLines("a\n\n")).toEqual(["a", ""]);
  });
});

describe("replaceLineRange (DFV-FR-44)", () => {
  const FILE = "one\ntwo\nthree\nfour\n";

  it("replaces the addressed line and carries the rest of the file through", () => {
    expect(replaceLineRange(FILE, 1, 2, "TWO")).toBe("one\nTWO\nthree\nfour\n");
  });

  it("splits a line in two when the replacement carries a newline", () => {
    // What Enter inside a row does: the row becomes two rows, and the file
    // grows by one line rather than the row growing a `<br>`.
    expect(replaceLineRange(FILE, 1, 2, "tw\no")).toBe(
      "one\ntw\no\nthree\nfour\n",
    );
  });

  it("joins two lines when a range of two is replaced by one", () => {
    // What Backspace at the head of a row does.
    expect(replaceLineRange(FILE, 1, 3, "twothree")).toBe(
      "one\ntwothree\nfour\n",
    );
  });

  it("removes the addressed lines when the replacement is empty", () => {
    expect(replaceLineRange(FILE, 1, 3, "")).toBe("one\nfour\n");
  });

  it("keeps the file's terminator exactly as it found it", () => {
    // The terminator is a property of the file, not of the row that was edited,
    // so an edit neither adds nor removes one.
    expect(replaceLineRange("a\nb", 0, 1, "A")).toBe("A\nb");
    expect(replaceLineRange("a\nb\n", 0, 1, "A")).toBe("A\nb\n");
    // And a target emptied outright stays empty rather than becoming a lone
    // newline, which would read as a one-line file that is not there.
    expect(replaceLineRange("only\n", 0, 1, "")).toBe("");
  });

  it("clamps a range the target no longer holds instead of dropping the edit", () => {
    // A row can be edited in the same tick a re-derivation shortened the target
    // under it. Refusing would lose the author's typing to a race they cannot
    // see; clamping appends it where the target now ends.
    expect(replaceLineRange("a\n", 9, 10, "z")).toBe("a\nz\n");
    expect(replaceLineRange("a\nb\n", -3, 1, "A")).toBe("A\nb\n");
  });

  it("writes into an empty target as its first line", () => {
    // DFV-FR-54 / DCR-FR-24: an empty target is a place to write, and typing
    // into it is an ordinary edit.
    expect(replaceLineRange("", 0, 1, "first")).toBe("first");
  });
});

describe("blockLineRange (DFV-FR-47)", () => {
  it("spans every line of the block's own source", () => {
    expect(blockLineRange({ line: 4, source: "# Heading" })).toEqual([4, 5]);
    expect(blockLineRange({ line: 0, source: "```\nbody\n```" })).toEqual([0, 3]);
  });

  it("addresses exactly the lines a rich edit replaces", () => {
    const doc = "# T\n\npara one\n\n```\ncode\n```\n";
    const [from, to] = blockLineRange({ line: 4, source: "```\ncode\n```" });
    expect(replaceLineRange(doc, from, to, "```\nedited\n```")).toBe(
      "# T\n\npara one\n\n```\nedited\n```\n",
    );
  });
});

describe("a target written with CRLF (GTC-FR-17, EDT-FR-40)", () => {
  it("keeps one line-ending convention throughout an edit", () => {
    // A checkout under a convention that differs from the committed blob's
    // loads as it lies, and splitting on `\n` alone would strip the `\r` from
    // the edited line and leave it on every other — a whole-file diff nobody
    // authored, written to disk (EDT-FR-40).
    const crlf = "one\r\ntwo\r\nthree\r\n";
    expect(targetLines(crlf)).toEqual(["one", "two", "three"]);
    expect(replaceLineRange(crlf, 1, 2, "TWO")).toBe("one\r\nTWO\r\nthree\r\n");
    // A split, a join, and a paste each stay on the file's own convention.
    expect(replaceLineRange(crlf, 1, 2, "t\nwo")).toBe(
      "one\r\nt\r\nwo\r\nthree\r\n",
    );
    expect(replaceLineRange(crlf, 0, 2, "onetwo")).toBe("onetwo\r\nthree\r\n");
    expect(replaceLineRange(crlf, 1, 2, "a\r\nb")).toBe(
      "one\r\na\r\nb\r\nthree\r\n",
    );
  });

  it("leaves an LF file on LF even when the pasted text carries CRLF", () => {
    expect(replaceLineRange("one\ntwo\n", 1, 2, "a\r\nb")).toBe("one\na\nb\n");
  });
});
