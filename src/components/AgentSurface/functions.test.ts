import { describe, expect, it } from "vitest";

import { callFunction, isPresent, resolveValue, type Scope } from "./functions";

const scope: Scope = { model: { n: 3, name: "Ada", list: [], blank: "  " }, base: "" };

describe("surface functions", () => {
  it("ASF-FR-GHUZ: formatString reads pointers and keeps an escaped one literal", () => {
    expect(callFunction("formatString", { value: "Hi ${/name}, ${/n} items" }, scope)).toBe(
      "Hi Ada, 3 items",
    );
    expect(callFunction("formatString", { value: "\\${/name} ${/none}" }, scope)).toBe("${/name} ");
    const item: Scope = { model: { items: [{ t: "x" }] }, base: "/items/0", index: 0 };
    expect(callFunction("formatString", { value: "${t}" }, item)).toBe("x");
  });

  it("ASF-FR-GHUZ: formatNumber, formatDate, and pluralize", () => {
    expect(callFunction("formatNumber", { value: 1.5, decimals: 2, grouping: false }, scope)).toMatch(
      /^1[.,]50$/,
    );
    expect(callFunction("formatDate", { value: "2026-03-04T05:06:07", format: "yyyy-MM-dd HH:mm" }, scope)).toBe(
      "2026-03-04 05:06",
    );
    expect(callFunction("formatDate", { value: "not a date", format: "yyyy" }, scope)).toBeUndefined();
    expect(callFunction("pluralize", { value: 1, one: "item", other: "items" }, scope)).toBe("item");
    expect(callFunction("pluralize", { value: 5, one: "item", other: "items" }, scope)).toBe("items");
    expect(callFunction("pluralize", { value: 0, zero: "none", other: "items" }, scope)).toBe("none");
  });

  it("ASF-FR-GHUZ: the checks and the logic", () => {
    expect(isPresent("x")).toBe(true);
    expect(isPresent("  ")).toBe(false);
    expect(isPresent([])).toBe(false);
    expect(isPresent(null)).toBe(false);
    expect(isPresent(false)).toBe(true);
    expect(callFunction("length", { value: "abc", min: 1, max: 3 }, scope)).toBe(true);
    expect(callFunction("length", { value: "abcd", max: 3 }, scope)).toBe(false);
    expect(callFunction("numeric", { value: "5", min: 1, max: 10 }, scope)).toBe(true);
    expect(callFunction("numeric", { value: "x", min: 1 }, scope)).toBe(false);
    expect(callFunction("and", { values: [true, true] }, scope)).toBe(true);
    expect(callFunction("or", { values: [false, true] }, scope)).toBe(true);
    expect(callFunction("not", { value: true }, scope)).toBe(false);
    expect(callFunction("@index", { offset: 1 }, { ...scope, index: 2 })).toBe(3);
    expect(callFunction("@index", {}, scope)).toBeUndefined();
  });

  it("ASF-FR-GHUZ: an unknown function, or one that fails, gives no value and throws nothing", () => {
    expect(callFunction("openUrl", { url: "https://example.com" }, scope)).toBeUndefined();
    expect(callFunction("toString", {}, scope)).toBeUndefined();
    expect(callFunction("formatDate", { value: {}, format: "yyyy" }, scope)).toBeUndefined();
    expect(callFunction("formatNumber", { value: "not a number" }, scope)).toBeUndefined();
  });

  it("ASF-FR-ENRG: a dynamic value is a literal, a binding, or a nested call", () => {
    expect(resolveValue("plain", scope)).toBe("plain");
    expect(resolveValue({ "@path": "/n" }, scope)).toBe(3);
    expect(
      resolveValue({ "@call": "not", args: { value: { "@call": "required", args: { value: { "@path": "/blank" } } } } }, scope),
    ).toBe(true);
  });
});
