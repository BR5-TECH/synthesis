/** The zoom steps of the PDF viewer, in percent (PDV-FR-HQTN). */
export const ZOOM_STEPS = [25, 50, 75, 100, 125, 150, 200, 300, 400] as const;

/** The index of the default zoom, 100 percent. */
export const DEFAULT_ZOOM_INDEX = ZOOM_STEPS.indexOf(100);

/** PDF points are 1/72 inch. A CSS pixel is 1/96 inch. */
export const CSS_UNITS = 96 / 72;

/** Use a stored zoom index only when it names a step. */
export function validZoomIndex(value: unknown): number {
  return typeof value === "number" &&
    Number.isInteger(value) &&
    value >= 0 &&
    value < ZOOM_STEPS.length
    ? value
    : DEFAULT_ZOOM_INDEX;
}

/** The scale that PDF.js gets for a zoom step. 100 percent is actual size. */
export function scaleOf(zoomIndex: number): number {
  return (ZOOM_STEPS[zoomIndex] / 100) * CSS_UNITS;
}

/** Keep a page number inside 1..count. */
export function clampPage(page: number, count: number): number {
  if (!Number.isFinite(page)) return 1;
  return Math.min(Math.max(Math.trunc(page), 1), Math.max(count, 1));
}
