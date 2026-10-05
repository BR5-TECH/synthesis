import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import {
  BESIDE_MIN_WIDTH,
  CARD_MIN_WIDTH,
  MARGIN_GUTTERS,
  PAGE_INSET,
  PAGE_OUTER,
} from "../hooks/useCommentArrangement";
import { blocksFor, decl, layersOf, sheet } from "./cssRules";

/**
 * Stylesheet invariants no rendering test can see — panels, the status bar, Markdown bodies and the Flow surface.
 *
 * One part of the group `./cssRules.ts` heads, which carries the whole rule
 * these are written under.
 */

describe("an empty vertical panel centres its block (SNV-FR-60)", () => {
  // This is the whole of the requirement. SNV-FR-60 exists because the Drafts
  // panel's empty state rendered flush to its body's top-left corner, three
  // elements at three insets with the sentence running the panel's full width
  // — the defect that motivated the rule. Every property that fixes it lives in
  // CSS, so with `css: false` the component tests cannot tell a centred block
  // from the broken one: delete these declarations and every assertion in
  // DraftsPanel/Comments/Changes still passes, and the defect ships again.

  it("centres the block in the region the list would have occupied", () => {
    const css = sheet("kit.css");
    // The centring is the scroll container's, not the block's: a percentage
    // height on the block would overflow the container's padding and raise a
    // scrollbar over an empty panel.
    const body = blocksFor(css, ".vpanel__body--empty");
    expect(body, "no rule for .vpanel__body--empty").toHaveLength(1);
    expect(decl(body[0], "display")).toBe("flex");
    expect(decl(body[0], "align-items")).toBe("center");
    expect(decl(body[0], "justify-content")).toBe("center");
  });

  it("stacks the block centred, on a measure narrower than the panel", () => {
    const css = sheet("kit.css");
    const block = blocksFor(css, ".panel-empty");
    expect(block, "no rule for .panel-empty").toHaveLength(1);
    // Centred across the panel's width, and its parts centred on each other.
    expect(decl(block[0], "display")).toBe("flex");
    expect(decl(block[0], "flex-direction")).toBe("column");
    expect(decl(block[0], "align-items")).toBe("center");
    expect(decl(block[0], "text-align")).toBe("center");
    // The constrained measure: the sentence wraps into a block rather than
    // running the full width of a narrow surface.
    const measure = decl(block[0], "max-width");
    expect(measure, ".panel-empty sets no max-width").not.toBeNull();
    expect(measure).toMatch(/^\d+(\.\d+)?ch$/);
    expect(Number.parseFloat(measure!)).toBeLessThanOrEqual(48);
  });

  it("leaves the block's own parts free of margins that would uncentre them", () => {
    const css = sheet("kit.css");
    for (const name of [".panel-empty__line", ".panel-empty__body"]) {
      const block = blocksFor(css, name);
      expect(block, `no rule for ${name}`).toHaveLength(1);
      // A UA paragraph margin would push the block off the vertical centre it
      // was just placed on, by a different amount per panel.
      expect(decl(block[0], "margin"), name).toBe("0");
    }
  });
});

describe("a list narrowed to nothing sits where the list was (SNV-FR-61)", () => {
  // The same defect as SNV-FR-60's, in the other state: a message left in the
  // region's top-left corner reads as a stray line of text rather than as the
  // answer to what the author just typed. It went unnoticed because three
  // panels each rolled their own treatment of it.

  it("centres the message in the region the list would have occupied", () => {
    const css = sheet("kit.css");
    // On the scroll container, for the reason the empty block's is.
    const body = blocksFor(css, ".vpanel__body--filtered");
    expect(body, "no rule for .vpanel__body--filtered").toHaveLength(1);
    expect(decl(body[0], "display")).toBe("flex");
    expect(decl(body[0], "align-items")).toBe("center");
    expect(decl(body[0], "justify-content")).toBe("center");
  });

  it("sets the message on a centred measure narrower than the panel", () => {
    const css = sheet("kit.css");
    const block = blocksFor(css, ".panel-filtered");
    expect(block, "no rule for .panel-filtered").toHaveLength(1);
    expect(decl(block[0], "text-align")).toBe("center");
    // A UA paragraph margin would push it off the centre it was just placed on.
    expect(decl(block[0], "margin")).toBe("0");
    const measure = decl(block[0], "max-width");
    expect(measure, ".panel-filtered sets no max-width").not.toBeNull();
    expect(measure).toMatch(/^\d+(\.\d+)?ch$/);
    expect(Number.parseFloat(measure!)).toBeLessThanOrEqual(48);
  });
});

