import { describe, expect, it } from "vitest";
import { blocksFor, decl, sheet } from "./cssRules";

/**
 * Stylesheet invariants of the draft discussion column's message stream
 * (`../../specifications/ui/DDS-draft-discussion.md`).
 *
 * One part of the group `./cssRules.ts` heads. Everything here is a fact about
 * the stylesheet that no rendering test can see — jsdom computes no layout and
 * no cascade — and each one is load-bearing: revert it and every component
 * assertion still passes while the column is wrong on screen.
 */

const style = () => sheet("kit/discussion-stream.css");

describe("the accent means one thing (DDS-FR-CVLM)", () => {
  it("DDS-FR-CVLM: no message, row, or card takes the accent for any other reason", () => {
    // Two of these three are the author's own choice — the option they picked
    // and the radio row they picked. The third is the question chip, which
    // keeps its own tone from the chip vocabulary (DDS-FR-MCUP) and is neither
    // a message nor a row nor a card. The author's own rail is a `box-shadow`
    // rather than a fill, so it is asserted separately below.
    //
    // This scan is whole-file on purpose: a per-selector assertion guards only
    // the selector it names, and a fourth accent surface added anywhere else
    // would slip past one.
    const filled: string[] = [];
    for (const rule of style().matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
      const background = decl(rule[2], "background") ?? "";
      if (background.includes("--accent")) filled.push(rule[1].trim());
    }
    expect(filled.sort()).toEqual(
      [
        '.dds-stream-chip[data-kind="question"]',
        ".dds-question__option[data-chosen]",
        '.dds-discussion .discussion-questions__choice[data-selected="true"]',
      ].sort(),
    );
  });

  it("DDS-FR-VTKD: the author's own message is a rail and an indent, never a fill", () => {
    const own = blocksFor(style(), ".dds-message[data-own]")[0];
    expect(own, "no rule for the author's own message").toBeDefined();
    // The device is the active tab's own rail.
    expect(decl(own, "box-shadow")).toContain("--accent");
    expect(decl(own, "padding-left")).toBeTruthy();
    // A fill here would give the accent a second meaning, and a border or an
    // alignment would make the column read as two columns.
    expect(decl(own, "background")).toBeNull();
    expect(decl(own, "border")).toBeNull();
    expect(decl(own, "margin-left")).toBeNull();
    expect(decl(own, "max-width")).toBeNull();
  });
});

describe("a card is drawn only where an exchange carries state (DDS-FR-BRHN)", () => {
  it("DDS-FR-BRHN: plain talk carries no border and no fill", () => {
    const message = blocksFor(style(), ".dds-message")[0];
    expect(message, "no rule for .dds-message").toBeDefined();
    expect(decl(message, "background")).toBeNull();
    expect(decl(message, "border")).toBeNull();
    expect(decl(message, "border-radius")).toBeNull();
  });

  it("DDS-FR-BRHN: the question card and the change row are bordered and unshadowed", () => {
    // One pixel of border and no elevation: the card says "there is state here",
    // and a shadow would say "this floats above the conversation".
    for (const selector of [".dds-question", ".dds-change"]) {
      const card = blocksFor(style(), selector)[0];
      expect(card, `no rule for ${selector}`).toBeDefined();
      expect(decl(card, "background")).toContain("--bg-canvas");
      expect(decl(card, "border")).toContain("--border-1");
      expect(decl(card, "border-radius")).toContain("--r-md");
      expect(decl(card, "box-shadow")).toBeNull();
    }
  });
});

describe("body copy keeps its tone (DDS-FR-GBWP)", () => {
  it("DDS-FR-GBWP: a message body is secondary, and only the author's own is primary", () => {
    const body = blocksFor(style(), ".dds-message__body")[0];
    expect(decl(body, "color")).toContain("--fg-2");
    const own = blocksFor(style(), '.dds-message[data-own] .dds-message__body')[0];
    expect(decl(own, "color")).toContain("--fg-1");
  });

  it("DDS-FR-GBWP: an option is never set in the quietest tone", () => {
    // `--fg-4` fails the contrast floor at body size, so it is meta and
    // placeholders alone. An unselected option is 13px content and takes
    // `--fg-3`; the chosen one goes to `--fg-1`.
    const option = blocksFor(style(), ".dds-question__option")[0];
    expect(decl(option, "color")).toContain("--fg-3");
    expect(decl(option, "color")).not.toContain("--fg-4");
    const chosen = blocksFor(style(), ".dds-question__option[data-chosen]")[0];
    expect(decl(chosen, "color")).toContain("--fg-1");
  });

  it("DDS-FR-LWPC: the time is monospace meta and is never emphasised", () => {
    const time = blocksFor(style(), ".dds-stream__time")[0];
    expect(decl(time, "font-family")).toContain("--font-mono");
    expect(decl(time, "color")).toContain("--fg-4");
    expect(decl(time, "font-weight")).toContain("--fw-regular");
  });
});

