import { describe, expect, it } from "vitest";
import {
  FONT_ROLE_DEFAULTS,
  FONT_ROLE_ORDER,
  FONT_SIZE_MAX,
  FONT_SIZE_MIN,
  LINE_HEIGHT_MAX,
  LINE_HEIGHT_MIN,
  applyFontRoles,
  fontFamilyValue,
  isFontSizeInRange,
  isLineHeightInRange,
  withFontRole,
} from "./fontRoles";
import type { FontSettings } from "../types";

/**
 * The three typographic roles (OVW-FR-13) and the one place that turns a stored
 * record into the custom properties the stylesheet reads (OVW-FR-14).
 */
describe("typographic roles", () => {
  const root = () => document.createElement("div");

  it("OVW-FR-13: names exactly three roles, and the UI role is one of them", () => {
    // The partition is total and the UI role is the fallthrough, so a surface
    // added later needs no typographic decision to render correctly. A fourth
    // role here would mean some text belongs to none of them.
    expect([...FONT_ROLE_ORDER]).toEqual(["ui", "rich", "source"]);
    expect(Object.keys(FONT_ROLE_DEFAULTS).sort()).toEqual([
      "rich",
      "source",
      "ui",
    ]);
  });

  it("OVW-FR-14: an unset field renders in the built-in default", () => {
    // The stylesheet already declares the built-in value, so an unset field
    // must *remove* the override rather than write a number of its own —
    // otherwise this module and the stylesheet each carry a copy of the default
    // and can drift apart.
    const el = root();
    applyFontRoles(el, {});
    for (const role of FONT_ROLE_ORDER) {
      expect(el.style.getPropertyValue(`--font-${role}-family`)).toBe("");
      expect(el.style.getPropertyValue(`--font-${role}-size`)).toBe("");
      expect(el.style.getPropertyValue(`--font-${role}-line-height`)).toBe("");
    }
    // An entirely absent record is the same state as an empty one.
    applyFontRoles(el, undefined);
    expect(el.getAttribute("style")).toBeFalsy();
  });

  it("OVW-FR-14: applies each role's chosen family, size, and line height", () => {
    const el = root();
    applyFontRoles(el, {
      ui: { family: "Helvetica Neue", sizePx: 12, lineHeight: 1.4 },
      rich: { family: "Georgia", sizePx: 17 },
      source: { family: "Fira Code", lineHeight: 1.7 },
    });

    expect(el.style.getPropertyValue("--font-ui-family")).toContain(
      '"Helvetica Neue"',
    );
    expect(el.style.getPropertyValue("--font-ui-size")).toBe("12px");
    expect(el.style.getPropertyValue("--font-ui-line-height")).toBe("1.4");

    expect(el.style.getPropertyValue("--font-rich-family")).toContain(
      '"Georgia"',
    );
    expect(el.style.getPropertyValue("--font-rich-size")).toBe("17px");
    // Each of the three fields is independently unset (GSS-FR-29).
    expect(el.style.getPropertyValue("--font-rich-line-height")).toBe("");

    expect(el.style.getPropertyValue("--font-source-family")).toContain(
      '"Fira Code"',
    );
    expect(el.style.getPropertyValue("--font-source-size")).toBe("");
    expect(el.style.getPropertyValue("--font-source-line-height")).toBe("1.7");
  });

  it("GLS-FR-22: a chosen family sits in front of the built-in stack, not instead of it", () => {
    // This is the whole mechanism behind GLS-FR-22: a family uninstalled since
    // it was chosen falls back to the built-in face on its own, so the stored
    // selection is left exactly as stored and reinstalling the font restores
    // the role with no reselection. Replacing the stack would leave the role
    // with nothing to fall back to.
    const value = fontFamilyValue("source", "Fira Code");
    expect(value.startsWith('"Fira Code", ')).toBe(true);
    expect(value).toContain(FONT_ROLE_DEFAULTS.source.fallback);
    expect(value.endsWith("monospace")).toBe(true);

    // No family chosen: the built-in stack alone.
    expect(fontFamilyValue("ui", undefined)).toBe(
      FONT_ROLE_DEFAULTS.ui.fallback,
    );
    expect(fontFamilyValue("ui", "   ")).toBe(FONT_ROLE_DEFAULTS.ui.fallback);
  });

  it("quotes a family name so it is data rather than CSS syntax", () => {
    // `Times New Roman` unquoted is three identifiers, and a name carrying a
    // quote would close the string and let what follows read as further
    // declarations — a stored family reaching the root is the one string here
    // that did not come from this codebase.
    expect(fontFamilyValue("ui", "Times New Roman")).toContain(
      '"Times New Roman"',
    );
    const hostile = 'Evil", monospace; background: url(x)';
    const applied = fontFamilyValue("ui", hostile);
    expect(applied).toContain('\\"');
    expect(applied).not.toContain('Evil", monospace; background');

    // And the browser's own parser is the final word: an unparseable value is
    // dropped, so the property is either the escaped name or nothing at all —
    // never an injected declaration.
    const el = root();
    applyFontRoles(el, { ui: { family: hostile } });
    expect(el.style.getPropertyValue("background")).toBe("");
  });

  it("GLS-FR-19: clamps a stored value that is out of range rather than applying it", () => {
    // The section is the gate (GLS-FR-19) and storage bounds nothing
    // (GSS-FR-29) — so a record edited by hand, or written by an older build,
    // can carry a size the section would never have committed. Applying it
    // verbatim would leave the app unreadable with the only control to fix it
    // rendered at the same size.
    const el = root();
    applyFontRoles(el, {
      ui: { sizePx: 400, lineHeight: 40 },
      source: { sizePx: 0.5, lineHeight: 0 },
    });
    expect(el.style.getPropertyValue("--font-ui-size")).toBe(
      `${FONT_SIZE_MAX}px`,
    );
    expect(el.style.getPropertyValue("--font-ui-line-height")).toBe(
      `${LINE_HEIGHT_MAX}`,
    );
    expect(el.style.getPropertyValue("--font-source-size")).toBe(
      `${FONT_SIZE_MIN}px`,
    );
    expect(el.style.getPropertyValue("--font-source-line-height")).toBe(
      `${LINE_HEIGHT_MIN}`,
    );
  });

  it("ignores a non-finite size rather than writing NaN into the root", () => {
    const el = root();
    applyFontRoles(el, {
      ui: { sizePx: Number.NaN, lineHeight: Number.POSITIVE_INFINITY },
    });
    expect(el.style.getPropertyValue("--font-ui-size")).toBe("");
    expect(el.style.getPropertyValue("--font-ui-line-height")).toBe("");
  });

  it("GLS-FR-19: bounds accept the usable range and refuse either side of it", () => {
    expect(isFontSizeInRange(FONT_SIZE_MIN)).toBe(true);
    expect(isFontSizeInRange(FONT_SIZE_MAX)).toBe(true);
    expect(isFontSizeInRange(FONT_SIZE_MIN - 1)).toBe(false);
    expect(isFontSizeInRange(FONT_SIZE_MAX + 1)).toBe(false);
    expect(isFontSizeInRange(Number.NaN)).toBe(false);

    expect(isLineHeightInRange(LINE_HEIGHT_MIN)).toBe(true);
    expect(isLineHeightInRange(LINE_HEIGHT_MAX)).toBe(true);
    expect(isLineHeightInRange(LINE_HEIGHT_MIN - 0.1)).toBe(false);
    expect(isLineHeightInRange(LINE_HEIGHT_MAX + 0.1)).toBe(false);

    // Every built-in default is inside the range it is bounded by: a default
    // the section would refuse to commit is a default no reset could restore.
    for (const role of FONT_ROLE_ORDER) {
      expect(isFontSizeInRange(FONT_ROLE_DEFAULTS[role].sizePx)).toBe(true);
      expect(isLineHeightInRange(FONT_ROLE_DEFAULTS[role].lineHeight)).toBe(
        true,
      );
    }
  });

  it("GLS-FR-23: replacing one role leaves the other two untouched", () => {
    const before: FontSettings = {
      ui: { family: "Helvetica Neue", sizePx: 12 },
      rich: { family: "Georgia" },
      source: { family: "Fira Code", sizePx: 14 },
    };
    // A reset is an empty role, not a missing one.
    const after = withFontRole(before, "rich", {});
    expect(after.rich).toEqual({});
    expect(after.ui).toEqual(before.ui);
    expect(after.source).toEqual(before.source);
  });

  it("re-applying replaces the previous role rather than layering on it", () => {
    // A role changed twice must not leave the first choice behind: the applier
    // runs on every change, and a stale property would keep re-typesetting the
    // app in a family the user has already moved off.
    const el = root();
    applyFontRoles(el, { ui: { family: "Georgia", sizePx: 20 } });
    applyFontRoles(el, { ui: { family: "Helvetica Neue" } });
    expect(el.style.getPropertyValue("--font-ui-family")).toContain(
      '"Helvetica Neue"',
    );
    expect(el.style.getPropertyValue("--font-ui-family")).not.toContain(
      "Georgia",
    );
    expect(el.style.getPropertyValue("--font-ui-size")).toBe("");
  });
});
