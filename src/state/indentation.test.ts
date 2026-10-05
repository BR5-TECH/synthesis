import { describe, expect, it } from "vitest";
import {
  DEFAULT_INDENTATION,
  detectIndentation,
  indentationLabel,
  indentUnit,
  sameIndentation,
} from "./indentation";

// EDT-FR-37 / EDT-FR-38 — client-side indentation detection and the unit a Tab
// keypress inserts. No backend operation is involved (EDT contract boundary).

describe("detectIndentation (EDT-FR-37)", () => {
  it("reports tabs for a body indented with tab characters", () => {
    const body = "# Title\n\n- one\n\t- nested\n\t- also nested\n";
    expect(detectIndentation(body)).toEqual({ kind: "tabs" });
  });

  it("reports spaces with a width of 2 for a body with no indented line", () => {
    // EDT-FR-37, second half: the explicit default, not an accident of the
    // detector returning something arbitrary.
    const body = "# Title\n\nA paragraph.\n\nAnother one.\n";
    expect(detectIndentation(body)).toEqual({ kind: "spaces", width: 2 });
    expect(detectIndentation(body)).toEqual(DEFAULT_INDENTATION);
    expect(detectIndentation("")).toEqual(DEFAULT_INDENTATION);
  });

  it("takes the smallest positive indent as the unit, not the deepest", () => {
    // A nested Markdown list indented 2/4/6 has a unit of 2. A detector that
    // averaged, or that took the first indent it saw, would answer 4 here.
    const body = "- a\n  - b\n    - c\n      - d\n";
    expect(detectIndentation(body)).toEqual({ kind: "spaces", width: 2 });

    const four = "- a\n    - b\n        - c\n";
    expect(detectIndentation(four)).toEqual({ kind: "spaces", width: 4 });
  });

  it("calls a mostly-tab file tabs even with space-aligned continuation lines", () => {
    // Inserting spaces into a tab-indented file is the wrong answer, so tabs
    // win on a tie as well as on a majority.
    const body = "\ta\n\tb\n  aligned continuation\n";
    expect(detectIndentation(body)).toEqual({ kind: "tabs" });
  });

  it("ignores whitespace-only lines, which carry no indentation information", () => {
    // Trailing padding on a blank line must not be read as a 4-space unit.
    const body = "- a\n    \n        - b\n";
    expect(detectIndentation(body)).toEqual({ kind: "spaces", width: 8 });
  });

  it("reports a width outside the offered choices when that is the real unit", () => {
    // The status bar offers 2/4/8, but a CommonMark ordered list whose
    // continuation lines align under "1. " is indented three spaces. Detection
    // reports the artifact's real unit (EDT-FR-37) rather than rounding to a
    // choice; the control widens to accommodate it.
    const body = "1. one\n   continuation\n2. two\n";
    expect(detectIndentation(body)).toEqual({ kind: "spaces", width: 3 });
    expect(indentUnit(detectIndentation(body))).toBe("   ");
  });

  it("clamps a pathological indent so Tab never inserts an absurd run", () => {
    const body = "x\n" + " ".repeat(40) + "deeply indented\n";
    const detected = detectIndentation(body);
    expect(detected).toEqual({ kind: "spaces", width: 8 });
    expect(indentUnit(detected).length).toBeLessThanOrEqual(8);
  });
});

describe("indentUnit (EDT-FR-38)", () => {
  it("is a single tab character for tabs and n spaces for spaces", () => {
    expect(indentUnit({ kind: "tabs" })).toBe("\t");
    expect(indentUnit({ kind: "spaces", width: 2 })).toBe("  ");
    expect(indentUnit({ kind: "spaces", width: 4 })).toBe("    ");
  });
});

describe("indentationLabel (STB-FR-21)", () => {
  it("names the convention the way the status bar renders it", () => {
    expect(indentationLabel({ kind: "tabs" })).toBe("Tabs");
    expect(indentationLabel({ kind: "spaces", width: 2 })).toBe("Spaces: 2");
  });
});

describe("sameIndentation", () => {
  it("compares kind and width", () => {
    expect(sameIndentation({ kind: "tabs" }, { kind: "tabs" })).toBe(true);
    expect(
      sameIndentation({ kind: "spaces", width: 2 }, { kind: "spaces", width: 2 }),
    ).toBe(true);
    expect(
      sameIndentation({ kind: "spaces", width: 2 }, { kind: "spaces", width: 4 }),
    ).toBe(false);
    expect(sameIndentation({ kind: "tabs" }, { kind: "spaces", width: 2 })).toBe(
      false,
    );
  });
});
