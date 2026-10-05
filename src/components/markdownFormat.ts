/**
 * The text transformations behind the draft editor's formatting toolbar
 * (`../../specifications/ui/NAW-new-artifact.md` NAW-FR-10).
 *
 * Kept pure and separate from the component: a formatting action is a function
 * from (text, selection) to (text, selection), and every edge worth caring
 * about — toggling a mark off, switching one heading level for another, running
 * a list across several lines — is a property of that function rather than of
 * the textarea it happens to be wired to.
 */

export type MarkdownAction =
  | "bold"
  | "italic"
  | "code"
  | "h1"
  | "h2"
  | "h3"
  | "bullet"
  | "ordered"
  | "quote";

export interface TextRange {
  text: string;
  start: number;
  end: number;
}

/** The marker each inline action wraps its selection in. */
const WRAP: Partial<Record<MarkdownAction, string>> = {
  bold: "**",
  italic: "*",
  code: "`",
};

/** The prefix each block action puts at the head of every line it spans. */
const BLOCK: Partial<Record<MarkdownAction, string>> = {
  h1: "# ",
  h2: "## ",
  h3: "### ",
  bullet: "- ",
  quote: "> ",
};

/**
 * Every prefix a block action replaces. A line already carrying one of these is
 * re-marked rather than accumulating a second prefix, so `## x` becoming a
 * bullet is `- x` and not `- ## x`.
 */
const BLOCK_PREFIX = /^(\s*)(#{1,6} +|[-*+] +|\d+\. +|> ?)?/;

function isMarked(range: TextRange, marker: string): boolean {
  const { text, start, end } = range;
  const outside =
    text.slice(start - marker.length, start) === marker &&
    text.slice(end, end + marker.length) === marker;
  if (!outside) return false;
  // `*` is a prefix of `**`, so a bold span reads as italic-marked unless the
  // character beyond the marker is checked. Without this, hitting Italic on
  // bold text peels one asterisk off each side and leaves broken Markdown.
  if (marker === "*") {
    const before = text.slice(start - 2, start - 1);
    const after = text.slice(end + 1, end + 2);
    if (before === "*" || after === "*") return false;
  }
  return true;
}

function toggleInline(range: TextRange, marker: string): TextRange {
  const { text, start, end } = range;
  if (isMarked(range, marker)) {
    return {
      text:
        text.slice(0, start - marker.length) +
        text.slice(start, end) +
        text.slice(end + marker.length),
      start: start - marker.length,
      end: end - marker.length,
    };
  }
  const selected = text.slice(start, end);
  return {
    text: text.slice(0, start) + marker + selected + marker + text.slice(end),
    // The selection keeps naming the same characters, so a second activation
    // of the same button removes exactly what the first added.
    start: start + marker.length,
    end: end + marker.length,
  };
}

/** The line boundaries the selection sits inside. */
function lineSpan(text: string, start: number, end: number): [number, number] {
  const from = text.lastIndexOf("\n", start - 1) + 1;
  const next = text.indexOf("\n", end);
  return [from, next === -1 ? text.length : next];
}

function alreadyBlock(line: string, action: MarkdownAction): boolean {
  if (action === "ordered") return /^\s*\d+\. /.test(line);
  const prefix = BLOCK[action]!;
  if (action === "bullet") return /^\s*[-*+] /.test(line);
  return line.trimStart().startsWith(prefix.trimEnd() + " ");
}

function toggleBlock(range: TextRange, action: MarkdownAction): TextRange {
  const { text, start, end } = range;
  const [from, to] = lineSpan(text, start, end);
  const lines = text.slice(from, to).split("\n");
  // Removing only when *every* line that could carry the mark already does: a
  // partly-marked selection is completed rather than cleared, which is what an
  // author dragging across a half-formatted list expects. Blank lines are
  // excluded from the vote, and a selection of nothing but blank lines is
  // therefore an add — an empty document is where a heading is typed, not where
  // one is removed.
  const marked = lines.filter((line) => line.trim() !== "");
  const remove =
    marked.length > 0 && marked.every((line) => alreadyBlock(line, action));

  let n = 0;
  const next = lines
    .map((line) => {
      const stripped = line.replace(BLOCK_PREFIX, "$1");
      if (remove) return stripped;
      if (line.trim() === "" && lines.length > 1) return line;
      n += 1;
      const prefix = action === "ordered" ? `${n}. ` : BLOCK[action]!;
      const indent = /^\s*/.exec(stripped)?.[0] ?? "";
      return indent + prefix + stripped.slice(indent.length);
    })
    .join("\n");

  return {
    text: text.slice(0, from) + next + text.slice(to),
    // The whole affected block stays selected, so the author sees what changed
    // and can undo it with a second activation.
    start: from,
    end: from + next.length,
  };
}

export function applyMarkdown(range: TextRange, action: MarkdownAction): TextRange {
  const marker = WRAP[action];
  return marker ? toggleInline(range, marker) : toggleBlock(range, action);
}
