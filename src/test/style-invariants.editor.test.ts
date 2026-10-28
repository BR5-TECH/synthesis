import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { blocksFor, decl, sheet } from "./cssRules";

/**
 * Stylesheet invariants no rendering test can see — the editing surface, the New Artifact control, and the discussion column beside the document.
 *
 * One part of the group `./cssRules.ts` heads, which carries the whole rule
 * these are written under.
 */

describe("the editing surface reads as a page set on a field (EDT-FR-63)", () => {
  // The whole requirement is presentational. jsdom applies no stylesheet, so
  // every component test renders identically whether the page has an edge, an
  // inset, and a field behind it or is a wall of text filling the tab — which
  // is the state this change exists to end.

  /** The two tones that have to stay apart from one another under a theme. */
  function tones(themeBlock: string) {
    const css = sheet("colors_and_type.css");
    const m = css.match(
      new RegExp(`${themeBlock}\\s*\\{([^}]*)\\}`, "s"),
    );
    expect(m, `no ${themeBlock} block`).not.toBeNull();
    const read = (name: string) => {
      const d = decl(m![1], name);
      expect(d, `${themeBlock} defines no ${name}`).not.toBeNull();
      // `--doc-page-bg` is declared as a reference to the canvas; resolve it so
      // the comparison is between colours rather than between a colour and a
      // name that happens to differ from it.
      const ref = d!.match(/^var\(\s*(--[A-Za-z0-9-]+)\s*\)$/);
      return (ref ? decl(m![1], ref[1]) : d)!.trim().toLowerCase();
    };
    return {
      page: read("--doc-page-bg"),
      field: read("--doc-field-bg"),
    };
  }

  it("resolves the page's own metrics to real, non-zero lengths", () => {
    // Every framing assertion below names a `var()`. A token that resolves to
    // `0` — an inset of nothing, a measure of nothing — satisfies all of them
    // and ships the page as the wall of text this requirement exists to end,
    // so the values themselves are pinned here rather than only their names.
    // Read from the sheet as a whole rather than from a `:root` block: these
    // names are globally unique, and the first `:root` rule follows an
    // `@import` that the block reader folds into its selector.
    const root = sheet("colors_and_type.css");
    /** A custom property's value, wherever in the sheet it is declared. */
    const token = (name: string): string | null => {
      const m = root.match(new RegExp(`(?:^|[;{])\\s*${name}\\s*:\\s*([^;]+)`));
      return m ? m[1].trim() : null;
    };
    const px = (name: string) => {
      const value = token(name);
      expect(value, `no ${name} is defined`).not.toBeNull();
      // Summed rather than parsed off the front: `--doc-page-box` is a
      // `calc()` of the measure plus the padding drawn inside it, and reading
      // only its first term makes it compare equal to the measure.
      const terms = value!.match(/-?[\d.]+px/g);
      expect(terms, `${name} is not a length: ${value}`).not.toBeNull();
      return terms!.reduce((sum, t) => sum + Number.parseFloat(t), 0);
    };
    expect(px("--doc-page-inset")).toBeGreaterThan(0);
    expect(px("--doc-page-width")).toBeGreaterThan(0);
    // EDT-FR-63: the page pads its text clear of its own edge, and the box the
    // margin beside it is measured against is wider than the measure by that
    // padding. Equal values mean something stopped accounting for it.
    expect(token("--doc-page-padding")).not.toBeNull();
    expect(px("--doc-page-box")).toBeGreaterThan(px("--doc-page-width"));
    expect(token("--doc-page-radius")).not.toBeNull();
  });

  it("EDT-FR-63: keeps the page and the field distinct under either theme", () => {
    // The page and the field it sits on collapsing into one colour is exactly
    // the "no visual difference between the editor area and its boundaries"
    // this requirement answers, and no rendering test can see it.
    for (const theme of ['\\[data-theme="light"\\]', '\\[data-theme="dark"\\]']) {
      const { page, field } = tones(theme);
      expect(page, `${theme}: ${page}/${field}`).not.toBe(field);
    }
  });

  it("frames the WYSIWYG page with a background, a hairline, an elevation, and an inset", () => {
    const blocks = blocksFor(sheet("kit.css"), ".editor");
    expect(blocks).toHaveLength(1);
    const page = blocks[0];
    expect(decl(page, "background")).toBe("var(--doc-page-bg)");
    expect(decl(page, "border")).toBe("1px solid var(--border-1)");
    expect(decl(page, "box-shadow")).toBe("var(--shadow-2)");
    // Inset from the tab's boundaries on every side, and centred: `0 auto`
    // leaves the page flush against the band above it, which is the defect.
    // EDT-FR-61: the page is centred in the document column. The discussion
    // column is a column of its own, so the page has no reason to move toward
    // one edge.
    expect(decl(page, "margin")).toBe("var(--doc-page-inset) auto");
  });

  it("DDS-FR-TGBX: sizes every page on one measure, and keeps the inset an inset when the tab is narrow", () => {
    // DDS-FR-TGBX is the third thing this rule buys, and the reason it matters
    // most on a draft: the document column of the New Artifact tab is a
    // fraction of the tab, and the author can drag it narrower than the page's
    // own measure. The clamp is what makes the page take the COLUMN's measure
    // there instead of overflowing it — the sheet, its edge, and its recessed
    // field are all preserved, and only the measure yields.
    // Two defects at once, both invisible to a DOM test and both found only by
    // driving the app:
    //
    //  - `margin: … auto` produces a horizontal inset only while there is
    //    width left over. On a tab narrower than the page — the file rail shown
    //    at the minimum window size the shell supports — the sheet runs flush
    //    to both boundaries, and EDT-FR-63's "inset on every side" stops
    //    holding exactly where the tab is most crowded. The `min()` is the fix.
    //
    //  - a page sized without a definite `width` is at the mercy of which axis
    //    its flex parent runs: the raw-text sheet is an item of the tab's
    //    COLUMN, where `margin: … auto` lands on the cross axis and suppresses
    //    the stretch that would otherwise give it its width — it collapsed to
    //    the textarea's intrinsic ~270px with `max-width` never binding.
    const shrink = `min( var(--doc-page-outer), calc(100% - var(--doc-page-inset) * 2) )`;
    const normalise = (v: string | null) => v?.replace(/\s+/g, " ").trim() ?? null;
    const pages: Array<[string, string]> = [
      ["kit.css", ".editor"],
      ["kit.css", ".editor__source-wrap"],
      ["components.css", ".diff-rich--page"],
    ];
    for (const [file, selector] of pages) {
      const block = blocksFor(sheet(file), selector);
      expect(block, `no rule for ${selector}`).toHaveLength(1);
      expect(normalise(decl(block[0], "max-width")), selector).toBe(shrink);
      expect(decl(block[0], "width"), selector).toBe("100%");
      // The measure only means the same thing for all three under one box
      // model — content-box would make each differ from the others by its own
      // padding and border.
      expect(decl(block[0], "box-sizing"), selector).toBe("border-box");
    }
  });

  it("puts the frontmatter region on the page rather than on the field above it", () => {
    // EDT-FR-18 / EDT-FR-63: the YAML block is part of the editing surface. Set
    // at its own width it rendered as a narrower box floating on the field
    // between the band and the sheet, with none of its edges lining up with the
    // page's. It takes the page's box and joins the sheet below it.
    const css = sheet("kit.css");
    const fm = blocksFor(css, ".editor__frontmatter");
    expect(fm).toHaveLength(1);
    expect(decl(fm[0], "width")).toBe("100%");
    expect(decl(fm[0], "max-width")).toContain("--doc-page-outer");
    // The region is centred as the page is (EDT-FR-61), so their edges stay
    // on one line.
    expect(decl(fm[0], "margin")).toBe("var(--doc-page-inset) auto 0 auto");
    // EDT-FR-63: the region's edge is the page's edge on every side that has
    // one — the same hairline at the same weight, and the same soft elevation.
    // The head used to draw a 3px accent rule inside its leading edge with the
    // `box-shadow` the elevation needed, so the joined page had a heavy leading
    // edge against hairlines elsewhere AND no elevation above the seam. Neither
    // is visible to a DOM assertion; both are the first thing a reader sees.
    expect(decl(fm[0], "border")).toBe("1px solid var(--border-1)");
    for (const side of ["border-left", "border-right", "border-top"]) {
      expect(decl(fm[0], side), `${side} weights one edge over the others`)
        .toBeNull();
    }
    expect(decl(fm[0], "box-shadow")).toBe("var(--shadow-2)");
    const page = blocksFor(css, ".editor");
    expect(page, "no single rule carries the page's own box").toHaveLength(1);
    expect(decl(fm[0], "box-shadow")).toBe(decl(page[0], "box-shadow"));
    // …and nothing ELSEWHERE puts the weight back. The assertions above speak
    // for one rule, and the accent bar's likeliest return is not in it: a
    // `::before` overlay, a `border-left-width`, a `border-inline-start`, or a
    // theme-scoped override are all invisible to an exact-selector lookup. So
    // sweep every rule whose selector names the region and require that the one
    // asserted above is the only one that draws an edge or an elevation.
    const drawsAnEdge = /(?:^|;)\s*(?:box-shadow|border-(?:left|right|top|inline-start|inline-end)\b[a-z-]*)\s*:/i;
    for (const m of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
      const selector = m[1].replace(/\s+/g, " ").trim();
      if (!selector.includes("editor__frontmatter")) continue;
      if (selector === ".editor__frontmatter") continue; // asserted above
      expect(
        drawsAnEdge.test(m[2]),
        `${selector} draws an edge of its own — the region's outline is the page's, set in one place`,
      ).toBe(false);
    }
    // Joined: no bottom edge of its own, and square where it meets the sheet.
    expect(decl(fm[0], "border-bottom")).toBe("0");
    expect(decl(fm[0], "border-radius")).toBe(
      "var(--doc-page-radius) var(--doc-page-radius) 0 0",
    );
    // …and the sheet gives up its top inset and top corners in return.
    const joined = blocksFor(css, '.editor-tab[data-frontmatter="on"] .editor');
    expect(joined, "the page never yields its top to the region").toHaveLength(1);
    expect(decl(joined[0], "margin-top")).toBe("0");
    expect(decl(joined[0], "border-top")).toBe("0");
  });

  it("registers the frontmatter region's chrome with the page's gutter", () => {
    // EDT-FR-18 / EDT-FR-63: the page's gutter is shared, so everything the
    // region draws stands on the same two lines the prose it heads does. The
    // label, the collapsed summary bar, the editable YAML and its highlight
    // layer, and the count along the bottom edge all did; the minimize control
    // (EDT-FR-20) took `--sp-2` instead and hung 8px off the trailing edge,
    // leaving the head row ragged against the foot row directly below it.
    const css = sheet("kit.css");
    const gutter = "--doc-page-gutter";
    for (const selector of [
      ".editor__frontmatter-label",
      ".editor__frontmatter--collapsed",
      ".editor__frontmatter-input",
      ".editor__frontmatter-foot",
    ]) {
      // A class may be declared over several rules (the YAML textarea shares
      // one with its highlight layer and carries a second of its own), so the
      // gutter is read from the LAST rule that sets padding at all — the one
      // the cascade leaves standing. `some()` over all of them would accept a
      // later `padding: 0` that wins while an earlier rule still carried it.
      const blocks = blocksFor(css, selector);
      expect(blocks, `no rule for ${selector}`).not.toHaveLength(0);
      const padded = blocks.filter((b) => decl(b, "padding") !== null);
      expect(padded, `${selector} sets no padding at all`).not.toHaveLength(0);
      const last = padded[padded.length - 1];
      expect(decl(last, "padding"), `${selector} is off the page's gutter`)
        .toContain(gutter);
    }
    // …except collapsed, where the bar supplies the gutter itself and the label
    // inside it must NOT add a second one. Pinned because it is the rule that
    // makes the collapsed mode correct, and deleting it double-indents the
    // label to 96px with every assertion above still green.
    const inBar = blocksFor(
      css,
      ".editor__frontmatter--collapsed .editor__frontmatter-label",
    );
    expect(inBar, "the collapsed label re-takes the bar's own gutter").toHaveLength(1);
    expect(decl(inBar[0], "padding")).toBe("0");

    // The control takes the gutter less its own overhang (see kit.css). Pinned
    // exactly rather than by `toContain`, because every way this goes wrong
    // keeps both token names in the value: the wrong sign pushes the glyph
    // further out, the wrong divisor halves the correction, the wrong constant
    // misses the glyph's width. The arithmetic IS the fix.
    const min = blocksFor(css, ".editor__frontmatter-min");
    expect(min).toHaveLength(1);
    expect(decl(min[0], "margin")).toBe(
      "2px calc(var(--doc-page-gutter) - (var(--ctl-h-lg) - 14px) / 2) 0 0",
    );
    // …and that arithmetic is only true under two things it does not own.
    // `--ctl-h-lg` has to be the button's OUTER width — there is no global
    // box-sizing reset in this project, and `.btn` carries a 1px transparent
    // border, so under content-box the overhang is a pixel more than the term.
    expect(decl(min[0], "box-sizing")).toBe("border-box");
    const iconBtn = blocksFor(css, ".btn.btn--icon");
    expect(iconBtn, "the icon button's measure moved").toHaveLength(1);
    expect(decl(iconBtn[0], "width")).toBe("var(--ctl-h-lg)");
    // …and 14px has to be the glyph the JSX actually asks for. A magic number
    // across two files with nothing tying them together is a silent drift.
    const editor = readFileSync(
      resolve(process.cwd(), "src/components/Editor/regions.tsx"),
      "utf8",
    );
    expect(
      editor,
      "the minimize glyph's size no longer matches the CSS that registers it",
    ).toContain("<Icon.Minimize size={14} />");
  });

  it("frames the raw-text surface as the same page, at the same measure", () => {
    // EDT-FR-63: both editing modes render on the same sheet — the raw-text
    // surface is the page the WYSIWYG surface is, not a bare region. The
    // measure has to match too, or a mode toggle moves the text.
    const source = blocksFor(sheet("kit.css"), ".editor__source-wrap");
    expect(source).toHaveLength(1);
    expect(decl(source[0], "background")).toBe("var(--doc-page-bg)");
    expect(decl(source[0], "border")).toBe("1px solid var(--border-1)");
    expect(decl(source[0], "box-shadow")).toBe("var(--shadow-2)");
    expect(decl(source[0], "margin")).toBe(
      "var(--doc-page-inset) auto var(--doc-page-inset) auto",
    );
  });

  it("puts the field behind both modes and the band on it, not on the page", () => {
    const css = sheet("kit.css");
    const tab = blocksFor(css, ".editor-tab");
    expect(tab).toHaveLength(1);
    expect(decl(tab[0], "background")).toBe("var(--doc-field-bg)");
    // EFR-FR-AVGS: the band sits on the field, so the controls over a document
    // are never mistaken for part of it.
    const band = blocksFor(css, ".editor__toolbar");
    expect(band).toHaveLength(1);
    expect(decl(band[0], "background")).toBe("var(--doc-field-bg)");
  });

  it("frames a Diff tab's Rich rendering as the same page over the same field (DFV-FR-38)", () => {
    const css = sheet("components.css");
    const page = blocksFor(css, ".diff-rich--page");
    expect(page).toHaveLength(1);
    expect(decl(page[0], "border")).toBe("1px solid var(--border-1)");
    expect(decl(page[0], "box-shadow")).toBe("var(--shadow-2)");
    expect(decl(page[0], "margin")).toBe("var(--doc-page-inset) auto");
    // The measure and the narrow-tab shrink are asserted for all three pages
    // together, above — the point of that guard is that they agree.

    // The field is declared only for the rendering that has a page to set on
    // it: Source takes no page, because a gutter aligned to a bounded measure
    // would be reading the file as a document it is deliberately not rendering.
    const field = blocksFor(css, '.diff-view__body[data-page="on"]');
    expect(field).toHaveLength(1);
    expect(decl(field[0], "background")).toBe("var(--doc-field-bg)");
    const body = blocksFor(css, ".diff-view__body");
    expect(body).toHaveLength(1);
    expect(decl(body[0], "background")).toBeNull();

    // DFV-FR-38: Side-by-side sets each revision on a page of its own with the
    // field showing between them, in place of the hairline rule the source
    // modes divide their panes with. The rows are laid out cell-by-cell for
    // alignment, so the two pages are drawn BEHIND them as continuous sheets —
    // which is where the requirement actually lives. Asserting only that the
    // row's rule is gone would leave the pair as two bare columns on a field.
    // Folded, because the shared framing is declared once across both halves
    // and each half then names only the side it is offset to.
    // Hung off the ROWS' own wrapper rather than off the scroller: an
    // absolutely positioned child of a scroll container is laid out against
    // that container's padding box — one viewport tall — and scrolls away with
    // the content, so past a screen's worth of scrolling the pages were gone
    // and the document sat on the bare field.
    expect(
      blocksFor(css, ".diff-rich--split .diff-sbs__sheets").join(";"),
    ).toContain("position: relative");
    expect(css).not.toMatch(/\.diff-sbs__scroll::(before|after)/);
    const leftHalf = blocksFor(
      css,
      ".diff-rich--split .diff-sbs__sheets::before",
    ).join(";");
    const rightHalf = blocksFor(
      css,
      ".diff-rich--split .diff-sbs__sheets::after",
    ).join(";");
    expect(leftHalf, "no left-hand page in the split view").not.toBe("");
    expect(rightHalf, "no right-hand page in the split view").not.toBe("");
    for (const half of [leftHalf, rightHalf]) {
      expect(decl(half, "background")).toBe("var(--doc-page-bg)");
      expect(decl(half, "border")).toBe("1px solid var(--border-1)");
      expect(decl(half, "box-shadow")).toBe("var(--shadow-2)");
    }
    // Each offset to its own side, with the field showing between them. The
    // inset itself is the scroller's padding now, so the sheets sit at the
    // wrapper's own edges.
    expect(decl(leftHalf, "left")).toBe("0");
    expect(decl(rightHalf, "right")).toBe("0");

    // The sheets paint BEHIND the rows, and both halves have to say so. At
    // `z-index: auto` the `::after` sheet — the wrapper's last child — painted
    // over every positioned row before it and the whole NEW revision went
    // blank, while `::before` happened to be the first child and so painted
    // under its own rows. `pointer-events: none` hides that from hit-testing
    // but not from painting, which is why no interaction test could see it.
    expect(decl(leftHalf, "z-index")).toBe("0");
    expect(decl(rightHalf, "z-index")).toBe("0");

    const split = blocksFor(css, ".diff-rich--split .diff-sbs__row");
    expect(split).toHaveLength(1);
    expect(decl(split[0], "background")).toBe("none");
    expect(decl(split[0], "gap")).toBe("var(--sp-4)");
    expect(Number(decl(split[0], "z-index"))).toBeGreaterThan(0);
  });
});

