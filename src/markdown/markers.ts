/**
 * The Markdown markers that HTML comments carry (EDT-FR-QALB, EDT-FR-XPMC,
 * EDT-FR-UYPS).
 *
 * Markdown has no syntax for vertical space or for alignment, so the author
 * writes both as HTML comments and the file stays valid Markdown. This module
 * is the one grammar for them. The WYSIWYG surface, the static block parser of
 * the Diff tab and of the Document tab, and their renderers all read a line
 * with it, so a marker means the same thing on each rich surface.
 *
 * It is pure: it reads text and gives back what the text means. It never
 * interprets comment text as HTML (EDT-FR-KMAR).
 */

/** EDT-FR-QALB: the four levels of a vertical break. V1 is the biggest. */
export type BreakLevel = 1 | 2 | 3 | 4;

/** EDT-FR-XPMC: the alignments a marker can set. Left has no marker. */
export type MarkerAlignment = "center" | "right";

/** EDT-FR-GVJM: the alignment of a block, `null` being the default left. */
export type BlockAlignment = MarkerAlignment | null;

/** EDT-FR-CSKH: the height of each spacer, in body line heights. */
export const BREAK_LINE_HEIGHTS: Record<BreakLevel, number> = {
  1: 12,
  2: 8,
  3: 5,
  4: 3,
};

export const BREAK_LEVELS: readonly BreakLevel[] = [1, 2, 3, 4];

/** What one line of Markdown is, when it is a marker. */
export type MarkerLine =
  | { kind: "break"; level: BreakLevel }
  | { kind: "align"; value: MarkerAlignment };

// EDT-FR-QALB / EDT-FR-XPMC: up to three spaces of indent, as a CommonMark
// HTML block allows, spaces or tabs inside the comment, and nothing after it
// but white space. Case is significant: the canonical form is lowercase.
// A carriage return at the end of a CRLF line is white space too.
const BREAK_RE = /^ {0,3}<!--[ \t]*(%+)[ \t]*-->[ \t\r]*$/;
const ALIGN_RE = /^ {0,3}<!--[ \t]*(center|right)[ \t]*-->[ \t\r]*$/;

/** EDT-FR-QALB: five or more `%` is V4. */
function levelOf(percents: number): BreakLevel {
  return Math.min(Math.max(percents, 1), 4) as BreakLevel;
}

/**
 * EDT-FR-QALB, EDT-FR-XPMC: what a single line means, or `null` when it is not
 * a break and not an alignment marker. The caller decides whether the line
 * stands at the top level (EDT-FR-ANTD).
 */
export function classifyMarkerLine(line: string): MarkerLine | null {
  const brk = BREAK_RE.exec(line);
  if (brk) return { kind: "break", level: levelOf(brk[1].length) };
  const align = ALIGN_RE.exec(line);
  if (align) return { kind: "align", value: align[1] as MarkerAlignment };
  return null;
}

/** EDT-FR-OZPO: the canonical spelling of a break of `level`. */
export function canonicalBreak(level: BreakLevel): string {
  return `<!-- ${"%".repeat(level)} -->`;
}

/** EDT-FR-OZPO: the canonical spelling of an alignment marker. */
export function canonicalAlign(value: MarkerAlignment): string {
  return `<!-- ${value} -->`;
}

/** EDT-FR-XKTS / EDT-FR-HDCZ: the accessible name of a break of `level`. */
export function breakLabel(level: BreakLevel): string {
  return `Vertical break V${level}`;
}

/** EDT-FR-UYPS: the accessible name of a chip. */
export const COMMENT_CHIP_LABEL = "HTML comment";

/** A line that starts an HTML comment, after an indent of up to 3 spaces. */
export function startsComment(line: string): boolean {
  return /^ {0,3}<!--/.test(line);
}

/**
 * EDT-FR-UYPS: the HTML comment that starts at `lines[start]` and stands as a
 * block of its own, or `null`.
 *
 * The comment opens at the start of the line and ends at the first `-->` after
 * its `<!--`, on that line or on a later one. Only white space may follow the
 * close on its line, else the comment is inline text of a paragraph. A comment
 * that never closes is not a comment. `end` is the index after its last line.
 */
export function scanBlockComment(
  lines: readonly string[],
  start: number,
): { end: number; raw: string } | null {
  const first = lines[start];
  if (first === undefined || !startsComment(first)) return null;
  const open = first.indexOf("<!--");
  const body: string[] = [];
  for (let i = start; i < lines.length; i++) {
    const line = lines[i];
    const from = i === start ? open + 4 : 0;
    const close = line.indexOf("-->", from);
    body.push(i === start ? line.slice(open) : line);
    if (close === -1) continue;
    if (line.slice(close + 3).trim() !== "") return null;
    body[body.length - 1] = (i === start ? line.slice(open, close + 3) : line.slice(0, close + 3));
    return { end: i + 1, raw: body.join("\n") };
  }
  return null;
}

/**
 * EDT-FR-UYPS: split inline text into prose and the comments inside it, in
 * order. A `<!--` with no `-->` after it is prose.
 */
export function splitInlineComments(
  text: string,
): Array<{ kind: "text"; text: string } | { kind: "comment"; raw: string }> {
  const out: Array<{ kind: "text"; text: string } | { kind: "comment"; raw: string }> = [];
  let at = 0;
  while (at < text.length) {
    const open = text.indexOf("<!--", at);
    if (open === -1) break;
    const close = text.indexOf("-->", open + 4);
    if (close === -1) break;
    if (open > at) out.push({ kind: "text", text: text.slice(at, open) });
    out.push({ kind: "comment", raw: text.slice(open, close + 3) });
    at = close + 3;
  }
  if (at < text.length) out.push({ kind: "text", text: text.slice(at) });
  return out;
}

/**
 * EDT-FR-IGZL: the text a chip tooltip shows, which is the comment text
 * without its `<!--` and `-->` delimiters, with its line breaks kept.
 */
export function commentText(raw: string): string {
  const lines = raw.replace(/^<!--/, "").replace(/-->$/, "").split("\n");
  while (lines.length > 0 && lines[0].trim() === "") lines.shift();
  while (lines.length > 0 && lines[lines.length - 1].trim() === "") lines.pop();
  // The first line follows `<!--` on its own line, so its indent is not the
  // indent of the block; only the lines after it share one.
  const indents = lines
    .slice(1)
    .filter((line) => line.trim() !== "")
    .map((line) => /^[ \t]*/.exec(line)![0].length);
  const common = indents.length > 0 ? Math.min(...indents) : 0;
  return lines
    .map((line, i) => (i === 0 ? line.trim() : line.slice(common).trimEnd()))
    .join("\n")
    .trim();
}
