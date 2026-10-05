import { describe, expect, it } from "vitest";

import {
  ACTIVITY_BAR_PX,
  clampPanelPx,
  DEFAULT_VPANEL_FRACTION,
  fractionToPx,
  pxToFraction,
  shellInnerWidth,
  VPANEL_MAX_FRACTION,
  VPANEL_MIN_PX,
  BPANEL_MAX_FRACTION,
  BPANEL_MIN_PX,
  DEFAULT_BPANEL_PX,
  bottomPanelPx,
  clampBottomPanelPx,
} from "./panelLayout";

// SNV-shell-navigation.md SNV-FR-34 / SNV-FR-35 (SNV-FR-34, SNV-FR-35).

describe("shellInnerWidth", () => {
  it("excludes the activity bar from the shell's own width", () => {
    expect(shellInnerWidth(1280)).toBe(1280 - ACTIVITY_BAR_PX);
  });

  it("never returns a negative width for a shell narrower than the activity bar", () => {
    expect(shellInnerWidth(20)).toBe(0);
    expect(shellInnerWidth(0)).toBe(0);
  });

  it("treats a non-finite width as zero rather than propagating NaN", () => {
    expect(shellInnerWidth(Number.NaN)).toBe(0);
    expect(shellInnerWidth(Number.POSITIVE_INFINITY)).toBe(0);
  });
});

describe("clampPanelPx — SNV-FR-34", () => {
  it("leaves a width inside both bounds untouched", () => {
    // 1000px inner: floor 180, ceiling 500.
    expect(clampPanelPx(300, 1000)).toBe(300);
  });

  it("stops at the 180px floor when dragged below it", () => {
    expect(clampPanelPx(40, 1000)).toBe(VPANEL_MIN_PX);
    expect(clampPanelPx(0, 1000)).toBe(VPANEL_MIN_PX);
    expect(clampPanelPx(-500, 1000)).toBe(VPANEL_MIN_PX);
  });

  it("stops at 50% of the shell's inner width when dragged past it", () => {
    expect(clampPanelPx(900, 1000)).toBe(500);
    expect(clampPanelPx(Number.MAX_SAFE_INTEGER, 1000)).toBe(500);
  });

  it("lets the floor win when the shell is too narrow for both bounds", () => {
    // Inner 300 → ceiling 150, which is below the 180 floor. SNV-FR-34 says
    // the floor wins: a panel under 180px cannot render its tree legibly,
    // whereas one taking >50% of a very narrow shell is merely cramped.
    expect(VPANEL_MIN_PX).toBeGreaterThan(300 * VPANEL_MAX_FRACTION);
    expect(clampPanelPx(10, 300)).toBe(VPANEL_MIN_PX);
    expect(clampPanelPx(280, 300)).toBe(VPANEL_MIN_PX);
  });

  it("is exactly at the boundary where the two bounds cross", () => {
    // The crossover is inner = 360 (ceiling 180 == floor 180).
    expect(clampPanelPx(999, 360)).toBe(VPANEL_MIN_PX);
    // A hair wider and the ceiling is genuinely above the floor again.
    expect(clampPanelPx(999, 400)).toBe(200);
  });

  it("falls back to the floor for a zero, negative, or non-finite shell", () => {
    expect(clampPanelPx(300, 0)).toBe(VPANEL_MIN_PX);
    expect(clampPanelPx(300, -100)).toBe(VPANEL_MIN_PX);
    expect(clampPanelPx(300, Number.NaN)).toBe(VPANEL_MIN_PX);
  });

  it("falls back to the floor for a non-finite width", () => {
    expect(clampPanelPx(Number.NaN, 1000)).toBe(VPANEL_MIN_PX);
  });
});

describe("fractionToPx — SNV-FR-35", () => {
  it("resolves a fraction against the shell's inner width", () => {
    expect(fractionToPx(0.25, 1200)).toBe(300);
  });

  it("keeps the same proportion as the shell grows", () => {
    // SNV-FR-35: widening the window leaves the panel at the same share.
    const narrow = fractionToPx(0.25, 1200);
    const wide = fractionToPx(0.25, 2000);
    expect(narrow / 1200).toBeCloseTo(0.25, 10);
    expect(wide / 2000).toBeCloseTo(0.25, 10);
    expect(wide).toBeGreaterThan(narrow);
  });

  it("re-applies the clamps at the new size", () => {
    // 0.9 is above the ceiling at every size — it resolves to exactly half.
    expect(fractionToPx(0.9, 1200)).toBe(600);
    expect(fractionToPx(0.9, 2000)).toBe(1000);
    // 0.05 is below the floor on a small shell.
    expect(fractionToPx(0.05, 1200)).toBe(VPANEL_MIN_PX);
  });

  it("substitutes the default for a never-persisted (0) fraction", () => {
    // The backend returns 0.0 for a slot that was never written; the panel must
    // not collapse to the floor on a first launch.
    expect(fractionToPx(0, 1200)).toBe(
      clampPanelPx(DEFAULT_VPANEL_FRACTION * 1200, 1200),
    );
    expect(fractionToPx(0, 1200)).toBeGreaterThan(VPANEL_MIN_PX);
  });

  it("substitutes the default for a negative or non-finite fraction", () => {
    const expected = clampPanelPx(DEFAULT_VPANEL_FRACTION * 1200, 1200);
    expect(fractionToPx(-1, 1200)).toBe(expected);
    expect(fractionToPx(Number.NaN, 1200)).toBe(expected);
  });

  it("reproduces roughly the shell's historical 280px panel at 1280px", () => {
    // Regression guard on DEFAULT_VPANEL_FRACTION: a first launch after this
    // landed should look like every launch before it.
    const px = fractionToPx(DEFAULT_VPANEL_FRACTION, shellInnerWidth(1280));
    expect(px).toBeGreaterThan(250);
    expect(px).toBeLessThan(300);
  });
});

