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
 * EDT-FR-LJAP: the diagram with its aspect ratio kept, at the largest size at
 * which all of it shows in the area, centred in the area.
 */
export function fitTransform(area: Size, diagram: Size): ViewTransform {
  if (!usable(area) || !usable(diagram)) return { scale: 1, x: 0, y: 0 };
  const room = {
    width: Math.max(area.width - 2 * FIT_MARGIN, 1),
    height: Math.max(area.height - 2 * FIT_MARGIN, 1),
  };
  const scale = clampZoom(Math.min(room.width / diagram.width, room.height / diagram.height));
  return {
    scale,
    x: (area.width - diagram.width * scale) / 2,
    y: (area.height - diagram.height * scale) / 2,
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

/** EDT-FR-MZIF: the zoom factor of one wheel event. Up zooms in. */
export function wheelFactor(deltaY: number, deltaMode: number): number {
  // A line is about 16 pixels, a page about 800.
  const pixels = deltaMode === 1 ? deltaY * 16 : deltaMode === 2 ? deltaY * 800 : deltaY;
  // A wheel notch of 100 pixels zooms by about 20%.
  return Math.exp(-pixels * 0.002);
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
