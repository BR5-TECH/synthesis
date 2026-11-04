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
 *
 * HTML comments are blocks of their own (DFV-FR-SZZX): a vertical break, and
 * an ordinary comment on lines of its own, which renders as a chip. An
 * alignment marker is part of the block it aligns (EDT-FR-XPMC).
 */

import {
  classifyMarkerLine,
  scanBlockComment,
  startsComment,
  type MarkerAlignment,
} from "../markdown/markers";

export type BlockKind =
  | "heading"
  | "paragraph"
  | "list-item"
  | "table-row"
  | "quote"
  | "code"
  | "rule"
  | "break"
  | "comment";

export interface MarkdownBlock {
  kind: BlockKind;
  /**
   * The block's literal source. This is its identity when the two revisions are
   * aligned: two blocks are the same block when their source is identical.
   */
  source: string;
  /** The text to render, with the construct's own syntax stripped. */
  text: string;
  /** Heading level 1..6, or the level 1..4 of a vertical break (EDT-FR-QALB). */
  level?: number;
  /**
   * DFV-FR-SZZX / EDT-FR-XPMC: the alignment of the block, set by the marker
   * line directly above it. The marker line is part of `source` and of `line`,
   * so a changed marker marks this block.
   */
  align?: MarkerAlignment;
  /**
   * True on the block whose `source` holds the alignment marker. The items of
   * an aligned list and the rows of an aligned table carry the `align` of the
   * first one, because a list and a table align as a whole (EDT-FR-GVJM).
   */
  alignMarker?: boolean;
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

/** The column the content of a list item starts at, after its marker. */
function itemContentIndent(block: MarkdownBlock): number {
  const lines = block.source.split("\n");
  const first = lines[block.alignMarker ? 1 : 0] ?? "";
  const match = /^(\s*)([-*+]|\d+[.)])(\s+)/.exec(first);
  if (!match) return 2;
  const gap = match[3].length > 4 ? 1 : match[3].length;
  return match[1].length + match[2].length + gap;
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

  const isLineStart = (line: string) =>
    HEADING.test(line) ||
    FENCE.test(line) ||
    RULE.test(line) ||
    LIST_ITEM.test(line) ||
    QUOTE.test(line) ||
    TABLE_ROW.test(line) ||
    line.trim() === "";
  // EDT-FR-UYPS: a comment on lines of its own interrupts a paragraph.
  const isBlockStart = (at: number) =>
    isLineStart(lines[at]) || (startsComment(lines[at]) && scanBlockComment(lines, at) !== null);

  // EDT-FR-XPMC: the marker waiting for the block on the next line.
  let pendingAlign: { value: MarkerAlignment; source: string; line: number } | null = null;
  const push = (block: MarkdownBlock) => {
    if (pendingAlign) {
      block.align = pendingAlign.value;
      block.alignMarker = true;
      block.source = `${pendingAlign.source}\n${block.source}`;
      block.line = pendingAlign.line;
      pendingAlign = null;
    }
    // EDT-FR-GVJM: the rest of an aligned list or table takes its alignment.
    // A table ends at a blank line. A list goes on over one while its items
    // keep the kind of their marker; another marker starts another list.
    const previous = blocks[blocks.length - 1];
    const adjacent =
      previous !== undefined && previous.line + previous.source.split("\n").length === block.line;
    const sameList =
      block.kind === "list-item" &&
      previous?.kind === "list-item" &&
      (adjacent ||
        (previous.ordered === block.ordered &&
          (block.ordered || previous.marker === block.marker)));
    const sameTable = block.kind === "table-row" && previous?.kind === "table-row" && adjacent;
    if (!block.align && previous?.align && (sameList || sameTable)) {
      block.align = previous.align;
    }
    blocks.push(block);
  };

