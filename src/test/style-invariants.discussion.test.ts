import { describe, expect, it } from "vitest";
import { blocksFor, decl, sheet } from "./cssRules";

/**
 * Stylesheet invariants of the draft discussion surface that no rendering test
 * can see (`../../specifications/ui/DDS-draft-discussion.md`,
 * `../../specifications/ui/DCR-draft-change-review.md`).
 *
 * One part of the group `./cssRules.ts` heads, which carries the whole rule
 * these are written under. Everything here is a fact about the stylesheet that
 * jsdom reports nothing about — it computes no layout and no cascade — and each
 * one is load-bearing: revert it and every component assertion still passes
 * while the surface is wrong on screen.
 */

const style = () => sheet("components/draft-discussion.css");

describe("what a proposed change looks like in the prose (DCR-FR-07)", () => {
  it("marks an insertion, a deletion, and a held change in their own tokens", () => {
    // DCR-FR-07 names the three fills and the three rails outright, because the
    // author reads the kind of a change off them before they read a word of it.
    const add = blocksFor(style(), ".hunk--add");
    expect(add, "no rule for .hunk--add").toHaveLength(1);
    expect(decl(add[0], "background")).toContain("--diff-add-bg");
    expect(decl(add[0], "border-left")).toContain("--ok");

    const del = blocksFor(style(), ".hunk--del");
    expect(del, "no rule for .hunk--del").toHaveLength(1);
    expect(decl(del[0], "background")).toContain("--diff-del-bg");
    expect(decl(del[0], "box-shadow")).toContain("--danger");

    const held = blocksFor(style(), ".hunk--discussing");
    expect(held, "no rule for .hunk--discussing").toHaveLength(1);
    expect(decl(held[0], "background")).toContain("--warn-soft");
    expect(decl(held[0], "border-left-color")).toContain("--warn");
  });

  it("DCR-FR-28: carries a removal by more than its colour", () => {
    // A deletion whose only mark is a fill is a deletion an author who cannot
    // tell the two fills apart reads as an ordinary paragraph — and accepts.
    const del = blocksFor(style(), ".hunk--del");
    expect(decl(del[0], "text-decoration")).toContain("line-through");
  });

  it("DCR-FR-31: draws a lost change back rather than re-tinting it", () => {
    const lost = blocksFor(style(), ".hunk--lost");
    expect(lost, "no rule for .hunk--lost").toHaveLength(1);
    expect(decl(lost[0], "opacity")).toBeTruthy();
  });
});