describe("the New Artifact tab's collapsed control never covers its page (NAW-FR-27)", () => {
  // The collapsed control is the one piece of chrome the tab always carries. It
  // sits in the bottom-trailing corner of the FIELD, so whether it can reach the
  // page is a question about width: on a window wide enough to leave real field
  // beside the page it cannot, and a strip held open below the page buys nothing
  // — it only makes a draft's page stop short of where an artifact's ends. So
  // the strip is held exactly when the field is too narrow to hold the control
  // beside the page, and not otherwise.
  //
  // What the control OPENS overlays instead: sizing the strip to whichever
  // surface was showing resized the page under the author every time they
  // reached for an action, which is a worse trade than briefly covering the
  // page's bottom-trailing corner with a menu that the next click dismisses.
  it("holds a strip of field open only when the field cannot hold the control", () => {
    const css = sheet("components.css");
    const surface = blocksFor(css, ".draft-editor__surface");
    expect(surface).toHaveLength(1);
    expect(decl(surface[0], "background")).toBe("var(--doc-field-bg)");
    // Every term is a token the control is placed by or the page is sized by, so
    // the strip, the thing it clears, and the page it clears it from cannot
    // drift apart. `(100% - page) / 2` is the field on one side — a percentage
    // in padding resolves against the inline size — and the multiplier turns the
    // comparison into a step.
    const strip = decl(surface[0], "padding-bottom")!.replace(/\s+/g, " ");
    expect(strip).toBe(
      "clamp( 0px, (var(--draft-actions-clear) - (100% - var(--doc-page-box)) / 2) * 1000, calc(var(--draft-actions-size) + var(--sp-4) * 2) )",
    );
    // …and the clearance it steps on is the control's own placement, not a
    // second number that could drift from `.draft-actions__toggle`.
    //
    // ACT-FR-09: the load-bearing half is the positioning context. The control
    // is absolutely positioned, so the element carrying `position: relative` is
    // the box whose corner it keeps — and that box is now the tab's body rather
    // than the document column, which is what makes the corner hold while the
    // discussion column is shown, hidden and resized.
    const body = blocksFor(css, ".draft-workspace__body")[0];
    expect(decl(body, "position")).toBe("relative");
    // The clearance travels with it. Both tokens also stand on `:root`, so this
    // is a statement about where they are stated rather than about what they
    // resolve to.
    expect(decl(body, "--draft-actions-clear")!.replace(/\s+/g, " ")).toBe(
      "calc( var(--sp-4) + var(--draft-actions-size) + var(--sp-2) )",
    );
    expect(decl(body, "--draft-actions-size")).toBe("36px");
    // The document column is a positioning context of its own for what it lays
    // over its page, and it must not become the control's again.
    expect(decl(blocksFor(css, ".draft-editor")[0], "--draft-actions-clear"))
      .toBeNull();
    const toggle = blocksFor(css, ".draft-actions__toggle")[0];
    expect(decl(toggle, "right")).toBe("var(--sp-4)");
    expect(decl(toggle, "width")).toBe("var(--draft-actions-size)");

    // ACT-FR-09: **one control, one strip.** The Editor holds this same strip
    // on `.editor__document`, because on an Editor tab that wrapper is the
    // outermost thing of the document column. A draft tab renders the Editor inside
    // its own surface, so both fire for one control and the page stops two
    // strips above the column's foot — a band of empty field with nothing in it
    // but the control. The surface keeps it and the Editor's stands down.
    const nested = blocksFor(
      sheet("components/the-drafts-tree.css"),
      ".draft-editor .editor__document",
    );
    expect(nested).toHaveLength(1);
    expect(decl(nested[0], "padding-bottom")).toBe("0");

    // ACT-FR-09: the one state that reaches the strip is the column's **foot**
    // — a column ending in a bar of its own holds no strip open below it, the
    // bar being the chrome the foot ends in and the control standing over it.
    // It is admitted by name and pinned to exactly nought, so it cannot grow
    // into a second number beside the clamp above.
    const foot = blocksFor(css, '.draft-editor__surface[data-foot="bar"]');
    expect(foot).toHaveLength(1);
    expect(decl(foot[0], "padding-bottom")).toBe("0");

    // ACT-FR-09 / DDS-FR-TGBX: the other state that reaches it is the
    // discussion column being **shown**, which puts the control's corner over
    // that column rather than over this one. Nothing is then held open here and
    // the page runs to the column's foot, level with an artifact's page in an
    // Editor tab. The clamp above is for the one arrangement left: the
    // discussion hidden, this column holding the tab's whole width.
    const beside = blocksFor(
      css,
      '.dds-split[data-discussion="shown"] .draft-editor__surface',
    );
    expect(beside).toHaveLength(1);
    expect(decl(beside[0], "padding-bottom")).toBe("0");
    // …and the inset the foot then matches is the page's own, at the head and
    // the foot alike. Pinned together, because a `padding-top` added to the
    // surface would break the symmetry with nothing else failing.
    const page = blocksFor(sheet("kit/dashboard-widgets.css"), ".editor")[0];
    expect(decl(page, "margin")?.replace(/\s+/g, " ")).toBe(
      "var(--doc-page-inset) auto",
    );
    expect(decl(surface[0], "padding-top")).toBeNull();

    // Nothing about which surface is **open** may reach the page's height. A
    // rule that resized the surface as a menu or a composer came and went is
    // the behaviour this replaced, and it is still forbidden.
    for (const m of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
      const selector = m[1].trim().replace(/\s+/g, " ");
      if (!selector.includes(".draft-editor__surface")) continue;
      if (selector === ".draft-editor__surface") continue;
      if (selector === '.draft-editor__surface[data-foot="bar"]') continue;
      if (
        selector ===
        '.dds-split[data-discussion="shown"] .draft-editor__surface'
      ) {
        continue;
      }
      expect(decl(m[2], "padding-bottom"), selector).toBeNull();
      expect(decl(m[2], "height"), selector).toBeNull();
    }
  });

  it("gives the page more to scroll while an overlay covers its foot", () => {
    // The overlays DO cover text — the composer reaches ~200px into the
    // measure at a wide window and further at a narrow one. What keeps that
    // from costing anything is this: while one is open the page can be
    // scrolled past its own last line, so the line comes out from under it.
    //
    // It is padding on the SCROLL CONTAINERS, not on the surface that holds
    // the sheet's box — the box must not move, which is the guard above.
    const css = sheet("components.css");
    const scrollers = [
      ".draft-editor[data-overlay=\"on\"] .editor",
      ".draft-editor[data-overlay=\"on\"] .editor__source",
      // EFR-FR-DOQR: the find-highlight layer shares the textarea's box, so it
      // takes the clearance too or glyphs stop registering near the foot.
      ".draft-editor[data-overlay=\"on\"] .editor__source-hl",
    ];
    for (const selector of scrollers) {
      const block = blocksFor(css, selector);
      expect(block, `no scroll clearance for ${selector}`).toHaveLength(1);
      // Taller than the tallest overlay's reach above the page's foot.
      expect(
        Number.parseInt(decl(block[0], "padding-bottom")!, 10),
        selector,
      ).toBeGreaterThan(200);
    }
  });

  it("floats the menu and the composer above the page rather than beside it", () => {
    // They overlay now, so they have to WIN against what they overlay: the
    // Editor's toolbar band is `position: sticky` with a stacking order of its
    // own, and a menu that lost to it would open underneath the chrome.
    const css = sheet("components.css");
    const band = blocksFor(sheet("kit.css"), ".editor__toolbar");
    expect(band).toHaveLength(1);
    const bandLayer = Number(decl(band[0], "z-index"));
    for (const name of [".draft-actions__menu", ".draft-composer", ".draft-actions__toggle"]) {
      const block = blocksFor(css, name);
      expect(block, `no rule for ${name}`).toHaveLength(1);
      expect(Number(decl(block[0], "z-index")), name).toBeGreaterThan(bandLayer);
    }
    // And each is opaque where it meets the document, because there is now a
    // page behind it rather than field. The menu itself is only a column —
    // what has to cover the text is the entries in it.
    for (const name of [".draft-actions__item", ".draft-composer", ".draft-actions__toggle"]) {
      const block = blocksFor(css, name);
      expect(decl(block[0], "background"), name).toBe("var(--bg-elevated)");
    }
  });

  it("anchors the control and its menu to the field's trailing corner", () => {
    const css = sheet("components.css");
    const toggle = blocksFor(css, ".draft-actions__toggle");
    expect(toggle).toHaveLength(1);
    expect(decl(toggle[0], "position")).toBe("absolute");
    expect(decl(toggle[0], "right")).toBe("var(--sp-4)");
    expect(decl(toggle[0], "bottom")).toBe("var(--sp-4)");

    // The menu stacks directly above the control, so it cannot be laid out
    // over it or drift away from its corner. The composer does not — see
    // below — it floats free of the control entirely.
    const above = `calc(var(--sp-4) + var(--draft-actions-size) + var(--sp-2))`;
    const menu = blocksFor(css, ".draft-actions__menu");
    expect(menu, "no rule for .draft-actions__menu").toHaveLength(1);
    expect(decl(menu[0], "position")).toBe("absolute");
    expect(decl(menu[0], "right")).toBe("var(--sp-4)");
    expect(decl(menu[0], "bottom")).toBe(above);
  });

  it("ACT-FR-13: floats the opening composer free of the control's corner, horizontally centred and low in the window", () => {
    // `position: fixed` against the viewport rather than `absolute` against
    // the tab, so it holds this position through a resize regardless of
    // which host mounted it, escaping any local stacking context the tab
    // holds.
    const css = sheet("components.css");
    const block = blocksFor(css, ".draft-composer");
    expect(block, "no rule for .draft-composer").toHaveLength(1);
    expect(decl(block[0], "position")).toBe("fixed");
    expect(decl(block[0], "left")).toBe("50%");
    // Its lower quarter rather than dead centre — clear of a page's own
    // content while still reading as detached from the control.
    expect(decl(block[0], "top")).toBe("75%");
    expect(decl(block[0], "transform")).toBe("translate(-50%, -50%)");
    // The same default width the detached overlay it becomes opens at
    // (the former overlay default) — the width
    // follows the overlay; the vertical position does not.
    expect(decl(block[0], "width")).toBe("max(360px, 50vw)");
  });

  it("ACT-FR-13: the space around the composer's field is the surface's padding and no more", () => {
    // Invisible to a component test — with `css: false` jsdom lays every box
    // out at zero size — so the stylesheet is the only place this can be held.
    // `.comment-card__reply` carries a top margin for the case it was written
    // for (a reply footer under the comments above it, `kit.css`), and here,
    // where it is the only thing in the box, that margin lands on top of
    // `.draft-composer`'s own padding and the field sits low in a box taller
    // above it than below.
    const css = sheet("components.css");
    const block = blocksFor(css, ".draft-composer > .comment-card__reply:first-child");
    expect(
      block,
      "no rule zeroing the reply footer's top margin in the composer",
    ).toHaveLength(1);
    expect(decl(block[0], "margin-top")).toBe("0");
    // `:first-child` and not the bare child selector: with the disabled-state
    // explanation above it (ACT-FR-17) that margin is the gap between the two
    // and has to stay.
    expect(blocksFor(css, ".draft-composer > .comment-card__reply")).toHaveLength(0);
  });
});

