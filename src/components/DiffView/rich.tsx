import { useMemo } from "react";
import { alignBy } from "../../diff/lineAlign";
import type { AlignedPair } from "../../diff/lineAlign";
import { parseInline, parseMarkdownBlocks } from "../../diff/markdown";
import type { MarkdownBlock } from "../../diff/markdown";
import { changedRanges, diffWords, splitByRanges } from "../../diff/wordDiff";
import type { Range } from "../../diff/wordDiff";
import { targetLines } from "../../diff/targetEdit";
import { READ_ONLY_PROPS, type TargetEditing } from "../DiffTarget";
import {
  EditableRichRun,
  LIVE_RUN_LIMIT,
  type BlockMark,
  type RichUnitMark,
} from "../DiffRichTarget";
import { frontmatterLineCount } from "../markdownFidelity";
import type { FileRevisions } from "../../types";

// ---------------------------------------------------------------------------
// Rich rendering (DFV-FR-17 .. DFV-FR-22)
// ---------------------------------------------------------------------------

/**
 * A block carries its change as a whole, and — when it replaced a counterpart —
 * additionally marks the words within it that differ, so a one-word edit to a
 * long paragraph is visible as that one word rather than as "this paragraph
 * changed somewhere".
 *
 * This is the **static** rendering, which is what the original is always drawn
 * with and what the target is drawn with wherever it is not being edited: a
 * read-only comparison, an unresolved conflict, or a run that has not been
 * reached yet (`DiffRichTarget.tsx`).
 */
function RichBlock({
  block,
  mark,
  ranges,
  target,
}: {
  block: MarkdownBlock;
  mark: BlockMark;
  ranges?: Range[];
  /**
   * DFV-FR-41: whether this is the target's content. The read-only state is
   * carried by every rendering of the original — the left pane, a removed
   * block, and the predecessor of a replacement — and by none of the target's,
   * whose own surface says what it is.
   */
  target?: boolean;
}) {
  return (
    <div
      className="diff-block"
      data-mark={mark}
      data-kind={block.kind}
      {...(target ? {} : READ_ONLY_PROPS)}
    >
      {/* The marker is what carries the marking without colour. */}
      <span className="diff-block__marker" aria-hidden>
        {mark === "added" ? "+" : mark === "removed" ? "\u2212" : ""}
      </span>
      {/* `doc` is the class the Editor puts on its ProseMirror node, so the
          rendered document is typeset by exactly the rules that typeset it in
          the Editor. */}
      <div className="diff-block__body doc">
        <MarkdownBody block={block} ranges={ranges} />
      </div>
    </div>
  );
}

/**
 * One block as a rich stream renders it, before the target's blocks are grouped
 * into the documents they actually form.
 */
interface RichItem {
  key: string;
  block: MarkdownBlock;
  mark: BlockMark;
  ranges?: Range[];
  /** Target content, and so the author's to type into (DFV-FR-41). */
  target: boolean;
}

type RichGroup =
  | { kind: "block"; item: RichItem }
  | { kind: "run"; items: RichItem[] };

/**
 * Whether these blocks, taken alone, are the document they are part of
 * (DFV-FR-47).
 *
 * A run is edited by parsing its own lines and serialising them back, which is
 * only faithful if those lines *are* a document. Two constructs are not:
 *
 * - A **table row** without the `|---|---|` that makes the rows a table. On its
 *   own it is a paragraph of pipes, and writing it back writes escaped prose
 *   where a table row was.
 * - A **nested list item** whose parent item is outside the run. On its own it
 *   is a top-level item, and writing it back flattens the nesting.
 *
 * Both are reachable — side-by-side pairs every block into a row of its own,
 * and a removed block can fall anywhere in Unified — so a run that is not a
 * document renders as the static blocks it is rather than corrupting the file
 * the first time it is typed into.
 */
function selfContained(items: RichItem[]): boolean {
  const holdsDivider = items.some((item) => item.block.isDivider);
  if (items.some((item) => item.block.kind === "table-row") && !holdsDivider) {
    return false;
  }
  const first = items[0].block;
  return !(first.kind === "list-item" && (first.depth ?? 0) > 0);
}

