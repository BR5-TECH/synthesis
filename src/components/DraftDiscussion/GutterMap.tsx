/**
 * DCR-FR-BQNL: the gutter map on the outer edge of the document scroller.
 *
 * One mark per change, placed where the change is in the prompt, so a change
 * outside the viewport stays visible as a mark rather than being something the
 * author has to scroll to find. A mark far from what is on screen is drawn
 * back, so distance reads without a second colour.
 */
import { hunkLabel, type DecoratedHunk } from "./hunkDecorations";

export function GutterMap({
  hunks,
  focused,
  extent,
  onSelect,
}: {
  hunks: readonly DecoratedHunk[];
  focused: string | null;
  /** The size of the document the positions are within. */
  extent: number;
  onSelect: (hunkId: string) => void;
}) {
  if (hunks.length === 0) return null;
  return (
    <div
      className="dds-gutter"
      role="group"
      aria-label="Proposed changes in this document"
      data-testid="hunk-gutter-map"
    >
      {hunks.map((hunk) => {
        const at = extent > 0 ? Math.min(1, Math.max(0, hunk.from / extent)) : 0;
        return (
          <button
            key={hunk.id}
            type="button"
            className="dds-gutter__mark"
            data-kind={hunk.kind}
            data-focused={hunk.id === focused}
            data-lost={hunk.lost}
            style={{ top: `${at * 100}%` }}
            // DCR-FR-28: a real control with a name, not a coloured span. The
            // accelerators are the primary way between the changes, but a mark
            // that can only be clicked is a route a keyboard cannot take at all
            // — and the map is how a change outside the viewport is found.
            aria-label={hunkLabel(hunk)}
            aria-current={hunk.id === focused}
            title={hunkLabel(hunk)}
            onClick={() => onSelect(hunk.id)}
          />
        );
      })}
    </div>
  );
}
