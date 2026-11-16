/**
 * SMP-FR-SWCL: the splitter between the canvas and the Discussion column. A
 * `separator` with a value, so the width is readable and movable without a
 * pointer: the arrow keys nudge it, and Home and End reach the two ends of the
 * range from 280px to 560px.
 */
import { useRef, useState, type KeyboardEvent, type PointerEvent } from "react";

import {
  DISCUSSION_WIDTH_MAX,
  DISCUSSION_WIDTH_MIN,
} from "../../state/specMap/discussion";

/** How far one arrow key moves the column, in pixels. */
const STEP = 16;

export function ColumnSplitter({
  width,
  onWidth,
}: {
  width: number;
  onWidth: (width: number) => void;
}) {
  const [dragging, setDragging] = useState(false);
  const start = useRef<{ x: number; width: number } | null>(null);

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    start.current = { x: e.clientX, width };
    e.currentTarget.setPointerCapture?.(e.pointerId);
    setDragging(true);
  };
  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    const from = start.current;
    // The column is on the right, so a move to the left makes it wider.
    if (from) onWidth(from.width + (from.x - e.clientX));
  };
  const end = () => {
    start.current = null;
    setDragging(false);
  };
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const next =
      e.key === "ArrowLeft"
        ? width + STEP
        : e.key === "ArrowRight"
          ? width - STEP
          : e.key === "Home"
            ? DISCUSSION_WIDTH_MIN
            : e.key === "End"
              ? DISCUSSION_WIDTH_MAX
              : null;
    if (next === null) return;
    e.preventDefault();
    onWidth(next);
  };

  return (
    <div
      className="dds-splitter map-discussion-splitter"
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize the discussion column"
      aria-valuemin={DISCUSSION_WIDTH_MIN}
      aria-valuemax={DISCUSSION_WIDTH_MAX}
      aria-valuenow={width}
      tabIndex={0}
      data-dragging={dragging}
      data-testid="map-discussion-splitter"
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={end}
      onPointerCancel={end}
      onKeyDown={onKeyDown}
    >
      <span className="dds-splitter__grip" aria-hidden="true" />
    </div>
  );
}