/**
 * Consecutive target blocks, gathered into the runs they are edited as
 * (DFV-FR-47).
 *
 * A run ends wherever the stream puts something that is not the target between
 * two of its blocks — a removed block in **Unified** (DFV-FR-20), a block
 * deleted outright in **Final** (DFV-FR-22) — because the author cannot type
 * across a passage of the original, and the run's own document has to be
 * exactly the target lines it occupies.
 */
function groupRuns(
  items: RichItem[],
  editable: boolean,
  frontmatter: number,
): RichGroup[] {
  const groups: RichGroup[] = [];
  let run: RichItem[] = [];
  const flush = () => {
    if (run.length === 0) return;
    if (selfContained(run)) groups.push({ kind: "run", items: run });
    else for (const item of run) groups.push({ kind: "block", item });
    run = [];
  };
  for (const item of items) {
    // The frontmatter region is the one part of a Markdown file the WYSIWYG
    // surface deliberately does not hold (per `EDT-editor.md` EDT-FR-18): the
    // Editor splits it off before parsing, and a run that swallowed it would
    // serialise the fences back as a horizontal rule with a paragraph between
    // them — a file destroyed by an edit made three screens below it.
    if (!editable || !item.target || item.block.line < frontmatter) {
      flush();
      groups.push({ kind: "block", item });
      continue;
    }
    run.push(item);
  }
  flush();
  return groups;
}

/** The half-open line range of a rendered block in the revision it came from. */
function blockLines(block: MarkdownBlock): [number, number] {
  return [block.line, block.line + block.source.split("\n").length];
}

/**
 * A single-column rich rendering: the blocks in order, with the target's
 * gathered into editable documents.
 */
function RichStream({
  items,
  editing,
  targetText,
  className,
  testid,
}: {
  items: RichItem[];
  editing?: TargetEditing;
  /** The whole target, which is what a run's own lines are sliced out of. */
  targetText: string | null;
  className: string;
  testid: string;
}) {
  const editable = editing != null;
  const lines = useMemo(() => targetLines(targetText ?? ""), [targetText]);
  const frontmatter = useMemo(
    () => frontmatterLineCount(targetText ?? ""),
    [targetText],
  );
  const groups = useMemo(
    () => groupRuns(items, editable, frontmatter),
    [items, editable, frontmatter],
  );
  const deferred =
    groups.filter((group) => group.kind === "run").length > LIVE_RUN_LIMIT;
  return (
    <div className={className} data-testid={testid}>
      {groups.map((group) =>
        group.kind === "block" ? (
          <RichBlock
            key={group.item.key}
            block={group.item.block}
            mark={group.item.mark}
            ranges={group.item.ranges}
            target={editable && group.item.target}
          />
        ) : (
          <RichRun
            key={group.items[0].key}
            items={group.items}
            lines={lines}
            editing={editing!}
            deferred={deferred}
          />
        ),
      )}
    </div>
  );
}

/** One run of target blocks, as the document those lines are. */
function RichRun({
  items,
  lines,
  editing,
  deferred,
}: {
  items: RichItem[];
  /** The target's lines, which the run's own are sliced from. */
  lines: string[];
  editing: TargetEditing;
  deferred: boolean;
}) {
  const from = items[0].block.line;
  const to = blockLines(items[items.length - 1].block)[1];
  // A table's `|---|---|` is structure rather than a row: the parse reads it as
  // a block of its own, and the document the run renders has no node for it —
  // so leaving it in would shift every marking after it onto the wrong row.
  const units: RichUnitMark[] = items
    .filter((item) => !item.block.isDivider)
    .map((item) => ({
      mark: item.mark,
      kind: item.block.kind,
      ranges: item.ranges,
      // What the ranges were measured against, so the run can put them back on
      // the text its own document holds where the two differ.
      basis: blockDiffBasis(item.block) ?? undefined,
    }));
  return (
    <EditableRichRun
      source={lines.slice(from, to).join("\n")}
      from={from}
      to={to}
      units={units}
      editing={editing}
      deferred={deferred}
    >
      {items.map((item) => (
        <RichBlock
          key={item.key}
          block={item.block}
          mark={item.mark}
          ranges={item.ranges}
          target
        />
      ))}
    </EditableRichRun>
  );
}

