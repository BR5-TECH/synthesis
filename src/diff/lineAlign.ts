/**
 * Pairing two revisions into aligned rows (`DFV-diff-viewer.md` DFV-FR-11 /
 * DFV-FR-12).
 *
 * The side-by-side mode shows each revision's file **in full**, not only its
 * changed regions, and a line sits opposite its counterpart down the whole
 * file. Where one side has no counterpart the other carries an inert filler
 * row — that is what keeps the alignment from drifting after the first
 * unbalanced change.
 *
 * The backend computes the hunks the unified mode renders; it does not pair
 * whole files, because the pairing is a property of how this surface lays a
 * comparison out rather than of the comparison itself. So the pairing is here,
 * as a pure function over the two texts `"get file revisions for comparison"`
 * returns (DFV-FR-25).
 */

/** One revision's half of an aligned row. */
export interface AlignedSide {
  /** 1-based line number in that revision (DFV-FR-12: its own, never the other's). */
  lineno: number;
  content: string;
}

/**
 * One aligned row: a unit of the old revision opposite its counterpart in the
 * new one. A `null` side is the inert filler of DFV-FR-11 — no number, no
 * content.
 *
 * Generic over the unit, because rich rendering aligns *blocks* on exactly the
 * same terms that source rendering aligns lines (DFV-FR-21 defers to DFV-FR-12
 * for its marking and to DFV-FR-19 only for its granularity).
 */
export interface AlignedPair<T> {
  old: T | null;
  new: T | null;
  /**
   * DFV-FR-12: whether this row carries change marking. An unchanged unit is
   * marked on neither side; every other row is marked on the side(s) present —
   * the left showing what the change acted on, the right showing the outcome.
   */
  changed: boolean;
}

/** One row of a side-by-side source view. */
export type AlignedRow = AlignedPair<AlignedSide>;

/**
 * A file's lines. A trailing newline terminates the last line rather than
 * starting an empty one, so `"a\n"` is one line and `""` is none.
 *
 * Line terminators are **normalised on both sides** before anything is compared
 * (`DFV-diff-viewer.md` non-functional requirements, on the terms
 * `../../specifications/core/GTC-git.md` GTC-FR-17 normalises them), so a
 * checkout convention that differs from the committed blob's renders as no
 * change rather than as every line changed — and a `\r` never reaches the
 * screen as a character of the line's content. The bytes a write puts on disk
 * are unaffected: their convention is the file's own and is preserved by the
 * splice that edits it.
 */
export function splitLines(text: string): string[] {
  if (text === "") return [];
  const lines = text
    .split("\n")
    .map((line) => (line.endsWith("\r") ? line.slice(0, -1) : line));
  if (lines[lines.length - 1] === "") lines.pop();
  return lines;
}

/**
 * The quadratic table is bounded: past this many cells the two revisions have
 * so little in common that a precise pairing costs more than it is worth, and
 * the middle is rendered as one wholesale replacement instead. The rows are
 * still aligned and still complete — only the pairing inside the changed region
 * is coarser.
 */
const LCS_CELL_BUDGET = 4_000_000;

/** Longest-common-subsequence lengths for `a[0..)` against `b[0..)`. */
function lcsTable(a: readonly string[], b: readonly string[]): Uint32Array {
  const width = b.length + 1;
  const table = new Uint32Array((a.length + 1) * width);
  for (let i = a.length - 1; i >= 0; i--) {
    for (let j = b.length - 1; j >= 0; j--) {
      table[i * width + j] =
        a[i] === b[j]
          ? table[(i + 1) * width + (j + 1)] + 1
          : Math.max(table[(i + 1) * width + j], table[i * width + (j + 1)]);
    }
  }
  return table;
}

/**
 * One step of the edit script. Each op carries the index it occupies in the
 * revision(s) it belongs to, so a row's line numbers fall straight out of it
 * rather than being recounted from the rows built so far.
 */
type Op =
  | { kind: "equal"; oldIndex: number; newIndex: number }
  | { kind: "del"; oldIndex: number }
  | { kind: "add"; newIndex: number };

/**
 * How much of two sequences is a shared head and a shared tail.
 *
 * This is what keeps the quadratic table from being built at all for the
 * ordinary case — one edit inside an otherwise identical sequence — so it is
 * also how a caller can price the alignment before asking for it.
 */
