// Supports PPK-FR-01, PPK-FR-02, PPK-FR-03, PPK-FR-04, PPK-FR-05: the recent-projects list (PPK-project-picker.md) renders
// each row's "last opened" timestamp via formatRelative. No dedicated test
// scenario covers time formatting on its own; these unit tests pin the helper
// behind that rendering.
import { describe, expect, it } from "vitest";
import { formatRelative } from "./ProjectPicker";

const ANCHOR = new Date("2026-05-17T12:00:00Z");

function ago(seconds: number): string {
  return new Date(ANCHOR.getTime() - seconds * 1000).toISOString();
}

describe("formatRelative", () => {
  it("returns 'just now' for very recent times", () => {
    expect(formatRelative(ago(0), ANCHOR)).toBe("just now");
    expect(formatRelative(ago(30), ANCHOR)).toBe("just now");
    expect(formatRelative(ago(59), ANCHOR)).toBe("just now");
  });

  it("formats minutes", () => {
    expect(formatRelative(ago(60), ANCHOR)).toBe("1m ago");
    expect(formatRelative(ago(59 * 60), ANCHOR)).toBe("59m ago");
  });

  it("formats hours", () => {
    expect(formatRelative(ago(60 * 60), ANCHOR)).toBe("1h ago");
    expect(formatRelative(ago(23 * 3600), ANCHOR)).toBe("23h ago");
  });

  it("formats days", () => {
    expect(formatRelative(ago(24 * 3600), ANCHOR)).toBe("1d ago");
    expect(formatRelative(ago(6 * 86400), ANCHOR)).toBe("6d ago");
  });

  it("formats weeks", () => {
    expect(formatRelative(ago(7 * 86400), ANCHOR)).toBe("1w ago");
    expect(formatRelative(ago(28 * 86400), ANCHOR)).toBe("4w ago");
  });

  it("formats months", () => {
    expect(formatRelative(ago(35 * 86400), ANCHOR)).toBe("1mo ago");
    expect(formatRelative(ago(330 * 86400), ANCHOR)).toBe("11mo ago");
  });

  it("pins the weeks/months crossover at 30–34 days", () => {
    // floor(30/7)=4, w<5 so still weeks
    expect(formatRelative(ago(30 * 86400), ANCHOR)).toBe("4w ago");
    expect(formatRelative(ago(34 * 86400), ANCHOR)).toBe("4w ago");
    // floor(35/7)=5, w<5 is false, falls through to months: floor(35/30)=1
    expect(formatRelative(ago(35 * 86400), ANCHOR)).toBe("1mo ago");
  });

  it("formats years", () => {
    expect(formatRelative(ago(365 * 86400), ANCHOR)).toBe("1y ago");
    expect(formatRelative(ago(2 * 365 * 86400), ANCHOR)).toBe("2y ago");
  });

  it("clamps future timestamps to 'just now'", () => {
    const future = new Date(ANCHOR.getTime() + 60 * 1000).toISOString();
    expect(formatRelative(future, ANCHOR)).toBe("just now");
  });

  it("returns input verbatim on unparsable ISO string", () => {
    expect(formatRelative("not a date", ANCHOR)).toBe("not a date");
  });
});