describe("hover and focus (DDS-FR-PWDG)", () => {
  it("DDS-FR-PWDG: hover darkens and deepens the border, and re-tints nothing", () => {
    const hover = blocksFor(style(), ".dds-change:hover:not(:disabled)")[0];
    expect(hover, "no hover rule for the change row").toBeDefined();
    expect(decl(hover, "background")).toContain("--bg-hover");
    expect(decl(hover, "border-color")).toContain("--border-2");
    // Never the accent: hover is not a state the author chose.
    expect(decl(hover, "background")).not.toContain("--accent");
  });

  it("DDS-FR-PWDG: every focusable the stream introduces draws a ring", () => {
    for (const selector of [
      ".dds-change:focus-visible",
      ".dds-discussion .comment-card--stream .comment-composer:focus-within",
    ]) {
      const focus = blocksFor(style(), selector)[0];
      expect(focus, `no focus rule for ${selector}`).toBeDefined();
      expect(decl(focus, "box-shadow")).toContain("--shadow-focus");
    }
    // DDS-FR-HQTX: a focused field also swaps its border to the accent.
    const field = blocksFor(
      style(),
      ".dds-discussion .comment-card--stream .comment-composer:focus-within",
    )[0];
    expect(decl(field, "border-color")).toContain("--accent");
  });
});

describe("the answering block and the reply field stay docked (DDS-FR-QZAV)", () => {
  it("DDS-FR-QZAV: only the transcript scrolls, so the field is reachable at every position", () => {
    // An author reading back through a long discussion must still reach the
    // field they reply in. The scroller is the transcript alone; the block and
    // the composer sit outside it and take no scroll of their own.
    const card = blocksFor(style(), ".comment-card--stream")[0];
    expect(card, "no rule for the stream card").toBeDefined();
    expect(decl(card, "display")).toBe("flex");
    expect(decl(card, "flex-direction")).toBe("column");

    const scroller = blocksFor(
      style(),
      ".comment-card--stream > .comment-card__messages",
    )[0];
    expect(scroller, "the transcript is not the scroller").toBeDefined();
    expect(decl(scroller, "overflow-y")).toBe("auto");
    expect(decl(scroller, "flex")).toContain("1");

    const reply = blocksFor(
      style(),
      ".dds-discussion .comment-card--stream > .comment-card__reply",
    )[0];
    expect(reply, "no rule for the docked reply field").toBeDefined();
    // `0 0 auto`: it takes its own height and never the scroller's growth.
    expect(decl(reply, "flex")).toContain("0 0 auto");

    // The card's own rules must be declared where they beat `.comment-card`,
    // which sets a frame and a 12-pixel padding. The two selectors carry equal
    // specificity, so the tie goes to source order and `kit.css` is imported
    // last. Declared beside the column instead, the card keeps the rail card's
    // box and overflows the panel by its padding — which jsdom cannot see.
    expect(blocksFor(sheet("components/draft-discussion.css"), ".comment-card--stream"))
      .toHaveLength(0);
  });
});

