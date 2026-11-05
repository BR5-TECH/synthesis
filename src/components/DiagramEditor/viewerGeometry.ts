/**
 * EDT-FR-LJAP, EDT-FR-MZIF, EDT-FR-PDBC: the zoom and the pan of the diagram
 * viewer overlay. Pure functions of the sizes, so each opening starts from the
 * same fit and nothing is remembered.
 */

export interface Size {
  width: number;
  height: number;
}

/**
 * Where the diagram is drawn in the diagram area: scaled by `scale` from its
 * natural size, with its top-left corner at `x`, `y` of the area.
 */
export interface ViewTransform {
  scale: number;
  x: number;
  y: number;
}

/** EDT-FR-MZIF: the zoom stays between 10% and 1000% of the natural size. */
export const MIN_ZOOM = 0.1;
export const MAX_ZOOM = 10;
/** EDT-FR-MZIF: how much one press of `+` or `-` zooms. */
export const ZOOM_STEP = 1.25;
/** EDT-FR-PDBC: how far one arrow key pans, in pixels. */
export const PAN_STEP = 48;
/** EDT-FR-LJAP: the free space round a fitted diagram, in pixels. */
export const FIT_MARGIN = 16;

export const clampZoom = (scale: number) => Math.min(Math.max(scale, MIN_ZOOM), MAX_ZOOM);

const usable = (size: Size) => size.width > 0 && size.height > 0;

/**
 * EDT-FR-LJAP: below this zoom, a fit of all of the diagram is too small to
 * read, and the fit fills the area in one direction only.
 */
export const FIT_THRESHOLD = 0.8;
/** EDT-FR-LJAP: a fit in one direction does not enlarge the diagram. */
const ONE_DIRECTION_MAX = 1;

/**
 * EDT-FR-LJAP: the position of a fitted diagram on one axis. It is centred
 * when it fits in the area, else it starts at the margin.
 */
const place = (area: number, size: number) =>
  size <= area ? (area - size) / 2 : FIT_MARGIN;

/**
 * EDT-FR-LJAP: the fit of the diagram, with its aspect ratio kept. All of the
 * diagram shows when that needs a zoom of `FIT_THRESHOLD` or more. Else the
 * diagram fills the area in the direction that needs the smaller reduction,
 * at not more than its natural size, and extends past the area in the other.
 */
export function fitTransform(area: Size, diagram: Size): ViewTransform {
  if (!usable(area) || !usable(diagram)) return { scale: 1, x: 0, y: 0 };
  const room = {
    width: Math.max(area.width - 2 * FIT_MARGIN, 1),
    height: Math.max(area.height - 2 * FIT_MARGIN, 1),
  };
  const across = room.width / diagram.width;
  const down = room.height / diagram.height;
  const whole = clampZoom(Math.min(across, down));
  const scale =
    whole >= FIT_THRESHOLD
      ? whole
      : clampZoom(Math.min(Math.max(across, down), ONE_DIRECTION_MAX));
  return {
    scale,
    x: place(area.width, diagram.width * scale),
    y: place(area.height, diagram.height * scale),
  };
}

/**
 * EDT-FR-MZIF: the transform zoomed by `factor` about the point `px`, `py` of
 * the area. The point of the diagram under it stays where it is.
 */
export function zoomAt(t: ViewTransform, factor: number, px: number, py: number): ViewTransform {
  const scale = clampZoom(t.scale * factor);
  const ratio = scale / t.scale;
  return { scale, x: px - (px - t.x) * ratio, y: py - (py - t.y) * ratio };
}

/** EDT-FR-PDBC: the transform panned by `dx`, `dy` pixels. */
export function panBy(t: ViewTransform, dx: number, dy: number): ViewTransform {
  return { ...t, x: t.x + dx, y: t.y + dy };
}

/** A wheel delta in pixels. A line is about 16 pixels, a page about 800. */
const wheelPixels = (delta: number, deltaMode: number) =>
  deltaMode === 1 ? delta * 16 : deltaMode === 2 ? delta * 800 : delta;

/**
 * EDT-FR-MZIF: the zoom factor of one wheel event with Ctrl or Cmd held. Up
 * zooms in. A trackpad pinch arrives as a wheel event with Ctrl held and small
 * deltas, so `pinch` makes those deltas zoom more. One event zooms by not more
 * than one step of `+` or `-`. Thus a mouse wheel notch with Ctrl held, which
 * is also a `pinch` event, zooms by one full step.
 */
export function wheelFactor(deltaY: number, deltaMode: number, pinch = false): number {
  // Without `pinch`, a wheel notch of 100 pixels zooms by about 20%.
  const rate = pinch ? 0.02 : 0.002;
  const factor = Math.exp(-wheelPixels(deltaY, deltaMode) * rate);
  return Math.min(Math.max(factor, 1 / ZOOM_STEP), ZOOM_STEP);
}

/**
 * EDT-FR-PDBC: the pan of one wheel event, in pixels. The diagram moves as a
 * page moves when it scrolls. With Shift held, a vertical wheel pans
 * horizontally, for a platform that does not turn it into a horizontal delta.
 */
export function wheelPan(
  deltaX: number,
  deltaY: number,
  deltaMode: number,
  shiftKey: boolean,
): { dx: number; dy: number } {
  let x = wheelPixels(deltaX, deltaMode);
  let y = wheelPixels(deltaY, deltaMode);
  if (shiftKey && x === 0) {
    x = y;
    y = 0;
  }
  // `0 - x` and not `-x`, so that no delta gives a negative zero.
  return { dx: 0 - x, dy: 0 - y };
}

/**
 * The natural size of a rendered diagram: its view box, else its width and
 * height in pixels. Null when it states neither.
 */
export function naturalSize(svg: Element): Size | null {
  const box = svg.getAttribute("viewBox");
  if (box) {
    const parts = box.trim().split(/[\s,]+/).map(Number);
    if (parts.length === 4 && parts[2] > 0 && parts[3] > 0) {
      return { width: parts[2], height: parts[3] };
    }
  }
  const width = parseFloat(svg.getAttribute("width") ?? "");
  const height = parseFloat(svg.getAttribute("height") ?? "");
  if (width > 0 && height > 0 && !/%/.test(svg.getAttribute("width") ?? "")) {
    return { width, height };
  }
  return null;
}
