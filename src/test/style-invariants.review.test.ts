import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { blocksFor, decl, layersOf, sheet } from "./cssRules";

/**
 * Stylesheet invariants no rendering test can see — the kit's optional field, the stage row and the graduation run region.
 *
 * One part of the group `./cssRules.ts` heads, which carries the whole rule
 * these are written under.
 */

describe("the kit's optional field is one quiet rule", () => {
  // The field the graduation escalation and the discussion questions write
  // their optional text in (`.draft-review__feedback`). jsdom loads no
  // stylesheet, so only these rules can see it go back to a box.

  it("writes the feedback field on one rule, not in a box", () => {
    const css = sheet("kit.css");
    const blocks = blocksFor(css, ".draft-review__feedback");
    expect(blocks, "no rule for .draft-review__feedback").toHaveLength(1);
    const field = blocks[0];
    // Order matters in the declaration itself — the shorthand reset has to come
    // first — but what this guards is the outcome: three edges gone, one left.
    expect(decl(field, "border")).toBe("0");
    expect(decl(field, "border-bottom")).toMatch(/^1px solid /);
    // Re-adding an edge one longhand at a time clears the reset above and still
    // draws part of a box, so the sides are asserted absent rather than merely
    // un-asserted. Logical properties too — `border-block-start` is the same
    // edge under another name.
    expect(field).not.toMatch(/border-(top|left|right|inline|block)[\w-]*\s*:/);
    // A radius on a single rule rounds nothing and, if the box ever comes back,
    // is the thing that makes it read as a box again.
    expect(decl(field, "border-radius")).toBe("0");

    // The kit's focus ring is a rounded rect: on a field with one edge it would
    // draw the three that were just removed. The indicator has to stay on that
    // one edge — an inset shadow doubling the rule. Asserted as a shape, not as
    // the word `inset`: `inset 0 0 0 1px` carries that word and draws all four
    // sides. Zero x-offset, a vertical offset, zero blur, no spread.
    const focused = blocksFor(css, ".draft-review__feedback:focus");
    expect(focused, "the field never asserts itself on focus").toHaveLength(1);
    const ring = decl(focused[0], "box-shadow");
    expect(ring, "no focus indicator at all").not.toBeNull();
    expect(ring).toMatch(/^inset\s+0\s+-?\d+px\s+0\s+var\(--[\w-]+\)$/);
    // Nor an outline, which would box it just the same. Nothing else sets one:
    // the element carries no `.input`, and no sheet has a base `textarea` rule,
    // so this is the only thing between the field and the UA's own ring.
    expect(decl(focused[0], "outline")).toBe("none");

    // Closed world: the five rules above are all of them. A later
    // `[data-theme="dark"] .draft-review__feedback` or a `:focus-visible`
    // re-adding the ring matches none of the selectors asserted here and would
    // otherwise pass every one of them.
    const owned = [...css.matchAll(/([^{}]+)\{[^{}]*\}/g)]
      .flatMap((m) => m[1].split(","))
      .map((s) => s.trim().replace(/\s+/g, " "))
      .filter((s) => s.includes("draft-review__feedback"));
    expect(owned.sort()).toEqual([
      ".draft-review__feedback",
      ".draft-review__feedback::placeholder",
      ".draft-review__feedback:disabled",
      ".draft-review__feedback:focus",
      ".draft-review__feedback:hover",
    ]);
    // The hover state is the field's only other affordance, and nothing else
    // covers it.
    const hovered = blocksFor(css, ".draft-review__feedback:hover")[0];
    expect(decl(hovered, "border-bottom-color")).toBe("var(--border-3)");
  });

  it("counts the field's own padding and rule into its cap", () => {
    // The cap is `4em` of line-height plus a pixel term, and under border-box
    // that term IS the padding and the rule — it is not a fudge factor. Changing
    // the padding without it leaves the field capped at four lines plus a sliver
    // of a fifth, which is the cap failing in the one form nobody eyeballs.
    const field = blocksFor(sheet("kit.css"), ".draft-review__feedback")[0];
    const sum = (value: string | null) =>
      (value?.match(/-?[\d.]+px/g) ?? []).reduce(
        (total, term) => total + Number.parseFloat(term),
        0,
      );
    const padding = decl(field, "padding");
    expect(padding, "the field sets no padding").not.toBeNull();
    const vertical = Number.parseFloat(padding!.split(/\s+/)[0]);
    const rule = sum(decl(field, "border-bottom"));
    expect(sum(decl(field, "max-height"))).toBe(2 * vertical + rule);
  });

  it("caps the growing field at four lines and scrolls past them", () => {
    // The component writes the height from the content on every
    // keystroke, so the bound has to live here — without it a pasted essay
    // grows the foot until it has eaten the comparison. `resize` is off for the
    // same reason: a dragged height is overwritten on the next keystroke.
    const field = blocksFor(sheet("kit.css"), ".draft-review__feedback")[0];
    const cap = decl(field, "max-height");
    expect(cap, "the field grows without bound").not.toBeNull();
    expect(cap).toMatch(/\b4em\b/);
    expect(decl(field, "line-height")).toBe("var(--lh-snug)");
    expect(decl(field, "overflow-y")).toBe("auto");
    expect(decl(field, "resize")).toBe("none");
    // The cap counts the padding and the hairline only under border-box.
    expect(decl(field, "box-sizing")).toBe("border-box");
  });
});

