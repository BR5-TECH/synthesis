/**
 * The editable **target** of a comparison (`DFV-diff-viewer.md` DFV-FR-41 …
 * DFV-FR-50).
 *
 * A diff has two revisions on screen and exactly one of them can be typed into.
 * Everything here exists to make that distinction structural rather than a
 * matter of remembering: an original row is rendered by the plain components in
 * `DiffView`, and a target row is rendered by one of these — so a surface that
 * forgot the rule would render an original that is inert rather than an original
 * that takes the caret.
 *
 * What the two surfaces here have in common is how they address the buffer:
 * every editable unit knows the half-open **line range** it occupies in the
 * whole target and hands back what that range should now hold (DFV-FR-44). It
 * is the same contract for a Unified row, a side-by-side right-pane row, a Final
 * row, and a rich block, so Unified's hunk windowing costs the file outside the
 * hunk nothing.
 *
 * This module carries the **Source** rows, which are literal text edited as
 * literal text. The rich target is a document rather than a list of rows and
 * lives in `DiffRichTarget.tsx`, but it addresses the buffer through the same
 * `TargetEditing` record defined here.
 */
import {
  useCallback,
  useLayoutEffect,
  useRef,
  type KeyboardEvent as ReactKeyboardEvent,
  type ClipboardEvent as ReactClipboardEvent,
} from "react";
import { targetLines } from "../diff/targetEdit";
import type { Segment } from "../diff/wordDiff";
import type { TokenSpan } from "../state/syntaxHighlight";

/**
 * What a surface needs in order to be the target rather than a reading of it.
 *
 * Deliberately not "the session" or "the candidate buffer": a Diff tab's target
 * is the artifact's editing session (DFV-FR-42) and a review's is the proposal's
 * candidate (`DCR-draft-change-review.md` DCR-FR-25), and the rows must not be
 * able to tell which they are rendering — or the six mode combinations would
 * have to be written twice.
 */
export interface TargetEditing {
  /**
   * DFV-FR-50 / DCR-FR-28: the accessible name every editable unit carries,
   * naming the file and identifying the revision as the target, so a
   * screen-reader user landing in one knows which revision their keystrokes
   * reach.
   */
  label: string;
  /**
   * The whole target **as it currently stands**, read at the moment it is
   * needed.
   *
   * A function rather than a string, and that is load-bearing rather than a
   * style: the buffer behind it is a mutable field of a stable object (an
   * artifact's editing session, a candidate buffer), so a snapshot taken when
   * this record was built is frozen at that moment however much the author has
   * since typed. `EditableLine`'s Backspace-join reads the line above out of
   * this, and a frozen snapshot there silently replaces that line's current
   * text with what it held when the surface mounted — an edit destroyed by one
   * keystroke, with nothing on screen to say so.
   */
  currentText: () => string;
  /** DFV-FR-44: what lines `[from, to)` should now hold. */
  replaceLines: (from: number, to: number, replacement: string) => void;
  /**
   * Bumped whenever the whole target is replaced from **outside** an editing
   * surface — an undo or redo, a load adopted into the session, a conflict
   * resolved by taking the other side.
   *
   * A rich run holds its own document and cannot simply follow the text it was
   * seeded with: while the author is typing, that text is deliberately the
   * pre-edit one (DFV-FR-43), so following it would undo them keystroke by
   * keystroke. It follows this instead — and it has to be a counter rather than
   * the text, because a traversal can restore exactly the text the run was
   * seeded with and still owe the author a document that says so.
   */
  seed?: number;
  /**
   * DFV-FR-53 / DCR-FR-29: an unresolved external change or candidate conflict
   * makes the editing surface inert without making it an original.
   */
  disabled?: boolean;
}

/**
 * The read-only treatment every rendering of the **original** carries
 * (DFV-FR-41).
 *
 * Spread onto the element rather than left to each call site to remember,
 * because the requirement is that no original anywhere accepts a caret and
 * reports itself read-only — a rule that only holds if it is stated once.
 */
export const READ_ONLY_PROPS = {
  /**
   * `aria-readonly` is only mapped on a widget role, so a bare `<span>` carrying
   * it reaches the accessibility tree as plain text with no state at all — the
   * requirement silently unmet. `textbox` is the honest role: it is the same
   * kind of thing the row opposite is, and the state says the one thing that
   * distinguishes them.
   */
  role: "textbox" as const,
  "aria-readonly": true,
  "aria-multiline": false,
  "data-readonly": "true",
} as const;

// ---------------------------------------------------------------------------
// Caret bookkeeping
// ---------------------------------------------------------------------------

