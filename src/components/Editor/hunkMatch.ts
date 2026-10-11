/**
 * Finding a proposed change's text in the document the author is looking at
 * (`../../../specifications/ui/DCR-draft-change-review.md` DCR-FR-05,
 * DCR-FR-06).
 *
 * A change names the text it alters, copied from the prompt as the agent read
 * it — which is **Markdown source**. The surface it has to be found in is the
 * rendered document, where the syntax that carried that text is gone: an
 * inline-code span has lost its backticks, a heading its hashes, a list item
 * its bullet, and a paragraph break has become one newline instead of two. A
 * change quoting any of them is not in the rendered text at all, character for
 * character, and a plain search for it fails — silently, because a change that
 * cannot be placed is simply not drawn. An author then reads a review bar
 * saying five changes and sees one.
 *
 * So both sides are reduced to the form the two of them share: the words, with
 * every mark of syntax and every difference of spacing taken out. The reduction
 * carries an index back to the real text, so a match found in the reduced form
 * is returned as a range in the document itself and the decoration still lands
 * on the exact words.
 *
 * This is **not** fuzzy matching, and it is deliberately not: nothing here
 * drops a word, and nothing tolerates a difference in the letters. What it
 * ignores is punctuation the renderer itself removed and spacing the renderer
 * itself changed — a difference the agent could not have avoided and the author
 * cannot see.
 */
import { offsetToPos, type DocText, type PosRange } from "../findHighlight";

/**
 * Text reduced to what survives rendering, and where each character came from.
 *
 * `map[i]` is the offset in the original text of the reduced form's character
 * `i`. It is what turns a match in the reduced form back into a range in the
 * text the document is addressed by.
 */
export interface Reduced {
  text: string;
  map: number[];
}

/**
 * The characters Markdown spends on syntax **wherever they stand**.
 *
 * Dropped from both sides, so a literal one in the prompt is dropped from the
 * change quoting it as well and the two still meet. That symmetry is what makes
 * dropping them safe: this decides where a change is, and the document itself
 * decides what it says.
 */
const SYNTAX = new Set(["`", "*", "_", "#", ">", "~", "[", "]", "|"]);

/**
 * The length of a Markdown marker that only counts at the head of a line, or 0.
 *
 * A hyphen is a list bullet at the head of a line and a character of the word
 * `version-only` anywhere else, and a digit is a numbered item's number at the
 * head of a line and a figure anywhere else — so neither can be dropped by
 * spelling. Only the renderer's own reading of position tells them apart, and
 * this is that reading.
 */
function lineMarkerLength(text: string, at: number): number {
  const rest = text.slice(at, at + 12);
  const marker = /^(?:[-+]\s|\d{1,3}[.)]\s)/.exec(rest);
  return marker === null ? 0 : marker[0].length;
}

/**
 * Whether a line opens or closes a fenced code block.
 *
 * The fence and the language beside it are the block's syntax and reach no
 * rendered text, so the whole line goes — where an ordinary line keeps every
 * word it holds.
 */
function fenceLength(text: string, at: number): number {
  if (!text.startsWith("```", at) && !text.startsWith("~~~", at)) return 0;
  const end = text.indexOf("\n", at);
  return (end === -1 ? text.length : end) - at;
}

/** Reduce text to the words it holds, keeping an index back to it. */
export function reduce(text: string): Reduced {
  let out = "";
  const map: number[] = [];
  let pendingSpace = false;
  let atLineStart = true;
  let i = 0;
  while (i < text.length) {
    const ch = text[i];
    if (ch === "\n" || ch === "\r") {
      // Every run of space becomes one space, whatever it was made of: a
      // paragraph break in the source is one newline in the rendered text, and
      // an indented list item carries leading space in one and none in the
      // other.
      pendingSpace = out.length > 0;
      atLineStart = true;
      i += 1;
      continue;
    }
    if (ch === " " || ch === "\t") {
      pendingSpace = out.length > 0;
      i += 1;
      continue;
    }
    if (atLineStart) {
      const fence = fenceLength(text, i);
      if (fence > 0) {
        i += fence;
        continue;
      }
      const marker = lineMarkerLength(text, i);
      if (marker > 0) {
        // Still at the head of the line: a nested item carries two markers.
        i += marker;
        continue;
      }
      atLineStart = false;
    }
    if (SYNTAX.has(ch)) {
      i += 1;
      continue;
    }
    if (pendingSpace) {
      out += " ";
      // The space stands for the run that produced it, so it is indexed at the
      // first character of what follows — which keeps every real character's
      // own offset exact.
      map.push(i);
      pendingSpace = false;
    }
    out += ch;
    map.push(i);
    i += 1;
  }
  return { text: out, map };
}