describe("the attach surface stays inside the discussion column (CMT-FR-45 / CMT-FR-01)", () => {
  // Every property below was found by driving the app in a browser, and none of
  // them is visible to a component test: with `css: false`, jsdom renders the
  // menu at zero size wherever it is anchored, so a rule that slices 39% of the
  // surface off passes the whole Vitest suite.
  //
  // The discussion column scrolls within its own bounds and the detached
  // overlay is a fixed box, so the surface must hang from whichever of the control's edges has the
  // composer behind it. A control ending a control row
  // (`.draft-composer__actions`, `justify-content: flex-end`) opens leftward;
  // one leading the inline field (CMT-FR-45) opens rightward. Get it backwards
  // and the container slices the entries off — which is exactly what the
  // inline field did while the anchor was still trailing-only.
  //
  // It must also not be wider than the menu, because overflow past a *leading*
  // edge contributes nothing to `scrollWidth` and is unreachable by scrolling,
  // panning, or any other means.

  it("anchors a trailing control's surface leftward, into the row", () => {
    const css = sheet("kit.css");
    const menu = blocksFor(css, ".comment-attach__menu");
    expect(menu, "no rule for .comment-attach__menu").toHaveLength(1);
    expect(decl(menu[0], "right")).toBe("0");
    expect(
      decl(menu[0], "left"),
      "opening rightward from a trailing control runs the menu out of the column",
    ).toBeNull();
  });

  it("anchors a leading control's surface rightward, across the field", () => {
    const css = sheet("kit.css");
    const leading = blocksFor(
      css,
      '.comment-attach[data-align="leading"] .comment-attach__menu',
    );
    expect(
      leading,
      "no leading-edge anchor: the inline field's menu opens out of the surface",
    ).toHaveLength(1);
    expect(decl(leading[0], "left")).toBe("0");
    expect(
      decl(leading[0], "right"),
      "the trailing anchor must be released, or both edges pin and the menu stretches",
    ).toBe("auto");
  });

  it("opens upward, since the control sits at the card's foot", () => {
    const css = sheet("kit.css");
    const menu = blocksFor(css, ".comment-attach__menu")[0];
    expect(decl(menu, "bottom")).toContain("100%");
    expect(decl(menu, "top")).toBeNull();
  });

  it("floats the composer clear of the foot of whatever surface holds it", () => {
    // CVP-FR-SDMQ. As a flush band this ran into the window's own bottom edge in
    // a tab and read as a field sliced off by the bottom of the screen. It
    // floats on every surface now, so the overlay and the tab say the same
    // thing about the same object. jsdom applies no CSS, so only the stylesheet
    // itself can hold this.
    const css = sheet("kit.css");
    const foot = blocksFor(css, ".comment-card--embedded > .comment-card__reply");
    expect(foot, "no footer rule for an embedded conversation").toHaveLength(1);
    expect(decl(foot[0], "box-shadow"), "nothing says it floats").not.toBeNull();
    expect(decl(foot[0], "border-radius")).not.toBeNull();
    expect(
      decl(foot[0], "border-top"),
      "a rule across the surface is the band this replaced",
    ).toBeNull();
    // The margin is what holds it off the edge; without it the shadow just
    // decorates a band that is still cut by the window.
    const margin = decl(foot[0], "margin");
    expect(margin, "no margin holding it off the surface's edge").not.toBeNull();
    expect(margin).toContain("var(--sp-3)");

    // And no surface takes it back to a band of its own.
    for (const scoped of [
      ".conversation-tab .comment-card--embedded > .comment-card__reply",
      ".conversation-overlay .comment-card--embedded > .comment-card__reply",
    ]) {
      expect(
        blocksFor(css, scoped),
        `${scoped} gives one surface a footer of its own`,
      ).toHaveLength(0);
    }
  });

  it("leaves the field itself alone in every mode", () => {
    // CVP-FR-55: the footer around it may differ by surface; the field may not,
    // or a composer stops being one thing the author learns once.
    const css = sheet("kit.css");
    for (const scoped of [
      ".conversation-tab .comment-composer",
      ".conversation-overlay .comment-composer",
      ".dds-discussion .comment-composer",
    ]) {
      expect(
        blocksFor(css, scoped),
        `${scoped} gives the field a look of its own`,
      ).toHaveLength(0);
    }
  });

  it("gives the link form no width of its own", () => {
    const css = sheet("kit.css");
    // A `min-width` on the form overflows the leading edge — the same clipping
    // bug the trailing anchor fixed, just mirrored. The form inherits the
    // menu's width and its controls shrink to fit.
    for (const block of blocksFor(css, ".comment-attach__link")) {
      expect(decl(block, "min-width")).toBeNull();
      expect(decl(block, "width")).toBeNull();
    }
    const children = blocksFor(css, ".comment-attach__link > *");
    expect(children, "the form's controls must be told to shrink").toHaveLength(1);
    expect(decl(children[0], "min-width")).toBe("0");
    expect(decl(children[0], "max-width")).toBe("100%");
  });

  it("bounds what attachments can add to a card's height", () => {
    const css = sheet("kit.css");
    // An unbounded list pushes the card's own composer below the foot of the
    // surface that holds it, where it cannot be clicked at all.
    const list = blocksFor(css, ".comment__attachments");
    expect(list, "no rule for .comment__attachments").toHaveLength(1);
    expect(decl(list[0], "max-height")).not.toBeNull();
    expect(decl(list[0], "overflow-y")).toBe("auto");

    const thumb = blocksFor(css, ".comment-attach__thumb img");
    expect(thumb, "no rule for .comment-attach__thumb img").toHaveLength(1);
    expect(decl(thumb[0], "max-width")).toBe("100%");
    expect(decl(thumb[0], "max-height")).not.toBeNull();
  });

  it("keeps the panel row's two counts apart (CMP-FR-25)", () => {
    const css = sheet("kit.css");
    // Without this the row reads `1 reply3` — the counts run together with no
    // spacing, and the attachment count is nowhere near the trailing edge the
    // layout note puts it at.
    const replies = blocksFor(css, ".comment-row__replies");
    expect(replies, "no rule for .comment-row__replies").toHaveLength(1);
    expect(decl(replies[0], "display")).toBe("flex");
    expect(decl(replies[0], "justify-content")).toBe("space-between");

    const marker = blocksFor(css, ".comment-row__attachments");
    expect(marker, "no rule for .comment-row__attachments").toHaveLength(1);
    expect(decl(marker[0], "margin-left")).toBe("auto");
  });
});