/** Runs of text, with the ones inside a changed range wrapped for marking. */
function Marked({
  text,
  offset,
  ranges,
}: {
  text: string;
  offset: number;
  ranges?: Range[];
}) {
  if (!ranges || ranges.length === 0) return <>{text}</>;
  return (
    <>
      {splitByRanges(text, offset, ranges).map((run, index) =>
        run.changed ? (
          <mark className="diff-word" key={index}>
            {run.text}
          </mark>
        ) : (
          <span key={index}>{run.text}</span>
        ),
      )}
    </>
  );
}

/**
 * The text a block's inline markup renders to, with the syntax removed.
 *
 * This — not the block's source — is what the word marking is computed
 * against, because it is what the reader sees. Diffing the source would put a
 * changed range at an offset that no longer exists once `**bold**` has become
 * two fewer characters on each side.
 */
function inlineText(text: string): string {
  return parseInline(text)
    .map((span) => span.text)
    .join("");
}

function Inline({ text, ranges }: { text: string; ranges?: Range[] }) {
  const spans = parseInline(text);
  // The ranges are offsets into the concatenated span text, so each span has to
  // know where it starts in that string to mark the right part of itself.
  let cursor = 0;
  return (
    <>
      {spans.map((span, index) => {
        const offset = cursor;
        cursor += span.text.length;
        const content = (
          <Marked text={span.text} offset={offset} ranges={ranges} />
        );
        switch (span.kind) {
          case "strong":
            return <strong key={index}>{content}</strong>;
          case "em":
            return <em key={index}>{content}</em>;
          case "code":
            return <code key={index}>{content}</code>;
          case "link":
            // The rendered document shows the link's text rather than its
            // syntax. It is not navigable: the tab renders no affordance that
            // acts on anything (DFV-FR-06).
            return (
              <a key={index} title={span.href} aria-disabled>
                {content}
              </a>
            );
          default:
            return <span key={index}>{content}</span>;
        }
      })}
    </>
  );
}

/**
 * What the word marking of a replaced block is computed over.
 *
 * A fenced code block is literal, so its own text is the basis. Everything else
 * is marked against its *rendered* text (`inlineText`). A table row is left to
 * its whole-block marking: its cells are already a fine-grained unit, and there
 * is no single string whose offsets would address them.
 */
function blockDiffBasis(block: MarkdownBlock): string | null {
  switch (block.kind) {
    case "code":
      return block.text;
    case "table-row":
    case "rule":
      return null;
    default:
      return inlineText(block.text);
  }
}

/** The changed ranges of each side of a replaced block, when there are any. */
function blockWordRanges(
  oldBlock: MarkdownBlock | null,
  newBlock: MarkdownBlock | null,
): { old?: Range[]; new?: Range[] } {
  if (!oldBlock || !newBlock) return {};
  const before = blockDiffBasis(oldBlock);
  const after = blockDiffBasis(newBlock);
  if (before == null || after == null) return {};
  const words = diffWords(before, after);
  if (!words) return {};
  return { old: changedRanges(words.old), new: changedRanges(words.new) };
}

/**
 * DFV-FR-17: the formatted document — headings, emphasis, lists, tables, code.
 *
 * The elements are the ones the Editor's WYSIWYG surface emits, and the caller
 * wraps them in the same `doc` class the Editor gives ProseMirror, so a Spec
 * read here and the same Spec open in the Editor are typeset identically
 * rather than merely similarly. Nothing about the document's appearance is
 * defined twice.
 */
