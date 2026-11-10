import { describe, expect, it } from "vitest";

import { changeFileTarget, deriveChangeName, isValidChangeName } from "./changeName";

describe("the change name rule", () => {
  it.each(["a", "add-logout", "a1-b2-c3", "x".repeat(64), "0-9"])(
    "OSM-FR-IHUO: %s is a valid change name",
    (name) => expect(isValidChangeName(name)).toBe(true),
  );

  it.each([
    ["the empty name", ""],
    ["a name of 65 characters", "x".repeat(65)],
    ["archive", "archive"],
    ["an uppercase letter", "Add-logout"],
    ["a leading hyphen", "-add"],
    ["a trailing hyphen", "add-"],
    ["a double hyphen", "add--logout"],
    ["an underscore", "add_logout"],
    ["a space", "add logout"],
    ["a slash", "add/logout"],
    ["a letter outside a-z", "café"],
  ])("OSM-FR-IHUO: %s is not valid", (_why, name) => {
    expect(isValidChangeName(name)).toBe(false);
  });
});

describe("the change name the start dialog derives", () => {
  it("GSD-FR-CKSV: lowercases, turns each run of other characters into one hyphen, and adds the first four of the id", () => {
    expect(deriveChangeName("Add Logout  Button!", "3F2A9c-77")).toBe("add-logout-button-3f2a");
  });

  it("GSD-FR-CKSV: trims hyphens at both ends of the name part", () => {
    expect(deriveChangeName("  --Logout__ ", "abcd")).toBe("logout-abcd");
  });

  it("GSD-FR-CKSV: the id part takes letters and digits alone", () => {
    expect(deriveChangeName("Logout", "d-1")).toBe("logout-d1");
    expect(deriveChangeName("Logout", "e3b0c442-98fc")).toBe("logout-e3b0");
  });

  it("GSD-FR-CKSV: cuts the name part to 59 characters, and trims a hyphen the cut leaves", () => {
    const long = deriveChangeName("a".repeat(80), "1234");
    expect(long).toBe(`${"a".repeat(59)}-1234`);
    expect(long).toHaveLength(64);
    expect(isValidChangeName(long)).toBe(true);
    // The 59th character is a hyphen, so the cut ends on one and drops it.
    const cut = deriveChangeName(`${"b".repeat(58)} tail`, "abcd");
    expect(cut).toBe(`${"b".repeat(58)}-abcd`);
  });

  it("GSD-FR-CKSV: a name with no letter or digit gives the id part alone", () => {
    expect(deriveChangeName("!!! ???", "ABCD1234")).toBe("abcd");
    expect(deriveChangeName("Ünïcödé", "9f8e")).toBe("n-c-d-9f8e");
  });

  it("GSD-FR-CKSV / OSM-FR-IHUO: every derived name of a draft with a usable id is valid", () => {
    for (const name of ["Archive", "x", "Ω", "a b c", "  ", "Release 2.0 — final!"]) {
      expect(isValidChangeName(deriveChangeName(name, "a1b2c3"))).toBe(true);
    }
  });
});

describe("the change a project file belongs to", () => {
  it("LIB-FR-OKIZ: a file below openspec/changes/<name>/ names the change and its change-relative path", () => {
    expect(changeFileTarget("openspec/changes/add-logout/proposal.md")).toEqual({
      name: "add-logout",
      file: "proposal.md",
    });
    expect(changeFileTarget("openspec/changes/add-logout/specs/auth/spec.md")).toEqual({
      name: "add-logout",
      file: "specs/auth/spec.md",
    });
  });

  it.each([
    ["a file below the archive", "openspec/changes/archive/2025-01-01-old/proposal.md"],
    ["a name that is not valid", "openspec/changes/Add_Logout/proposal.md"],
    ["a file directly in openspec/changes", "openspec/changes/README.md"],
    ["a live spec", "openspec/specs/auth/spec.md"],
    ["another folder", "docs/openspec/changes/add-logout/proposal.md"],
  ])("LIB-FR-OKIZ: %s belongs to no change", (_why, path) => {
    expect(changeFileTarget(path)).toBeNull();
  });
});