describe("the stage row is one continuous track (RPV-FR-19, RPV-FR-20)", () => {
  // The whole of the requirement lives in CSS. With `css: false` the component
  // tests cannot tell a joined track from a scatter of dots with gaps between
  // them — which is exactly what shipped: the marks were spaced by a flex gap
  // and the connector spanned only that gap, leaving the width of each label
  // blank between one mark and the next.

  /**
   * Every block whose selector list holds `selector`, in source order, joined.
   *
   * A declaration written in a grouped rule and refined in a rule of its own is
   * one property with two writes at equal specificity, so the cascade takes the
   * later. `decl` already reads the last occurrence, and joining in source order
   * is what makes it read across the pair rather than only within one of them.
   */
  const track = (selector: string) =>
    blocksFor(sheet("components.css"), selector).join(";");

  it("RPV-FR-19: the marks are evenly spaced across the width", () => {
    const css = sheet("components.css");
    const [stage] = blocksFor(css, ".run-progress__stage");
    // Even spacing is the whole of "the marks read as points on one line": each
    // cell owns the track on either side of its own mark, so equal cells are
    // what make equal segments. Sized from the content instead, the cells take
    // their labels' widths and the track lands somewhere else on every joint.
    expect(decl(stage, "flex")).toBe("1 1 0");
    // The mark sits at the cell's centre rather than at its leading edge. Left
    // aligned, the last mark landed a label's width short of the trailing edge
    // and the track read as cut off before it got there.
    expect(decl(stage, "align-items")).toBe("center");
    // No padding on the cell: what separates one stage's words from the next is
    // taken inside the text, so the cell's centre stays exactly on its mark.
    expect(decl(stage, "padding-right")).toBeNull();
    expect(decl(stage, "padding")).toBeNull();
    expect(blocksFor(css, ".run-progress__stage:last-child")).toEqual([]);
  });

  it("RPV-FR-19: the row's two ends are the same distance inside its edges", () => {
    // With the marks centred, the first cell's leading half and the last cell's
    // trailing half are the two margins — equal, because the cells are equal —
    // and neither is drawn. Drawing either would run the track off an end that
    // has no mark on it.
    expect(decl(track(".run-progress__stage:first-child::before"), "content")).toBe(
      "none",
    );
    expect(decl(track(".run-progress__stage:last-child::after"), "content")).toBe(
      "none",
    );
  });

  it("RPV-FR-19: the stages abut, so nothing shows between a mark and the track", () => {
    const css = sheet("components.css");
    const [stages] = blocksFor(css, ".run-progress__stages");
    expect(stages).toBeDefined();
    // A gap here is the width of the break that made the row read as separate
    // dots. The space between labels is padding inside the text instead, which
    // the track spans rather than stops at.
    expect(decl(stages, "gap")).toBe("0");

    const [stage] = blocksFor(css, ".run-progress__stage");
    // Percentages on the track halves resolve against the padding box, so the
    // cell has to be sized border-box for `width: 50%` to land on its mark.
    expect(decl(stage, "box-sizing")).toBe("border-box");
  });

  it("RPV-FR-19: the track is two halves per stage, meeting under the mark", () => {
    const lead = track(".run-progress__stage::before");
    const trail = track(".run-progress__stage::after");
    for (const half of [lead, trail]) {
      expect(decl(half, "content")).toBe('""');
      expect(decl(half, "position")).toBe("absolute");
      expect(decl(half, "background")).toBe("var(--border-2)");
    }
    // The leading half runs from the cell's edge to its centre and the trailing
    // half from the centre to its far edge, so consecutive halves meet exactly
    // under a mark however wide the cells are. One span per cell instead puts
    // the joint somewhere other than on a mark the moment two cells differ —
    // which is what the narrow-width rule below produces.
    expect(decl(lead, "left")).toBe("0");
    expect(decl(lead, "width")).toBe("50%");
    expect(decl(trail, "left")).toBe("50%");
    expect(decl(trail, "width")).toBe("50%");
  });

  it("RPV-FR-20: the track carries completion along its own length", () => {
    // The host's own accent, up to the current mark and no further: both halves
    // of a complete stage and the half leading into the current one are behind
    // the run.
    for (const selector of [
      '.run-progress__stage[data-status="complete"]::before',
      '.run-progress__stage[data-status="complete"]::after',
      '.run-progress__stage[data-status="current"]::before',
    ]) {
      expect(decl(track(selector), "background"), selector).toBe("var(--accent)");
    }
    // And nothing paints the half **after** the current mark, which would run
    // the accent one segment further than the work has actually got.
    expect(
      blocksFor(
        sheet("components.css"),
        '.run-progress__stage[data-status="current"]::after',
      ),
    ).toEqual([]);
  });

  it("RPV-FR-21: the cells stay equal at the narrowest width", () => {
    const css = sheet("components.css");
    // RPV-FR-21 requires the track to stay evenly spaced at the smallest width
    // the host offers, and the cells are what make the spacing. Widening the
    // current stage's cell bunched the last two marks against the trailing
    // edge — measured gaps of 130, 130, 52 at a 387px region. "The others
    // reduce to their marks" is satisfied by their words going, not by their
    // share of the row going.
    const narrow = css.slice(css.indexOf("@container (max-width: 56px)"));
    const rule = narrow.slice(0, narrow.indexOf("@keyframes"));
    expect(rule).not.toContain("flex:");
  });

  it("RPV-FR-16: a label that does not fit wraps rather than being cut", () => {
    // GRU-FR-IZKI configures **eight** stages, so a cell holds half the width it
    // held with four — and at the shell's minimum 1280 that is 110px against
    // the 152px "Implementation acceptance" needs. Ellipsised, five of the
    // eight read as "Specification aut…", the current stage among them, which
    // is the one thing RPV-FR-16 forbids. Wrapped, every label stays whole and
    // the row still never scrolls sideways.
    const css = sheet("components.css");
    for (const selector of [".run-progress__label", ".run-progress__condition"]) {
      const [block] = blocksFor(css, selector);
      expect(decl(block, "white-space"), selector).toBe("normal");
      // `normal` offers no break inside a one-word stage name, so without this
      // a name wider than its cell spills over its neighbours.
      expect(decl(block, "overflow-wrap"), selector).toBe("anywhere");
      expect(decl(block, "text-overflow"), selector).toBeNull();
    }
    // RPV-FR-16: and whether a label fits is asked of the **stage's own** cell,
    // so the threshold does not have to be retuned for every stage count a host
    // may configure.
    const [stage] = blocksFor(css, ".run-progress__stage");
    expect(decl(stage, "container-type")).toBe("inline-size");
  });

  it("RPV-FR-19: the mark sits on the track rather than being crossed by it", () => {
    const css = sheet("components.css");
    const [mark] = blocksFor(css, ".run-progress__mark");
    expect(decl(mark, "position")).toBe("relative");
    expect(decl(mark, "z-index")).toBe("1");
    // Its own fill is what hides the line behind a hollow mark, and which
    // surface that fill has to match is the host's to name (RPV-FR-17).
    expect(decl(mark, "background")).toBe(
      "var(--progress-ground, var(--bg-canvas))",
    );
  });

  it("RPV-FR-21: the track brings no colour of its own", () => {
    for (const selector of [
      ".run-progress__stage::before",
      ".run-progress__stage::after",
      '.run-progress__stage[data-status="complete"]::before',
      '.run-progress__stage[data-status="current"]::before',
    ]) {
      const background = decl(track(selector), "background")!;
      expect(background.startsWith("var(--"), selector).toBe(true);
    }
  });
});