describe("the action chip covers no line of the document (DCR-FR-12)", () => {
  // The chip is placed over padding the change holds open for it. jsdom
  // measures nothing, so the only checkable half of "it never overlaps document
  // text" is that the space exists and is at least as tall as the chip — and
  // this is exactly the rule that would be lost to a tidy-up of the padding
  // shorthand, with nothing else failing.
  const CHIP_H = 34;

  it("reserves the chip's height above an insertion", () => {
    const add = blocksFor(style(), ".hunk--add");
    const padding = decl(add[0], "padding") ?? "";
    const top = Number.parseInt(padding.split(/\s+/)[0], 10);
    expect(Number.isFinite(top)).toBe(true);
    expect(top).toBeGreaterThanOrEqual(CHIP_H);
  });

  it("DCR-FR-07: reserves it in a block below a deletion, not in the struck text", () => {
    // A deletion is an INLINE decoration over the document's own words, and an
    // inline box grows for no vertical padding it declares — so padding on it
    // reserves nothing and the chip lands on the line below. The space is a
    // block of the change's own instead, and this is the rule that says so.
    const del = blocksFor(style(), ".hunk--del")[0];
    expect(decl(del, "padding-bottom")).toBeNull();
    const foot = blocksFor(style(), ".hunk--del-foot")[0];
    expect(foot, "no rule for .hunk--del-foot").toBeDefined();
    const height = Number.parseInt(decl(foot, "height") ?? "", 10);
    expect(height).toBeGreaterThanOrEqual(CHIP_H);
  });

  it("DCR-FR-09: the proposed text states its own typography rather than inheriting it", () => {
    // The block stands between the document's blocks. Beside a heading it would
    // otherwise read at the heading's size and weight, which is the defect this
    // states away: every property a heading rule sets is set here too.
    const add = blocksFor(style(), ".hunk--add")[0];
    expect(decl(add, "font-family")).toBe("var(--font-rich-family)");
    expect(decl(add, "font-size")).toBe("var(--fs-doc-md)");
    expect(decl(add, "font-weight")).toBe("var(--fw-regular)");
    expect(decl(add, "line-height")).toBe("var(--font-rich-line-height)");
    expect(decl(add, "letter-spacing")).toBe("normal");
    expect(decl(add, "text-decoration")).toBe("none");
  });

  it("DCR-FR-09: the proposed text's blank lines are its own surface's to lay out", () => {
    // The text is rendered Markdown inside the block. Whitespace preserved on
    // the block would draw every line break the serialiser keeps a second time.
    expect(decl(blocksFor(style(), ".hunk--add")[0], "white-space")).toBeNull();
  });

  it("DCR-FR-09, DCR-FR-12: the proposed text's surface adds no frame and no outer margin", () => {
    // Its first and last blocks are headings and paragraphs with `.doc`
    // margins. Kept, they would push the text down into nothing the chip needs
    // and leave a band at the foot of the change.
    const surface = blocksFor(style(), ".hunk__doc")[0];
    expect(surface, "no rule for .hunk__doc").toBeDefined();
    expect(decl(surface, "outline")).toBe("none");
    expect(decl(surface, "padding")).toBe("0");
    expect(decl(blocksFor(style(), ".hunk__doc > :first-child")[0], "margin-top")).toBe("0");
    expect(decl(blocksFor(style(), ".hunk__doc > :last-child")[0], "margin-bottom")).toBe("0");
  });

  it("DCR-FR-09: a proposed text shown as written keeps its line breaks", () => {
    expect(decl(blocksFor(style(), ".hunk__plain")[0], "white-space")).toBe("pre-wrap");
  });

  it("DCR-FR-12: the chip is one line however narrow the page is", () => {
    // The space a change holds open is the chip's own height. A chip that
    // wrapped to two lines would reach past it onto the first line of the text
    // it is about, which is exactly what the reserved space exists to prevent.
    expect(decl(blocksFor(style(), ".dds-chip")[0], "white-space")).toBe("nowrap");
  });

  it("is placed within the scroller rather than the window", () => {
    // `absolute` against the document's own scroller is what makes the chip
    // travel with the text it is about. Against the viewport it would hang in
    // one place while the author scrolled the change out from under it.
    const chip = blocksFor(style(), ".dds-chip");
    expect(chip, "no rule for .dds-chip").toHaveLength(1);
    expect(decl(chip[0], "position")).toBe("absolute");
    const scroller = blocksFor(sheet("kit.css"), ".editor");
    expect(decl(scroller[0], "position")).toBe("relative");
  });
});