  while (i < lines.length) {
    const line = lines[i];
    const start = i;

    if (line.trim() === "") {
      i++;
      continue;
    }

    // EDT-FR-ANTD: a comment indented to the content of the list item above
    // it is inside that item, as the editor reads it. It joins the item, so it
    // is no marker and no block of its own (DFV-FR-SZZX).
    const last = blocks[blocks.length - 1];
    const lead = /^ */.exec(line)![0].length;
    if (last?.kind === "list-item" && lead >= itemContentIndent(last) && line.trimStart().startsWith("<!--")) {
      const inner = scanBlockComment([line.slice(lead), ...lines.slice(i + 1)], 0);
      if (inner) {
        const end = i + inner.end;
        const from = last.line + last.source.split("\n").length;
        last.source = [last.source, ...lines.slice(from, end)].join("\n");
        last.text = `${last.text} ${inner.raw}`;
        i = end;
        continue;
      }
    }

    const comment = startsComment(line) ? scanBlockComment(lines, i) : null;
    if (comment) {
      const marker = comment.end === i + 1 ? classifyMarkerLine(line) : null;
      if (marker?.kind === "break") {
        push({ kind: "break", source: line, text: "", level: marker.level, line: start });
        i = comment.end;
        continue;
      }
      if (marker?.kind === "align") {
        const next = lines[i + 1];
        if (next !== undefined && next.trim() !== "" && !startsComment(next)) {
          pendingAlign = { value: marker.value, source: line, line: start };
          i++;
          continue;
        }
      }
      push({
        kind: "comment",
        source: lines.slice(i, comment.end).join("\n"),
        text: comment.raw,
        line: start,
      });
      i = comment.end;
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
      push({
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
      push({
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
      push({ kind: "rule", source: line, text: "", line: start });
      i++;
      continue;
    }

    if (TABLE_ROW.test(line)) {
      const inner = line.match(TABLE_ROW)![1];
      push({
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
      push({
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
        !isBlockStart(i) &&
        lines[i].startsWith(" ")
      ) {
        source.push(lines[i]);
        body.push(lines[i].trim());
        i++;
      }
      push({
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
    while (i < lines.length && !isBlockStart(i)) {
      source.push(lines[i]);
      i++;
    }
    push({
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
  | { kind: "link"; text: string; href: string }
  /**
   * EDT-FR-UYPS: an HTML comment inside the text, shown as a chip. It has no
   * text of its own: what a reader sees of it is the chip.
   */
  | { kind: "comment"; text: ""; raw: string };

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
  /(<!--[\s\S]*?--!?>)|(`[^`]+`)|(\*\*(?:[^\s*][^*]*[^\s*]|[^\s*])\*\*)|(__(?:[^\s_][^_]*[^\s_]|[^\s_])__)|(\*(?:[^\s*][^*]*[^\s*]|[^\s*])\*)|(_(?:[^\s_][^_]*[^\s_]|[^\s_])_)|(\[[^\]]*\]\([^)\s]*\))/;

/**
 * DFV-FR-17: the emphasis, code, and links a rendered document shows as
 * formatting rather than as syntax. Anything unrecognised stays literal text.
 */
export function parseInline(text: string): InlineSpan[] {
  const spans: InlineSpan[] = [];
  const hidden = hideEscaped(text);
  let rest = hidden;

  while (rest.length > 0) {
    const match = rest.match(INLINE);
    if (!match || match.index === undefined) break;
    if (match.index > 0) {
      spans.push({ kind: "text", text: showEscaped(rest.slice(0, match.index)) });
    }
    const token = match[0];
    if (token.startsWith("<!--")) {
      // EDT-FR-UYPS: a comment is raw text, so its backslashes stay. An
      // escape takes the same two characters as its sentinel, so the offsets
      // of the hidden text are offsets of the original.
      const at = hidden.length - rest.length + match.index;
      spans.push({ kind: "comment", text: "", raw: text.slice(at, at + token.length) });
    } else if (token.startsWith("`")) {
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
