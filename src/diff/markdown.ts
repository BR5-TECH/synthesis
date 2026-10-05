/**
 * The block model rich rendering marks change against (`DFV-diff-viewer.md`
 * DFV-FR-17 / DFV-FR-19).
 *
 * Rich rendering marks change at **block granularity**: the smallest rendered
 * block containing a change — a paragraph, heading, list item, table row, block
 * quote, or fenced code block — carries the treatment as a whole, and nothing
 * inside a block is marked. So the parse has to produce exactly those units:
 * a list is not one block but one block per item, and a table is one block per
 * row, or a one-word edit would mark a whole list or a whole table.
 *
 * This is a renderer's parse, not a specification-complete Markdown one. It
 * covers the constructs DFV-FR-17 names — headings, emphasis, lists, tables,
 * code blocks, links — because those are what a Spec or a Skill is written in.
 * Anything it does not recognise falls through to a paragraph and renders as
 * its own text, which is a legible outcome rather than a broken one.
 */

export type BlockKind =
  | "heading"
  | "paragraph"
  | "list-item"
  | "table-row"
  | "quote"
  | "code"
  | "rule";

export interface MarkdownBlock {
  kind: BlockKind;
  /**
   * The block's literal source. This is its identity when the two revisions are
   * aligned: two blocks are the same block when their source is identical.
   */
  source: string;
  /** The text to render, with the construct's own syntax stripped. */
  text: string;
  /** Heading level 1..6. */
  level?: number;
  /** List item: how deep it is nested, and its bullet or number. */
  depth?: number;
  marker?: string;
  ordered?: boolean;
  /** Fenced code: the info string, when one was given. */
  language?: string;
  /** Table row: its cells, already trimmed. */
  cells?: string[];
  /** Table rows: the `|---|---|` separator is structure, not content. */
  isDivider?: boolean;
  /**
   * 0-based index of the block's first line in the revision it was parsed from.
   *
   * What makes a rendered block addressable in the buffer behind it: the
   * editable target of a Diff tab or a review edits `source` in place
   * (DFV-FR-47), and it can only splice the edit back if it knows which lines
   * the block occupied. `line` plus `source`'s own line count is that range.
   */
  line: number;
}

