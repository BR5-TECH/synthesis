import { describe, expect, it } from "vitest";

import { editDistance, matchBareNickname, withSpokenTags } from "./spokenTags";

const roster = { nicknames: ["arch", "archer", "ux"], ready: ["arch", "archer", "ux"] };

describe("spoken addresses", () => {
  it("VOM-FR-TAQJ: turns a spoken address at the start into a tag", () => {
    expect(withSpokenTags("at arch, check this.", roster)).toBe("@arch check this.");
    expect(withSpokenTags("Hey UX: what now?", roster)).toBe("@ux what now?");
    expect(withSpokenTags("At all, listen.", roster)).toBe("@all listen.");
  });

  it("VOM-FR-TAQJ: takes the longest nickname", () => {
    expect(withSpokenTags("at archer look", roster)).toBe("@archer look");
  });

  it("VOM-FR-TAQJ: leaves other text as spoken", () => {
    expect(withSpokenTags("Look at arch today", roster)).toBe("Look at arch today");
    expect(withSpokenTags("at home today", roster)).toBe("at home today");
    expect(withSpokenTags("at architecture", roster)).toBe("at architecture");
  });

  it("VOM-FR-TAQJ: `all` needs a roster", () => {
    expect(withSpokenTags("at all times", { nicknames: [], ready: [] })).toBe("at all times");
  });
});

describe("a bare nickname", () => {
  const names = ["Helga", "archer", "ux", "bob", "Margaret"];
  const helga = { nicknames: names, ready: names };

  it("VOM-FR-SBSS: the first word that names an agent becomes a tag", () => {
    expect(withSpokenTags("Helga, check this", helga)).toBe("@Helga check this");
    expect(withSpokenTags("helga check this", helga)).toBe("@Helga check this");
    expect(withSpokenTags("UX. What now?", helga)).toBe("@ux What now?");
  });

  it("VOM-FR-SBSS, VOM-FR-TAQJ: the `at` and `hey` forms come first", () => {
    expect(withSpokenTags("hey archer, look", helga)).toBe("@archer look");
  });

  it("VOM-FR-SBSS: a first word that names nobody stays as spoken", () => {
    expect(withSpokenTags("Check this, Helga", helga)).toBe("Check this, Helga");
    expect(withSpokenTags("Hello, there", helga)).toBe("Hello, there");
  });

  it("VOM-FR-CXXP: a close misspelling followed by a comma matches", () => {
    expect(withSpokenTags("Helge, check this", helga)).toBe("@Helga check this");
    expect(withSpokenTags("Archar, look", helga)).toBe("@archer look");
    expect(withSpokenTags("Margerit, look", helga)).toBe("@Margaret look");
    expect(withSpokenTags("Arshor, look", helga)).toBe("Arshor, look");
  });

  it("VOM-FR-CXXP: no comma, a weak match, or a short nickname gives no tag", () => {
    expect(withSpokenTags("Helge check this", helga)).toBe("Helge check this");
    expect(withSpokenTags("Hilgo, check this", helga)).toBe("Hilgo, check this");
    expect(withSpokenTags("Bab, look", helga)).toBe("Bab, look");
    expect(withSpokenTags("Ox, look", helga)).toBe("Ox, look");
  });

  it("VOM-FR-CXXP: a tie gives no tag", () => {
    expect(matchBareNickname("Mara", true, ["Mira", "Maya"])).toBeNull();
    expect(matchBareNickname("Mira", true, ["Mira", "Maya"])).toBe("Mira");
  });

  it("VOM-FR-CXXP: the distance ignores case", () => {
    expect(editDistance("HELGA", "helga")).toBe(0);
    expect(editDistance("helge", "helga")).toBe(1);
    expect(editDistance("", "abc")).toBe(3);
  });

  it("VOM-FR-TAQJ: matching uses the roster only", () => {
    expect(withSpokenTags("Helga, check this", { nicknames: [], ready: [] })).toBe(
      "Helga, check this",
    );
  });
});
