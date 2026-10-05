import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { readStylesheet } from "./readStylesheet";

/**
 * Conformance test for the agent title's indent (CTA-FR-KFUF, CMP-FR-30).
 *
 * An agent's name is rendered with a marker in front of it — `✦ ` — inside the
 * same span as the handle. The title line sits below that span and has to align
 * with the **name**, not with the marker, or it reads as a caption on the glyph
 * rather than as the role of the person named.
 *
 * The indent is a hidden copy of the marker in a `::before`, rather than a
 * measured padding, so it is exactly one marker wide in whatever font, size, and
 * weight the author line resolves to. That correctness depends on the two copies
 * of the marker — the one rendered in the component and the one in the
 * stylesheet — being the same string. They live in different languages and
 * different files, so nothing but this test keeps them in step: change the glyph
 * in the TSX alone and the alignment silently goes off by the width of the
 * difference, which is precisely the kind of drift a screenshot review catches
 * once and never again.
 *
 * Layout itself is not asserted here and cannot be — jsdom performs none. What
 * this pins is the invariant the layout rests on.
 */

const root = resolve(__dirname, "..");

function read(rel: string): string {
  return readFileSync(resolve(root, rel), "utf8");
}

/** The one `::before` rule both title lines share, and the marker it reserves. */
function spacerRule(css: string): { body: string; content: string } {
  const rule = css.match(
    /\.comment__title::before,\s*\.comment-row__title::before\s*\{([^}]*)\}/,
  );
  if (!rule) throw new Error("the shared title-indent ::before rule is missing");
  const content = rule[1].match(/content:\s*"((?:[^"\\]|\\.)*)"/);
  if (!content) throw new Error("the title-indent rule reserves no content");
  return { body: rule[1], content: content[1] };
}

describe("the agent title indents to the name rather than to the marker", () => {
  const css = readStylesheet("kit.css");

  it("gives both title lines a hidden spacer carrying the marker", () => {
    // One rule serves the rail and the panel, so those two cannot disagree with
    // each other — only with the components, which the next test covers.
    const { body } = spacerRule(css);
    // Hidden, so it occupies its width without being seen or announced.
    expect(body).toMatch(/visibility:\s*hidden/);
    // Weight-matched: a glyph's advance width depends on it, and the author
    // span it stands in for is semibold.
    expect(body).toMatch(/font-weight:\s*var\(--fw-semi\)/);
  });

  it.each([["components/CommentRail/ThreadCard.tsx"], ["components/Comments.tsx"]])(
    "keeps the marker %s renders identical to the reserved spacer",
    (component) => {
      const rendered = read(component).match(/kind === "agent" && "([^"]*)"/);
      expect(
        rendered,
        `${component} no longer renders a literal agent marker; ` +
          "update this test and the ::before spacer together",
      ).not.toBeNull();

      expect(
        spacerRule(css).content,
        "the marker the component renders and the spacer the stylesheet " +
          "reserves have drifted apart, so the title no longer lines up with " +
          "the name",
      ).toBe(rendered![1]);
    },
  );
});
