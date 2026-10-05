import { describe, expect, it } from "vitest";

import { stripCitationMarkers } from "./citationMarkers";

/** The span an OpenAI model writes for two web-search sources. */
const SPAN = "citeturn0search1turn0search2";

describe("stripCitationMarkers", () => {
  it.each([
    [`See this ${SPAN}.`, "See this."],
    [`route. ${SPAN} Next`, "route. Next"],
    [`tabs \t \t${SPAN}!`, "tabs!"],
    [`${SPAN} leading`, " leading"],
    [`kept line\n${SPAN}`, "kept line\n"],
    [`nbsp ${SPAN}`, "nbsp "],
    [`a ${SPAN} ${SPAN}.`, "a."],
    [`日本語 ${SPAN}。🙂`, "日本語。🙂"],
    [`  \t${SPAN}`, ""],
    [SPAN, ""],
  ])("CVL-FR-VSSU, CMT-FR-AWIE: a span goes with the spaces before it (%j)", (input, expected) => {
    expect(stripCitationMarkers(input)).toBe(expected);
  });

  it.each([
    ["a b c d", "a b d"],
    ["a citeturn0", "a citeturn0"],
    ["stray close", "stray close"],
    ["first", "first"],
    ["edges ", "edges "],
  ])("CVL-FR-VSSU: a lone character of the block goes alone (%j)", (input, expected) => {
    expect(stripCitationMarkers(input)).toBe(expected);
  });

  it.each(["", "plain prose", "日本語 🙂", " and "])(
    "CVL-FR-VSSU: text without a marker is returned as it is (%j)",
    (text) => {
      expect(stripCitationMarkers(text)).toBe(text);
    },
  );

  it("CVL-FR-VSSU: removing twice removes nothing more", () => {
    const once = stripCitationMarkers(`x ${SPAN} y z`);
    expect(stripCitationMarkers(once)).toBe(once);
  });
});