describe("the pass timeline is one line (GRU-FR-YYXN)", () => {
  /** Every block holding `selector`, in source order, joined — see `track`. */
  const rule = (selector: string) =>
    blocksFor(sheet("components.css"), selector).join(";");

  // With `css: false` the component tests cannot see that the rule joining the
  // passes runs anywhere near their marks. The first draft put it at 13px while
  // the marks sat at 26px, which drew a line straight through the carets — a
  // defect no assertion about the DOM could have caught.

  it("GRU-FR-YYXN: the rule, the marks, and the account sit on one x", () => {
    const css = sheet("components.css");
    const lead = rule(".graduation__pass-row::before");
    const [account] = blocksFor(css, ".graduation__account");
    // The mark's centre is the row's leading padding, the caret, the gap
    // between them, and the mark's own radius. Both the rule and the open
    // account's edge are placed from that one expression, so neither can drift
    // off the marks while the other stays on them.
    const x = "calc(var(--sp-1) + 10px + var(--sp-2) + 3px)";
    expect(decl(lead, "left")).toBe(x);
    expect(decl(rule(".graduation__pass-row::after"), "left")).toBe(x);
    expect(decl(account, "margin")).toContain(x);
    expect(decl(lead, "width")).toBe("2px");
    expect(decl(account, "border-left")).toBe("2px solid var(--border-2)");
    expect(decl(lead, "background")).toBe("var(--border-2)");
  });

  it("GRU-FR-YYXN: the sequence begins and ends on a mark", () => {
    const css = sheet("components.css");
    // `50%` of the row rather than a measured offset: the mark is centred in
    // the row, so 50% is where it sits however the row's text is set. A literal
    // would be right at one font size and wrong at the next.
    const [first] = blocksFor(
      css,
      ".graduation__pass:first-child .graduation__pass-row::before",
    );
    expect(decl(first, "content")).toBe("none");
    const [last] = blocksFor(
      css,
      '.graduation__pass:last-child:not([data-open="true"]) .graduation__pass-row::after',
    );
    expect(decl(last, "content")).toBe("none");
  });

  it("GRU-FR-YYXN: the rule runs unbroken past an open account", () => {
    const css = sheet("components.css");
    const [account] = blocksFor(css, ".graduation__account");
    // The room above and below an account's text is **padding** rather than
    // margin, so its rule runs the whole height of the box and meets the row's
    // own at each end. As margin it left a gap of that much at both ends, and
    // the timeline read as coming apart at whichever pass the author had
    // opened — a break invisible while every row was closed, which is why no
    // assertion about the closed list could catch it.
    const margin = decl(account, "margin")!.split(/\s+/);
    expect(margin.slice(0, 3)).toEqual(["0", "0", "0"]);
    expect(decl(account, "margin-top")).toBeNull();
    expect(decl(account, "margin-bottom")).toBeNull();
    const padding = decl(account, "padding")!.split(/\s+/);
    expect(padding[0]).not.toBe("0");
    expect(padding[2]).not.toBe("0");
  });

  it("GRU-FR-YYXN: the account's second heading brings no treatment of its own", () => {
    const css = sheet("components.css");
    const [next] = blocksFor(css, ".graduation__revision-head--next");
    // The modifier gives the second heading the room a paragraph break would
    // give it and nothing else: a tone, an inset, or a rule on it would take
    // the two headings off one left edge, which is the defect it exists after.
    expect(decl(next, "margin-top")).toBeTruthy();
    for (const prop of ["color", "padding-left", "border-left", "margin-left"]) {
      expect(decl(next, prop), prop).toBeNull();
    }
    // And the rules that used to indent, rule, and dim the correction are gone
    // rather than left as dead CSS a later class name could pick up again.
    expect(blocksFor(css, ".graduation__revision-correction")).toHaveLength(0);
    expect(blocksFor(css, ".graduation__revision-disclosure")).toHaveLength(0);
  });

  it("GRU-FR-YYXN: the open row takes the queue rail's own selected tone", () => {
    const css = sheet("components.css");
    const [open] = blocksFor(
      css,
      '.graduation__pass[data-open="true"] .graduation__pass-row',
    );
    const [railRow] = blocksFor(css, ".graduation__row--selected");
    // "The rows take the same selected treatment the queue rail's own rows
    // take" is the whole of GRU-FR-YYXN here, and it is a claim about two rules
    // agreeing — which no assertion about either one alone can hold.
    expect(decl(open, "background")).toBe(decl(railRow, "background"));
    expect(decl(open, "background")!.startsWith("var(--")).toBe(true);
  });

  it("GRU-FR-NBRO: a row keeps a visible focus ring", () => {
    const css = sheet("components.css");
    const [focus] = blocksFor(css, ".graduation__pass-row:focus-visible");
    expect(focus).toBeDefined();
    // Reachable from the keyboard is worth nothing if the author cannot see
    // where they are.
    expect(decl(focus, "outline")).toContain("var(--accent)");
    expect(decl(focus, "outline")).not.toBe("none");
  });

  it("GRU-FR-YYXN: the mark on a row with a new account is visible and is a token", () => {
    const css = sheet("components.css");
    const [dot] = blocksFor(
      css,
      '.graduation__pass-row[data-new="true"] .graduation__pass-name::after',
    );
    expect(dot).toBeDefined();
    // The dot is the only thing that makes "new" visible at a glance; an empty
    // `content` removes it without failing a single component test.
    expect(decl(dot, "content")).toBe('"•"');
    expect(decl(dot, "color")).toBe("var(--accent)");
  });

  it("GRU-FR-YYXN: a hollow mark is a hole in its line, in every state", () => {
    const css = sheet("components.css");

    // The stage row runs its track behind the mark, so the mark's own fill is
    // what makes the hole — and which surface that fill has to match is the
    // host's to name (RPV-FR-17). `--bg-canvas` is the document page rather
    // than the panel, and a mark drawn in it reads as an off-colour dot.
    const [stageMark] = blocksFor(css, ".run-progress__mark");
    expect(decl(stageMark, "z-index")).toBe("1");
    expect(decl(stageMark, "background")).toContain("var(--progress-ground");
    const [section] = blocksFor(css, ".graduation");
    expect(decl(section, "--progress-ground")).toBe("var(--bg-panel)");

    // The timeline breaks its rule around the mark instead, which is why that
    // mark carries no fill: the row tints itself on hover, and a fill matching
    // the untinted panel turns the mark into a pale dot for as long as the
    // pointer is on the row. Nothing to hide, nothing to match.
    const [rowMark] = blocksFor(css, ".graduation__pass-mark");
    expect(decl(rowMark, "background")).toBe("transparent");
    const above = rule(".graduation__pass-row::before");
    const below = rule(".graduation__pass-row::after");
    // Stopping a mark's radius plus a hair short on either side is the whole of
    // it; run to `50%` instead and the rule reappears behind the mark.
    expect(decl(above, "height")).toBe("calc(50% - 5px)");
    expect(decl(below, "top")).toBe("calc(50% + 5px)");
  });

  it("GRU-FR-YYXN, GRU-FR-GXQE: the list grows with its rows and scrolls with the region", () => {
    const css = sheet("components.css");
    const [list] = blocksFor(css, ".graduation__pass-list");
    expect(decl(list, "max-height")).toBeNull();
    expect(decl(list, "overflow-y")).toBeNull();
    // The region is the scroller the list moves with.
    const [region] = blocksFor(css, ".graduation__run");
    expect(decl(region, "overflow")).toBe("auto");
  });

  it("GRU-FR-YYXN: an account's text wraps rather than widening the panel", () => {
    const css = sheet("components.css");
    const [text] = blocksFor(css, ".graduation__revision-text");
    // Line breaks kept by the box rather than by a parser, and an unbroken
    // token — a URL a model wrote — wrapped rather than allowed to stretch the
    // panel it is in.
    expect(decl(text, "white-space")).toBe("pre-wrap");
    expect(decl(text, "overflow-wrap")).toBe("anywhere");
  });
});