describe("the measure and the one breakpoint (DDS-FR-ZMXQ, DDS-FR-LWPC)", () => {
  it("DDS-FR-ZMXQ: the column is 820 above the crossover and the panel less 28 below it", () => {
    // The exact gutter matters: it is what makes the column centred rather than
    // merely narrow, and a wrong one is invisible to every rendering test.
    const column = blocksFor(style(), ".dds-stream__column")[0];
    const width = decl(column, "width") ?? "";
    expect(width.replace(/\s+/g, "")).toBe(
      "min(var(--doc-page-width),calc(100%-var(--sp-3-5)*2))",
    );
    // `--sp-3-5` is 14px, taken on each side: the 28 the requirement names.
    const tokens = sheet("colors_and_type.css");
    expect(tokens).toContain("--sp-3-5: 14px");
    expect(tokens).toContain("--doc-page-width: 820px");
  });

  it("DDS-FR-ZMXQ, CTA-FR-UUXA: every surface of the column stands on that one measure", () => {
    // The transcript, the answering block, the reply field, the line that
    // stands in for a composer, and the empty-column statement. One of them on
    // a measure of its own is a column with two left edges.
    const measure = decl(
      blocksFor(style(), ".dds-stream__column")[0],
      "width",
    )!.replace(/\s+/g, "");
    for (const selector of [
      ".dds-discussion .discussion-questions",
      ".dds-discussion .comment-card--stream > .comment-card__reply",
      ".dds-discussion .discussion-questions__composer-note",
      // CTA-FR-UUXA: the refusal under the reply field.
      ".dds-discussion .comment-card--stream > .comment-card__error",
    ]) {
      const block = blocksFor(style(), selector)[0];
      expect(block, `no rule for ${selector}`).toBeDefined();
      expect(decl(block, "width")?.replace(/\s+/g, ""), selector).toBe(measure);
    }
    expect(
      decl(blocksFor(sheet("components/draft-discussion.css"), ".dds-open")[0], "width")
        ?.replace(/\s+/g, ""),
    ).toBe(measure);
  });

  it("DDS-FR-LWPC: the one breakpoint reads the panel rather than the window", () => {
    // The column is sized by the splitter, so a media query would read a width
    // that says nothing about it. A container query is the only thing that can.
    const css = style();
    expect(css).toContain("@container (max-width: 620px)");
    expect(css).not.toContain("@media (max-width");
    // And the column declares itself a container, or the query matches nothing.
    const panel = blocksFor(
      sheet("components/draft-discussion.css"),
      ".dds-discussion",
    )[0];
    expect(decl(panel, "container-type")).toBe("inline-size");

    const narrow = css.slice(css.indexOf("@container (max-width: 620px)"));
    expect(narrow).toContain("flex-wrap: wrap");
  });
});

describe("the answering block belongs to this column (DQA-FR-VNKQ)", () => {
  it("DQA-FR-VNKQ: the question area takes the canvas tone every card here takes", () => {
    // Outside this column the block's ground is white in neither theme. Here it
    // is the tone the question card and the change row sit on, which is what
    // makes the block one of them rather than a region of its own.
    const group = blocksFor(
      style(),
      ".dds-discussion .discussion-questions__group",
    )[0];
    expect(group, "no rule for the answer group in this column").toBeDefined();
    expect(decl(group, "background")).toContain("--bg-canvas");
    expect(decl(group, "border")).toContain("--border-1");
    expect(decl(group, "border-radius")).toContain("--r-md");

    // And the fieldset around it carries no second box, or the question would
    // sit in a border it notches.
    const fieldset = blocksFor(
      style(),
      ".dds-discussion .discussion-questions__question",
    )[0];
    expect(decl(fieldset, "background")).toBe("none");
    expect(decl(fieldset, "border")).toBe("0");
  });

  it("DQA-FR-VNKQ: the group adds no box of its own where no card is drawn", () => {
    // Every other presentation renders exactly as it did before the wrapper
    // existed, which `display: contents` is what guarantees.
    const shared = blocksFor(
      sheet("kit/discussion-questions.css"),
      ".discussion-questions__group",
    )[0];
    expect(shared, "no default rule for the answer group").toBeDefined();
    expect(decl(shared, "display")).toBe("contents");
  });
});

describe("a theme changes token values alone (DDS-FR-RJEV)", () => {
  it("DDS-FR-RJEV: the stream names no literal colour", () => {
    // Every colour is a token, so the two themes differ by value alone. A hex
    // here would be a colour one theme could not move.
    const offenders: string[] = [];
    for (const rule of style().matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
      for (const line of rule[2].split(";")) {
        if (/#[0-9a-f]{3,8}\b|\brgba?\(/i.test(line)) {
          offenders.push(`${rule[1].trim()}: ${line.trim()}`);
        }
      }
    }
    expect(offenders).toEqual([]);
  });

  it("DDS-FR-RJEV: no theme block changes the column's geometry", () => {
    // The stream declares nothing under a theme selector at all, which is the
    // strongest form of "a theme swaps tokens and moves nothing".
    expect(style()).not.toContain("[data-theme");
  });
});
