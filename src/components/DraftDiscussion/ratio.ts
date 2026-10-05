/**
 * How the New Artifact tab is split between its two columns
 * (`DDS-draft-discussion.md` DDS-FR-PNXR).
 *
 * Three presets and a free drag. A preset is stored by name rather than by the
 * number it currently means, so a preset keeps its meaning if the numbers ever
 * change; a drag is stored as the document column's fraction of the pair.
 */

/** DDS-FR-PNXR: the three presets, in the order the accelerator cycles them. */
export const RATIO_PRESETS = ["doc70", "even", "chat70"] as const;

export type RatioPreset = (typeof RATIO_PRESETS)[number];

/** A preset by name, or the document column's own fraction of the two. */
export type SplitRatio = RatioPreset | number;

/** What the tab opens on before the author has chosen anything. */
export const DEFAULT_RATIO: SplitRatio = "even";

/**
 * The narrowest either column is dragged to, as a fraction of the pair.
 *
 * A column dragged past this stops being a column and becomes a rail, which is
 * the shape this surface exists to replace — so the drag stops rather than
 * letting the discussion reach a measure it cannot be read at.
 */
export const MIN_FRACTION = 0.25;
export const MAX_FRACTION = 1 - MIN_FRACTION;

const PRESET_FRACTIONS: Record<RatioPreset, number> = {
  doc70: 0.7,
  even: 0.5,
  chat70: 0.3,
};

/** How much of the pair the **document** column takes. */
export function documentFraction(ratio: SplitRatio): number {
  if (typeof ratio === "number") return clampFraction(ratio);
  return PRESET_FRACTIONS[ratio];
}

export function clampFraction(fraction: number): number {
  if (!Number.isFinite(fraction)) return PRESET_FRACTIONS.even;
  return Math.min(MAX_FRACTION, Math.max(MIN_FRACTION, fraction));
}

/** DDS-FR-PNXR: the next preset in the cycle. */
export function cycleRatio(ratio: SplitRatio): RatioPreset {
  // A dragged split has no place in the cycle, so the accelerator enters it at
  // the head rather than picking whichever preset the drag happens to be near.
  // Snapping to the nearest would make one keypress mean two different things
  // depending on a pixel the author cannot see.
  if (typeof ratio === "number") return RATIO_PRESETS[0];
  const at = RATIO_PRESETS.indexOf(ratio);
  return RATIO_PRESETS[(at + 1) % RATIO_PRESETS.length];
}

/** What the segmented control calls each preset. */
export const PRESET_LABELS: Record<RatioPreset, string> = {
  doc70: "Document",
  even: "Even",
  chat70: "Discussion",
};

export const PRESET_TITLES: Record<RatioPreset, string> = {
  doc70: "Give the document most of the width",
  even: "Split the width evenly",
  chat70: "Give the discussion most of the width",
};

/**
 * The grid the two columns and their splitter are laid out on.
 *
 * Fractions rather than pixels, so the split keeps its proportion when the
 * window changes size.
 */
export function gridTemplate(ratio: SplitRatio): string {
  const doc = documentFraction(ratio);
  return `${doc}fr var(--dds-splitter-w) ${1 - doc}fr`;
}

/** Read a persisted value back, ignoring anything this build does not know. */
export function parseRatio(stored: string | undefined): SplitRatio | null {
  if (stored === undefined || stored.trim() === "") return null;
  if ((RATIO_PRESETS as readonly string[]).includes(stored)) {
    return stored as RatioPreset;
  }
  // `Number("")` is 0, which would clamp to the narrowest split the drag
  // allows — so an empty stored value would silently give the author a
  // quarter-width document column they never chose.
  const asNumber = Number(stored);
  if (!Number.isFinite(asNumber)) return null;
  return clampFraction(asNumber);
}

/** How a ratio is written to the layout record. */
export function serialiseRatio(ratio: SplitRatio): string {
  return typeof ratio === "number" ? String(ratio) : ratio;
}