export function commonExtent(
  a: readonly string[],
  b: readonly string[],
): { head: number; tail: number } {
  let head = 0;
  while (head < a.length && head < b.length && a[head] === b[head]) head++;
  let tail = 0;
  while (
    tail < a.length - head &&
    tail < b.length - head &&
    a[a.length - 1 - tail] === b[b.length - 1 - tail]
  ) {
    tail++;
  }
  return { head, tail };
}

/** The edit script pairing `a` into `b`, in file order. */
function editScript(a: readonly string[], b: readonly string[]): Op[] {
  const ops: Op[] = [];

  const { head, tail } = commonExtent(a, b);
  for (let i = 0; i < head; i++) {
    ops.push({ kind: "equal", oldIndex: i, newIndex: i });
  }

  const midA = a.slice(head, a.length - tail);
  const midB = b.slice(head, b.length - tail);

  const wholesale =
    midA.length === 0 ||
    midB.length === 0 ||
    midA.length * midB.length > LCS_CELL_BUDGET;

  if (wholesale) {
    for (let i = 0; i < midA.length; i++) {
      ops.push({ kind: "del", oldIndex: head + i });
    }
    for (let j = 0; j < midB.length; j++) {
      ops.push({ kind: "add", newIndex: head + j });
    }
  } else {
    const width = midB.length + 1;
    const table = lcsTable(midA, midB);
    let i = 0;
    let j = 0;
    while (i < midA.length && j < midB.length) {
      if (midA[i] === midB[j]) {
        ops.push({ kind: "equal", oldIndex: head + i, newIndex: head + j });
        i++;
        j++;
      } else if (table[(i + 1) * width + j] >= table[i * width + (j + 1)]) {
        ops.push({ kind: "del", oldIndex: head + i });
        i++;
      } else {
        ops.push({ kind: "add", newIndex: head + j });
        j++;
      }
    }
    while (i < midA.length) ops.push({ kind: "del", oldIndex: head + i++ });
    while (j < midB.length) ops.push({ kind: "add", newIndex: head + j++ });
  }

  for (let k = 0; k < tail; k++) {
    ops.push({
      kind: "equal",
      oldIndex: a.length - tail + k,
      newIndex: b.length - tail + k,
    });
  }
  return ops;
}

/**
 * DFV-FR-11 / DFV-FR-12: pair two sequences into aligned rows, matching on
 * `key`. Two units are the same unit when their keys are identical.
 */
export function alignBy<T>(
  a: readonly T[],
  b: readonly T[],
  key: (item: T) => string,
): AlignedPair<T>[] {
  const rows: AlignedPair<T>[] = [];

  // Removals and additions in one changed region are paired row-for-row, so a
  // unit and the unit that replaced it read opposite each other; whichever side
  // runs out first faces filler for the surplus (DFV-FR-11, DFV-FR-12).
  let pendingDel: T[] = [];
  let pendingAdd: T[] = [];
  const flush = () => {
    const width = Math.max(pendingDel.length, pendingAdd.length);
    for (let k = 0; k < width; k++) {
      rows.push({
        old: pendingDel[k] ?? null,
        new: pendingAdd[k] ?? null,
        changed: true,
      });
    }
    pendingDel = [];
    pendingAdd = [];
  };

  for (const op of editScript(a.map(key), b.map(key))) {
    if (op.kind === "equal") {
      flush();
      rows.push({ old: a[op.oldIndex], new: b[op.newIndex], changed: false });
    } else if (op.kind === "del") {
      pendingDel.push(a[op.oldIndex]);
    } else {
      pendingAdd.push(b[op.newIndex]);
    }
  }
  flush();
  return rows;
}

/**
 * DFV-FR-11 / DFV-FR-12: pair two revisions' lines into aligned rows.
 *
 * `null` on a side means the comparison has no version of the file there —
 * every line of the other side is then a change (DFV-FR-15).
 */
export function alignLines(
  oldText: string | null,
  newText: string | null,
): AlignedRow[] {
  const number = (text: string | null): AlignedSide[] =>
    text == null
      ? []
      : splitLines(text).map((content, index) => ({
          lineno: index + 1,
          content,
        }));
  return alignBy(number(oldText), number(newText), (side) => side.content);
}
