/**
 * MPV-FR-WYTL .. MPV-FR-RQWN: the body diff of a plan proposal row. Both
 * Markdown revisions are parsed into blocks, the blocks are aligned, a
 * replaced block that reads as an edit of the block before it gets its words
 * marked inline, a replaced code block gets its lines marked, and runs of
 * unchanged blocks fold. Pure, so the renderer and
 * its tests read one answer.
 */
import { alignBy } from "./lineAlign";
import { parseInline, parseMarkdownBlocks, type MarkdownBlock } from "./markdown";
import { diffWords, tokenize } from "./wordDiff";

/** The typeset kind of a block (MPV-FR-PVXW). */
export type BodyBlockKind = "h2" | "h3" | "p" | "quote" | "li" | "code" | "plain";

/** The face of a run of text. */
export type PieceStyle = "text" | "strong" | "em" | "code";

/** A run of text with one face, marked as inserted or deleted inside an edited block. */
export interface Piece {
  text: string;
  style: PieceStyle;
  mark?: "ins" | "del";
}

/** MPV-FR-ZCWQ / MPV-FR-JVZN: one line of a code block, marked inside an edited block. */
export interface CodeLine {
  text: string;
  mark?: "ins" | "del";
}

export interface BodyBlock {
  /** `add` and `del` take a gutter mark; `edit` and `ctx` take none. */
  mode: "add" | "del" | "edit" | "ctx";
  kind: BodyBlockKind;
  /** The nesting depth of a list item. */
  depth: number;
  pieces: Piece[];
  /** MPV-FR-ZCWQ: the lines of a code block, with their indentation. */
  lines?: CodeLine[];
  /** MPV-FR-ZCWQ: the language of a fenced code block, for example `mermaid`. */
  language?: string;
}

export type BodyItem = { type: "block"; block: BodyBlock } | { type: "fold"; blocks: BodyBlock[] };

export interface BodyDiff {
  items: BodyItem[];
  added: number;
  removed: number;
  edited: number;
}

/** The blocks that carry text. A rule, a break, and a comment have none. */
function textBlocks(text: string | null): MarkdownBlock[] {
  return parseMarkdownBlocks(text).filter(
    (b) => b.kind !== "rule" && b.kind !== "break" && b.kind !== "comment" && !b.isDivider,
  );
}

function kindOf(block: MarkdownBlock): BodyBlockKind {
  switch (block.kind) {
    case "heading":
      return (block.level ?? 1) <= 2 ? "h2" : "h3";
    case "paragraph":
      return "p";
    case "quote":
      return "quote";
    case "list-item":
      return "li";
    case "code":
      return "code";
    default:
      return "plain";
  }
}

/** The text a block shows, with the inline syntax kept for `parseInline`. */
function inlineSource(block: MarkdownBlock): string | null {
  switch (block.kind) {
    case "code":
      return null;
    case "table-row":
      return (block.cells ?? []).join(" | ");
    default:
      return block.text;
  }
}

/** The runs of a block with their faces. Code and table cells are plain. */
function piecesOf(block: MarkdownBlock): Piece[] {
  const source = inlineSource(block);
  if (source === null) return [{ text: block.text, style: "text" }];
  const pieces: Piece[] = [];
  for (const span of parseInline(source)) {
    if (span.kind === "comment" || span.text === "") continue;
    const style: PieceStyle =
      span.kind === "strong" || span.kind === "em" || span.kind === "code" ? span.kind : "text";
    pieces.push({ text: span.text, style });
  }
  return pieces;
}

function plainOf(pieces: readonly Piece[]): string {
  return pieces.map((p) => p.text).join("");
}

/** The face of each character of the text of `pieces`. */
function facesOf(pieces: readonly Piece[]): PieceStyle[] {
  const faces: PieceStyle[] = [];
  for (const piece of pieces) for (let i = 0; i < piece.text.length; i++) faces.push(piece.style);
  return faces;
}

/** Push `text[from, to)` as pieces split at each change of face. */
function pushSlice(
  out: Piece[],
  text: string,
  faces: readonly PieceStyle[],
  from: number,
  to: number,
  mark?: "ins" | "del",
) {
  let start = from;
  for (let i = from + 1; i <= to; i++) {
    if (i === to || faces[i] !== faces[start]) {
      if (i > start) {
        const last = out[out.length - 1];
        const style = faces[start] ?? "text";
        const slice = text.slice(start, i);
        if (last && last.style === style && last.mark === mark) last.text += slice;
        else out.push(mark ? { text: slice, style, mark } : { text: slice, style });
      }
      start = i;
    }
  }
}

/**
 * MPV-FR-PPHV: the new block with the removed words in `del` and the added
 * words in `ins`, or null when the two blocks do not read as one edit.
 */
