/**
 * Unified-diff hunks computed **here**, from two revisions the surface already
 * holds.
 *
 * Every other comparison in the application asks the backend for its hunks,
 * because every other comparison is between revisions Git has
 * (`GTC-git.md` GTC-FR-16). A draft's files are not: `.synthesis/drafts/` is
 * committed but outside the project scan (DRS-FR-04, ASC-FR-09), and a proposal
 * is a revision Git has never held, so no operation anywhere can produce a
 * comparison for one — which is why
 * `DCR-draft-change-review.md` owns the derivation itself (DCR-FR-07, UI
 * contract boundary).
 *
 * What it does *not* own is how the result reads: the hunks produced here are
 * the same `DiffPayload` the backend returns, rendered by the same components
 * a Diff tab renders, so a proposed change and a committed one are read the
 * same way (DCR-FR-07).
 */
import { alignLines } from "./lineAlign";
import type { DiffHunk, DiffLine, DiffPayload } from "../types";

/**
 * DFV-FR-09: how many unchanged lines sit either side of a changed region.
 *
 * Three is what `git diff` defaults to, and matching it is the point — a hunk
 * here should look like a hunk there.
 */
const CONTEXT_LINES = 3;

/**
 * The unified hunks between two revisions.
 *
 * Rows come from the same aligner the side-by-side and rich modes use
 * (`alignLines`), so the three modes cannot disagree about what changed: a line
 * paired there is a context line here, and a line the aligner left unpaired is
 * an addition or a removal in both.
 */
export function hunksBetween(
  oldText: string | null,
  newText: string | null,
): DiffPayload {
  const rows = alignLines(oldText, newText);

  /**
   * The rows flattened into unified order: a replaced pair reads as its removal
   * followed by its addition, which is the order a reader expects and the order
   * `DFV-FR-32`'s word marking pairs them back up in.
   */
  const lines: Array<DiffLine & { changed: boolean }> = [];
  for (const row of rows) {
    if (!row.changed && row.old && row.new) {
      lines.push({
        kind: "context",
        oldLineno: row.old.lineno,
        newLineno: row.new.lineno,
        content: row.old.content,
        changed: false,
      });
      continue;
    }
    if (row.old) {
      lines.push({
        kind: "del",
        oldLineno: row.old.lineno,
        content: row.old.content,
        changed: true,
      });
    }
    if (row.new) {
      lines.push({
        kind: "add",
        newLineno: row.new.lineno,
        content: row.new.content,
        changed: true,
      });
    }
  }

  // The indices of every changed line, which is what the windows are grown
  // around. No changes at all means no hunks — DFV-FR-28's empty state, and the
  // reason this returns a payload rather than throwing on an identical pair.
  const changedAt = lines.flatMap((line, index) => (line.changed ? [index] : []));
  if (changedAt.length === 0) return { isBinary: false, hunks: [] };

  // Merge the context windows: two changes closer together than twice the
  // context belong in one hunk rather than two that would overlap.
  const windows: Array<[number, number]> = [];
  for (const index of changedAt) {
    const start = Math.max(0, index - CONTEXT_LINES);
    const end = Math.min(lines.length - 1, index + CONTEXT_LINES);
    const last = windows[windows.length - 1];
    if (last && start <= last[1] + 1) {
      last[1] = Math.max(last[1], end);
    } else {
      windows.push([start, end]);
    }
  }

  const hunks: DiffHunk[] = windows.map(([start, end]) => {
    const slice = lines.slice(start, end + 1);
    return {
      header: hunkHeader(slice),
      lines: slice.map(({ changed: _changed, ...line }) => line),
    };
  });
  return { isBinary: false, hunks };
}

/**
 * The `@@ -a,b +c,d @@` line, in Git's own spelling.
 *
 * A side with no lines at all reports a start of `0`, which is what Git does
 * for a file that has no such revision — an addition's old side, and a
 * deletion's new one.
 */
function hunkHeader(lines: DiffLine[]): string {
  const olds = lines.flatMap((l) => (l.oldLineno == null ? [] : [l.oldLineno]));
  const news = lines.flatMap((l) => (l.newLineno == null ? [] : [l.newLineno]));
  const span = (numbers: number[]): string =>
    numbers.length === 0 ? "0,0" : `${numbers[0]},${numbers.length}`;
  return `@@ -${span(olds)} +${span(news)} @@`;
}
