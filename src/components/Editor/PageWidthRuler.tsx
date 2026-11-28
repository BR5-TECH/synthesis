/**
 * EPW-FR-ADPJ / EPW-FR-WOZO: the page width ruler at the foot of the document
 * column.
 *
 * A thin strip with a scale of ticks, the page as a light band, and two handles
 * at the edges of the text on the page. A drag of either handle sets the page
 * width (EPW-FR-AOVQ). The page is centred in the column, so both edges move by
 * the same distance. The ruler is outside the zoom region (EZM-FR-UJWE): its
 * own size never changes, but its handles follow the zoomed page.
 */
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";

import { useEditorPageWidth } from "../../hooks/useEditorPageWidth";
import { useEditorZoomLevel } from "../../hooks/useEditorZoom";
import {
  PAGE_WIDTH_DEFAULT,
  PAGE_WIDTH_LARGE_STEP,
  PAGE_WIDTH_MAX,
  PAGE_WIDTH_MIN,
  PAGE_WIDTH_STEP,
  clampPageWidth,
  commitEditorPageWidth,
  endEditorPageWidthDrag,
  getEditorPageWidth,
  previewEditorPageWidth,
  startEditorPageWidthDrag,
} from "../../state/editorPageWidth";

type Side = "left" | "right";

/** The page metrics of `colors_and_type.css`, for a document with no stylesheet. */
const FALLBACK_GUTTER = 48;
const FALLBACK_INSET = 16;
/** The hairline that the page draws on each side. */
const PAGE_BORDER = 1;

/** A length token of the root, in pixels, or `fallback` when it does not resolve. */
function rootPx(name: string, fallback: number): number {
  if (typeof document === "undefined") return fallback;
  const value = parseFloat(getComputedStyle(document.documentElement).getPropertyValue(name));
  return Number.isFinite(value) ? value : fallback;
}

/** Where the page and the edges of its text fall across a ruler of `rulerWidth`. */
export interface RulerGeometry {
  pageLeft: number;
  pageRight: number;
  textLeft: number;
  textRight: number;
}

/**
 * EPW-FR-KCZU / EPW-FR-WOZO: the page is `width × zoom` of text plus its
 * gutters and hairlines, and never wider than the column less its inset.
 */
export function rulerGeometry(
  rulerWidth: number,
  width: number,
  zoom: number,
  gutter = FALLBACK_GUTTER,
  inset = FALLBACK_INSET,
): RulerGeometry {
  const frame = gutter * 2 + PAGE_BORDER * 2;
  const outer = Math.max(0, Math.min(width * zoom + frame, rulerWidth - inset * 2));
  const text = Math.max(0, outer - frame);
  const centre = rulerWidth / 2;
  return {
    pageLeft: centre - outer / 2,
    pageRight: centre + outer / 2,
    textLeft: centre - text / 2,
    textRight: centre + text / 2,
  };
}

/**
 * EPW-FR-AOVQ / EPW-FR-IMWS: the width after a drag that moved the pointer
 * `outward` pixels away from the page (negative toward it). It counts from the
 * width at the start of the drag, not from where the handle stands, so a drag
 * outward on a page that the column caps makes the width larger.
 */
export function widthFromDrag(startWidth: number, outward: number, zoom: number): number {
  if (!(zoom > 0) || !Number.isFinite(outward)) return clampPageWidth(startWidth);
  return clampPageWidth(startWidth + (outward * 2) / zoom);
}

/** EPW-FR-IUHL: the width after a key on the handle of `side`, or `null` for another key. */
export function widthAfterKey(width: number, side: Side, key: string, shift: boolean): number | null {
  const step = shift ? PAGE_WIDTH_LARGE_STEP : PAGE_WIDTH_STEP;
  const outward = side === "left" ? "ArrowLeft" : "ArrowRight";
  const inward = side === "left" ? "ArrowRight" : "ArrowLeft";
  if (key === outward) return clampPageWidth(width + step);
  if (key === inward) return clampPageWidth(width - step);
  if (key === "Home") return PAGE_WIDTH_MIN;
  if (key === "End") return PAGE_WIDTH_MAX;
  return null;
}

/** EPW-FR-LSZF: the store logs a refused save; the ruler keeps the width on screen. */
function commit(width: number) {
  void commitEditorPageWidth(width).catch(() => undefined);
}

