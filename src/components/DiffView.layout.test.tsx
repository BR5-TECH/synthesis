import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  screen,
  within,
} from "@testing-library/react";
import { readFileSync } from "node:fs";

import { resetDiffModes } from "../state/diffModes";
import { resetAppPreferencesCache } from "../state/appPreferences";
import {
  makeWireBackend,
  target,
} from "../test/diffViewFixtures";
import { readStylesheet } from "../test/readStylesheet";

const invokeMock = vi.fn();
const unlistenMock = vi.fn();
let listeners: Record<string, (event: { payload: unknown }) => void> = {};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, cb: (event: { payload: unknown }) => void) => {
    listeners[event] = cb;
    return unlistenMock;
  }),
}));

const wireBackend = makeWireBackend(invokeMock);

const callsTo = (name: string) =>
  invokeMock.mock.calls.filter((c) => c[0] === name);

beforeEach(() => {
  invokeMock.mockReset();
  unlistenMock.mockReset();
  listeners = {};
  resetDiffModes();
  resetAppPreferencesCache();
  wireBackend();
});

afterEach(cleanup);

/**
 * Soft wrap, the centred toolbar, and the Editor-matched rich page are all
 * stylesheet behaviour, and `vitest.config.ts` sets `css: false` — jsdom loads
 * no stylesheet, so `getComputedStyle` sees nothing. These read the real CSS
 * and assert the rules directly, the way `ProjectPicker.test.tsx` does for the
 * picker's scroll regions. A rendering test alone could not fail if the rules
 * regressed.
 */