describe("each column is its own scroll region (DDS-FR-KTVW)", () => {
  it("keeps the discussion's composer out of its scroller", () => {
    // `min-height: 0` is the whole of it: a flex item's automatic minimum is
    // its content, so without this the transcript refuses to shrink and pushes
    // the composer off the foot of the column the moment a conversation is
    // longer than the window. There is no layout here to measure — `auto`
    // instead of `0` IS the defect.
    const column = blocksFor(style(), ".dds-discussion");
    expect(column, "no rule for .dds-discussion").toHaveLength(1);
    expect(decl(column[0], "min-height")).toBe("0");
    expect(decl(column[0], "flex-direction")).toBe("column");

    const body = blocksFor(style(), ".dds-discussion__body");
    expect(decl(body[0], "min-height")).toBe("0");
    // The jump control hangs over the foot of the scroller, so the body has to
    // be what it is positioned against.
    expect(decl(body[0], "position")).toBe("relative");
  });

  it("DDS-FR-PNXR: lays the two columns out in fractions rather than pixels", () => {
    // Pixels would keep one column's width and give every change of window size
    // to the other, so the split the author chose would not survive a resize.
    const split = blocksFor(style(), ".dds-split");
    expect(split, "no rule for .dds-split").toHaveLength(1);
    expect(decl(split[0], "display")).toBe("grid");
    expect(decl(split[0], "min-width")).toBe("0");
    expect(decl(split[0], "min-height")).toBe("0");
  });

  it("DDS-FR-KTVW: the grid fills the tab and paints no ground of its own", () => {
    // Both halves of one rule. A band inside the tab leaves the discussion
    // column short of the trailing edge every other panel of the shell meets,
    // and puts the tab's action row out of line with the columns beneath it. A
    // background of its own puts a second ground under the page, so the
    // document column reads differently here than in every other tab — and both
    // are invisible to jsdom, which computes no cascade and no layout.
    const split = blocksFor(style(), ".dds-split");
    expect(decl(split[0], "padding")).toBeNull();
    expect(decl(split[0], "background")).toBeNull();
    expect(decl(split[0], "background-color")).toBeNull();
  });

  it("DDS-FR-KTVW: the split is never taller than the tab it is in", () => {
    // A grid row defaults to `auto`, which is the height of its tallest child.
    // With a column longer than the window that makes the grid taller than the
    // tab, the workspace grows past its own bounds, and the tab's action row —
    // the ratio control, the proposal marker, the draft's name — is pushed off
    // the top of the window. It happens at 1280×800, which is the minimum size
    // the shell supports, and it is invisible to jsdom and to a tall window
    // alike: `auto` instead of `minmax(0, 1fr)` IS the defect.
    const split = blocksFor(style(), ".dds-split");
    expect(decl(split[0], "grid-template-rows")).toBe("minmax(0, 1fr)");
    // And the other half of it: a grid item's automatic minimum is its content,
    // so each column needs a floor of its own.
    const doc = blocksFor(style(), ".dds-split > .draft-editor");
    expect(doc, "no floor for the document column").toHaveLength(1);
    expect(decl(doc[0], "min-height")).toBe("0");
    const chat = blocksFor(style(), ".dds-discussion");
    expect(decl(chat[0], "min-height")).toBe("0");
  });

  it("DDS-FR-ZMXQ: the card fills the column and the content column holds the measure", () => {
    // The measure moved off the card and onto the content column inside it. A
    // max-width here would inset the scroller too, and the stream's own gutters
    // would then be taken twice — which is invisible in jsdom and obvious on
    // screen.
    const body = blocksFor(style(), ".dds-discussion__body > .comment-card--stream");
    expect(body, "no rule for the stream card in the column").toHaveLength(1);
    expect(decl(body[0], "width")).toBe("100%");
    expect(decl(body[0], "max-width")).toBeNull();
  });

  it("DDS-FR-ZMXQ: the content column is one centred rule and no second layout", () => {
    // `min()` is the whole of it: 820 while the panel can hold that plus its
    // gutters, the panel less those gutters below it, centred either way. A
    // media query here would read the window rather than the panel, which the
    // splitter sizes.
    const stream = blocksFor(sheet("kit/discussion-stream.css"), ".dds-stream")[0];
    expect(stream, "no rule for .dds-stream").toBeDefined();
    expect(decl(stream, "justify-content")).toBe("center");

    const column = blocksFor(
      sheet("kit/discussion-stream.css"),
      ".dds-stream__column",
    )[0];
    const width = decl(column, "width") ?? "";
    expect(width).toContain("min(");
    expect(width).toContain("--doc-page-width");
    expect(width).toContain("100%");
  });
});

describe("the gutter map rides the scroller's edge (DCR-FR-BQNL)", () => {
  it("marks each kind in its own token and draws distance back", () => {
    const track = blocksFor(style(), ".dds-gutter");
    expect(track, "no rule for .dds-gutter").toHaveLength(1);
    expect(decl(track[0], "position")).toBe("absolute");
    // The track itself takes no pointer, or it would swallow every click on the
    // last few characters of every line beside it.
    expect(decl(track[0], "pointer-events")).toBe("none");

    for (const [kind, token] of [
      ["add", "--ok"],
      ["del", "--danger"],
      ["replace", "--accent"],
    ]) {
      const mark = blocksFor(style(), `.dds-gutter__mark[data-kind="${kind}"]`);
      expect(mark, `no rule for a ${kind} mark`).toHaveLength(1);
      expect(decl(mark[0], "background")).toContain(token);
    }

    // DCR-FR-28: the change under review is marked by size as well as by
    // opacity, so it is findable without colour discrimination.
    const focused = blocksFor(style(), '.dds-gutter__mark[data-focused="true"]');
    expect(focused, "no rule for the focused mark").toHaveLength(1);
    expect(decl(focused[0], "height")).toBeTruthy();
  });
});

