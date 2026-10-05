import { Fragment, useMemo } from "react";
import type { ReactNode } from "react";
import { tokensByLine, type TokenSpan } from "../../state/syntaxHighlight";
import { useRevisionSpans } from "../../hooks/useSourceTokens";
import { alignLines } from "../../diff/lineAlign";
import type { AlignedSide } from "../../diff/lineAlign";
import { diffWords } from "../../diff/wordDiff";
import type { Segment } from "../../diff/wordDiff";
import { EditableLine, READ_ONLY_PROPS, type TargetEditing } from "../DiffTarget";
import type { DiffLine, DiffPayload, FileRevisions } from "../../types";

// ---------------------------------------------------------------------------
// Source rendering
// ---------------------------------------------------------------------------

/**
 * DFV-FR-10: the placeholder holding the gutter column a row has no number in.
 * Non-numeric, and the same width as a number, so rows never shift.
 */
const NO_LINE = "·";

/**
 * The words that actually differ inside a replaced line, against the line's own
 * whole-line marking. A line with no segments is marked whole — it is wholly
 * new, wholly gone, or too unlike its counterpart to read as one edit.
 */
function LineContent({
  content,
  segments,
  tokens,
}: {
  content: string;
  segments?: Segment[];
  /** DFV-FR-57: this row's syntax tokens, in the row's own coordinates. */
  tokens?: readonly TokenSpan[];
}) {
  // DFV-FR-41: every rendering of the ORIGINAL goes through here, so the
  // read-only state assistive technology is owed is stated once rather than at
  // each of the four call sites that could forget it.
  if (!segments && (!tokens || tokens.length === 0))
    return (
      <span className="diff-line__text" {...READ_ONLY_PROPS}>
        {content}
      </span>
    );
  return (
    <span className="diff-line__text" {...READ_ONLY_PROPS}>
      {sourcePieces(content, segments, tokens).map((piece, index) => (
        <Fragment key={index}>{piece}</Fragment>
      ))}
    </span>
  );
}

/**
 * DFV-FR-57: one row's text cut at every decoration boundary — the words that
 * differ (DFV-FR-32) and the syntax tokens — with each piece wearing whatever
 * covers it.
 *
 * The `<mark>` is outside the token's span rather than inside it, because
 * marking wins wherever the two meet: the word-level treatment is a background
 * the token's colour is read *on*, so a colour can neither carry the marking nor
 * defeat it. Nothing is inserted or dropped, so the row still reads as the line
 * it is.
 */