describe("the graduation run region is bounded to the viewport (GRU-FR-MYFA)", () => {
  // The foot is "pinned so the decisions are reachable without scrolling to the
  // end of a long comparison" — which is only true while the modal fits the
  // window. Unbounded, opening the Request-changes composer pushed the modal's
  // head off the top of the viewport and its confirm below the fold at the
  // minimum window size the shell supports, with nothing able to scroll it into
  // view: `.scrim` does not scroll and `.modal` hides its overflow. No
  // component test can see that, and a modal that cannot be confirmed is the
  // one failure the author cannot work around.

  it("bounds the modal and lets only its body take the slack", () => {
    const css = sheet("components.css");
    const [modal] = blocksFor(css, ".modal.modal--wide");
    expect(modal).toBeDefined();
    expect(decl(modal, "max-height")).toBe("calc(100vh - 32px)");
    expect(decl(modal, "display")).toBe("flex");
    expect(decl(modal, "flex-direction")).toBe("column");

    // Every band keeps its height; the body takes what is left.
    const [bands] = blocksFor(css, ".modal.modal--wide > *");
    expect(decl(bands, "flex")).toBe("none");

    // Read at the selector that actually wins. `.modal--wide .modal__body`
    // further down the file sets a 60vh cap at equal specificity, so a
    // two-class selector here would declare an override the cascade discards —
    // and this assertion would pass over a dead declaration.
    const [body] = blocksFor(css, ".modal--wide .modal__body.graduation-review");
    expect(body).toBeDefined();
    expect(decl(body, "flex")).toBe("1 1 auto");
    // Without this a flex item's floor is its content height, which is the
    // whole of the overflow.
    expect(decl(body, "min-height")).toBe("0");
    expect(decl(body, "max-height")).toBe("none");
    // And the rule that would otherwise win must come earlier in the file, or
    // the extra class is buying nothing.
    expect(css.indexOf(".modal--wide .modal__body.graduation-review")).toBeLessThan(
      css.indexOf(".modal--wide .modal__body {"),
    );
  });

  it("keeps the rail and the file region scrolling within themselves", () => {
    const css = sheet("components.css");
    // The body hides its own overflow, so the two columns are what scroll —
    // which is what keeps a long change set from growing the modal.
    const [body] = blocksFor(css, ".modal--wide .modal__body.graduation-review");
    expect(decl(body, "overflow")).toBe("hidden");
    const [rail] = blocksFor(css, ".graduation-review__rail");
    expect(decl(rail, "overflow")).toBe("auto");
    const [file] = blocksFor(css, ".graduation-review__file");
    expect(decl(file, "overflow")).toBe("auto");
  });
});