describe("the Editor tab's discussion column stands beside the document column (EDT-FR-61)", () => {
  // The grid is the draft tab's `.dds-split`, and the document column is the
  // Editor's own. With `css: false` a component test sees the same DOM whether
  // the column can shrink or not, and whether the control's strip is held
  // open below the page or not. These rules are what the eye sees.

  it("EDT-FR-61, DDS-FR-KTVW: the document column is a flex column that can shrink to nothing", () => {
    const column = blocksFor(sheet("kit.css"), ".dds-split > .editor__column");
    expect(column, "no rule lays out the Editor's document column").toHaveLength(1);
    // A grid item's automatic minimum is its content. A long source line would
    // then push the discussion column off the tab.
    expect(decl(column[0], "min-width")).toBe("0");
    expect(decl(column[0], "min-height")).toBe("0");
    // The frontmatter region, the page, and the review bar stack in it.
    expect(decl(column[0], "display")).toBe("flex");
    expect(decl(column[0], "flex-direction")).toBe("column");

    // The page wrapper takes what the column has left, and also shrinks.
    const documentBlocks = blocksFor(sheet("kit.css"), ".editor__document");
    const layout = documentBlocks.find((b) => decl(b, "display") !== null);
    expect(layout, "no rule lays out the page wrapper").toBeDefined();
    expect(decl(layout!, "flex")).toBe("1");
    expect(decl(layout!, "min-width")).toBe("0");
    expect(decl(layout!, "min-height")).toBe("0");
  });

  it("ACT-FR-09: the page wrapper holds the control's strip, and stands it down while the column shows", () => {
    // The same step the New Artifact tab holds on its surface, with the same
    // tokens, so the strip and the control it clears cannot drift apart.
    const kit = sheet("kit.css");
    const strips = blocksFor(kit, ".editor__document")
      .map((b) => decl(b, "padding-bottom"))
      .filter((v): v is string => v !== null);
    expect(strips, "the page wrapper holds no strip, or holds two").toHaveLength(1);
    const draftStrip = decl(
      blocksFor(sheet("components.css"), ".draft-editor__surface")[0],
      "padding-bottom",
    );
    expect(strips[0].replace(/\s+/g, " ")).toBe(draftStrip!.replace(/\s+/g, " "));

    // DDS-FR-TGBX: with the discussion column shown, the control's corner falls
    // over that column, so nothing is held open below the page.
    const shown = blocksFor(
      kit,
      '.editor__split[data-discussion="shown"] .editor__document',
    );
    expect(shown).toHaveLength(1);
    expect(decl(shown[0], "padding-bottom")).toBe("0");

    // One control, one strip: a draft's embedded Editor leaves the strip to
    // the draft's own surface.
    const nested = blocksFor(
      sheet("components/the-drafts-tree.css"),
      ".draft-editor .editor__document",
    );
    expect(nested).toHaveLength(1);
    expect(decl(nested[0], "padding-bottom")).toBe("0");
  });

  it("EDT-FR-61: no rule of the retired margin layout stays in the stylesheets", () => {
    // The page no longer moves toward one edge for a margin of cards, and no
    // card is placed beside or below it. A rule that comes back for any of
    // these lays out a region the tab no longer renders.
    const all = ["colors_and_type.css", "components.css", "kit.css"]
      .map((name) => sheet(name))
      .join("\n");
    for (const retired of [
      "--page-lead",
      "--comment-card-min-w",
      "--comment-margin-need",
      "--comment-card-floor",
      ".editor__with-rail",
      ".editor__comments-toggle",
      ".comment-rail__aligned",
      ".comment-rail__unaligned",
      ".comment-rail__pinned-head",
      "data-comments=",
    ]) {
      expect(all.includes(retired), `${retired} is still in the stylesheets`).toBe(false);
    }
    // The container itself, but not the classes the column still borrows
    // (`__blocked`, `__route`, `__section`, `__disclosure`).
    expect(all).not.toMatch(/\.comment-rail(?![_\w-])/);
  });
});
