import { describe, expect, it } from "vitest";
import { hunksBetween } from "./localHunks";

/**
 * DFV-FR-19: the hunks a comparison is read through.
 *
 * The rendering is the Diff tab's own; what is tested here is the derivation,
 * which is this surface's because a draft file is outside Git and outside the
 * scan, so nothing else can produce a comparison for one.
 */
describe("hunksBetween (DFV-FR-19)", () => {
  it("reports no hunks when the two revisions are identical", () => {
    // DFV-FR-28: what the empty state is rendered from. A proposal that changes
    // nothing is refused before it is ever recorded (PDC-FR-07), so this is the
    // author-edited-it-back case rather than a proposal nobody should have made.
    const text = "# Spec\n\nOne.\n";
    expect(hunksBetween(text, text)).toEqual({ isBinary: false, hunks: [] });
  });

  it("numbers a context line on both sides and a change on one", () => {
    // DFV-FR-10: every rendered row carries at least one line number, and the
    // column it has none in is what the placeholder fills.
    const before = "a\nb\nc\n";
    const after = "a\nB\nc\n";
    const { hunks } = hunksBetween(before, after);
    expect(hunks).toHaveLength(1);

    const kinds = hunks[0].lines.map((l) => l.kind);
    expect(kinds).toEqual(["context", "del", "add", "context"]);

    const [first, del, add] = hunks[0].lines;
    expect(first).toMatchObject({ oldLineno: 1, newLineno: 1, content: "a" });
    // A removal has an old number and no new one; an addition the reverse.
    expect(del.oldLineno).toBe(2);
    expect(del.newLineno).toBeUndefined();
    expect(add.newLineno).toBe(2);
    expect(add.oldLineno).toBeUndefined();
  });

  it("puts a replaced line's removal immediately before its addition", () => {
    // The order DFV-FR-32's word marking pairs them back up in — and the order a
    // reader expects of a unified diff.
    const { hunks } = hunksBetween("the quick brown fox\n", "the quick red fox\n");
    expect(hunks[0].lines.map((l) => [l.kind, l.content])).toEqual([
      ["del", "the quick brown fox"],
      ["add", "the quick red fox"],
    ]);
  });

  it("windows each change in context and merges windows that would overlap", () => {
    // DFV-FR-09: unchanged context around each region, and nothing outside a
    // hunk. Two changes far apart are two hunks; two close together are one,
    // rather than two whose context overlaps.
    const before = Array.from({ length: 40 }, (_, i) => `line ${i}`).join("\n");
    const far = before.split("\n");
    far[2] = "changed near the top";
    far[35] = "changed near the bottom";
    const apart = hunksBetween(before, far.join("\n"));
    expect(apart.hunks).toHaveLength(2);

    const near = before.split("\n");
    near[10] = "changed";
    near[12] = "also changed";
    expect(hunksBetween(before, near.join("\n")).hunks).toHaveLength(1);

    // Nothing outside a hunk: the 40-line file yields far fewer than 40 rows.
    const rendered = apart.hunks.flatMap((h) => h.lines).length;
    expect(rendered).toBeLessThan(20);
  });

  it("writes a header in Git's own spelling, with 0 for a side that has none", () => {
    const { hunks } = hunksBetween("a\nb\n", "a\nb\nc\n");
    expect(hunks[0].header).toMatch(/^@@ -\d+,\d+ \+\d+,\d+ @@$/);

    // An addition outright has no old revision at all.
    const added = hunksBetween(null, "only\n");
    expect(added.hunks[0].header).toContain("-0,0");
    expect(added.hunks[0].lines.every((l) => l.kind === "add")).toBe(true);

    // And a deletion has no new one.
    const deleted = hunksBetween("only\n", null);
    expect(deleted.hunks[0].header).toContain("+0,0");
    expect(deleted.hunks[0].lines.every((l) => l.kind === "del")).toBe(true);
  });

  it("treats an empty file as a file rather than as an absent revision", () => {
    // DFV-FR-15 draws the same distinction: `""` exists and is empty, `null`
    // does not exist. Emptying a file is a change; it is not a deletion.
    const { hunks } = hunksBetween("a\n", "");
    expect(hunks).toHaveLength(1);
    expect(hunks[0].lines.map((l) => l.kind)).toEqual(["del"]);
  });

  it("never reports itself binary, a draft file being text by construction", () => {
    expect(hunksBetween("a", "b").isBinary).toBe(false);
    expect(hunksBetween("a", "a").isBinary).toBe(false);
  });
});
