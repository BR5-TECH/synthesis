/**
 * Splicing an edit made in one rendered row back into the whole target
 * (`DFV-diff-viewer.md` DFV-FR-44).
 *
 * A diff renders *rows*, and Unified renders only the rows inside a hunk — but
 * the thing being edited is the complete revision, not the window onto it. So
 * every editable surface in the tab addresses the target the same way: by the
 * half-open line range the row (or the rich block) occupies in it, handing back
 * the text that range should now hold. What is outside the range is carried
 * through untouched, which is what makes a correction inside a hunk one edit to
 * one file rather than a patch applied to a window.
 *
 * Pure, and deliberately ignorant of who holds the buffer: a Diff tab's target
 * is the artifact's editing session (DFV-FR-42) and a review's is the proposal's
 * candidate buffer (`DCR-draft-change-review.md` DCR-FR-25), and neither
 * difference reaches this far.
 */

/**
 * The target's lines, as a diff numbers them.
 *
 * A trailing newline terminates the last line rather than starting an empty
 * one — the same reading `parseMarkdownBlocks` and the line aligner take, so a
 * row's index here addresses the line those two named.
 */
export function targetLines(text: string): string[] {
  const lines = text.split("\n").map((line) =>
    line.endsWith("\r") ? line.slice(0, -1) : line,
  );
  if (lines.length > 1 && lines[lines.length - 1] === "") lines.pop();
  return lines;
}

/**
 * The separator the text is already written with.
 *
 * A buffer really can hold CRLF — a checkout under one convention loads as it
 * lies, and the project's own convention is applied by the write (EDT-FR-40) —
 * and splitting on `"\n"` alone would leave every untouched line carrying its
 * `\r` while the edited one lost it. The file would then go to disk with mixed
 * endings, which is a whole-file diff nobody authored.
 */
function separatorOf(text: string): string {
  return text.includes("\r\n") ? "\r\n" : "\n";
}

/** Whether the text ends on a line terminator, which splicing has to put back. */
function endsWithNewline(text: string): boolean {
  return text.length > 0 && text.endsWith("\n");
}

/**
 * Replace lines `[from, to)` of `text` with `replacement`, which may itself hold
 * newlines (a paste, or an Enter that split a row in two) or be empty (a row
 * joined into its predecessor).
 *
 * Out-of-range indices are clamped rather than refused: a row can be edited in
 * the same tick a re-derivation shortened the target under it, and dropping the
 * keystroke would lose the author's typing to a race they cannot see.
 */
export function replaceLineRange(
  text: string,
  from: number,
  to: number,
  replacement: string,
): string {
  const separator = separatorOf(text);
  const lines = targetLines(text);
  const start = Math.max(0, Math.min(from, lines.length));
  const end = Math.max(start, Math.min(to, lines.length));
  // A replacement carrying its own terminators — a paste from elsewhere — is
  // normalised to the text's own, for the reason `separatorOf` gives.
  const inserted =
    replacement === "" && end > start
      ? []
      : replacement.replace(/\r\n/g, "\n").split("\n");
  const next = [...lines.slice(0, start), ...inserted, ...lines.slice(end)];
  const joined = next.join(separator);
  // The terminator is a property of the file rather than of the row that was
  // edited, so an edit never adds or removes one. A target that had none and is
  // now empty stays empty rather than becoming a lone newline.
  return endsWithNewline(text) && joined !== "" ? `${joined}${separator}` : joined;
}

/**
 * The half-open line range a rich block occupies in the revision it was parsed
 * from (DFV-FR-47).
 */
export function blockLineRange(block: {
  line: number;
  source: string;
}): [number, number] {
  return [block.line, block.line + block.source.split("\n").length];
}
