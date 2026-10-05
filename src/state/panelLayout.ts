/**
 * Vertical-panel sizing geometry (SNV-shell-navigation.md SNV-FR-34 / SNV-FR-35).
 *
 * The panel's width is held as a **fraction of the shell's inner width** — the
 * window's content width less the activity bar — rather than as a pixel count.
 * That is what makes the width invariant under a change of window size: resize,
 * maximize, enter/leave full-screen, or restore on a different display, and the
 * panel keeps the same proportion of the shell while the viewport absorbs the
 * difference (SNV-FR-35).
 *
 * Pure by design: no React, no DOM, no `invoke`. The clamping rule is the part
 * most likely to regress silently (an off-by-one on the floor is invisible until
 * a laptop-sized window makes the panel unusable), so it lives here where it is
 * unit-testable against exact numbers.
 */

/**
 * Width of the activity bar, in CSS pixels. Mirrors the first track of
 * `.shell { grid-template-columns: 44px ... }` in `styles/kit.css` — the shell's
 * *inner* width excludes it, because the activity bar is chrome the panel never
 * shares with the viewport.
 */
export const ACTIVITY_BAR_PX = 44;

/** SNV-FR-34: the panel is never rendered narrower than this. */
export const VPANEL_MIN_PX = 180;

/** SNV-FR-34: the panel never takes more than this share of the shell's inner width. */
export const VPANEL_MAX_FRACTION = 0.5;

/**
 * The fraction used when nothing has been persisted. Chosen to reproduce the
 * shell's long-standing 280px panel at a typical 1280px-wide window, so a first
 * launch after this landed looks like every launch before it.
 */
export const DEFAULT_VPANEL_FRACTION = 0.22;

/**
 * The width available to the vertical panel and the main viewport together:
 * the shell's own width less the activity bar. Never negative.
 */
export function shellInnerWidth(shellWidth: number): number {
  if (!Number.isFinite(shellWidth)) return 0;
  return Math.max(0, shellWidth - ACTIVITY_BAR_PX);
}

/**
 * SNV-FR-34: clamp a panel width in pixels to the floor and ceiling.
 *
 * The two bounds can conflict — on a shell narrower than 360px the 180px floor
 * exceeds the 50% ceiling. The floor wins there, deliberately: a panel below
 * `VPANEL_MIN_PX` cannot render its tree legibly, whereas one that takes more
 * than half a very narrow shell is merely cramped. Clamping the other way round
 * would let the panel collapse to a sliver on the smallest supported window.
 */
export function clampPanelPx(px: number, innerWidth: number): number {
  if (!Number.isFinite(innerWidth) || innerWidth <= 0) return VPANEL_MIN_PX;
  const maxPx = innerWidth * VPANEL_MAX_FRACTION;
  if (VPANEL_MIN_PX >= maxPx) return VPANEL_MIN_PX;
  if (!Number.isFinite(px)) return VPANEL_MIN_PX;
  return Math.min(Math.max(px, VPANEL_MIN_PX), maxPx);
}

/**
 * Resolve a stored fraction to the pixel width to render at, clamped for the
 * current shell (SNV-FR-34 / SNV-FR-35).
 *
 * A non-finite or non-positive fraction — including the `0.0` the backend
 * returns for a slot that was never written — falls back to
 * `DEFAULT_VPANEL_FRACTION` rather than collapsing the panel.
 */
export function fractionToPx(fraction: number, innerWidth: number): number {
  const f =
    Number.isFinite(fraction) && fraction > 0
      ? fraction
      : DEFAULT_VPANEL_FRACTION;
  return clampPanelPx(f * innerWidth, innerWidth);
}

/**
 * Convert a dragged pixel width back to the fraction to persist. The pixel
 * value is clamped first, so a fraction outside the bounds is never stored —
 * what is read back on the next launch is exactly what was rendered.
 */
export function pxToFraction(px: number, innerWidth: number): number {
  if (!Number.isFinite(innerWidth) || innerWidth <= 0) {
    return DEFAULT_VPANEL_FRACTION;
  }
  return clampPanelPx(px, innerWidth) / innerWidth;
}

// ---------------------------------------------------------------------------
// Bottom panel (SNV-FR-50 – SNV-FR-53)
// ---------------------------------------------------------------------------

/**
 * SNV-FR-51: the bottom panel is never rendered shorter than this. Below it the
 * panel cannot show its header and a line of output at the same time, which is
 * the least that makes it worth having open.
 */
export const BPANEL_MIN_PX = 120;

/** SNV-FR-51: the panel never takes more than this share of the shell's height. */
export const BPANEL_MAX_FRACTION = 0.75;

/** The height used when nothing has been persisted. */
export const DEFAULT_BPANEL_PX = 280;

/**
 * SNV-FR-51: clamp a bottom-panel height in pixels to the floor and ceiling.
 *
 * The height is kept in pixels rather than as a fraction — unlike the vertical
 * panel (SNV-FR-34) — because a bottom panel is read as "about this many lines
 * of output", a quantity that does not want to grow with the window. The 75%
 * ceiling is what keeps it from crowding the viewport out; re-applying it on
 * every resize (SNV-FR-52) is what makes a pixel height safe on a short window.
 *
 * The two bounds conflict on a shell under 160px tall, and the floor wins there
 * for the same reason it does for the vertical panel: an unreadably short panel
 * is worse than a cramped viewport on a window nothing is usable at anyway.
 */
export function clampBottomPanelPx(px: number, shellHeight: number): number {
  if (!Number.isFinite(shellHeight) || shellHeight <= 0) return BPANEL_MIN_PX;
  const maxPx = shellHeight * BPANEL_MAX_FRACTION;
  if (BPANEL_MIN_PX >= maxPx) return BPANEL_MIN_PX;
  if (!Number.isFinite(px)) return BPANEL_MIN_PX;
  return Math.min(Math.max(px, BPANEL_MIN_PX), maxPx);
}

/**
 * Resolve a stored height to the pixels to render at, clamped for the current
 * shell. A non-finite or non-positive value — including the `0.0` the backend
 * returns for a slot that was never written — falls back to the default rather
 * than collapsing the panel.
 */
export function bottomPanelPx(stored: number, shellHeight: number): number {
  const h = Number.isFinite(stored) && stored > 0 ? stored : DEFAULT_BPANEL_PX;
  return clampBottomPanelPx(h, shellHeight);
}