describe("the composer's placeholder is clipped rather than wrapped (CTA-FR-ZRXL)", () => {
  // CTA-FR-IAKP, CTA-FR-IGBO, CTA-FR-YGMB's second clause is entirely CSS: "the placeholder is clipped with
  // an ellipsis at the field's trailing edge, the field is still one line tall,
  // no second line is taken". A jsdom render lays nothing out, so with these
  // three declarations deleted every component test still passes and the
  // recipients wrap the composer to two lines again — which is exactly the
  // defect this file exists to catch.
  const css = sheet("kit.css");
  const blocks = blocksFor(css, ".comment-card__composer::placeholder");

  it("declares the rule exactly once", () => {
    expect(blocks).toHaveLength(1);
  });

  it("keeps the recipients on one line", () => {
    // The load-bearing half: without it a textarea's placeholder wraps, takes a
    // second line, and grows the field — the one project where that is worst
    // being the one enrolling many agents.
    expect(decl(blocks[0], "white-space")).toBe("nowrap");
  });

  it("clips what will not fit, at the trailing edge", () => {
    expect(decl(blocks[0], "overflow")).toBe("hidden");
    // Honoured on a textarea's placeholder by WebKit — the engine the packaged
    // macOS and Linux applications render in — and dropped by Chromium and
    // WebView2, which clip mid-character instead. Declared unconditionally
    // because it is free where it works and inert where it does not; there is
    // no CSS-only route to it on a textarea in the engines that drop it.
    expect(decl(blocks[0], "text-overflow")).toBe("ellipsis");
  });

  it("leaves no rule for the line the placeholder replaced", () => {
    // CTA-FR-YGMB: no line of text above the field, beside it, or below it states
    // a recipient. The rule that styled one is gone with it.
    expect(blocksFor(css, ".comment-composer__recipients")).toHaveLength(0);
  });
});