describe("a vertical panel's pinned footer centres its button (LIB-FR-VMAQ / DRP-FR-06)", () => {
  // The whole bundle is read, so a later part that overrides the shared rule
  // gives a second rule for the class and fails the count.
  const css = sheet("components.css");

  it("LIB-FR-VMAQ, DRP-FR-06: View map and + Draft share one footer rule, pinned below the tree", () => {
    const [block] = blocksFor(css, ".library__footer");
    expect(blocksFor(css, ".library__footer")).toEqual([block]);
    expect(blocksFor(css, ".drafts__footer")).toEqual([block]);
    expect(decl(block, "border-top")).toBe("1px solid var(--border-1)");
  });

  it("LIB-FR-VMAQ, DRP-FR-06: the footer centres its one button across the panel's width", () => {
    const [block] = blocksFor(css, ".library__footer");
    expect([decl(block, "display"), decl(block, "justify-content")]).toEqual(["flex", "center"]);
  });
});

describe("the status bar does not clip its own overlay (STB-FR-12)", () => {
  // `.status-bar__overlay` is positioned against `.status-bar__center` and grows
  // UPWARD out of the 26px strip. An `overflow: hidden` on the strip clips it to
  // the strip's height, which hides the in-flight operations overlay completely
  // — the state STB-FR-15 exists to surface. The DOM is unchanged either way, so
  // App.statusbar.test.tsx passes in both worlds.
  it("declares no overflow that would clip the overlay away", () => {
    const blocks = blocksFor(sheet("kit.css"), ".status-bar");
    expect(blocks).toHaveLength(1);
    for (const prop of ["overflow", "overflow-y"]) {
      const value = decl(blocks[0], prop);
      if (value !== null) expect(value).toBe("visible");
    }
  });

  // The truncation the strip needs is per-region instead, which is what keeps it
  // to one line without clipping anything that escapes it.
  it("truncates its operation label rather than the whole strip", () => {
    const [label] = blocksFor(sheet("kit.css"), ".status-bar__progress-label");
    expect(label).toBeDefined();
    expect(decl(label, "overflow")).toBe("hidden");
    expect(decl(label, "text-overflow")).toBe("ellipsis");
  });
});

describe("panel group headers stay separable (NTS-FR-11 / CMP-FR-05)", () => {
  // `.note-group__header` is shared by the Notes and Comments panels. The
  // Comments variant needs `display: flex` to seat its thread count; the Notes
  // variant needs a block container for `text-overflow: ellipsis` to apply at
  // all. Defining the flex layout on the shared class — as the Comments section
  // once did — silently kills truncation in a panel the author never edited.
  it("defines the shared class exactly once", () => {
    expect(blocksFor(sheet("kit.css"), ".note-group__header")).toHaveLength(1);
  });

  it("keeps the shared class a block, so its ellipsis applies", () => {
    const [base] = blocksFor(sheet("kit.css"), ".note-group__header");
    expect(decl(base, "display")).toBe("block");
    expect(decl(base, "text-overflow")).toBe("ellipsis");
  });

  it("puts the Comments panel's flex layout on its own modifier", () => {
    const [counted] = blocksFor(
      sheet("kit.css"),
      ".note-group__header--counted",
    );
    expect(counted).toBeDefined();
    expect(decl(counted, "display")).toBe("flex");
  });
});

describe("a file name is never case-transformed (SNV-FR-57)", () => {
  // Three surfaces render a filename inside a treatment that uppercases by
  // default. Each opts out through a modifier, and a component test can only
  // assert that the modifier is APPLIED — whether it still does anything lives
  // here. Without this, a modifier could lose its declaration and every
  // `toHaveClass` assertion would stay green.
  const cases: [string, string][] = [
    ["components.css", ".panel-header__title--file"],
    ["kit.css", ".note-group__header--entity"],
    ["colors_and_type.css", ".t-eyebrow__literal"],
  ];

  it.each(cases)("%s %s turns the transform off", (file, selector) => {
    const [block] = blocksFor(sheet(file), selector);
    expect(block).toBeDefined();
    expect(decl(block, "text-transform")).toBe("none");
  });
});

