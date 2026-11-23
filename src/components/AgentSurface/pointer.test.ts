import { describe, expect, it } from "vitest";

import { absolute, getAt, itemPointer, setAt } from "./pointer";

describe("surface JSON Pointers", () => {
  it("ASF-FR-ENRG: a pointer reads a value, with the RFC 6901 escapes", () => {
    const model = { a: { "b/c": 1, "d~e": [10, 20] } };
    expect(getAt(model, "/a/b~1c")).toBe(1);
    expect(getAt(model, "/a/d~0e/1")).toBe(20);
    expect(getAt(model, "")).toBe(model);
    expect(getAt(model, "/a/missing/deeper")).toBeUndefined();
    expect(getAt(model, "/a/d~0e/x")).toBeUndefined();
  });

  it("ASF-FR-ENRG: a write returns a copy, creates what is missing, and leaves the model as it was", () => {
    const model = { list: [{ name: "a" }], n: 1 };
    const next = setAt(model, "/list/0/name", "b");
    expect(next).toEqual({ list: [{ name: "b" }], n: 1 });
    expect(model.list[0].name).toBe("a");
    expect(setAt({}, "/x/y", true)).toEqual({ x: { y: true } });
    expect(setAt(model, "", "not an object")).toBe(model);
  });

  it("ASF-FR-TURS: a relative pointer reads from the template item", () => {
    expect(absolute("name", "/items/2")).toBe("/items/2/name");
    expect(absolute("/top", "/items/2")).toBe("/top");
    expect(itemPointer("/items", 3)).toBe("/items/3");
  });
});