function MarkdownBody({
  block,
  ranges,
}: {
  block: MarkdownBlock;
  ranges?: Range[];
}) {
  switch (block.kind) {
    case "heading": {
      const level = Math.min(Math.max(block.level ?? 1, 1), 6);
      const Tag = `h${level}` as "h1";
      return (
        <Tag>
          <Inline text={block.text} ranges={ranges} />
        </Tag>
      );
    }
    case "code":
      return (
        <pre data-language={block.language ?? ""}>
          <code>
            <Marked text={block.text} offset={0} ranges={ranges} />
          </code>
        </pre>
      );
    case "list-item": {
      // One block per item (DFV-FR-19), so each renders as a one-item list —
      // which is what keeps the bullet, the indent, and the typography the
      // Editor's lists have.
      const item = (
        <li>
          <Inline text={block.text} ranges={ranges} />
        </li>
      );
      const indent = { marginLeft: (block.depth ?? 0) * 20 };
      return block.ordered ? (
        <ol style={indent} start={Number.parseInt(block.marker ?? "1", 10) || 1}>
          {item}
        </ol>
      ) : (
        <ul style={indent}>{item}</ul>
      );
    }
    case "table-row":
      // A divider row is table structure rather than content; it renders as the
      // rule it stands for instead of as `|---|---|`.
      return block.isDivider ? (
        <div className="md-table-divider" aria-hidden />
      ) : (
        <div className="md-table-row">
          {(block.cells ?? []).map((cell, index) => (
            <span className="md-table-cell" key={index}>
              <Inline text={cell} />
            </span>
          ))}
        </div>
      );
    case "quote":
      return (
        <blockquote>
          <Inline text={block.text} ranges={ranges} />
        </blockquote>
      );
    case "rule":
      return <hr />;
    default:
      return (
        <p>
          <Inline text={block.text} ranges={ranges} />
        </p>
      );
  }
}

/** Blocks are the same block when their source is identical (DFV-FR-19). */
function alignBlocks(
  oldText: string | null,
  newText: string | null,
): AlignedPair<MarkdownBlock>[] {
  return alignBy(
    parseMarkdownBlocks(oldText),
    parseMarkdownBlocks(newText),
    (block) => `${block.kind}\0${block.source}`,
  );
}

/**
 * DFV-FR-20: the new revision's document with each changed block marked in
 * place, and a removed or replaced block immediately above the block that
 * replaces it, so the old rendering and its outcome read in sequence.
 */
export function RichUnified({
  revisions,
  editing,
}: {
  revisions: FileRevisions | null;
  editing?: TargetEditing;
}) {
  // The word marking is computed inside the same memo as the pairing, not in
  // the render body: `diffWords` tokenizes and aligns, and the shell re-renders
  // this tab for reasons that have nothing to do with the diff, so doing it per
  // render would repeat that work over every changed block for free.
  const items = useMemo(() => {
    const out: RichItem[] = [];
    alignBlocks(revisions?.old ?? null, revisions?.new ?? null).forEach(
      (pair, index) => {
        const words = pair.changed ? blockWordRanges(pair.old, pair.new) : {};
        if (!pair.changed && pair.new) {
          out.push({
            key: `c${index}`,
            block: pair.new,
            mark: "none",
            target: true,
          });
          return;
        }
        if (pair.old) {
          out.push({
            key: `r${index}`,
            block: pair.old,
            mark: "removed",
            ranges: words.old,
            target: false,
          });
        }
        if (pair.new) {
          out.push({
            key: `a${index}`,
            block: pair.new,
            mark: "added",
            ranges: words.new,
            target: true,
          });
        }
      },
    );
    return out;
  }, [revisions]);
  return (
    <RichStream
      items={items}
      editing={editing}
      targetText={revisions?.new ?? null}
      className="diff diff-rich diff-rich--page"
      testid="diff-rich-unified"
    />
  );
}

/**
 * DFV-FR-21: the old revision's document left and the new right, marked per
 * DFV-FR-12 at block granularity, aligned block-to-block with filler where a
 * block has no counterpart, sharing the single scroll position of DFV-FR-13.
 *
 * The target is a run per row rather than a run per document: a row is what
 * holds the two revisions opposite each other, and a document spanning rows
 * would decide its own height and take the alignment with it (DFV-FR-45).
 */