describe("no rule overflows its container for want of box-sizing", () => {
  // This project has no global box-sizing reset — it is declared per rule, as
  // the note on `.input` records. A rule that sets `width: 100%` (or a `calc()`
  // of it) alongside padding or a border therefore renders WIDER than its
  // container, and in a narrow vertical panel that is the difference between a
  // row fitting and a chip being clipped off the edge.
  //
  // The selector list is derived from the stylesheets rather than hardcoded, so
  // this catches the next rule as well as today's.
  it("declares box-sizing wherever width and padding meet", () => {
    const offenders: string[] = [];
    for (const file of ["components.css", "kit.css", "colors_and_type.css"]) {
      const css = sheet(file);
      for (const m of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
        const selector = m[1].trim().replace(/\s+/g, " ");
        const block = m[2];
        if (selector.startsWith("@") || !selector) continue;

        const width = decl(block, "width");
        const spans = width !== null && /^(100%|calc\()/.test(width);
        if (!spans) continue;

        const pads = ["padding", "padding-left", "padding-right"].some((p) => {
          const v = decl(block, p);
          return v !== null && /(?:^|\s)(?!0)[\d.]/.test(v);
        });
        const borders = ["border", "border-left", "border-right"].some((p) => {
          const v = decl(block, p);
          return v !== null && /(?:^|\s)(?!0)[\d.]/.test(v);
        });
        if (!pads && !borders) continue;

        if (decl(block, "box-sizing") === null) {
          offenders.push(`${file}: ${selector}`);
        }
      }
    }
    expect(offenders).toEqual([]);
  });
});

describe("a Markdown body takes its host's type treatment (NTS-FR-25 / CMP-FR-12)", () => {
  // `.doc` sets a size AND a colour on every block it renders. `.doc--ui` is
  // the modifier that makes that rendering fit a panel row, and the surfaces
  // using it — `.comment__body` in the Comments panel, the Notes panel, and the
  // rail — have already declared what a body reads like there. A modifier that
  // names a size of its own overrules all three, so the container renders at one
  // size and the paragraph inside it at another. Both are classes on the same
  // element in the DOM, so `toHaveClass` passes either way.
  //
  // Each of these checks EVERY rule carrying the selector, not the first one: a
  // rule appended later reinstates the defect while leaving the original — the
  // one a guard reading `blocks[0]` would inspect — untouched and passing. What
  // it asserts is that the property is declared exactly once across all of them,
  // which still permits the separate rule that gives a list its indent.
  const declaredOnceAs = (selector: string, prop: string, value: string) => {
    const declared = blocksFor(sheet("colors_and_type.css"), selector)
      .map((b) => decl(b, prop))
      .filter((v): v is string => v !== null);
    expect(declared).toEqual([value]);
  };

  const inheritSize = [
    ".doc--ui p",
    ".doc--ui ul",
    ".doc--ui ol",
    ".doc--ui h1",
    ".doc--ui h2",
    ".doc--ui h3",
    ".doc--ui blockquote",
    ".doc--ui pre",
  ];

  it.each(inheritSize)("%s takes the host's size", (selector) => {
    declaredOnceAs(selector, "font-size", "inherit");
  });

  // The colour half of the same defect: `.doc p` sets `--fg-2`, and a body whose
  // container declares `--fg-1` renders its own paragraphs a step lighter than
  // itself. A blockquote is deliberately absent — being set back from the prose
  // is what marks a passage as quoted, and that is a distinction in the content
  // rather than in the scale this modifier changes.
  const inheritColour = inheritSize.filter(
    (s) => s !== ".doc--ui blockquote" && s !== ".doc--ui pre",
  );

  it.each(inheritColour)("%s takes the host's colour", (selector) => {
    declaredOnceAs(selector, "color", "inherit");
  });

  // `.doc code` is already `0.92em` — relative to whatever encloses it. A second
  // relative notch on `.doc--ui code` does not replace that one, it multiplies
  // with it, and a fenced block lands two steps below the prose it explains.
  it("adds no second relative notch for code", () => {
    for (const block of blocksFor(sheet("colors_and_type.css"), ".doc--ui code")) {
      expect(decl(block, "font-size")).toBeNull();
    }
  });

  it("declares the surface's own body treatment once, on the container", () => {
    const blocks = blocksFor(sheet("kit.css"), ".comment__body");
    expect(blocks).toHaveLength(1);
    expect(decl(blocks[0], "font-size")).toBe("var(--fs-ui-sm)");
    expect(decl(blocks[0], "color")).toBe("var(--fg-1)");
  });

  // The defect has a second entrance. `kit.css` loads after `colors_and_type.css`
  // and carries its own `.comment__body` descendant rules, which match the
  // modifier's specificity exactly — so `.comment__body p { font-size: 13px }`
  // there reproduces the original mismatch with `.doc--ui` untouched and every
  // guard above still green. The host declares the treatment; nothing below it
  // re-declares a size.
  it("sets no size on a descendant of the container", () => {
    const css = sheet("kit.css");
    const offenders: string[] = [];
    for (const m of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
      const selector = m[1].trim().replace(/\s+/g, " ");
      if (!/^\.comment__body\s+\S/.test(selector)) continue;
      if (decl(m[2], "font-size") !== null) offenders.push(selector);
    }
    expect(offenders).toEqual([]);
  });
});

describe("the rail's pinned footer covers the rail's foot (CMT-FR-17)", () => {
  // The footer is opaque so the aligned cards behind it do not read through.
  // Anything it leaves uncovered at the bottom is a strip of whatever card runs
  // past it, sliced mid-line — so the gutter has to be its own padding rather
  // than an offset that leaves a gap.
  it("sits flush with the foot and takes the gutter as padding", () => {
    const blocks = blocksFor(sheet("kit.css"), ".comment-rail__unaligned");
    expect(blocks).toHaveLength(1);
    // Laid at the foot rather than stuck to it: `margin-top: auto` is what puts
    // it there when the head above does not fill the margin.
    expect(decl(blocks[0], "margin-top")).toBe("auto");
    expect(decl(blocks[0], "padding-bottom")).toBe("var(--sp-3)");
    // Covering the foot is only half of it: an opaque surface is what stops the
    // cards behind reading through the footer that sits over them. Which colour
    // is not free either — the rail sits on the FIELD beside the page
    // (CMT-FR-01, per `EDT-editor.md` EDT-FR-63), so a footer in any other one
    // paints a slab the width of the margin, ending wherever its cards end.
    expect(decl(blocks[0], "background")).toBe("var(--doc-field-bg)");
  });

  it("gives the pinned head at the rail's other end the same surface", () => {
    // CMT-FR-62: the Discussion section is pinned at the head on exactly the
    // terms the footer is pinned at the foot, so it is opaque in the same colour
    // — one rail, not two surfaces that happen to share a margin.
    const blocks = blocksFor(sheet("kit.css"), ".comment-rail__pinned-head");
    expect(blocks).toHaveLength(1);
    expect(decl(blocks[0], "background")).toBe("var(--doc-field-bg)");
  });

  it("bounds the head by the footer actually rendered, not by a fraction", () => {
    // The two sections are laid out, not each stuck to an end and bounded by a
    // guess. A fraction cannot see the difference between a collapsed resolved
    // disclosure — one row — and an expanded one, so it charged the discussions
    // the same either way and left two-fifths of the margin empty below them.
    const rail = blocksFor(sheet("kit.css"), ".comment-rail")[0];
    expect(decl(rail, "display")).toBe("flex");
    expect(decl(rail, "flex-direction")).toBe("column");

    // The head takes what is left: its natural height while that fits, shrinking
    // and scrolling within itself only once it does not. Nothing reserves a
    // share of the margin from it.
    const head = blocksFor(sheet("kit.css"), ".comment-rail__pinned-head")[0];
    expect(decl(head, "flex")).toBe("0 1 auto");
    expect(decl(head, "min-height")).toBe("0");
    expect(decl(head, "overflow-y")).toBe("auto");
    expect(decl(head, "max-height")).toBeNull();

    // The foot takes what it needs and never grows into space the discussions
    // could use. Shrinkable like the head, so when both want more than the
    // margin has, each gives up a share of the deficit weighted by what it asked
    // for — a one-row footer beside twenty discussions keeps its row, and a
    // twenty-thread footer beside one discussion is the one that scrolls.
    const foot = blocksFor(sheet("kit.css"), ".comment-rail__unaligned")[0];
    expect(decl(foot, "flex")).toBe("0 1 auto");
    expect(decl(foot, "min-height")).toBe("0");
    expect(decl(foot, "overflow-y")).toBe("auto");
  });

  it("bounds the foot by what else the margin holds, not by a fraction (CMT-FR-63)", () => {
    // A fixed ceiling could not tell the difference between a margin with
    // threads still open in it and one whose every thread is resolved. It bound
    // both, so an artifact with nothing else to show spent the upper two-fifths
    // of its margin on nothing while the disclosure scrolled inside the lower
    // three — which is the defect this pair of rules exists to prevent.
    const css = sheet("kit.css");
    const foot = blocksFor(css, ".comment-rail__unaligned")[0];
    // Unbounded by default: with nothing else to show, the foot has the margin.
    expect(decl(foot, "max-height")).toBe("100%");

    // The ceiling means the whole box. `max-height` bounds the CONTENT box by
    // default, and this section carries padding and a top border — so the height
    // it kept back measured 87px against a floor of 96, and the card it was
    // keeping room for showed twelve pixels of itself.
    expect(decl(foot, "box-sizing")).toBe("border-box");

    // Bounded only while something else is there to be buried — a discussion at
    // the head, a thread aligned in the body, or the card being written. The
    // component sets the flag; CommentRail.test.tsx holds it to those three.
    const bounded = blocksFor(css, '.comment-rail__unaligned[data-bounded="true"]');
    expect(bounded, "no rule bounds the foot when the margin holds more").toHaveLength(1);
    expect(decl(bounded[0], "max-height")).toBe(
      "calc(100% - var(--comment-card-floor))",
    );
    // A real length rather than a fraction: what has to stay reachable is a
    // card, and a percentage of a short margin is not one.
    expect(sheet("colors_and_type.css")).toMatch(
      /--comment-card-floor:\s*96px/,
    );
  });

  it("keeps the aligned layer out of that layout", () => {
    // The cards are placed at their anchors' offsets and the whole layer is
    // translated by the body's scroll (CMT-FR-27). It has to stay absolutely
    // positioned, or it becomes a third flex item and takes height from the two
    // sections that are supposed to divide the margin between them.
    const aligned = blocksFor(sheet("kit.css"), ".comment-rail__aligned")[0];
    expect(decl(aligned, "position")).toBe("absolute");
    expect(decl(aligned, "top")).toBe("0");
  });

  it("keeps the head section's own gutter on its header rather than on the section", () => {
    // The section scrolls within itself, and its header sticks to the top of it.
    // Padding on the SECTION is a strip the header cannot reach — a card
    // scrolling under it shows through above the word Discussion, sliced
    // mid-line, which is the same defect the footer's own padding rule avoids at
    // the other end. So the gutter belongs to the header, which is opaque and
    // travels with the top edge.
    const section = blocksFor(sheet("kit.css"), ".comment-rail__pinned-head")[0];
    expect(decl(section, "padding-top")).toBeNull();
    const header = blocksFor(
      sheet("kit.css"),
      ".comment-rail__pinned-head > .comment-rail__section-title",
    );
    expect(header).toHaveLength(1);
    expect(decl(header[0], "position")).toBe("sticky");
    expect(decl(header[0], "top")).toBe("0");
    expect(decl(header[0], "padding-top")).toBe("var(--sp-3)");
    expect(decl(header[0], "background")).toBe("var(--doc-field-bg)");
  });

  it("starts the New Artifact tab's rail below that tab's formatting toolbar", () => {
    // An Editor tab's rail is laid inside the box below the toolbar; a draft
    // tab's is a child of the whole surface, toolbar included. Without the
    // offset the head section's opaque surface starts a toolbar's height too
    // high and cuts a notch out of the formatting controls, with the section's
    // own header behind them.
    const blocks = blocksFor(sheet("kit.css"), ".draft-editor > .comment-rail");
    expect(blocks).toHaveLength(1);
    expect(decl(blocks[0], "top")).toBe(
      "calc(var(--editor-toolbar-h) + var(--doc-page-inset))",
    );
  });

  it("ends that rail level with the page rather than above the action control", () => {
    // A DELIBERATE DEVIATION from NAW-FR-27's clause that the margin's pinned
    // sections end above the action control. The two share the field's trailing
    // edge — the control is `--sp-4` from the frame and the rail `--sp-6`, so
    // most of the control is over the rail whatever height it is given — and
    // ending above it cost the foot a control's height of margin at every window
    // size. Pinned here so that restoring the requirement is a deliberate act
    // rather than something that drifts back.
    const rail = blocksFor(sheet("kit.css"), ".draft-editor > .comment-rail")[0];
    expect(decl(rail, "bottom")).toBe("var(--doc-page-inset)");

    // What the deviation costs is bounded by this: the collapsed disclosure is
    // the row that now sits beside the control, so its BOX is shortened to clear
    // it. Padding would not do — it is inside the button's own box and so still
    // part of what the button can be clicked on, which would leave the two
    // sharing pixels and the control taking clicks meant for the row.
    const row = blocksFor(sheet("kit.css"), ".draft-editor .comment-rail__disclosure");
    expect(row).toHaveLength(1);
    expect(decl(row[0], "width")).toBe("calc(100% - var(--draft-actions-clear))");
    expect(decl(row[0], "padding-right")).toBeNull();
  });
});

describe("an edge label outranks the nodes it sits between (FLO-FR-19)", () => {
  // The label is placed at the edge's midpoint, which falls inside a node
  // whenever the label is wider than the gap between the two it joins. Both
  // boxes are opaque; without a stacking order the node paints last and slices
  // the label with no ellipsis to show for it.
  //
  // The comparison is derived rather than fixed: the invariant is that the label
  // outranks EVERY node, so a `z-index` added to a node state later — a selected
  // node lifting itself, say — has to be answered here rather than silently
  // winning against a hardcoded `> 0`.
  it("declares a stacking order above every node", () => {
    const css = sheet("kit.css");
    const blocks = blocksFor(css, ".flow-edge-label");
    expect(blocks).toHaveLength(1);
    const label = Number(decl(blocks[0], "z-index"));
    expect(label).toBeGreaterThan(0);

    const nodeLayers: number[] = [];
    for (const m of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
      const selectors = m[1].split(",").map((s) => s.trim());
      if (!selectors.some((s) => /^\.flow-node\b/.test(s))) continue;
      const z = decl(m[2], "z-index");
      if (z !== null && z !== "auto") nodeLayers.push(Number(z));
    }
    for (const z of nodeLayers) expect(label).toBeGreaterThan(z);
  });
});


describe("a loop's own chrome outranks what the loop holds (FLO-FR-38)", () => {
  // Members are absolutely-placed SIBLINGS of the loop, later in the viewport,
  // so paint order alone puts them over the container's header — and a member
  // dropped near the top of a loop takes every press meant for the loop's
  // picker, its remove affordances, and its click-throughs. Found in a browser;
  // invisible to every rendering test, since jsdom applies no stylesheet and
  // hit-testing in it does not depend on one.
  it("declares a stacking order above every node and every edge label", () => {
    const css = sheet("kit.css");
    const chrome = blocksFor(css, ".flow-loop__head").filter(
      (b) => decl(b, "z-index") !== null,
    );
    expect(chrome).toHaveLength(1);
    const z = Number(decl(chrome[0], "z-index"));
    expect(z).toBeGreaterThan(0);
    // A z-index on a statically-positioned box does nothing at all.
    expect(decl(chrome[0], "position")).toBe("relative");

    // The reference column shares the rule, so it cannot drift below the header
    // it hangs from.
    const refs = blocksFor(css, ".flow-loop__refs").filter(
      (b) => decl(b, "z-index") !== null,
    );
    expect(refs).toHaveLength(1);
    expect(Number(decl(refs[0], "z-index"))).toBe(z);

    // Derived rather than fixed: a layer added to a node state or to the edge
    // labels later has to be answered here rather than silently winning.
    for (const other of [
      ...layersOf(css, /^\.flow-node\b/),
      ...layersOf(css, /^\.flow-edge-label\b/),
    ]) {
      expect(z).toBeGreaterThan(other);
    }
  });

  // Raising the column put a box over the members; the box has to be no bigger
  // than what it draws, or it takes presses aimed at everything showing through
  // it — the members underneath, and the edge labels of a graph drawn inside the
  // loop. Both were measured in a browser after the stacking order landed.
  it("raises the rows without raising the ground between them", () => {
    const css = sheet("kit.css");
    const column = blocksFor(css, ".flow-loop__refs");
    // Each row is as wide as its content, not as wide as the container.
    const stretched = column.filter((b) => decl(b, "align-items") !== null);
    expect(stretched).toHaveLength(1);
    expect(decl(stretched[0], "align-items")).toBe("flex-start");
    // The gaps between the rows pass pointers through…
    const inert = column.filter((b) => decl(b, "pointer-events") === "none");
    expect(inert).toHaveLength(1);
    // …and each row takes its own back, or the column draws affordances that
    // cannot be pressed at all.
    const rows = blocksFor(css, ".flow-loop__refs > *");
    expect(rows).toHaveLength(1);
    expect(decl(rows[0], "pointer-events")).toBe("auto");
  });

  // `.flow-node__port` carries the node's `top` and is declared later in the
  // same sheet, so a single-class rule for the loop's port is dead at equal
  // specificity — the port renders 12px below the loop's top corner instead of
  // at the middle of its border, which is the side an edge into a loop lands on.
  it("places the loop's port with a rule that outranks the node's", () => {
    const css = sheet("kit.css");
    const loopPort = [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)]
      .map((m) => [m[1].trim(), m[2]] as const)
      .filter(([sel, body]) => sel.includes(".flow-loop__port") && decl(body, "top"));
    expect(loopPort).toHaveLength(1);
    const [selector, body] = loopPort[0];
    expect(decl(body, "top")).toBe("50%");
    // Two class names on one element, which beats `.flow-node__port`'s one.
    expect(selector.match(/\.flow-loop__port/g)?.length).toBeGreaterThan(1);
  });
});