describe("Diff viewer layout (asserted against the stylesheet)", () => {
  const components = readStylesheet("components.css");
  const tokens = readStylesheet("colors_and_type.css");
  const kit = readStylesheet("kit.css");

  /**
   * The declarations of the rule whose selector list holds `selector` as a
   * whole member.
   *
   * Matched member-by-member rather than as a substring: a substring match finds
   * `.diff-toolbar` inside `.some-borrower .diff-toolbar` too, and would then
   * assert a borrower's override against a requirement about the toolbar itself
   * — passing or failing for a rule nobody asked about, whichever happened to
   * appear first in the file.
   */
  const memberList = (selectors: string) =>
    selectors
      .replace(/\/\*[\s\S]*?\*\//g, "")
      .split(",")
      .map((one) => one.trim().replace(/\s+/g, " "))
      .filter((one) => one.length > 0);

  const rule = (css: string, selector: string) => {
    // A caller may name one member or a whole comma list; either way every
    // member it names has to be on the rule it matches.
    const wanted = memberList(selector.replace(/\\/g, ""));
    let found: string | null = null;
    for (const block of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
      const members = memberList(block[1]);
      if (wanted.every((one) => members.includes(one))) {
        found = block[2];
        break;
      }
    }
    expect(found, `expected \`${selector} { … }\` in the stylesheet`).not.toBeNull();
    return found!;
  };

  it("marks a run's blocks without colour and without content (DFV-FR-41 / DFV-FR-47)", () => {
    // The editable rich target has no wrapper per block to hang a marker span
    // off — a wrapper inside the document would be content, and content is what
    // the serialiser writes to disk. So the sign character is drawn from the
    // decoration's own attribute, and the tint is the same pair the static
    // blocks take.
    expect(rule(components, '.diff-run \[data-glyph\]::before')).toMatch(
      /content\s*:\s*attr\(data-glyph\)/,
    );
    expect(rule(components, '.diff-run \[data-mark="added"\]')).toMatch(
      /background\s*:\s*var\(--diff-add-bg\)/,
    );
    expect(rule(components, '.diff-run \[data-mark="removed"\]')).toMatch(
      /background\s*:\s*var\(--diff-del-bg\)/,
    );
    // The rich target carries no chrome of its own — no box, no rule, no focus
    // ring. Anything drawn around or beside the text is the tell that this is a
    // widget rather than the document, and the run is as tall as whatever it
    // holds, so even a rule at its leading edge runs the height of the file in
    // Final. What says the caret is here is the caret (DFV-FR-47).
    expect(rule(components, ".diff-run__doc:focus-visible")).toMatch(
      /outline\s*:\s*none/,
    );
    expect(components).not.toMatch(/\.diff-run[^{]*:focus-within/);
    expect(components).not.toMatch(/diff-run__caret/);
    // A Source row is one line, so it can carry the treatment DFV-FR-50 asks
    // for without becoming a box: a rule at its own leading edge, inset so
    // nothing reflows.
    const row = rule(components, ".diff-line__text--target:focus-visible");
    expect(row).toMatch(/box-shadow\s*:\s*inset 2px 0 0 var\(--accent\)/);
    expect(row).not.toMatch(/outline\s*:\s*\d/);
    // And the run's edges do not push its content off the measure the original
    // opposite it sits on (DFV-FR-45).
    expect(rule(components, ".diff-run__doc > :first-child")).toMatch(
      /margin-top\s*:\s*0/,
    );
    // And the run's text starts on the same measure a static block's does, so a
    // removed block rendered beside one does not sit at a different indent.
    expect(rule(components, '.diff-run \[data-mark\]')).toMatch(
      /padding-left\s*:\s*calc\(12px \+ var\(--sp-2\)\)/,
    );
  });

  it("draws the split view's two pages behind the whole document, not one screen of it (DFV-FR-38)", () => {
    // An absolutely positioned child of a scroll container is laid out against
    // that container's padding box — one viewport tall — and then scrolls away
    // with the content: past a screen's worth of scrolling the pages were gone
    // and the document sat on the bare field. They hang off the rows' own
    // wrapper instead, which is as tall as the document.
    const sheets =
      ".diff-rich--split .diff-sbs__sheets::before,\n" +
      ".diff-rich--split .diff-sbs__sheets::after";
    expect(rule(components, sheets)).toMatch(/position\s*:\s*absolute/);
    // Top to bottom of the wrapper, whatever that is.
    expect(rule(components, sheets)).toMatch(/top\s*:\s*0/);
    expect(rule(components, sheets)).toMatch(/bottom\s*:\s*0/);
    expect(rule(components, ".diff-rich--split .diff-sbs__sheets")).toMatch(
      /position\s*:\s*relative/,
    );
    expect(components).toMatch(/\.diff-sbs__sheets::before \{ left: 0; \}/);
    expect(components).toMatch(/\.diff-sbs__sheets::after \{ right: 0; \}/);
    // And nothing hangs them off the scroller any more.
    expect(components).not.toMatch(/\\.diff-sbs__scroll::(before|after)/);
  });

  it("soft-wraps every line rather than scrolling sideways", () => {
    const text = rule(components, ".diff-line__text");
    expect(text).toMatch(/white-space\s*:\s*pre-wrap/);
    // `anywhere`, so an unbroken token — a URL, a minified line, a base64 blob
    // — wraps too instead of forcing the row wide.
    expect(text).toMatch(/overflow-wrap\s*:\s*anywhere/);
    // The old `white-space: pre` on the row itself is gone; leaving it would
    // override nothing but would make the intent unreadable.
    expect(components).not.toMatch(/\.diff-line\s*\{[^}]*white-space\s*:\s*pre\s*;/);
  });

  it("lets the body scroll vertically instead of clipping the file", () => {
    // The body is a column flex container, so its children are flex items and
    // default to `flex-shrink: 1`. Left at that they shrink to the body's
    // height rather than overflowing it, and because `.diff` clips its own
    // overflow the file is cut off at the fold with no scrollbar anywhere.
    const body = rule(components, ".diff-view__body");
    expect(body).toMatch(/overflow-y\s*:\s*auto/);
    expect(body).toMatch(/min-height\s*:\s*0/);
    expect(rule(components, ".diff-view__body > *")).toMatch(
      /flex\s*:\s*none/,
    );

    // Side-by-side is the exception: it owns the scroller its two panes share
    // (DFV-FR-13), so it fills the body rather than growing past it.
    const sideBySide = rule(components, ".diff-view__body > .diff-sbs");
    expect(sideBySide).toMatch(/flex\s*:\s*1/);
    expect(sideBySide).toMatch(/min-height\s*:\s*0/);
    expect(rule(components, ".diff-sbs__scroll")).toMatch(
      /overflow-y\s*:\s*auto/,
    );
  });

  it("gives the diff viewer no horizontal scroller in either pane", () => {
    expect(rule(components, ".diff-view__body")).toMatch(
      /overflow-x\s*:\s*hidden/,
    );
    expect(rule(components, ".diff-sbs__scroll")).toMatch(
      /overflow-x\s*:\s*hidden/,
    );
    // Including the one element that would otherwise bring its own: `.doc pre`
    // sets `overflow-x: auto`, so the rich code block has to reset it.
    expect(rule(components, ".diff-block__body pre")).toMatch(
      /overflow-x\s*:\s*hidden/,
    );
    // And nothing under the diff viewer reintroduces one.
    const diffRules = components
      .slice(components.indexOf("/* ---------- DIFF TAB"))
      .match(/overflow-x\s*:\s*(auto|scroll)/g);
    expect(diffRules).toBeNull();
  });

  it("keeps the gutter columns a fixed width (DFV non-functional)", () => {
    // `min-width` would let row 9999 and row 10000 size differently, stepping
    // their text columns sideways relative to each other — and in side-by-side
    // breaking the alignment of DFV-FR-11.
    const gutter = rule(components, ".diff-line__gutter");
    // A single `width`, identical on every row — which a ratio of the Source
    // role's size is just as much as a literal was (DFV-FR-37), while also
    // holding a four-digit number at any size the role carries.
    expect(gutter).toMatch(
      /width\s*:\s*(\d+px|calc\(\s*var\(--font-source-size\))/,
    );
    expect(gutter).not.toMatch(/width\s*:\s*auto/);
    expect(gutter).not.toMatch(/min-width\s*:/);
  });

  it("centres the cluster whatever the write state says (DFV-FR-07 / DFV-FR-51)", () => {
    // Three columns with EQUAL outer tracks: that is what keeps the middle one
    // centred on the row however long the trailing write state's text grows.
    const toolbar = rule(components, ".diff-toolbar");
    expect(toolbar).toMatch(/display\s*:\s*grid/);
    expect(toolbar).toMatch(
      /grid-template-columns\s*:\s*minmax\(0,\s*1fr\)\s+auto\s+minmax\(0,\s*1fr\)/,
    );
    expect(rule(components, ".diff-toolbar__cluster")).toMatch(
      /grid-column\s*:\s*2/,
    );
    // `margin-left: auto` anywhere in the row is the arrangement this moved
    // away from: it absorbs all the free space, which silently turns any
    // centring above it into a no-op and leaves the cluster on the leading edge.
    expect(rule(components, ".diff-toolbar__write-state")).not.toMatch(
      /margin(-left|-right)?\s*:\s*auto/,
    );
    expect(rule(components, ".diff-toolbar__group")).not.toMatch(
      /margin(-left|-right)?\s*:\s*auto/,
    );
    // And the two things that sit beside the cluster take opposite ends rather
    // than the same one, which would stack them.
    expect(rule(components, ".diff-toolbar__note")).toMatch(/grid-column\s*:\s*1/);
    expect(rule(components, ".diff-toolbar__write-state")).toMatch(
      /grid-column\s*:\s*3/,
    );
    // Every child is pinned to ROW 1. Auto-placement is row-major and sparse,
    // so the note — which sits in column 1 but comes after the cluster in DOM
    // order — would start a second row and drag the write state down with it,
    // making the toolbar taller on a file rich rendering does not apply to than
    // on one it does. The layout notes say the row keeps its position whatever
    // the file is.
    expect(rule(components, ".diff-toolbar > *")).toMatch(/grid-row\s*:\s*1/);
  });

  it("renders the rich page on the Editor's own metrics, defined once", () => {
    // The claim in `RichBlock` is that nothing about the document's appearance
    // is defined twice. These are the two rules that would otherwise drift.
    expect(tokens).toMatch(/--doc-page-outer\s*:/);
    expect(tokens).toMatch(/--doc-page-padding\s*:/);
    for (const decl of [rule(kit, ".editor"), rule(components, ".diff-rich--page")]) {
      // The measure is `--doc-page-outer` rather than `--doc-page-width`: both
      // rules size on the page's whole outer box under `border-box`, so a Diff
      // tab's Rich rendering and the Editor's page land on one measure
      // (DFV-FR-38, `EDT-editor.md` EDT-FR-63).
      expect(decl).toMatch(/var\(--doc-page-outer\)/);
      expect(decl).toMatch(/var\(--doc-page-padding\)/);
    }
  });

  it("overrides every metric the source modes set, so rich matches the Editor", () => {
    // `.diff` dresses the source modes in monospace chrome metrics. Any of the
    // three left unset would be inherited by an element `.doc` does not
    // restate — an h4, a table cell — and rich would not match the Editor.
    const chrome = rule(components, ".diff");
    const rich = rule(components, ".diff-rich");
    for (const property of ["font-family", "font-size", "line-height"]) {
      expect(chrome).toMatch(new RegExp(`${property}\\s*:`));
      expect(rich).toMatch(new RegExp(`${property}\\s*:`));
    }
  });
});