/**
 * Where a change's text sits in the document, or null where it is not there.
 *
 * The occurrence nearest the recorded hint wins, exactly as an anchored
 * comment's does (`../../state/commentAnchors.ts`): the text is the identity
 * and the offset only decides which of several places is meant.
 */
export function findRange(
  index: DocText,
  reducedDoc: Reduced,
  needle: string,
  hint: number,
): PosRange | null {
  return findReduced(index, reducedDoc, reduce(needle).text, hint);
}

/** `findRange` for a needle that is already reduced. */
function findReduced(
  index: DocText,
  reducedDoc: Reduced,
  wanted: string,
  hint: number,
): PosRange | null {
  if (wanted === "") return null;

  // The hint is an offset into the prompt's source; the search runs over the
  // reduced form. It is only a tie-breaker, so an approximate one is enough —
  // and the reduced form is never longer than the source it came from.
  let best = -1;
  let bestDistance = Infinity;
  for (let from = 0; ; ) {
    const at = reducedDoc.text.indexOf(wanted, from);
    if (at === -1) break;
    const distance = Math.abs((reducedDoc.map[at] ?? at) - hint);
    if (distance < bestDistance) {
      best = at;
      bestDistance = distance;
    }
    from = at + 1;
  }
  if (best === -1) return null;

  const start = reducedDoc.map[best];
  const last = reducedDoc.map[best + wanted.length - 1];
  if (start === undefined || last === undefined) return null;
  return spanToPositions(index, start, last);
}

/**
 * The document range holding the characters at two text offsets, ends included.
 *
 * `rangeToPositions` is the find panel's mapper and refuses a span that crosses
 * a paragraph: positions and offsets advance at different rates there, so a
 * span that matched one length would rewrite another, and Replace must never do
 * that. A decoration is under no such rule — an inline decoration across blocks
 * marks the text in each of them — and a proposed change routinely replaces
 * several paragraphs at once. Refusing those here is what leaves a review bar
 * naming changes the document does not draw.
 *
 * Both ends must be real characters, which is what the reduction guarantees:
 * every offset it records is a character's own, and the space it inserts for a
 * run of space is recorded at the character that follows it.
 */
export function spanToPositions(
  index: DocText,
  start: number,
  last: number,
): PosRange | null {
  const from = offsetToPos(index, start);
  const lastPos = offsetToPos(index, last);
  if (from === null || lastPos === null || lastPos < from) return null;
  return { from, to: lastPos + 1 };
}

/**
 * The end of the text an insertion follows, as a position in the document.
 *
 * An insertion names no text of its own, so it is placed at the end of what it
 * comes after.
 */
export function findAfter(
  index: DocText,
  reducedDoc: Reduced,
  follows: string,
  hint: number,
): PosRange | null {
  const range = findRange(index, reducedDoc, follows, hint);
  return range === null ? null : { from: range.to, to: range.to };
}

/** What a change has to be placed by. Its text, and what it does with it. */
export interface PlaceableHunk {
  kind: "add" | "del" | "replace";
  /** The exact prompt text the change removes or replaces. Empty for an insertion. */
  before: string;
  /** The exact prompt text the change follows. Empty at the head of the prompt. */
  lead: string;
  hint: number;
}

/**
 * DCR-FR-LGHZ, DCR-FR-TSNW: the texts a change is searched for, each already
 * rendered by the Editor's parser (`./hunkNeedle`). Null where a text could not
 * be rendered.
 */
