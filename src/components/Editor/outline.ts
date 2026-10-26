/**
 * EDT-FR-PNGU / EDT-FR-JVXN / EDT-FR-OCJG: the headings that the table of
 * contents panel lists, read from either editing surface.
 *
 * Pure functions over the document, so the panel computes a list only while it
 * is open and a test can read one without mounting an editor.
 */
import type { Node as PMNode } from "@tiptap/pm/model";
import { parseInline } from "../../diff/markdown";
import { FRONTMATTER_RE } from "../markdownFidelity";

export type HeadingLevel = 1 | 2 | 3 | 4 | 5 | 6;

/** Where a selected entry moves the caret to (EDT-FR-YSJX). */
export type OutlineTarget =
  /** The position of the heading node in the rich body. */
  | { kind: "rich"; pos: number }
  /** The offset of the start of the heading line in the source text. */
  | { kind: "source"; offset: number };

export interface OutlineEntry {
  level: HeadingLevel;
  /** The heading text as plain text, with no inline formatting. */
  text: string;
  target: OutlineTarget;
}

function asLevel(value: unknown): HeadingLevel | null {
  return typeof value === "number" && value >= 1 && value <= 6
    ? (value as HeadingLevel)
    : null;
}

/**
 * EDT-FR-JVXN: the headings of the rich body, in document order.
 *
 * The frontmatter region is not part of the body, and a code block holds text
 * only, so neither can give an entry. The walk does not go into a textblock,
 * which holds no heading.
 */
export function richHeadings(doc: PMNode): OutlineEntry[] {
  const entries: OutlineEntry[] = [];
  doc.descendants((node, pos) => {
    if (node.type.name === "heading") {
      const level = asLevel(node.attrs.level);
      if (level !== null) {
        entries.push({
          level,
          text: node.textContent.trim(),
          target: { kind: "rich", pos },
        });
      }
      return false;
    }
    return !node.isTextblock;
  });
  return entries;
}

/** An opening or closing code fence: three or more backticks or tildes. */
const FENCE_RE = /^ {0,3}(`{3,}|~{3,})(.*)$/;
/**
 * The block quote markers and the list marker that a heading line can carry,
 * so a heading in a quote or a list item is listed, as the rich body lists it.
 */
const CONTAINER_RE =
  /^ {0,3}(?:(?:>[ \t]?)+(?:(?:[-*+]|\d{1,9}[.)])[ \t]+)?|(?:[-*+]|\d{1,9}[.)])[ \t]+)/;
/** An ATX heading: one to six `#`, then a space or the end of the line. */
const ATX_RE = /^ {0,3}(#{1,6})(?:[ \t]+(.*?))?[ \t]*$/;
/** An optional closing sequence of `#`, which a space must come before. */
const CLOSING_RE = /(?:^|[ \t]+)#+[ \t]*$/;

/** EDT-FR-PNGU: the text of a heading line with its inline syntax removed. */
export function plainHeadingText(source: string): string {
  const withoutImages = source.replace(/!\[([^\]]*)\]\([^)]*\)/g, "$1");
  const text = parseInline(withoutImages)
    .map((span) => span.text)
    .join("");
  return text.replace(/~~(.+?)~~/g, "$1").trim();
}

/**
 * EDT-FR-OCJG: the ATX headings of the source text, in document order.
 *
 * Lines inside the leading frontmatter block and inside a fenced code block
 * are not headings. A fence closes only on a fence of the same character that
 * is at least as long and carries nothing after it; a fence that does not
 * close holds the rest of the text. One pass over the lines, so a document of
 * several thousand lines costs nothing visible.
 */
export function sourceHeadings(text: string): OutlineEntry[] {
  const entries: OutlineEntry[] = [];
  const front = FRONTMATTER_RE.exec(text);
  let offset = front ? front[0].length : 0;
  let fence: { char: string; length: number } | null = null;

  while (offset <= text.length) {
    const newline = text.indexOf("\n", offset);
    const end = newline === -1 ? text.length : newline;
    // The markers of a block quote or a list item come off first, so a fence
    // and a heading in a quote or a list are read as the rich body reads them.
    const line = text.slice(offset, end).replace(/\r$/, "").replace(CONTAINER_RE, "");

    const fenceMatch = FENCE_RE.exec(line);
    if (fence) {
      if (
        fenceMatch &&
        fenceMatch[1][0] === fence.char &&
        fenceMatch[1].length >= fence.length &&
        fenceMatch[2].trim() === ""
      ) {
        fence = null;
      }
    } else if (fenceMatch && !(fenceMatch[1][0] === "`" && fenceMatch[2].includes("`"))) {
      fence = { char: fenceMatch[1][0], length: fenceMatch[1].length };
    } else {
      const heading = ATX_RE.exec(line);
      if (heading) {
        const content = (heading[2] ?? "").replace(CLOSING_RE, "");
        entries.push({
          level: heading[1].length as HeadingLevel,
          text: plainHeadingText(content),
          target: { kind: "source", offset },
        });
      }
    }

    if (newline === -1) break;
    offset = newline + 1;
  }
  return entries;
}
