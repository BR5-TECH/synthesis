/**
 * The marks of the fragment discussions in the Source of a change file
 * (`../../../specifications/ui/CMT-comments.md` CMT-FR-28, CMT-FR-AYSL,
 * CMT-FR-18; `OCW-openspec-change-workspace.md` OCW-FR-JBFZ).
 *
 * Each open fragment discussion of the file is resolved against the file's
 * text, and its range is drawn with the same anchor classes, attributes and
 * label as the comment highlights of a document. The focused discussion's
 * range is drawn distinctly. A mark adds no character, so the offsets of a
 * selection read the same text.
 */
import type { ReactNode } from "react";

import { resolveAnchor } from "../../state/commentAnchors";
import { discussionFragment, type Discussion } from "../../types";
import {
  ANCHOR_ATTR,
  ANCHOR_CLASS,
  ANCHOR_FOCUSED_ATTR,
  FOCUSED_ANCHOR_CLASS,
  anchorLabel,
} from "../commentHighlight";

/** What the change discussion lends the Source of its files. */
export interface FragmentMarks {
  discussions: readonly Discussion[];
  /** CMT-FR-28: the focused discussion. */
  focused: string | null;
  /** CMT-FR-28: a mark was activated. */
  onActivate: (threadId: string) => void;
}

/** One resolved range of an open fragment discussion of a file. */
export interface PlacedMark {
  threadId: string;
  start: number;
  end: number;
}

/**
 * CMT-FR-18, CMT-FR-28: the range of each open fragment discussion of `path`
 * in `text`. An orphaned discussion has no range, and a resolved one shows
 * no mark.
 */
export function marksIn(path: string, text: string, discussions: readonly Discussion[]): PlacedMark[] {
  const placed: PlacedMark[] = [];
  for (const discussion of discussions) {
    if (discussion.resolved) continue;
    const fragment = discussionFragment(discussion);
    if (fragment === null || fragment.path !== path) continue;
    const range = resolveAnchor(text, fragment);
    if (range === null || range.end <= range.start) continue;
    placed.push({ threadId: discussion.id, start: range.start, end: range.end });
  }
  return placed;
}

/**
 * CMT-FR-28: the text of `[from, to)` with each part that a mark covers drawn
 * as a highlight. Where two ranges overlap, the part names the focused one if
 * it is among them, and the first one otherwise.
 */
export function markedText(
  text: string,
  from: number,
  to: number,
  placed: readonly PlacedMark[],
  marks: FragmentMarks,
): ReactNode[] {
  const inside = placed.filter((m) => m.start < to && m.end > from);
  if (inside.length === 0) return to > from ? [text.slice(from, to)] : [];
  const cuts = new Set<number>([from, to]);
  for (const m of inside) {
    cuts.add(Math.max(m.start, from));
    cuts.add(Math.min(m.end, to));
  }
  const points = [...cuts].sort((a, b) => a - b);
  const parts: ReactNode[] = [];
  for (let i = 0; i + 1 < points.length; i += 1) {
    const [a, b] = [points[i], points[i + 1]];
    const covering = inside.filter((m) => m.start <= a && m.end >= b);
    if (covering.length === 0) {
      parts.push(text.slice(a, b));
      continue;
    }
    const mark = covering.find((m) => m.threadId === marks.focused) ?? covering[0];
    const focused = mark.threadId === marks.focused;
    parts.push(
      <span
        key={`mark:${mark.threadId}:${a}`}
        className={focused ? `${ANCHOR_CLASS} ${FOCUSED_ANCHOR_CLASS}` : ANCHOR_CLASS}
        {...{ [ANCHOR_ATTR]: mark.threadId, [ANCHOR_FOCUSED_ATTR]: String(focused) }}
        aria-label={anchorLabel(mark.threadId, focused)}
        data-testid="openspec-fragment-mark"
        onClick={() => {
          // A click that ends a selection is the selection, not an activation.
          if (window.getSelection()?.isCollapsed === false) return;
          marks.onActivate(mark.threadId);
        }}
      >
        {text.slice(a, b)}
      </span>,
    );
  }
  return parts;
}