describe("the review bar at the foot of the document column (DCR-FR-11, ACT-FR-09)", () => {
  const css = sheet("components/draft-discussion.css");

  it("DCR-FR-11: the bar is stuck to the foot, not the head", () => {
    // At the head it stood between the formatting toolbar and the first line of
    // the prompt; at the foot it sits where the hand already is.
    const bar = blocksFor(css, ".dds-review")[0];
    expect(decl(bar, "position")).toBe("sticky");
    expect(decl(bar, "bottom")).toBe("0");
    expect(decl(bar, "top")).toBeNull();
    // The rule that divides it from the scroller is above it, not below.
    expect(decl(bar, "border-top")).toBe("1px solid var(--border-1)");
    expect(decl(bar, "border-bottom")).toBeNull();
  });

  it("ACT-FR-09: the bar holds its trailing end clear of the control that stands over it", () => {
    // The control's corner is the tab's, so it reaches the bar only while the
    // discussion column is hidden and this column holds the whole tab. The
    // bar's own whole-proposal actions are at that end, and a control the
    // author cannot reach is worse than a band of empty field — so the
    // clearance is the measure the control is placed by rather than a second
    // number that could drift from it.
    const row = blocksFor(
      css,
      '.dds-split[data-discussion="hidden"] .dds-review__row',
    )[0];
    expect(row).toBeDefined();
    expect(decl(row, "padding-right")).toBe(
      "var(--draft-actions-clear, var(--sp-6))",
    );
    // With the column shown the corner is over the composer instead, and the
    // bar takes the whole width of its own column back.
    expect(
      blocksFor(css, '.draft-editor .dds-review__row'),
      "the bar still reserves the clearance whatever the split holds",
    ).toHaveLength(0);
  });

  it("DCR-FR-11: the bar stays inside its own column at the narrowest window", () => {
    // A flex row that cannot wrap cannot compress either: at the minimum window
    // size, with the split even, the whole-proposal actions ran past the
    // splitter and painted over the discussion column. The row wraps, and the
    // counter is the one part of it that may be cut short — the rest are
    // controls the author has to reach.
    const row = blocksFor(css, ".dds-review__row")[0];
    expect(decl(row, "flex-wrap")).toBe("wrap");
    const state = blocksFor(css, ".dds-review__state")[0];
    expect(decl(state, "min-width")).toBe("0");
    expect(decl(state, "text-overflow")).toBe("ellipsis");
    // DCR-FR-CXZG: and the row wraps before the two whole-proposal actions
    // rather than between them — split across two lines they read in the wrong
    // order, the primary action leading a row while the other keeps the
    // trailing end of the row above it.
    const all = blocksFor(css, ".dds-review__all")[0];
    expect(all, "the two whole-proposal actions are not one group").toBeDefined();
    expect(decl(all, "display")).toBe("flex");
    expect(decl(all, "margin-left")).toBe("auto");
  });

  it("ACT-FR-09, DDS-FR-QPHL: the discussion composer clears the control that stands over it", () => {
    // With the discussion column shown the control's corner falls on that
    // column, where Post is the trailing control. The clearance comes from the
    // same token, less the gutter the control already keeps from the frame.
    const reply = blocksFor(
      css,
      '.dds-split[data-discussion="shown"] .dds-discussion .comment-card__reply',
    )[0];
    expect(reply, "the composer reserves nothing for the control").toBeDefined();
    expect(decl(reply, "padding-right")?.replace(/\s+/g, " ")).toBe(
      "calc( var(--draft-actions-clear, var(--sp-6)) - var(--sp-4) )",
    );
  });

  it("DDS-FR-QPHL: the composer's foot stands on the control's own line, in either state the column has", () => {
    // The control keeps `--sp-4` from the tab's foot, so the composer keeps the
    // same from the column's. The column stands the composer in two different
    // boxes — the reply of the card at the foot of a transcript, and the
    // empty-column box where nothing has been said yet — and the measure has to
    // come out the same from either, or the two states disagree about where the
    // foot of the column is.
    const scale = sheet("colors_and_type.css");
    const root = blocksFor(scale, ":root")[0];
    const px = (token: string) => Number.parseInt(decl(root, token) ?? "", 10);

    const card = blocksFor(
      css,
      '.dds-split[data-discussion="shown"] .dds-discussion .comment-card--embedded > .comment-card__reply',
    )[0];
    expect(card, "the card's reply reserves no foot").toBeDefined();
    expect(decl(card, "padding-bottom")).toBe("var(--sp-2)");
    expect(decl(card, "margin-bottom")).toBe("var(--sp-2)");

    const empty = blocksFor(
      css,
      '.dds-split[data-discussion="shown"] .dds-discussion .dds-open',
    )[0];
    expect(empty, "the empty column reserves no foot").toBeDefined();
    expect(decl(empty, "padding-bottom")).toBe("var(--sp-4)");

    // Both add to THE CONTROL'S OWN gutter, read from the control's own rule.
    // Against a hard-coded `--sp-4` this case would stay green while someone
    // moved the control and left the composer where it was, which is exactly
    // the misalignment the requirement is about.
    const toggle = blocksFor(
      sheet("components/the-drafts-tree.css"),
      ".draft-actions__toggle",
    )[0];
    const gutter = decl(toggle, "bottom");
    expect(gutter, "the control states no gutter to align on").toBeDefined();
    expect(px("--sp-2") + px("--sp-2")).toBe(px(gutter!.replace(/var\(|\)/g, "")));
    expect(px(gutter!.replace(/var\(|\)/g, ""))).toBe(px("--sp-4"));
  });
});

