/**
 * DDS-FR-PNXR: the draggable divider between the two columns.
 *
 * A `separator` with an `aria-valuenow`, so the split is readable and movable
 * without a pointer: the arrow keys nudge it and Home / End reach the two ends
 * of the range the drag is allowed. A drag-only splitter would put the whole
 * ratio behind a pointer, which is the one thing DDS's accessibility
 * requirement rules out.
 */
import { useCallback, useEffect, useRef, useState } from "react";

import {
  MAX_FRACTION,
  MIN_FRACTION,
  clampFraction,
  documentFraction,
  type SplitRatio,
} from "./ratio";

/** How far one arrow key moves the split. */
const STEP = 0.02;

export function Splitter({
  ratio,
  onChange,
  boundsRef,
  hidden,
}: {
  ratio: SplitRatio;
  onChange: (fraction: number) => void;
  /** The element the two columns are laid out in, which the drag is measured against. */
  boundsRef: React.RefObject<HTMLElement | null>;
  /**
   * DDS-FR-XQMF: one of the two columns is hidden, so there is no split.
   *
   * Out of the layout and out of the tab order, rather than a separator that
   * reports a ratio between one column and nothing.
   */
  hidden?: boolean;
}) {
  const [dragging, setDragging] = useState(false);
  const fraction = documentFraction(ratio);
  // Read by the drag listeners, which are bound once per drag and would
  // otherwise close over the fraction as it stood when the drag began.
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  const measure = useCallback(
    (clientX: number) => {
      const bounds = boundsRef.current?.getBoundingClientRect();
      if (!bounds || bounds.width <= 0) return;
      onChangeRef.current(
        clampFraction((clientX - bounds.left) / bounds.width),
      );
    },
    [boundsRef],
  );

  useEffect(() => {
    if (!dragging) return;
    const move = (event: PointerEvent) => {
      // The listeners are on the window for the length of the drag, so a fast
      // movement out over the discussion column still steers the split rather
      // than being taken by whatever is under the pointer.
      event.preventDefault();
      measure(event.clientX);
    };
    const end = () => setDragging(false);
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", end);
    window.addEventListener("pointercancel", end);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", end);
    };
  }, [dragging, measure]);

  return (
    <div
      className="dds-splitter"
      role="separator"
      hidden={hidden}
      tabIndex={hidden ? -1 : 0}
      aria-label="Resize the document and discussion columns"
      aria-orientation="vertical"
      aria-valuemin={Math.round(MIN_FRACTION * 100)}
      aria-valuemax={Math.round(MAX_FRACTION * 100)}
      aria-valuenow={Math.round(fraction * 100)}
      aria-valuetext={`Document ${Math.round(fraction * 100)} percent`}
      data-dragging={dragging}
      onPointerDown={(event) => {
        event.preventDefault();
        setDragging(true);
      }}
      onKeyDown={(event) => {
        if (event.key === "ArrowLeft") onChange(clampFraction(fraction - STEP));
        else if (event.key === "ArrowRight") onChange(clampFraction(fraction + STEP));
        else if (event.key === "Home") onChange(MIN_FRACTION);
        else if (event.key === "End") onChange(MAX_FRACTION);
        else return;
        event.preventDefault();
      }}
    >
      <span className="dds-splitter__grip" aria-hidden="true" />
    </div>
  );
}
