import { describe, expect, it } from "vitest";

import { resolveSelection } from "./SettingsBody";
import { GLOBAL_SECTIONS } from "./GlobalSettings";
import { PROJECT_SECTIONS } from "./Settings";
import {
  GLOBAL_SECTION_KEYS,
  PROJECT_SECTION_KEYS,
  firstSectionOf,
} from "../settingsWindow";

/**
 * Which section a section address presents
 * (`../../specifications/ui/SWN-settings-windows.md` SWN-FR-QXTV, SWN-FR-MRNB,
 * SWN-FR-HZWD, SWN-FR-16).
 *
 * Pure, so the rule that decides what the window shows for a route is checked
 * without mounting the sections.
 */

describe("the section vocabulary", () => {
  it("SWN-FR-QXTV: the Global group lists 12 sections, the Project group 10, in the keys a route names", () => {
    expect(GLOBAL_SECTIONS.map(([key]) => key)).toEqual([...GLOBAL_SECTION_KEYS]);
    expect(PROJECT_SECTIONS.map(([key]) => key)).toEqual([...PROJECT_SECTION_KEYS]);
    expect(GLOBAL_SECTIONS).toHaveLength(12);
    expect(PROJECT_SECTIONS).toHaveLength(10);
  });

  it("SWN-FR-MRNB: the first section of the Global group is Appearance", () => {
    expect(firstSectionOf("global")).toBe("appearance");
    expect(firstSectionOf("project")).toBe("project");
  });
});

describe("resolving an address (SWN-FR-HZWD / SWN-FR-16)", () => {
  const both = ["global", "project"] as const;
  const globalOnly = ["global"] as const;

  it("SWN-FR-HZWD: names a scope and a section, with or without an item", () => {
    expect(resolveSelection("global/github", both)).toEqual({
      scope: "global",
      section: "github",
    });
    expect(resolveSelection("project/graduation", both)).toEqual({
      scope: "project",
      section: "graduation",
    });
    expect(resolveSelection("global/agents:new", both)).toEqual({
      scope: "global",
      section: "agents",
    });
  });

  it("SWN-FR-HZWD: a section that exists in both groups resolves to the group named", () => {
    expect(resolveSelection("global/docker", both)?.scope).toBe("global");
    expect(resolveSelection("project/docker", both)?.scope).toBe("project");
  });

  it("SWN-FR-HZWD: an address that names nothing the window has names nothing", () => {
    expect(resolveSelection(null, both)).toBeNull();
    expect(resolveSelection("agents", both)).toBeNull();
    expect(resolveSelection("global/graduation", both)).toBeNull();
    expect(resolveSelection("project/appearance", both)).toBeNull();
    expect(resolveSelection("team/agents", both)).toBeNull();
  });

  it("SWN-FR-16: no address presents a Project section while the Project group is absent", () => {
    for (const key of PROJECT_SECTION_KEYS) {
      expect(resolveSelection(`project/${key}`, globalOnly), key).toBeNull();
    }
    expect(resolveSelection("global/agents", globalOnly)).toEqual({
      scope: "global",
      section: "agents",
    });
  });
});