const HEADING = /^(#{1,6})\s+(.*)$/;
const FENCE = /^(\s*)(`{3,}|~{3,})\s*(\S*)\s*$/;
const RULE = /^\s{0,3}([-*_])\s*(?:\1\s*){2,}$/;
const LIST_ITEM = /^(\s*)([-*+]|\d+[.)])\s+(.*)$/;
const QUOTE = /^\s{0,3}>\s?(.*)$/;
const TABLE_ROW = /^\s*\|(.*)$/;
const TABLE_DIVIDER = /^[\s|:-]+$/;

function tableCells(line: string): string[] {
  return line
    .trim()
    .replace(/^\|/, "")
    .replace(/\|$/, "")
    .split("|")
    .map((cell) => cell.trim());
}

/**
 * DFV-FR-19: split Markdown into the blocks change is marked against. A `null`
 * revision (one the comparison has no version of) has no blocks at all.
 */
export function parseMarkdownBlocks(text: string | null): MarkdownBlock[] {
  if (text == null) return [];
  const lines = text.split("\n");
  // A trailing newline terminates the last line rather than starting an empty
  // one, so it must not become a line of its own inside a fenced block.
  if (lines.length > 0 && lines[lines.length - 1] === "") lines.pop();
  const blocks: MarkdownBlock[] = [];
  let i = 0;

  const isBlockStart = (line: string) =>
    HEADING.test(line) ||
    FENCE.test(line) ||
    RULE.test(line) ||
    LIST_ITEM.test(line) ||
    QUOTE.test(line) ||
    TABLE_ROW.test(line) ||
    line.trim() === "";

  while (i < lines.length) {
    const line = lines[i];
    const start = i;

    if (line.trim() === "") {
      i++;
      continue;
    }

    // A fence swallows everything up to its close, syntax included — the point
    // of a code block is that its content is not Markdown.
    const fence = line.match(FENCE);
    if (fence) {
      const [, , delimiter, language] = fence;
      const source: string[] = [line];
      const body: string[] = [];
      i++;
      while (i < lines.length) {
        const closing = lines[i].match(FENCE);
        source.push(lines[i]);
        if (closing && closing[2].startsWith(delimiter[0]) && closing[2].length >= delimiter.length) {
          i++;
          break;
        }
        body.push(lines[i]);
        i++;
      }
      blocks.push({
        kind: "code",
        source: source.join("\n"),
        text: body.join("\n"),
        language: language || undefined,
        line: start,
      });
      continue;
    }

    const heading = line.match(HEADING);
    if (heading) {
      blocks.push({
        kind: "heading",
        source: line,
        text: heading[2].trim(),
        level: heading[1].length,
        line: start,
      });
      i++;
      continue;
    }

    if (RULE.test(line)) {
      blocks.push({ kind: "rule", source: line, text: "", line: start });
      i++;
      continue;
    }

    if (TABLE_ROW.test(line)) {
      const inner = line.match(TABLE_ROW)![1];
      blocks.push({
        kind: "table-row",
        source: line,
        text: line.trim(),
        cells: tableCells(line),
        isDivider: TABLE_DIVIDER.test(inner) && inner.includes("-"),
        line: start,
      });
      i++;
      continue;
    }

    const quote = line.match(QUOTE);
    if (quote) {
      const source: string[] = [line];
      const body: string[] = [quote[1]];
      i++;
      while (i < lines.length) {
        const next = lines[i].match(QUOTE);
        if (!next) break;
        source.push(lines[i]);
        body.push(next[1]);
        i++;
      }
      blocks.push({
        kind: "quote",
        source: source.join("\n"),
        text: body.join("\n").trim(),
        line: start,
      });
      continue;
    }

    const item = line.match(LIST_ITEM);
    if (item) {
      const [, indent, marker, first] = item;
      const source: string[] = [line];
      const body: string[] = [first];
      i++;
      // A wrapped item continues on indented lines that start nothing else.
      while (
        i < lines.length &&
        lines[i].trim() !== "" &&
        !isBlockStart(lines[i]) &&
        lines[i].startsWith(" ")
      ) {
        source.push(lines[i]);
        body.push(lines[i].trim());
        i++;
      }
      blocks.push({
        kind: "list-item",
        source: source.join("\n"),
        text: body.join(" ").trim(),
        depth: Math.floor(indent.length / 2),
        marker,
        ordered: /\d/.test(marker),
        line: start,
      });
      continue;
    }

    const source: string[] = [line];
    i++;
    while (i < lines.length && !isBlockStart(lines[i])) {
      source.push(lines[i]);
      i++;
    }
    blocks.push({
      kind: "paragraph",
      source: source.join("\n"),
      text: source.join("\n").trim(),
      line: start,
    });
  }

  return blocks;
}

// ---------------------------------------------------------------------------
// Inline spans (DFV-FR-17)
// ---------------------------------------------------------------------------

export type InlineSpan =
  | { kind: "text"; text: string }
  | { kind: "strong"; text: string }
  | { kind: "em"; text: string }
  | { kind: "code"; text: string }
  | { kind: "link"; text: string; href: string };

/**
 * Ordered by precedence: code first (its content is literal), then the
 * two-character emphasis before the one-character one, then links.
 *
 * Every emphasis run requires a non-space character against each delimiter, so
 * arithmetic (`2 * 3 * 4`) and a snake_case identifier stay the text they are
 * rather than turning the line italic.
 */
/**
 * The characters a backslash may escape — exactly the ones {@link INLINE}
 * consumes, plus the backslash itself.
 *
 * Deliberately not every punctuation mark Markdown defines an escape for. This
 * renderer reads code spans, emphasis, strong emphasis and links and nothing
 * else (DFV-FR-17), so `\.` is not an escape here and stays two characters: a
 * Windows path pasted into a comment reads as it was pasted.
 */
const ESCAPABLE = "\\`*_[]";

/** A code point no document carries, marking where an escaped character stood. */
const SENTINEL = "\u0000";

/**
 * DFV-FR-17: a character a backslash protects is text rather than syntax.
 *
 * The escaped character is **replaced** by a sentinel and its index in
 * {@link ESCAPABLE}, rather than merely marked: leaving the character in place
 * would let the inline scan below read the very `*` the escape existed to hide.
 * A digit is what follows the sentinel because the scan consumes no digits.
 *
 * This pair — take the backslash out here, put the character back once the scan
 * is done — is what lets a value be escaped for Markdown **without changing its
 * text** (per `../../specifications/tools/ADQ-ask-discussion-questions-tool.md`
 * ADQ-FR-NUEB). The escaping side inserts the backslash; the reader never sees
 * one, and the formatting is never applied.
 */
function hideEscaped(text: string): string {
  if (!text.includes("\\")) return text;
  let out = "";
  for (let index = 0; index < text.length; index += 1) {
    const ch = text[index];
    const next = index + 1 < text.length ? text[index + 1] : undefined;
    const escaped = next === undefined ? -1 : ESCAPABLE.indexOf(next);
    if (ch === "\\" && escaped >= 0) {
      out += SENTINEL + String(escaped);
      index += 1;
      continue;
    }
    out += ch;
  }
  return out;
}

/** Put the escaped characters back, once the scan can no longer read them. */
function showEscaped(text: string): string {
  if (!text.includes(SENTINEL)) return text;
  let out = "";
  for (let index = 0; index < text.length; index += 1) {
    if (text[index] === SENTINEL) {
      const at = Number(text[index + 1]);
      if (Number.isInteger(at) && at >= 0 && at < ESCAPABLE.length) {
        out += ESCAPABLE[at];
        index += 1;
        continue;
      }
    }
    out += text[index];
  }
  return out;
}

const INLINE =
  /(`[^`]+`)|(\*\*(?:[^\s*][^*]*[^\s*]|[^\s*])\*\*)|(__(?:[^\s_][^_]*[^\s_]|[^\s_])__)|(\*(?:[^\s*][^*]*[^\s*]|[^\s*])\*)|(_(?:[^\s_][^_]*[^\s_]|[^\s_])_)|(\[[^\]]*\]\([^)\s]*\))/;

/**
 * DFV-FR-17: the emphasis, code, and links a rendered document shows as
 * formatting rather than as syntax. Anything unrecognised stays literal text.
 */
export function parseInline(text: string): InlineSpan[] {
  const spans: InlineSpan[] = [];
  let rest = hideEscaped(text);

  while (rest.length > 0) {
    const match = rest.match(INLINE);
    if (!match || match.index === undefined) break;
    if (match.index > 0) {
      spans.push({ kind: "text", text: showEscaped(rest.slice(0, match.index)) });
    }
    const token = match[0];
    if (token.startsWith("`")) {
      spans.push({ kind: "code", text: showEscaped(token.slice(1, -1)) });
    } else if (token.startsWith("**") || token.startsWith("__")) {
      spans.push({ kind: "strong", text: showEscaped(token.slice(2, -2)) });
    } else if (token.startsWith("[")) {
      const split = token.indexOf("](");
      spans.push({
        kind: "link",
        text: showEscaped(token.slice(1, split)),
        href: showEscaped(token.slice(split + 2, -1)),
      });
    } else {
      spans.push({ kind: "em", text: showEscaped(token.slice(1, -1)) });
    }
    rest = rest.slice(match.index + token.length);
  }

  if (rest.length > 0) spans.push({ kind: "text", text: showEscaped(rest) });
  return spans;
}