export interface RenderedNeedles {
  before: string | null;
  lead: string | null;
  /** The lead without its first line (`leadTail`), or null where none is left. */
  leadTail: string | null;
}

/**
 * DCR-FR-TSNW: the fewest words a shortened lead keeps. Fewer words match too
 * many places for the hint to choose between them reliably.
 */
const MIN_LEAD_WORDS = 3;

/** The number of words `text` holds once reduced. */
function wordCount(text: string): number {
  const reduced = reduce(text).text;
  return reduced === "" ? 0 : reduced.split(" ").length;
}

/**
 * DCR-FR-TSNW: the place an insertion's lead ends, found with as much of the
 * lead's end as the document holds.
 *
 * Words come off the start one at a time, and the first remainder found wins.
 */
function findLeadSuffix(
  index: DocText,
  reducedDoc: Reduced,
  lead: string,
  hint: number,
): PosRange | null {
  const words = reduce(lead).text.split(" ");
  for (let drop = 1; words.length - drop >= MIN_LEAD_WORDS; drop += 1) {
    const range = findReduced(index, reducedDoc, words.slice(drop).join(" "), hint);
    if (range !== null) return { from: range.to, to: range.to };
  }
  return null;
}

/** The first of the candidate texts the document holds, in order. */
function firstFound(
  find: (needle: string) => PosRange | null,
  candidates: (string | null)[],
): PosRange | null {
  for (const needle of candidates) {
    if (needle === null || needle === "") continue;
    const range = find(needle);
    if (range !== null) return range;
  }
  return null;
}

/**
 * Where a change sits in the document, or null where it cannot be placed.
 *
 * DCR-FR-05: a change is placed by its **kind**, which is how the backend
 * places it too. Reading the kind off the text instead lets the two disagree: a
 * replacement whose `before` was somehow empty would be placed as an insertion
 * — drawn as the green block on its own, the old text left unmarked, and the
 * author reading a rewrite as an addition. A replacement or a deletion naming
 * no text is a change this surface cannot draw, and it says so (DCR-FR-31)
 * rather than drawing half of it.
 *
 * DCR-FR-LGHZ: the rendered text is searched first. The source text comes after
 * it, so a change found by the source alone is still drawn.
 */
export function placeHunk(
  index: DocText,
  reducedDoc: Reduced,
  hunk: PlaceableHunk,
  rendered?: RenderedNeedles,
): PosRange | null {
  if (hunk.kind === "add") {
    // An insertion names no text of its own, so it is placed at the end of the
    // text it follows — or at the head of a prompt that holds none.
    if (hunk.lead === "") return { from: 1, to: 1 };
    // DCR-FR-TSNW: a tail is a shortened lead, so it keeps as many words.
    const tail = rendered?.leadTail ?? null;
    const usableTail =
      tail !== null && wordCount(tail) >= MIN_LEAD_WORDS ? tail : null;
    const found = firstFound(
      (lead) => findAfter(index, reducedDoc, lead, hunk.hint),
      [rendered?.lead ?? null, usableTail, hunk.lead],
    );
    if (found !== null) return found;
    // DCR-FR-TSNW: only the end of a lead must match. A tail that renders to
    // nothing (a rule, an image) holds no words, so the next text is tried.
    return firstFound(
      (lead) => findLeadSuffix(index, reducedDoc, lead, hunk.hint),
      [rendered?.leadTail ?? null, rendered?.lead ?? null, hunk.lead],
    );
  }
  if (hunk.before === "") {
    // DCR-FR-KDSV: a legacy proposal is one replacement covering the whole
    // prompt, so over a prompt that holds nothing it names nothing. There is
    // no text to strike and one place its proposed text can go.
    return reducedDoc.text === "" ? { from: 1, to: 1 } : null;
  }
  return firstFound(
    (before) => findRange(index, reducedDoc, before, hunk.hint),
    [rendered?.before ?? null, hunk.before],
  );
}