export function RichSideBySide({
  revisions,
  editing,
}: {
  revisions: FileRevisions | null;
  editing?: TargetEditing;
}) {
  const pairs = useMemo(
    () =>
      alignBlocks(revisions?.old ?? null, revisions?.new ?? null).map((pair) => ({
        pair,
        words: pair.changed ? blockWordRanges(pair.old, pair.new) : {},
      })),
    [revisions],
  );
  // A run of one block per row is what keeps the two panes opposite each other
  // (DFV-FR-45), with two exceptions that would render as nonsense on their
  // own: the frontmatter, which the WYSIWYG surface never holds, and a table's
  // `|---|---|`, which is structure rather than a row of content.
  const target = revisions?.new ?? null;
  const lines = useMemo(() => targetLines(target ?? ""), [target]);
  const frontmatter = useMemo(() => frontmatterLineCount(target ?? ""), [target]);
  const deferred =
    pairs.filter(({ pair }) => pair.new != null).length > LIVE_RUN_LIMIT;
  return (
    <div
      className="diff diff-sbs diff-rich diff-rich--split"
      data-testid="diff-rich-side-by-side"
    >
      <div className="diff-sbs__heading">
        <span className="diff-sbs__pane">old</span>
        <span className="diff-sbs__pane">new</span>
      </div>
      <div className="diff-sbs__scroll">
        {/* The rows' own wrapper, which is what the two page sheets are drawn
            behind (DFV-FR-38). It is as tall as the document rather than as
            tall as the viewport, which is the whole reason it exists. */}
        <div className="diff-sbs__sheets">
        {pairs.map(({ pair, words }, index) => {
          const mark: BlockMark = pair.changed ? "added" : "none";
          const item: RichItem | null = pair.new
            ? {
                key: `n${index}`,
                block: pair.new,
                mark,
                ranges: words.new,
                target: true,
              }
            : null;
          return (
            <div className="diff-sbs__row" key={index}>
              {pair.old ? (
                <RichBlock
                  block={pair.old}
                  mark={pair.changed ? "removed" : "none"}
                  ranges={words.old}
                />
              ) : (
                <div
                  className="diff-block"
                  data-mark="filler"
                  aria-hidden
                  {...READ_ONLY_PROPS}
                />
              )}
              {item == null ? (
                <div
                  className="diff-block"
                  data-mark="filler"
                  aria-hidden
                  {...READ_ONLY_PROPS}
                />
              ) : editing &&
                item.block.line >= frontmatter &&
                !item.block.isDivider &&
                selfContained([item]) ? (
                <RichRun
                  items={[item]}
                  lines={lines}
                  editing={editing}
                  deferred={deferred}
                />
              ) : (
                <RichBlock
                  block={item.block}
                  mark={item.mark}
                  ranges={item.ranges}
                  target={editing != null}
                />
              )}
            </div>
          );
        })}
        </div>
      </div>
    </div>
  );
}

/**
 * **Final** in rich rendering: the new revision's document, with a changed or
 * added block marked and word-segmented, and a block that was removed outright
 * shown in its place marked as removed.
 *
 * This is what distinguishes it from **Unified**, which shows a replaced block
 * *and* the block that replaced it (DFV-FR-20). Final shows the outcome, plus
 * only those blocks that have no outcome to show — which is why this is the mode
 * where the target is most often one unbroken document (DFV-FR-46).
 */
export function RichFinal({
  revisions,
  editing,
}: {
  revisions: FileRevisions | null;
  editing?: TargetEditing;
}) {
  // As in the source rendering: a file the comparison adds is new throughout,
  // and a document of nothing but marked blocks is harder to read than the
  // document itself.
  const wholeFileIsNew = revisions != null && revisions.old == null;
  const items = useMemo(() => {
    const out: RichItem[] = [];
    alignBlocks(revisions?.old ?? null, revisions?.new ?? null).forEach(
      (pair, index) => {
        const block = pair.new ?? pair.old;
        if (!block) return;
        const words = pair.changed ? blockWordRanges(pair.old, pair.new) : {};
        out.push({
          key: String(index),
          block,
          mark:
            wholeFileIsNew || !pair.changed
              ? "none"
              : pair.new
                ? "added"
                : "removed",
          ranges: pair.new ? words.new : undefined,
          // DFV-FR-46: the outcome is editable throughout; a block shown in
          // position marked as removed is the original's evidence for a
          // deletion and is not.
          target: pair.new != null,
        });
      },
    );
    return out;
  }, [revisions, wholeFileIsNew]);
  return (
    <RichStream
      items={items}
      editing={editing}
      targetText={revisions?.new ?? null}
      className="diff diff-rich diff-rich--page"
      testid="diff-rich-final"
    />
  );
}

