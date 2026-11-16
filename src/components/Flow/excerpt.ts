import { parseInline, parseMarkdownBlocks } from "../../diff/markdown";

/** The longest excerpt kept. Two lines of a card never show more. */
const EXCERPT_MAX = 280;

/**
 * FLO-FR-XCRP: the excerpt of a description: the text of its first Markdown
 * block that is not a heading, with the Markdown marks removed and each run
 * of white space made one space. `null` where no such block has text.
 */
export function descriptionExcerpt(markdown: string | null | undefined): string | null {
  if (!markdown) return null;
  for (const block of parseMarkdownBlocks(markdown)) {
    if (block.kind === "heading") continue;
    const plain = parseInline(block.text)
      .map((span) => span.text)
      .join("")
      .replace(/\s+/g, " ")
      .trim();
    if (plain.length === 0) continue;
    return plain.length > EXCERPT_MAX ? `${plain.slice(0, EXCERPT_MAX).trimEnd()}…` : plain;
  }
  return null;
}