/** The caret's character offset within `host`, or null when it is elsewhere. */
function caretOffset(host: HTMLElement): number | null {
  const selection = host.ownerDocument.getSelection();
  if (!selection || selection.rangeCount === 0) return null;
  const range = selection.getRangeAt(0);
  if (!host.contains(range.startContainer)) return null;
  const measure = range.cloneRange();
  measure.selectNodeContents(host);
  measure.setEnd(range.startContainer, range.startOffset);
  return measure.toString().length;
}

/** Put the caret back at `offset` characters into `host`. */
function setCaretOffset(host: HTMLElement, offset: number): void {
  const doc = host.ownerDocument;
  const selection = doc.getSelection();
  if (!selection) return;
  const walker = doc.createTreeWalker(host, NodeFilter.SHOW_TEXT);
  let remaining = offset;
  let node = walker.nextNode();
  while (node) {
    const length = node.textContent?.length ?? 0;
    if (remaining <= length) {
      const range = doc.createRange();
      range.setStart(node, remaining);
      range.collapse(true);
      selection.removeAllRanges();
      selection.addRange(range);
      return;
    }
    remaining -= length;
    node = walker.nextNode();
  }
  // No text node reached the offset — an empty row, or one the re-derivation
  // shortened. The end of what there is is the nearest honest answer.
  const range = doc.createRange();
  range.selectNodeContents(host);
  range.collapse(false);
  selection.removeAllRanges();
  selection.addRange(range);
}

/**
 * Render `content` into `host`, marking the words that differ (DFV-FR-32), and
 * put the caret back where it was.
 *
 * The content is written imperatively rather than as React children because the
 * element is `contentEditable`: the browser mutates it under React on every
 * keystroke, and a reconciliation over children React believes it owns would
 * fight the caret for the row. React owns the element; this owns what is inside
 * it.
 */
function paintSegments(
  host: HTMLElement,
  content: string,
  segments: Segment[] | undefined,
  tokens: readonly TokenSpan[] | undefined,
): void {
  const painted = `${marking(segments)}|${tokenKey(tokens)}`;
  if (host.textContent === content && host.dataset.painted === painted) {
    return;
  }
  const focused = host.ownerDocument.activeElement === host;
  const offset = focused ? caretOffset(host) : null;
  host.replaceChildren();
  for (const piece of paintPieces(content, segments, tokens)) {
    host.append(piece(host.ownerDocument));
  }
  host.dataset.painted = painted;
  if (focused && offset != null) setCaretOffset(host, offset);
}

/**
 * DFV-FR-57: the row's text cut at every decoration boundary — the words that
 * differ, and the syntax tokens — with each piece wearing whatever covers it.
 *
 * Change marking wins wherever the two meet, which is why the `<mark>` is the
 * outer element: the word-level marking is a background the token's colour is
 * read *on*, so a token colour can neither carry the marking nor defeat it.
 * Nothing is inserted or dropped — the concatenated pieces reproduce `content`
 * character for character, which is what keeps both presentation-only.
 */
function paintPieces(
  content: string,
  segments: Segment[] | undefined,
  tokens: readonly TokenSpan[] | undefined,
): Array<(doc: Document) => Node> {
  const spans = tokens ?? [];
  const bounds = new Set<number>([0, content.length]);
  let at = 0;
  const changedRanges: Array<{ start: number; end: number }> = [];
  for (const segment of segments ?? []) {
    if (segment.changed) {
      changedRanges.push({ start: at, end: at + segment.text.length });
    }
    at += segment.text.length;
    bounds.add(at);
  }
  for (const span of spans) {
    bounds.add(span.start);
    bounds.add(span.end);
  }
  const points = [...bounds]
    .filter((p) => p >= 0 && p <= content.length)
    .sort((a, b) => a - b);

  const pieces: Array<(doc: Document) => Node> = [];
  for (let i = 0; i < points.length - 1; i += 1) {
    const start = points[i];
    const end = points[i + 1];
    if (start === end) continue;
    const text = content.slice(start, end);
    const token = spans.find((s) => start >= s.start && end <= s.end);
    const changed = changedRanges.some((r) => start >= r.start && end <= r.end);
    pieces.push((doc) => {
      let node: Node = doc.createTextNode(text);
      if (token) {
        const wrap = doc.createElement("span");
        wrap.className = `hl hl--${token.role}`;
        wrap.append(node);
        node = wrap;
      }
      if (changed) {
        const mark = doc.createElement("mark");
        mark.className = "diff-word";
        mark.append(node);
        node = mark;
      }
      return node;
    });
  }
  // A row with no decoration at all is one text node, as it was before either
  // of them existed.
  if (pieces.length === 0) return [(doc) => doc.createTextNode(content)];
  return pieces;
}