function sourcePieces(
  content: string,
  segments: Segment[] | undefined,
  tokens: readonly TokenSpan[] | undefined,
): ReactNode[] {
  const spans = tokens ?? [];
  const bounds = new Set<number>([0, content.length]);
  const changed: Array<{ start: number; end: number }> = [];
  let at = 0;
  for (const segment of segments ?? []) {
    if (segment.changed) changed.push({ start: at, end: at + segment.text.length });
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

  const out: ReactNode[] = [];
  for (let i = 0; i < points.length - 1; i += 1) {
    const start = points[i];
    const end = points[i + 1];
    if (start === end) continue;
    const text = content.slice(start, end);
    const token = spans.find((s) => start >= s.start && end <= s.end);
    let node: ReactNode = token ? (
      <span className={`hl hl--${token.role}`}>{text}</span>
    ) : (
      text
    );
    if (changed.some((r) => start >= r.start && end <= r.end)) {
      node = <mark className="diff-word">{node}</mark>;
    }
    out.push(node);
  }
  return out.length > 0 ? out : [content];
}

/**
 * Pair the removed and added lines of one replacement so the words that differ
 * can be marked within them.
 *
 * A hunk states its lines in file order: a run of removals followed by the run
 * that replaced them. Pairing them index-wise is the same rule the side-by-side
 * alignment uses, so a line reads the same way in either mode.
 */
function segmentReplacements(lines: DiffLine[]): Array<{
  line: DiffLine;
  segments?: Segment[];
}> {
  const rows: Array<{ line: DiffLine; segments?: Segment[] }> = [];
  let index = 0;
  while (index < lines.length) {
    if (lines[index].kind !== "del") {
      rows.push({ line: lines[index] });
      index++;
      continue;
    }
    const removed: DiffLine[] = [];
    while (index < lines.length && lines[index].kind === "del") {
      removed.push(lines[index++]);
    }
    const added: DiffLine[] = [];
    while (index < lines.length && lines[index].kind === "add") {
      added.push(lines[index++]);
    }
    const group = [...removed, ...added].map((line) => ({ line }) as {
      line: DiffLine;
      segments?: Segment[];
    });
    for (let k = 0; k < Math.min(removed.length, added.length); k++) {
      const words = diffWords(removed[k].content, added[k].content);
      if (!words) continue;
      group[k].segments = words.old;
      group[removed.length + k].segments = words.new;
    }
    rows.push(...group);
  }
  return rows;
}

/**
 * DFV-FR-57: both revisions' syntax tokens, indexed by 0-based line.
 *
 * Two maps rather than one, because the two revisions are two texts: a line the
 * change removed is coloured by the language as the *old* revision spelled it,
 * and the line that replaced it by the new. One map over the target alone would
 * mis-colour the original's rows wherever the two differ, which is precisely
 * where a reader is looking.
 */
interface RevisionTokens {
  old: TokenSpan[][];
  new: TokenSpan[][];
}

/**
 * DFV-FR-57: the tokens of both revisions of one file, resolved once for the
 * file and applied to each.
 *
 * The language comes from the file's name and text on `EDT-editor.md`'s terms
 * (ESH-FR-JHPR), so a file whose final extension is `.md` receives none here
 * either — its reading is **Rich** (DFV-FR-17) — and one no language resolves
 * for renders plain. The work is deferred and stale-guarded exactly as the
 * Editor's is (ESH-FR-MJRH): a comparison paints its rows immediately and takes
 * their colour when it arrives.
 */
export function useRevisionTokens(
  fileName: string | undefined,
  revisions: FileRevisions | null,
  enabled: boolean,
): RevisionTokens {
  const name = fileName ?? "";
  const oldText = revisions?.old ?? "";
  const newText = revisions?.new ?? "";
  const on = enabled && fileName != null;
  // One resolution for the file, applied to both revisions. Resolving each from
  // its own text would let an unmapped file's two revisions disagree about what
  // language they are, which is the one thing DFV-FR-57 says they may not do.
  const { old: oldSpans, new: newSpans } = useRevisionSpans(
    name,
    oldText,
    newText,
    on,
  );
  const oldLines = useMemo(
    () => tokensByLine(oldText, oldSpans),
    [oldText, oldSpans],
  );
  const newLines = useMemo(
    () => tokensByLine(newText, newSpans),
    [newText, newSpans],
  );
  return useMemo(
    () => ({ old: oldLines, new: newLines }),
    [oldLines, newLines],
  );
}

/**
 * DFV-FR-10: the unified gutter's two numeric columns, old then new. Every
 * rendered content row carries at least one number, and the column a row has no
 * number in carries a placeholder of the same width rather than being blank of
 * layout.
 */
function UnifiedGutter({
  oldLineno,
  newLineno,
}: {
  oldLineno?: number;
  newLineno?: number;
}) {
  return (
    <>
      <span className="diff-line__gutter diff-line__gutter--old">
        {oldLineno ?? NO_LINE}
      </span>
      <span className="diff-line__gutter diff-line__gutter--new">
        {newLineno ?? NO_LINE}
      </span>
    </>
  );
}

/**
 * DFV-FR-09: the comparison's changed regions as hunks in one column, each
 * introduced by its hunk header, with unchanged context around each region. No
 * part of the file outside a hunk is rendered.
 */
export function SourceUnified({
  payload,
  editing,
  tokens,
}: {
  payload: DiffPayload;
  editing?: TargetEditing;
  /** DFV-FR-57: both revisions' tokens, by line. */
  tokens: RevisionTokens;
}) {
  const hunks = useMemo(
    () =>
      payload.hunks.map((hunk) => ({
        header: hunk.header,
        rows: segmentReplacements(hunk.lines),
      })),
    [payload],
  );
  return (
    <>
      {hunks.map((hunk, hi) => (
        <div className="diff" key={`${hunk.header}:${hi}`}>
          <div className="diff-line" data-kind="hunk">
            <span className="diff-line__gutter diff-line__gutter--old" />
            <span className="diff-line__gutter diff-line__gutter--new" />
            <span className="diff-line__sign" />
            <span className="diff-line__text">{hunk.header}</span>
          </div>
          {hunk.rows.map(({ line, segments }, li) => (
            <div className="diff-line" data-kind={line.kind} key={`${hi}:${li}`}>
              <UnifiedGutter
                oldLineno={line.oldLineno}
                newLineno={line.newLineno}
              />
              {/* Marking is conveyed by more than colour: the sign character is
                  what makes an addition legible without colour discrimination. */}
              <span className="diff-line__sign">
                {line.kind === "add" ? "+" : line.kind === "del" ? "−" : " "}
              </span>
              {/* DFV-FR-44: an added row and a context row both carry target
                  content and are editable; a removed row carries the original's
                  and is not. Editing a row edits the position it occupies in the
                  WHOLE target, so the file outside every hunk is carried through
                  untouched — the windowing is a choice about reading rather than
                  a truncation of what is written. */}
              {editing && line.newLineno != null ? (
                <EditableLine
                  content={line.content}
                  segments={segments}
                  /* DFV-FR-57: a row's colouring comes from the revision it
                     belongs to, so the two halves of a replacement each read in
                     their own revision's tokens. */
                  tokens={tokens.new[line.newLineno - 1]}
                  line={line.newLineno - 1}
                  editing={editing}
                />
              ) : (
                <LineContent
                  content={line.content}
                  segments={segments}
                  tokens={
                    line.newLineno != null
                      ? tokens.new[line.newLineno - 1]
                      : line.oldLineno != null
                        ? tokens.old[line.oldLineno - 1]
                        : undefined
                  }
                />
              )}
            </div>
          ))}
        </div>
      ))}
    </>
  );
}

/**
 * **Final** — the file as it will land, with what changed on the way visible.
 *
 * Where a line was added or replaced, only the outcome is shown, marked and
 * word-segmented: the reader is looking at the finished file, not at the pair
 * of revisions that produced it. Where a line was removed and nothing stands in
 * its place, the removed line is shown marked, because a deletion is otherwise
 * the one edit a finished file cannot show — the evidence for it is exactly the
 * text that is no longer there.
 *
 * The gutter stays one column of new-revision numbers; a removed line has no
 * number in that revision and carries the placeholder instead.
 */
export function SourceFinal({
  revisions,
  editing,
  tokens,
}: {
  revisions: FileRevisions | null;
  editing?: TargetEditing;
  tokens: RevisionTokens;
}) {
  const rows = useMemo(() => {
    const aligned = alignLines(revisions?.old ?? null, revisions?.new ?? null);
    return aligned.map((row) => {
      const words =
        row.changed && row.old && row.new
          ? diffWords(row.old.content, row.new.content)
          : null;
      return { row, words };
    });
  }, [revisions]);

  // A file the comparison adds is new in every line, so marking every line
  // says nothing the tab's own header does not already say. It reads as the
  // document it is.
  const wholeFileIsNew = revisions != null && revisions.old == null;

  return (
    <div className="diff" data-testid="diff-final">
      {rows.map(({ row, words }, index) => {
        // A replacement or an addition: show the outcome alone.
        const side = row.new ?? row.old;
        if (!side) return null;
        const kind =
          wholeFileIsNew || !row.changed ? "context" : row.new ? "add" : "del";
        return (
          <div className="diff-line" data-kind={kind} key={index}>
            <span className="diff-line__gutter diff-line__gutter--new">
              {row.new ? row.new.lineno : NO_LINE}
            </span>
            <span className="diff-line__sign">
              {kind === "add" ? "+" : kind === "del" ? "−" : " "}
            </span>
            {/* DFV-FR-46: the rendered outcome is the target and is editable
                throughout — added, replaced, and unmarked rows alike. A row
                shown in position marked as removed is the original's evidence
                for a deletion and is read-only; typing where it sits inserts
                into the target at that position rather than reviving it. */}
            {editing && row.new ? (
              <EditableLine
                content={row.new.content}
                segments={words?.new}
                tokens={tokens.new[row.new.lineno - 1]}
                line={row.new.lineno - 1}
                editing={editing}
              />
            ) : (
              <LineContent
                content={side.content}
                segments={row.new ? words?.new : undefined}
                tokens={
                  row.new
                    ? tokens.new[row.new.lineno - 1]
                    : row.old
                      ? tokens.old[row.old.lineno - 1]
                      : undefined
                }
              />
            )}
          </div>
        );
      })}
    </div>
  );
}

/**
 * DFV-FR-11 / DFV-FR-12 / DFV-FR-13: both revisions in full, aligned row for
 * row, in **one** scrolling element.
 *
 * The shared scroll position is structural rather than synchronised: one
 * scroller holding both columns cannot show the panes at different offsets, at
 * any velocity or however the scroll is driven, and neither pane can be
 * scrolled vertically on its own.
 */
export function SourceSideBySide({
  revisions,
  editing,
  tokens,
}: {
  revisions: FileRevisions | null;
  editing?: TargetEditing;
  tokens: RevisionTokens;
}) {
  const rows = useMemo(() => {
    const aligned = alignLines(revisions?.old ?? null, revisions?.new ?? null);
    return aligned.map((row) => {
      // A row with both sides present is a replacement, so the words that
      // differ within it can be marked (they are the actual edit).
      const words =
        row.changed && row.old && row.new
          ? diffWords(row.old.content, row.new.content)
          : null;
      return { row, words };
    });
  }, [revisions]);
  return (
    <div className="diff diff-sbs" data-testid="diff-side-by-side">
      <div className="diff-sbs__heading">
        <span className="diff-sbs__pane">old</span>
        <span className="diff-sbs__pane">new</span>
      </div>
      <div className="diff-sbs__scroll">
        {rows.map(({ row, words }, index) => (
          <div className="diff-sbs__row" key={index}>
            <SideCell
              side={row.old}
              changed={row.changed}
              kind="del"
              segments={words?.old}
              tokens={tokens.old}
            />
            {/* DFV-FR-45: the right pane is the target and is editable; the
                left is the original and is not, and neither is a filler row on
                either side. The caret cannot cross between them because the
                left pane renders no editable host at all. */}
            <SideCell
              side={row.new}
              changed={row.changed}
              kind="add"
              segments={words?.new}
              tokens={tokens.new}
              editing={editing}
            />
          </div>
        ))}
      </div>
    </div>
  );
}

/**
 * One half of an aligned row. An absent side is the inert filler of DFV-FR-11 —
 * no number and no content.
 */
function SideCell({
  side,
  changed,
  kind,
  segments,
  tokens,
  editing,
}: {
  side: AlignedSide | null;
  changed: boolean;
  kind: "add" | "del";
  segments?: Segment[];
  /** DFV-FR-57: this pane's own revision's tokens, by line. */
  tokens?: TokenSpan[][];
  /** Present on the right pane alone (DFV-FR-45). */
  editing?: TargetEditing;
}) {
  if (!side) {
    return (
      <div
        className="diff-line diff-sbs__cell"
        data-kind="filler"
        aria-hidden
        {...READ_ONLY_PROPS}
      >
        <span className="diff-line__gutter">{NO_LINE}</span>
        <span className="diff-line__sign" />
        <span className="diff-line__text" />
      </div>
    );
  }
  // DFV-FR-12: each side marks change in the terms of its own revision — the
  // left what the change acted on, the right the outcome.
  const marked = changed ? kind : "context";
  return (
    <div className="diff-line diff-sbs__cell" data-kind={marked}>
      <span className="diff-line__gutter">{side.lineno}</span>
      <span className="diff-line__sign">
        {marked === "add" ? "+" : marked === "del" ? "−" : " "}
      </span>
      {editing ? (
        <EditableLine
          content={side.content}
          segments={segments}
          tokens={tokens?.[side.lineno - 1]}
          line={side.lineno - 1}
          editing={editing}
        />
      ) : (
        <LineContent
          content={side.content}
          segments={segments}
          tokens={tokens?.[side.lineno - 1]}
        />
      )}
    </div>
  );
}

