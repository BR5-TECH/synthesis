import { describe, expect, it } from "vitest";

import { withSpokenTags } from "./spokenTags";

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