describe("the discussion column with nothing in it yet (DDS-FR-ZMXQ, DDS-FR-XQMF)", () => {
  const css = sheet("components/draft-discussion.css");

  it("DDS-FR-ZMXQ: the empty column stands on the measure a message stands on", () => {
    // What stands in the empty column is where the first message will come, so
    // it takes the measure a message takes and is centred on it. Left to run
    // the column's full width it reads as a paragraph of the interface rather
    // than as the place the conversation begins.
    const open = blocksFor(css, ".dds-open")[0];
    expect(open).toBeDefined();
    const column = blocksFor(
      sheet("kit/discussion-stream.css"),
      ".dds-stream__column",
    )[0];
    expect(decl(open, "width")).toBe(decl(column, "width"));
    expect(decl(open, "margin-inline")).toBe("auto");

    const statement = blocksFor(css, ".dds-open__statement")[0];
    expect(decl(statement, "text-align")).toBe("center");
  });

  it("DDS layout: the composer keeps the foot and the statement is centred above it", () => {
    // Two states, one declaration each. The auto margins take the free space
    // before `justify-content` sees it, so the statement is centred while it
    // stands and the composer is at the foot whether it stands or not.
    const open = blocksFor(css, ".dds-open")[0];
    expect(decl(open, "justify-content")).toBe("flex-end");
    const statement = blocksFor(css, ".dds-open__statement")[0];
    expect(decl(statement, "margin")).toBe("auto 0");
  });

  it("DDS-FR-XQMF: a hidden column and a hidden splitter leave the layout", () => {
    // Each of them declares a `display` of its own, and the browser's own rule
    // for the attribute is the weaker of the two — so the attribute alone would
    // hide neither.
    const hidden = blocksFor(css, ".dds-discussion[hidden]")[0];
    expect(hidden).toBeDefined();
    expect(decl(hidden, "display")).toBe("none");
    expect(blocksFor(css, ".dds-splitter[hidden]")[0]).toBeDefined();
  });
});
