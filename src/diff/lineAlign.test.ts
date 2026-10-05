import { describe, expect, it } from "vitest";

import { alignBy, alignLines, splitLines } from "./lineAlign";

/** A compact shape for what a row looks like: old | new, and whether marked. */
const shape = (rows: ReturnType<typeof alignLines>) =>
  rows.map((row) => [
    row.old ? `${row.old.lineno}:${row.old.content}` : null,
    row.new ? `${row.new.lineno}:${row.new.content}` : null,
    row.changed,
  ]);

describe("splitLines", () => {
  it("treats a trailing newline as terminating the last line, not starting one", () => {
    expect(splitLines("a\n")).toEqual(["a"]);
    expect(splitLines("a\nb\n")).toEqual(["a", "b"]);
    // A file with no trailing newline still has both its lines.
    expect(splitLines("a\nb")).toEqual(["a", "b"]);
    // An empty file has no lines at all — distinct from one blank line.
    expect(splitLines("")).toEqual([]);
    expect(splitLines("\n")).toEqual([""]);
  });
});

describe("alignLines (DFV-FR-11 / DFV-FR-12)", () => {
  it("pairs unchanged lines and marks neither side", () => {
    expect(shape(alignLines("a\nb\n", "a\nb\n"))).toEqual([
      ["1:a", "1:a", false],
      ["2:b", "2:b", false],
    ]);
  });

  it("pairs a removal against its replacement and fills the surplus (DFV-FR-11, DFV-FR-12)", () => {
    const rows = alignLines(
      "keep\nremoved\ntail\n",
      "keep\nfirst\nsecond\ntail\n",
    );
    expect(shape(rows)).toEqual([
      ["1:keep", "1:keep", false],
      ["2:removed", "2:first", true],
      // The left has no counterpart for the surplus addition.
      [null, "3:second", true],
      ["3:tail", "4:tail", false],
    ]);
  });

  it("numbers each side against its own revision", () => {
    const rows = alignLines("a\nb\nc\n", "a\nc\n");
    expect(shape(rows)).toEqual([
      ["1:a", "1:a", false],
      ["2:b", null, true],
      ["3:c", "2:c", false],
    ]);
  });

  it("keeps the alignment holding down the whole file after an unbalanced change", () => {
    const rows = alignLines(
      Array.from({ length: 40 }, (_, i) => `line ${i + 1}`).join("\n") + "\n",
      ["inserted", ...Array.from({ length: 40 }, (_, i) => `line ${i + 1}`)].join("\n") +
        "\n",
    );
    expect(rows).toHaveLength(41);
    expect(rows[0].old).toBeNull();
    // Every later row still pairs a line with the same text on both sides.
    for (let i = 1; i < rows.length; i++) {
      expect(rows[i].old!.content).toBe(rows[i].new!.content);
      expect(rows[i].old!.lineno).toBe(i);
      expect(rows[i].new!.lineno).toBe(i + 1);
    }
  });

  it("treats an absent revision as every line changed (DFV-FR-15)", () => {
    // The comparison deletes the file: nothing on the right, all marked.
    const deleted = alignLines("one\ntwo\n", null);
    expect(shape(deleted)).toEqual([
      ["1:one", null, true],
      ["2:two", null, true],
    ]);
    // And the comparison adds it: nothing on the left.
    const added = alignLines(null, "one\n");
    expect(shape(added)).toEqual([[null, "1:one", true]]);
  });

  it("renders an emptied file as a removal rather than as an absent revision", () => {
    // `Some("")` vs `null` is a distinction GTC-FR-16 draws deliberately.
    expect(shape(alignLines("one\n", ""))).toEqual([["1:one", null, true]]);
  });

  it("produces no rows when both revisions are empty", () => {
    expect(alignLines("", "")).toEqual([]);
    expect(alignLines(null, null)).toEqual([]);
  });

  it("stays linear on a large file with a single edit", () => {
    // Common prefix/suffix trimming is what keeps the quadratic table from
    // being built at all for the ordinary case.
    const lines = Array.from({ length: 20_000 }, (_, i) => `line ${i}`);
    const edited = [...lines];
    edited[10_000] = "edited";
    const rows = alignLines(lines.join("\n"), edited.join("\n"));
    expect(rows).toHaveLength(20_000);
    // One changed row and 19,999 matched ones is only reachable if the trim ran:
    // the quadratic table for 20,000 x 20,000 is far past the cell budget, so
    // without it the whole file would come back as one wholesale replacement.
    expect(rows.filter((r) => r.changed)).toHaveLength(1);
  });

  it("still aligns two wholly-different large files rather than hanging", () => {
    // Past the cell budget the middle is one wholesale replacement: coarser
    // pairing, but every line of both revisions is still present and aligned.
    const a = Array.from({ length: 4000 }, (_, i) => `alpha ${i}`).join("\n");
    const b = Array.from({ length: 4000 }, (_, i) => `beta ${i}`).join("\n");
    const rows = alignLines(a, b);
    expect(rows.filter((r) => r.old).length).toBe(4000);
    expect(rows.filter((r) => r.new).length).toBe(4000);
    expect(rows.every((r) => r.changed)).toBe(true);
  });
});

describe("alignBy (DFV-FR-21: blocks align on the same terms as lines)", () => {
  it("matches units whose key is identical", () => {
    const rows = alignBy(
      [{ id: "a" }, { id: "b" }],
      [{ id: "a" }, { id: "c" }],
      (item) => item.id,
    );
    expect(rows).toEqual([
      { old: { id: "a" }, new: { id: "a" }, changed: false },
      { old: { id: "b" }, new: { id: "c" }, changed: true },
    ]);
  });
});