/** A cheap identity for a row's tokens, so an unchanged set is not repainted. */
function tokenKey(tokens: readonly TokenSpan[] | undefined): string {
  if (!tokens || tokens.length === 0) return "";
  return tokens.map((t) => `${t.start}.${t.end}.${t.role}`).join(",");
}

/** A cheap identity for a segmentation, so an unchanged one is not repainted. */
function marking(segments: Segment[] | undefined): string {
  if (!segments) return "";
  return segments.map((s) => `${s.changed ? "1" : "0"}${s.text.length}`).join(",");
}

// ---------------------------------------------------------------------------
// Source rows (DFV-FR-44, DFV-FR-45, DFV-FR-46)
// ---------------------------------------------------------------------------

/**
 * One editable line of the target, in any of the three visualizations.
 *
 * Selection, typing, paste, cut, copy, and keyboard navigation are the
 * platform's own within a row (DFV-FR-50); what this adds is the three edits
 * that cross a row boundary and would otherwise be impossible — Enter splitting
 * a line, Backspace at the head of a line joining it to the one above, and a
 * paste carrying newlines — each expressed as one splice of the whole target so
 * the file outside the row is carried through untouched.
 *
 * A row belonging to the original is never rendered by this component at all
 * (DFV-FR-41): the caller renders those with the plain marked span, so no
 * keystroke can reach one however the focus got there.
 */
export function EditableLine({
  content,
  segments,
  tokens,
  line,
  editing,
}: {
  content: string;
  segments?: Segment[];
  /** DFV-FR-57: this row's syntax tokens, in the row's own coordinates. */
  tokens?: readonly TokenSpan[];
  /** 0-based index of this row's line in the whole target. */
  line: number;
  editing: TargetEditing;
}) {
  const ref = useRef<HTMLSpanElement | null>(null);

  // Layout rather than passive: the row is repainted as part of the same commit
  // that changed it, so no frame is ever presented with the caret in the wrong
  // place.
  useLayoutEffect(() => {
    const host = ref.current;
    if (host) paintSegments(host, content, segments, tokens);
  }, [content, segments, tokens]);

  const emit = useCallback(() => {
    const host = ref.current;
    if (!host) return;
    editing.replaceLines(line, line + 1, host.textContent ?? "");
  }, [editing, line]);

  const onKeyDown = useCallback(
    (event: ReactKeyboardEvent<HTMLSpanElement>) => {
      const host = ref.current;
      if (!host) return;
      const text = host.textContent ?? "";
      const at = caretOffset(host) ?? text.length;
      if (event.key === "Enter") {
        // A row is one line, so a new line is a new row: split the target here
        // rather than letting the browser put a `<br>` inside a row and make its
        // text two lines the gutter has one number for.
        event.preventDefault();
        editing.replaceLines(
          line,
          line + 1,
          `${text.slice(0, at)}\n${text.slice(at)}`,
        );
        return;
      }
      if (event.key === "Backspace" && at === 0 && line > 0) {
        // The browser cannot join two rows it renders as separate hosts, and
        // leaving the keystroke to do nothing would make the boundary between
        // two lines uncrossable by the one key everybody uses to cross it.
        event.preventDefault();
        const previous = targetLines(editing.currentText())[line - 1] ?? "";
        editing.replaceLines(line - 1, line + 1, `${previous}${text}`);
        return;
      }
    },
    [editing, line],
  );

  const onPaste = useCallback(
    (event: ReactClipboardEvent<HTMLSpanElement>) => {
      const host = ref.current;
      if (!host) return;
      const pasted = event.clipboardData.getData("text/plain");
      // Only a paste carrying newlines needs handling: a single-line paste is
      // ordinary text input and the platform's own behaviour is the right one.
      if (!pasted.includes("\n")) return;
      event.preventDefault();
      const text = host.textContent ?? "";
      const at = caretOffset(host) ?? text.length;
      editing.replaceLines(
        line,
        line + 1,
        `${text.slice(0, at)}${pasted}${text.slice(at)}`,
      );
    },
    [editing, line],
  );

  return (
    <span
      ref={ref}
      className="diff-line__text diff-line__text--target"
      role="textbox"
      aria-label={editing.label}
      aria-multiline={false}
      contentEditable={editing.disabled ? false : "plaintext-only"}
      suppressContentEditableWarning
      spellCheck={false}
      data-target="true"
      onInput={emit}
      onKeyDown={onKeyDown}
      onPaste={onPaste}
    />
  );
}