describe("pxToFraction — SNV-FR-36", () => {
  it("converts a dragged width back to a fraction", () => {
    expect(pxToFraction(300, 1200)).toBeCloseTo(0.25, 10);
  });

  it("round-trips through fractionToPx for an in-bounds width", () => {
    const inner = 1400;
    const px = 420;
    expect(fractionToPx(pxToFraction(px, inner), inner)).toBeCloseTo(px, 10);
  });

  it("clamps before converting, so an out-of-bounds drag is never persisted", () => {
    // What is stored is exactly what was rendered — dragging past the ceiling
    // persists the ceiling, not the raw pointer position.
    expect(pxToFraction(5000, 1200)).toBeCloseTo(VPANEL_MAX_FRACTION, 10);
    expect(pxToFraction(-200, 1200)).toBeCloseTo(VPANEL_MIN_PX / 1200, 10);
  });

  it("falls back to the default fraction when the shell has no width", () => {
    expect(pxToFraction(300, 0)).toBe(DEFAULT_VPANEL_FRACTION);
    expect(pxToFraction(300, Number.NaN)).toBe(DEFAULT_VPANEL_FRACTION);
  });
});

// ---------------------------------------------------------------------------
// Bottom panel (SNV-FR-51 / SNV-FR-52)
// ---------------------------------------------------------------------------

describe("clampBottomPanelPx — SNV-FR-51", () => {
  it("leaves a height between the bounds alone", () => {
    expect(clampBottomPanelPx(280, 1000)).toBe(280);
  });

  it("caps the panel at 75% of the shell's height", () => {
    // SNV-FR-51: the ceiling the user asked for — a 1000px shell tops out at
    // 750px of panel, leaving the viewport a quarter of the window.
    expect(clampBottomPanelPx(900, 1000)).toBe(750);
    expect(clampBottomPanelPx(Number.MAX_SAFE_INTEGER, 1000)).toBe(750);
    expect(clampBottomPanelPx(750, 1000)).toBe(750);
    expect(clampBottomPanelPx(751, 1000)).toBe(750);
  });

  it("holds the panel at its 120px floor", () => {
    expect(clampBottomPanelPx(40, 1000)).toBe(BPANEL_MIN_PX);
    expect(clampBottomPanelPx(0, 1000)).toBe(BPANEL_MIN_PX);
    expect(clampBottomPanelPx(-200, 1000)).toBe(BPANEL_MIN_PX);
  });

  it("lets the floor win when the shell is too short for both bounds", () => {
    // Under 160px the 120px floor exceeds the 75% ceiling. An unreadable panel
    // is worse than a cramped viewport on a window nothing is usable at.
    expect(BPANEL_MIN_PX / BPANEL_MAX_FRACTION).toBe(160);
    expect(clampBottomPanelPx(140, 150)).toBe(BPANEL_MIN_PX);
    expect(clampBottomPanelPx(10, 150)).toBe(BPANEL_MIN_PX);
  });

  it("falls back to the floor for a shell height it cannot use", () => {
    for (const bad of [0, -1, Number.NaN, Number.POSITIVE_INFINITY]) {
      expect(clampBottomPanelPx(300, bad)).toBe(BPANEL_MIN_PX);
    }
    expect(clampBottomPanelPx(Number.NaN, 1000)).toBe(BPANEL_MIN_PX);
  });
});

describe("bottomPanelPx — SNV-FR-52", () => {
  it("renders a stored height that fits, unchanged", () => {
    expect(bottomPanelPx(400, 1000)).toBe(400);
  });

  it("re-clamps a stored height against a shorter shell without losing it", () => {
    // SNV-FR-52: 600px stored, shown at 300px in a 400px shell, and back to
    // 600px when the window grows again — the stored value is never rewritten.
    expect(bottomPanelPx(600, 400)).toBe(300);
    expect(bottomPanelPx(600, 1000)).toBe(600);
  });

  it("substitutes the default for a slot that was never written", () => {
    // The backend reports 0.0 for an unwritten slot.
    expect(bottomPanelPx(0, 1000)).toBe(DEFAULT_BPANEL_PX);
    expect(bottomPanelPx(Number.NaN, 1000)).toBe(DEFAULT_BPANEL_PX);
    expect(bottomPanelPx(-50, 1000)).toBe(DEFAULT_BPANEL_PX);
  });

  it("clamps the default itself on a shell too short to hold it", () => {
    expect(bottomPanelPx(0, 300)).toBe(225);
  });
});
