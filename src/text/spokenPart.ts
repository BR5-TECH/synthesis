/**
 * The spoken part of a comment body (`CVL-conversation-loop.md` CVL-FR-AGNN).
 *
 * An agent comment carries one `<spoken>…</spoken>` tag, anywhere in its body.
 * Voice mode speaks that text, and a surface hides the tag. The stored body
 * keeps it. The rule matches `spoken_part` in
 * `src-tauri/src/comments/spoken_part.rs`.
 */

const OPEN = "<spoken>";
const CLOSE = "</spoken>";

interface Range {
  start: number;
  end: number;
}

/** A complete tag: where it stands, and where its text stands. */
interface SpokenTag {
  outer: Range;
  inner: Range;
}

/** The one set of space that both parsers trim. See `is_space` in the Rust one. */
const SPACE = "[\\s\\u0085]";
const LEADING = new RegExp(`^${SPACE}+`);
const TRAILING = new RegExp(`${SPACE}+$`);
const HOLDS_SPACE = new RegExp(SPACE);

function trimmed(text: string): string {
  return text.replace(LEADING, "").replace(TRAILING, "");
}

/**
 * The marker of a fence line. This is the rule of the Markdown renderer
 * (`../diff/markdown.ts`), so a tag counts as code where the author sees code:
 * any indent and at most one word after the marker. A backtick after the marker
 * makes the line inline code.
 */
function fenceOf(line: string): { marker: string; length: number } | null {
  const start = line.replace(LEADING, "");
  const marker = start[0];
  if (marker !== "`" && marker !== "~") return null;
  let length = 0;
  while (start[length] === marker) length += 1;
  const info = trimmed(start.slice(length));
  if (length < 3 || HOLDS_SPACE.test(info)) return null;
  return marker === "`" && info.includes("`") ? null : { marker, length };
}

function backtickRun(text: string, from: number): number {
  let length = 0;
  while (text[from + length] === "`") length += 1;
  return length;
}

/**
 * The inline code spans of one paragraph. A run of backticks closes at the next
 * run of the same length, and a run with no such partner is plain text.
 */
function inlineCode(paragraph: string, offset: number, ranges: Range[]): void {
  let at = 0;
  while (at < paragraph.length) {
    if (paragraph[at] !== "`") {
      at += 1;
      continue;
    }
    const length = backtickRun(paragraph, at);
    let probe = at + length;
    let close = -1;
    while (probe < paragraph.length) {
      if (paragraph[probe] !== "`") {
        probe += 1;
        continue;
      }
      const run = backtickRun(paragraph, probe);
      if (run === length) {
        close = probe + run;
        break;
      }
      probe += run;
    }
    if (close < 0) {
      at += length;
    } else {
      ranges.push({ start: offset + at, end: offset + close });
      at = close;
    }
  }
}

/** CVL-FR-AGNN: fenced blocks with their fence lines, and inline code spans. */
function codeRanges(text: string): Range[] {
  const ranges: Range[] = [];
  let fence: { marker: string; length: number; start: number } | null = null;
  let paragraph = 0;
  let offset = 0;
  while (offset < text.length) {
    const newline = text.indexOf("\n", offset);
    const next = newline < 0 ? text.length : newline + 1;
    const line = text.slice(offset, next).replace(/[\r\n]+$/, "");
    const marker = fenceOf(line);
    if (fence !== null) {
      if (marker !== null && marker.marker === fence.marker && marker.length >= fence.length) {
        ranges.push({ start: fence.start, end: next });
        fence = null;
        paragraph = next;
      }
    } else if (marker !== null || trimmed(line) === "") {
      inlineCode(text.slice(paragraph, offset), paragraph, ranges);
      paragraph = next;
      fence = marker === null ? null : { ...marker, start: offset };
    }
    offset = next;
  }
  if (fence !== null) ranges.push({ start: fence.start, end: text.length });
  else inlineCode(text.slice(paragraph), paragraph, ranges);
  return ranges;
}

function findOutside(text: string, needle: string, from: number, code: Range[]): number {
  let at = text.indexOf(needle, from);
  while (at >= 0) {
    const found = at;
    if (!code.some((range) => found >= range.start && found < range.end)) return found;
    at = text.indexOf(needle, found + needle.length);
  }
  return -1;
}

/** CVL-FR-AGNN: every complete tag outside code, in order. */
function spokenTags(body: string): SpokenTag[] {
  if (!body.includes(OPEN)) return [];
  const code = codeRanges(body);
  const tags: SpokenTag[] = [];
  let from = 0;
  for (;;) {
    const open = findOutside(body, OPEN, from, code);
    if (open < 0) return tags;
    const start = open + OPEN.length;
    const close = findOutside(body, CLOSE, start, code);
    if (close < 0) return tags;
    from = close + CLOSE.length;
    tags.push({ outer: { start: open, end: from }, inner: { start, end: close } });
  }
}

/**
 * CVL-FR-AGNN: the trimmed text of the first complete tag outside code. A blank
 * text, a tag that does not close, and a body with every tag in code give null.
 */
export function spokenPart(body: string): string | null {
  const [first] = spokenTags(body);
  if (!first) return null;
  const text = trimmed(body.slice(first.inner.start, first.inner.end));
  return text === "" ? null : text;
}

/**
 * CMT-FR-BMCI, CMT-FR-XWVW: `body` as a person reads it. Every complete tag goes
 * with its text, except that a body with nothing else shows the spoken part. A
 * body with no tag is returned as it is.
 */
export function visibleBody(body: string): string {
  const tags = spokenTags(body);
  if (tags.length === 0) return body;
  const written = trimmed(writtenText(body, tags));
  return written === "" ? (spokenPart(body) ?? "") : written;
}

function writtenText(body: string, tags: SpokenTag[]): string {
  let written = "";
  let at = 0;
  for (const tag of tags) {
    written += body.slice(at, tag.outer.start);
    at = tag.outer.end;
  }
  return written + body.slice(at);
}

/** CVL-FR-JPHT: `body` less every complete tag outside code. */
export function withoutSpokenTags(body: string): string {
  return writtenText(body, spokenTags(body));
}
