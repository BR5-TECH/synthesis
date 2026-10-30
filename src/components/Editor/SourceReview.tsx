/**
 * The review of a proposal on the **source surface** of a file with no rich
 * surface (`../../../specifications/ui/PCR-prompt-change-review.md`
 * PCR-FR-SKRV; `ESH-editor-source-files.md` ESH-FR-SRVW).
 *
 * A textarea cannot carry decorations in its text, so while a proposal is
 * pending the file text is drawn read-only, with the draft review's own
 * decorations: the struck-through text a change removes, the block with the
 * text it proposes, the chip, and the gutter map. Editing of the file resumes
 * when the proposal closes. The proposed text of a change stays editable, in a
 * plain text field in its block.
 */
import { useEffect, useMemo, useRef, type ReactNode } from "react";

import { findNearest } from "../../state/commentAnchors";
import { HUNK_ATTR, HUNK_CLASS, hunkLabel, type DecoratedHunk } from "../DraftDiscussion";
import { ReviewOverlay } from "./ReviewOverlay";
import type { EditorReview, ReviewHunk } from "./props";

/** The statement the surface makes while the file text is read-only. */
export const SOURCE_REVIEW_NOTE =
  "This file is read-only while a proposed change is under review. Decide the changes to edit it again.";

/**
 * PCR-FR-SKRV: where each change lands in the file text. A change names the
 * text it alters, so it is found near its hint rather than put at the hint.
 */
export function placeOnSource(text: string, hunks: readonly ReviewHunk[]): DecoratedHunk[] {
  const placed: DecoratedHunk[] = [];
  for (const hunk of hunks) {
    // DCR-FR-07: an accepted change is in the file already.
    if (hunk.state === "accepted") continue;
    let from: number;
    let to: number;
    if (hunk.kind === "add") {
      if (hunk.lead === "") {
        from = Math.min(Math.max(hunk.hint, 0), text.length);
      } else {
        const at = findNearest(text, hunk.lead, Math.max(0, hunk.hint - hunk.lead.length));
        if (at === -1) continue;
        from = at + hunk.lead.length;
      }
      to = from;
    } else {
      if (hunk.before === "") continue;
      const at = findNearest(text, hunk.before, hunk.hint);
      if (at === -1) continue;
      from = at;
      to = at + hunk.before.length;
    }
    placed.push({
      id: hunk.id,
      kind: hunk.kind,
      state: hunk.state,
      from,
      to,
      after: hunk.after,
      lost: hunk.lost,
      position: hunk.position,
      total: hunk.total,
      agent: hunk.agent,
      draft: hunk.draft,
      editable: hunk.editable,
    });
  }
  // Changes do not overlap (PPC-FR-RJCA). One that reaches back into the one
  // before it is not drawn, rather than drawn over it.
  placed.sort((a, b) => a.from - b.from || a.to - b.to);
  const disjoint: DecoratedHunk[] = [];
  for (const hunk of placed) {
    const last = disjoint[disjoint.length - 1];
    if (last && hunk.from < last.to) continue;
    disjoint.push(hunk);
  }
  return disjoint;
}

function classes(hunk: DecoratedHunk, part: string): string {
  const out = [HUNK_CLASS, `${HUNK_CLASS}--${part}`];
  if (hunk.state === "discussing") out.push(`${HUNK_CLASS}--discussing`);
  if (hunk.state === "rejected") out.push(`${HUNK_CLASS}--rejected`);
  if (hunk.lost) out.push(`${HUNK_CLASS}--lost`);
  return out.join(" ");
}

export function SourceReview({
  text,
  review,
}: {
  /** The file text as the session holds it. */
  text: string;
  review: EditorReview;
}) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const placed = useMemo(() => placeOnSource(text, review.hunks), [text, review.hunks]);
  const focused = placed.find((h) => h.id === review.focused) ?? null;

  // DCR-FR-31: what the surface could draw, told to whoever states it.
  const placedIds = placed.map((h) => h.id).join("\u0000");
  useEffect(() => {
    review.onPlaced(placedIds === "" ? [] : placedIds.split("\u0000"));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [placedIds]);

  const parts: ReactNode[] = [];
  let cursor = 0;
  for (const hunk of placed) {
    if (hunk.from > cursor) parts.push(text.slice(cursor, hunk.from));
    const isFocused = String(hunk.id === review.focused);
    // DCR-FR-07: the text a change removes, struck through where it stands.
    if (hunk.kind !== "add" && hunk.to > hunk.from) {
      parts.push(
        <span
          key={`${hunk.id}:del`}
          className={classes(hunk, "del")}
          {...{ [HUNK_ATTR]: hunk.id }}
          data-focused={isFocused}
        >
          {text.slice(hunk.from, hunk.to)}
        </span>,
      );
    }
    if (hunk.kind === "del") {
      // DCR-FR-12: the space a deletion holds open for its chip.
      parts.push(
        <span
          key={`${hunk.id}:foot`}
          className={`${classes(hunk, "del-foot")} hunk--source-block`}
          {...{ [HUNK_ATTR]: hunk.id }}
          aria-hidden="true"
        />,
      );
    } else {
      // DCR-FR-09 / DCR-FR-24: what the change proposes, in its own block.
      const label = hunkLabel(hunk);
      const editable = hunk.editable && hunk.state !== "rejected";
      const value = hunk.draft ?? hunk.after ?? "";
      parts.push(
        <span
          key={`${hunk.id}:add`}
          className={`${classes(hunk, "add")} hunk--source-block`}
          {...{ [HUNK_ATTR]: hunk.id }}
          role="group"
          aria-label={label}
          data-focused={isFocused}
        >
          <span className="hunk__gutter" aria-hidden="true">
            +
          </span>
          <span className="hunk__body">
            {editable ? (
              <textarea
                className="hunk__doc hunk__source-edit"
                aria-label={`Proposed text of ${label}`}
                value={value}
                spellCheck={false}
                rows={Math.max(1, value.split("\n").length)}
                onChange={(e) => review.onEdit(hunk.id, e.target.value)}
              />
            ) : (
              <span className="hunk__doc hunk__plain">{value}</span>
            )}
          </span>
        </span>,
      );
    }
    cursor = hunk.to;
  }
  if (cursor < text.length) parts.push(text.slice(cursor));

  return (
    <div
      className="editor__source-review"
      ref={hostRef}
      data-testid="source-review"
      onClick={(event) => {
        // DCR-FR-12: a press on a change is what puts it under review.
        const el = (event.target as Element | null)?.closest?.(`[${HUNK_ATTR}]`);
        const id = el?.getAttribute(HUNK_ATTR);
        if (id) review.onFocus(id);
      }}
    >
      {/* PCR-FR-SKRV: the surface says why the text takes no edit. */}
      <p className="editor__source-review-note t-ui-xs" role="note">
        {SOURCE_REVIEW_NOTE}
      </p>
      <div className="editor__source-review-text" aria-label="Source" role="document">
        {parts}
      </div>
      <ReviewOverlay
        review={review}
        placed={placed}
        focused={focused}
        hostRef={hostRef}
        extent={text.length}
      />
    </div>
  );
}