export function mergedWordPieces(before: readonly Piece[], after: readonly Piece[]): Piece[] | null {
  const oldText = plainOf(before);
  const newText = plainOf(after);
  if (diffWords(oldText, newText) === null) return null;
  const oldFaces = facesOf(before);
  const newFaces = facesOf(after);
  const out: Piece[] = [];
  let o = 0;
  let n = 0;
  let delFrom = -1;
  let insFrom = -1;
  // A run of white space that follows a change. It joins the change when
  // another change follows it, so `old words` reads as one deletion, not one
  // per word; it stays plain text when the next token is unchanged.
  let gap: { o: number; n: number } | null = null;
  const flush = (oEnd: number, nEnd: number) => {
    if (delFrom >= 0) pushSlice(out, oldText, oldFaces, delFrom, oEnd, "del");
    if (insFrom >= 0) pushSlice(out, newText, newFaces, insFrom, nEnd, "ins");
    delFrom = -1;
    insFrom = -1;
  };
  for (const pair of alignBy(tokenize(oldText), tokenize(newText), (t) => t)) {
    if (!pair.changed) {
      const token = pair.new!;
      const open = delFrom >= 0 || insFrom >= 0;
      if (open && token.trim() === "") {
        gap ??= { o, n };
      } else {
        if (gap) {
          flush(gap.o, gap.n);
          pushSlice(out, newText, newFaces, gap.n, n);
          gap = null;
        } else {
          flush(o, n);
        }
        pushSlice(out, newText, newFaces, n, n + token.length);
      }
      o += token.length;
      n += token.length;
      continue;
    }
    gap = null;
    if (pair.old !== null) {
      if (delFrom < 0) delFrom = o;
      o += pair.old.length;
    }
    if (pair.new !== null) {
      if (insFrom < 0) insFrom = n;
      n += pair.new.length;
    }
  }
  if (gap) {
    flush(gap.o, gap.n);
    pushSlice(out, newText, newFaces, gap.n, n);
  } else {
    flush(o, n);
  }
  return out;
}

function blockOf(block: MarkdownBlock, mode: BodyBlock["mode"], pieces = piecesOf(block)): BodyBlock {
  const out: BodyBlock = { mode, kind: kindOf(block), depth: block.depth ?? 0, pieces };
  if (block.kind === "code") {
    out.lines = block.text.split("\n").map((text) => ({ text }));
    if (block.language) out.language = block.language;
  }
  return out;
}

/**
 * MPV-FR-JVZN: the lines of the new code block, with each removed line in its
 * place before the lines that replaced it.
 */
export function codeLineDiff(before: string, after: string): CodeLine[] {
  const out: CodeLine[] = [];
  let dels: CodeLine[] = [];
  let inss: CodeLine[] = [];
  const flush = () => {
    out.push(...dels, ...inss);
    dels = [];
    inss = [];
  };
  for (const pair of alignBy(before.split("\n"), after.split("\n"), (line) => line)) {
    if (!pair.changed) {
      flush();
      out.push({ text: pair.new! });
      continue;
    }
    if (pair.old !== null) dels.push({ text: pair.old, mark: "del" });
    if (pair.new !== null) inss.push({ text: pair.new, mark: "ins" });
  }
  flush();
  return out;
}

/** MPV-FR-JVZN: a removed and an added code block of the same language are one edit. */
function sameCode(a: MarkdownBlock, b: MarkdownBlock): boolean {
  return a.kind === "code" && b.kind === "code" && (a.language ?? "") === (b.language ?? "");
}

/** MPV-FR-PPHV: whether two blocks can be one edited block. */
function sameKind(a: MarkdownBlock, b: MarkdownBlock): boolean {
  return kindOf(a) === kindOf(b) && kindOf(a) !== "code" && (a.depth ?? 0) === (b.depth ?? 0);
}

/**
 * MPV-FR-WYTL .. MPV-FR-VMYF: the body diff of `before` and `after`. An unchanged
 * block next to a change shows in full; a longer run of unchanged blocks folds.
 */
export function diffBody(before: string | null, after: string | null): BodyDiff {
  const flat: BodyBlock[] = [];
  let added = 0;
  let removed = 0;
  let edited = 0;
  const pairs = alignBy(textBlocks(before), textBlocks(after), (b) => `${b.kind}\0${b.source}`);
  for (const pair of pairs) {
    if (!pair.changed) {
      flat.push(blockOf(pair.new!, "ctx"));
      continue;
    }
    if (pair.old && pair.new && sameCode(pair.old, pair.new)) {
      flat.push({ ...blockOf(pair.new, "edit"), lines: codeLineDiff(pair.old.text, pair.new.text) });
      edited++;
      continue;
    }
    if (pair.old && pair.new && sameKind(pair.old, pair.new)) {
      const merged = mergedWordPieces(piecesOf(pair.old), piecesOf(pair.new));
      if (merged !== null) {
        flat.push(blockOf(pair.new, "edit", merged));
        edited++;
        continue;
      }
    }
    if (pair.old) {
      flat.push(blockOf(pair.old, "del"));
      removed++;
    }
    if (pair.new) {
      flat.push(blockOf(pair.new, "add"));
      added++;
    }
  }
  return { items: foldRuns(flat), added, removed, edited };
}

/** MPV-FR-VMYF: fold each run of two or more unchanged blocks no change touches. */
function foldRuns(flat: readonly BodyBlock[]): BodyItem[] {
  const near = (i: number) => {
    const isChange = (j: number) => j >= 0 && j < flat.length && flat[j].mode !== "ctx";
    return isChange(i - 1) || isChange(i + 1);
  };
  const items: BodyItem[] = [];
  let run: BodyBlock[] = [];
  const close = () => {
    if (run.length >= 2) items.push({ type: "fold", blocks: run });
    else for (const block of run) items.push({ type: "block", block });
    run = [];
  };
  flat.forEach((block, i) => {
    if (block.mode === "ctx" && !near(i)) {
      run.push(block);
      return;
    }
    close();
    items.push({ type: "block", block });
  });
  close();
  return items;
}

/** MPV-FR-VMYF: the count of the label strip, or null when nothing changed. */
export function bodyCountLabel(diff: BodyDiff): string | null {
  const parts: string[] = [];
  if (diff.added > 0) parts.push(`${diff.added} added`);
  if (diff.removed > 0) parts.push(`${diff.removed} removed`);
  if (diff.edited > 0) parts.push(`${diff.edited} edited`);
  if (parts.length === 0) return null;
  const first = diff.added || diff.removed || diff.edited;
  parts[0] = `${first} ${first === 1 ? "block" : "blocks"} ${parts[0].split(" ")[1]}`;
  return parts.join(", ");
}
