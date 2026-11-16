import { describe, expect, it } from "vitest";

import { readProjectWindowBoot } from "./projectWindowBoot";

describe("readProjectWindowBoot (PWN-FR-KQZB)", () => {
  it("PWN-FR-KQZB: reads the name of a created project window", () => {
    expect(readProjectWindowBoot("?projectWindow=1&name=a%26b%20c")).toEqual({ name: "a&b c" });
  });

  it("PWN-FR-KQZB: a page with no mark is not a created project window", () => {
    expect(readProjectWindowBoot("")).toBeNull();
    expect(readProjectWindowBoot("?settings=1")).toBeNull();
    expect(readProjectWindowBoot("?projectWindow=0&name=x")).toBeNull();
  });

  it("PWN-FR-KQZB: a mark with no name gives an empty name", () => {
    expect(readProjectWindowBoot("projectWindow=1")).toEqual({ name: "" });
  });
});