export function PageWidthRuler() {
  const width = useEditorPageWidth();
  const zoom = useEditorZoomLevel() / 100;
  const rulerRef = useRef<HTMLDivElement | null>(null);
  const [rulerWidth, setRulerWidth] = useState(0);
  const [metrics, setMetrics] = useState({ gutter: FALLBACK_GUTTER, inset: FALLBACK_INSET });
  const [dragging, setDragging] = useState<Side | null>(null);
  // Read by the drag listeners, which are bound once per drag.
  const zoomRef = useRef(zoom);
  zoomRef.current = zoom;
  /** Whether the pointer moved since the drag began, so a click sets nothing. */
  const movedRef = useRef(false);
  /** EPW-FR-AOVQ: where the drag began, and the width then. */
  const startRef = useRef({ x: 0, width: PAGE_WIDTH_DEFAULT });

  // EPW-FR-WOZO: the handles move with the page when the column width changes.
  useLayoutEffect(() => {
    const el = rulerRef.current;
    if (!el) return;
    setMetrics({
      gutter: rootPx("--doc-page-gutter", FALLBACK_GUTTER),
      inset: rootPx("--doc-page-inset", FALLBACK_INSET),
    });
    const measure = () => setRulerWidth(el.getBoundingClientRect().width);
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const widthAt = useCallback(
    (clientX: number): number => {
      const moved = clientX - startRef.current.x;
      const outward = dragging === "left" ? -moved : moved;
      return widthFromDrag(startRef.current.width, outward, zoomRef.current);
    },
    [dragging],
  );

  useEffect(() => {
    if (!dragging) return;
    const move = (event: PointerEvent) => {
      // On the window for the length of the drag, so a fast movement over the
      // page or out of the column still steers the width.
      event.preventDefault();
      movedRef.current = true;
      previewEditorPageWidth(widthAt(event.clientX));
    };
    const end = (event: PointerEvent) => {
      setDragging(null);
      // EPW-FR-JNCM: one save, when the drag ends. A click moves nothing and
      // saves nothing.
      if (!movedRef.current) {
        endEditorPageWidthDrag();
        return;
      }
      const next = event.type === "pointerup" ? widthAt(event.clientX) : null;
      commit(next ?? getEditorPageWidth());
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", end);
    window.addEventListener("pointercancel", end);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", end);
    };
  }, [dragging, widthAt]);

  // A ruler that goes away during a drag (its tab closes) still ends the drag,
  // so the store adopts the records of other windows again.
  const draggingRef = useRef(dragging);
  draggingRef.current = dragging;
  useEffect(
    () => () => {
      if (draggingRef.current === null) return;
      if (movedRef.current) commit(getEditorPageWidth());
      else endEditorPageWidthDrag();
    },
    [],
  );

  const geometry = rulerGeometry(rulerWidth, width, zoom, metrics.gutter, metrics.inset);
  const minorStep = PAGE_WIDTH_STEP * zoom;
  const majorStep = PAGE_WIDTH_LARGE_STEP * zoom;

  const handle = (side: Side) => {
    const x = side === "left" ? geometry.textLeft : geometry.textRight;
    return (
      <div
        key={side}
        className="page-ruler__handle"
        data-side={side}
        data-dragging={dragging === side || undefined}
        style={{ left: x }}
        role="separator"
        tabIndex={0}
        aria-orientation="vertical"
        aria-label={side === "left" ? "Page width, left edge" : "Page width, right edge"}
        aria-valuemin={PAGE_WIDTH_MIN}
        aria-valuemax={PAGE_WIDTH_MAX}
        aria-valuenow={width}
        aria-valuetext={`${width} pixels`}
        title="Drag to set the page width. Double-click for the default width."
        data-testid={`page-ruler-handle-${side}`}
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          event.preventDefault();
          (event.currentTarget as HTMLElement).focus();
          movedRef.current = false;
          startRef.current = { x: event.clientX, width: getEditorPageWidth() };
          startEditorPageWidthDrag();
          setDragging(side);
        }}
        onDoubleClick={() => commit(PAGE_WIDTH_DEFAULT)}
        onKeyDown={(event) => {
          const next = widthAfterKey(width, side, event.key, event.shiftKey);
          if (next === null) return;
          event.preventDefault();
          commit(next);
        }}
      >
        <span className="page-ruler__marker" aria-hidden="true" />
        {dragging === side && (
          <span className="page-ruler__label t-ui-xs" data-testid="page-ruler-label">
            {width} px
          </span>
        )}
      </div>
    );
  };

  return (
    <div
      ref={rulerRef}
      className="page-ruler"
      data-testid="page-width-ruler"
      data-dragging={dragging !== null || undefined}
    >
      <div
        className="page-ruler__page"
        aria-hidden="true"
        style={{
          left: geometry.pageLeft,
          width: Math.max(0, geometry.pageRight - geometry.pageLeft),
        }}
      />
      {/* EPW-FR-WOZO: the scale counts from the leading edge of the text, a
          minor tick for each 20 pixels of page width and a major one for each
          100, at the zoom level of the page. */}
      <div
        className="page-ruler__scale"
        aria-hidden="true"
        style={
          {
            "--page-ruler-origin": `${geometry.textLeft}px`,
            "--page-ruler-minor": `${minorStep}px`,
            "--page-ruler-major": `${majorStep}px`,
          } as React.CSSProperties
        }
      />
      {handle("left")}
      {handle("right")}
    </div>
  );
}
