/**
 * The width of the run region's paths column
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-KWRB,
 * GRU-FR-ZPTE).
 *
 * A fraction of the width of the region's two columns rather than a pixel
 * count, so a column sized in a wide window keeps its share of a narrower one.
 * The pass history takes what is left.
 */

/** GRU-FR-KWRB: the bounds that keep both columns usable. */
export const MIN_PATHS_FRACTION = 0.15;
export const MAX_PATHS_FRACTION = 0.7;

/** GRU-FR-KWRB: what a paths column nobody has sized takes. */
export const DEFAULT_PATHS_FRACTION = 0.3;

/**
 * GRU-FR-KWRB: a stored or dragged value, brought inside the bounds. A value
 * that is absent or is not a number reads as the default: a record written
 * before the preference existed holds none.
 */
export function clampPathsFraction(fraction: number | undefined | null): number {
  if (typeof fraction !== "number" || !Number.isFinite(fraction)) {
    return DEFAULT_PATHS_FRACTION;
  }
  return Math.min(MAX_PATHS_FRACTION, Math.max(MIN_PATHS_FRACTION, fraction));
}

/** GRU-FR-ZPTE: the width as the whole percentage the divider announces. */
export function pathsPercent(fraction: number): number {
  return Math.round(clampPathsFraction(fraction) * 100);
}

/** GRU-FR-ZPTE: where a pointer at `x` inside columns of `width` puts it. */
export function pathsFractionForDrag(x: number, width: number): number {
  if (!Number.isFinite(width) || width <= 0) return DEFAULT_PATHS_FRACTION;
  return clampPathsFraction(x / width);
}

/**
 * GRU-FR-ZPTE: what a key press on the divider does to the width. Left and
 * Right move it by one percentage point, Home sets the lower bound and End the
 * upper bound. `null` for a key the divider does not answer to, so the caller
 * leaves that event alone.
 */
export function pathsFractionForKey(
  key: string,
  fraction: number,
): number | null {
  switch (key) {
    case "ArrowLeft":
      return clampPathsFraction((pathsPercent(fraction) - 1) / 100);
    case "ArrowRight":
      return clampPathsFraction((pathsPercent(fraction) + 1) / 100);
    case "Home":
      return MIN_PATHS_FRACTION;
    case "End":
      return MAX_PATHS_FRACTION;
    default:
      return null;
  }
}