describe("a Flow node reads as part of the drawing (FLO-flow.md Layout notes)", () => {
  // The whole of this is presentational: a node's surface is lighter than the
  // elevated tone a modal takes, slightly translucent, and carries the canvas's
  // own dot texture. jsdom applies no stylesheet, so a revert to
  // `background: var(--bg-elevated)` — the opaque slab this replaced — renders
  // identically in every component test in the repo.

  /** A theme's declaration block, by the selector it is written under. */
  function themeBlock(selector: string): string {
    const css = sheet("colors_and_type.css");
    const m = css.match(new RegExp(`${selector}\\s*\\{([^}]*)\\}`, "s"));
    expect(m, `no ${selector} block`).not.toBeNull();
    return m![1];
  }

  const THEMES: [string, string][] = [
    ["light", '\\[data-theme="light"\\]'],
    ["dark", '\\[data-theme="dark"\\]'],
  ];

  it("paints the node's own tone and the canvas's dot pitch", () => {
    const blocks = blocksFor(sheet("kit.css"), ".flow-node");
    expect(blocks).toHaveLength(1);
    const node = blocks[0];
    expect(decl(node, "background-color")).toBe("var(--flow-node-bg)");
    expect(decl(node, "background-image")).toMatch(
      /radial-gradient\([^)]*var\(--flow-node-dot\)/,
    );
    // The same pitch the canvas's own grid is drawn at, so the two textures are
    // one texture at the view's default scale rather than two that nearly
    // agree.
    const canvas = blocksFor(sheet("kit.css"), ".flow");
    expect(canvas).toHaveLength(1);
    expect(decl(node, "background-size")).toBe(decl(canvas[0], "background-size"));
  });

  // The token guard above concatenates all three sheets before collecting what
  // is DEFINED, so a token declared in one theme block and not the other counts
  // as defined — and a node would inherit the light tone onto the dark canvas,
  // silently, with nothing failing.
  it("defines both of its tokens under each theme", () => {
    for (const [name, selector] of THEMES) {
      const block = themeBlock(selector);
      for (const token of ["--flow-node-bg", "--flow-node-dot"]) {
        expect(decl(block, token), `${name} defines no ${token}`).not.toBeNull();
      }
    }
  });

  // The requirement is a COMPARISON — lighter than the elevated tone — so the
  // difference is what gets pinned. A revert to `--bg-elevated`'s own value is
  // invisible to every other test here.
  it("keeps the node's surface off the elevated tone under either theme", () => {
    for (const [name, selector] of THEMES) {
      const block = themeBlock(selector);
      const elevated = decl(block, "--bg-elevated")!.trim().toLowerCase();
      const node = decl(block, "--flow-node-bg")!.trim().toLowerCase();
      expect(node, `${name}: the node took the elevated tone`).not.toBe(elevated);
      // Translucent, so the canvas reads through it rather than being covered.
      expect(node, `${name}: the node's surface is opaque`).toMatch(/rgba?\(/);
      const alpha = Number(node.split(",").pop()!.replace(")", "").trim());
      expect(alpha).toBeGreaterThan(0);
      expect(alpha).toBeLessThan(1);
    }
  });
});

// The add menu takes the action control's treatment from `components.css`
// (ACT-FR-05) and overrides only its anchoring in `kit.css`. Equal specificity,
// so the cascade decides — and the cascade here is the import order in
// `App.tsx`. Flip the two imports and the Flow's add menu anchors to the foot
// of the tab, under the graph it adds to (FLO-FR-07).
describe("the Flow add menu's anchoring wins over the treatment it borrows", () => {
  it("loads kit.css after components.css", () => {
    const entry = readFileSync(resolve(process.cwd(), "src/App.tsx"), "utf8");
    const at = (name: string) => {
      const i = entry.indexOf(`styles/${name}`);
      expect(i, `App.tsx imports no ${name}`).toBeGreaterThan(-1);
      return i;
    };
    expect(at("kit.css")).toBeGreaterThan(at("components.css"));
  });

  it("overrides every side the borrowed treatment anchors from", () => {
    const borrowed = blocksFor(sheet("components.css"), ".draft-actions__menu");
    expect(borrowed).toHaveLength(1);
    const own = blocksFor(sheet("kit.css"), ".flow-addmenu__list");
    expect(own).toHaveLength(1);
    // Whatever the control anchors from, this menu answers: an offset left
    // un-overridden is one the borrowed rule still sets, and the menu lands in
    // two corners at once.
    for (const side of ["top", "right", "bottom", "left"]) {
      if (decl(borrowed[0], side) === null) continue;
      expect(decl(own[0], side), `the add menu inherits ${side}`).not.toBeNull();
    }
  });
});

describe("a Markdown body is only ever rendered on its own surface", () => {
  // The `.doc--ui` guards above hold because the modifier always sits on a host
  // that declares the size the blocks inherit. `.doc--ui` on an element that
  // declares none inherits from whatever encloses it instead — on the Editor
  // page, the document scale, which is the defect relocated rather than fixed.
  //
  // One component emits the modifier, and it composes it onto a host class its
  // caller names: a card in the rail, and a Flow node rendering its inline
  // prompt (`FLO-flow.md` FLO-FR-13). So the invariant is not one fixed class —
  // it is that EVERY host handed to it declares its own `font-size`.
  it("emits the panel modifier from one component, onto a host that sizes itself", () => {
    const components = [
      "CommentMarkdown.tsx",
      "CommentRail/ThreadCard.tsx",
      "Comments.tsx",
      "Notes/index.tsx",
    ]
      .map((f) => [f, readFileSync(resolve(process.cwd(), "src/components", f), "utf8")] as const);
    const emitters = components.filter(([, src]) => src.includes("doc--ui"));
    expect(emitters.map(([f]) => f)).toEqual(["CommentMarkdown.tsx"]);

    const source = emitters[0][1];
    // The modifier is composed onto the caller's class rather than hardcoded,
    // and the default host is the rail's card.
    expect(source).toContain("${className} doc doc--ui");
    expect(source).toContain('className = "comment__body"');

    // Every host actually passed in, plus the default, sizes itself.
    const hosts = new Set(["comment__body"]);
    for (const file of [
      "CommentMarkdown.tsx",
      "CommentRail/ThreadCard.tsx",
      "Comments.tsx",
      "Notes/index.tsx",
      "Flow/nodes.tsx",
    ]) {
      const src = readFileSync(resolve(process.cwd(), "src/components", file), "utf8");
      for (const m of src.matchAll(/<CommentMarkdown[^>]*?className="([^"{]+)"/gs)) {
        hosts.add(m[1]);
      }
    }
    const css = sheet("kit.css");
    for (const host of hosts) {
      const blocks = blocksFor(css, `.${host}`);
      expect(
        blocks.some((b) => decl(b, "font-size") !== null),
        `.${host} hosts .doc--ui and must declare its own font-size`,
      ).toBe(true);
    }
  });
});

describe("the design system's own tokens are the only ones used", () => {
  // A `var(--name)` naming a property nothing defines resolves to nothing, and
  // the declaration is dropped in silence: a background disappears, a menu
  // renders transparent, a selection highlight never paints. Nothing fails, and
  // no rendering test can see it. This caught nine such declarations at once —
  // a whole feature's CSS written against a token scheme (`--bg-1/2/3`) this
  // design system does not have.
  it("references no custom property it never defines", () => {
    const css = ["colors_and_type.css", "components.css", "kit.css"]
      .map(sheet)
      .join("\n");
    const defined = new Set(
      [...css.matchAll(/(--[A-Za-z0-9-]+)\s*:/g)].map((m) => m[1]),
    );
    const used = new Set(
      [...css.matchAll(/var\(\s*(--[A-Za-z0-9-]+)/g)].map((m) => m[1]),
    );
    // A fallback (`var(--x, y)`) still names an undefined token; it is only the
    // fallback that saves it, and the name is still wrong. Both are reported.
    const undefinedTokens = [...used].filter((t) => !defined.has(t)).sort();
    expect(undefinedTokens).toEqual([]);
  });
});


describe("the in-flight overlay shows five rows and scrolls the rest (STB-FR-HJVM)", () => {
  // The row count is a length in the stylesheet, which jsdom does not lay out,
  // so only a reading of the rules can see that the list is capped at five rows
  // and scrolls vertically.
  const css = sheet("kit.css");

  it("STB-FR-HJVM: caps the row area at five rows and scrolls it vertically inside itself", () => {
    const blocks = blocksFor(css, ".status-bar__overlay-rows");
    expect(blocks).toHaveLength(1);
    expect(decl(blocks[0], "max-height")).toBe("calc(5 * var(--status-overlay-row-h))");
    expect(decl(blocks[0], "overflow-y")).toBe("auto");
    expect(decl(blocks[0], "overflow-x")).toBe("hidden");
  });

  it("STB-FR-HJVM: gives every row one fixed height, so five rows is a length", () => {
    const rows = blocksFor(css, ".status-bar__overlay-row");
    expect(rows).toHaveLength(1);
    expect(decl(rows[0], "height")).toBe("var(--status-overlay-row-h)");
    const overlay = blocksFor(css, ".status-bar__overlay");
    expect(overlay).toHaveLength(1);
    expect(decl(overlay[0], "--status-overlay-row-h")).not.toBeNull();
  });
});
