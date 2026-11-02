/** What the voice styles must hold, which jsdom cannot see (`VOM-voice-mode.md`). */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const css = readFileSync(resolve(__dirname, "../styles/kit/voice.css"), "utf8");

function block(selector: string): string {
  const at = css.indexOf(selector);
  expect(at, selector).toBeGreaterThanOrEqual(0);
  return css.slice(at, css.indexOf("}", at));
}

describe("the voice styles", () => {
  it("VOM-FR-IKIK: the selected option shows by a line, so by more than colour", () => {
    expect(block('.voice-toggle__option[aria-pressed="true"]')).toMatch(/box-shadow: inset/);
  });

  it.each(["listening", "hearing", "paused", "transcribing"])(
    "VOM-FR-VOUG: %s has a colour of its own",
    (state) => {
      expect(css).toContain(`.voice-area__record[data-vad="${state}"]`);
    },
  );

  it("DDS-FR-JKPC, VOM-FR-RTSU: no VAD part takes a line of its own", () => {
    expect(block(".voice-area__info")).toMatch(/flex: none/);
    expect(css).not.toContain("flex: 1 0 100%");
  });

  it("VOM-FR-TISN: the toggles keep their width in a narrow column", () => {
    expect(block(".voice-area__toggles")).toMatch(/min-width: min-content/);
  });
});
