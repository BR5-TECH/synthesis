import { describe, expect, it } from "vitest";

import {
  NEW_AGENT_ITEM,
  SETTINGS_WINDOW_TITLE,
  formatSectionAddress,
  parseSectionAddress,
  readSettingsWindowBoot,
} from "./settingsWindow";

/**
 * Which window a webview is, and where inside it to land
 * (`../specifications/ui/SWN-settings-windows.md` SWN-FR-01, SWN-FR-HZWD,
 * SWN-FR-17).
 *
 * Both windows boot one HTML entry point, so this decision is what stands
 * between the author and the Settings window rendering the whole shell inside an
 * 800 × 600 frame. Pure, so it is checkable without a desktop.
 */

describe("which window this is", () => {
  it("SWN-FR-01: recognises the Settings window by the query the backend built it with", () => {
    expect(readSettingsWindowBoot("?settings=1")).toEqual({ section: null });
    // The leading `?` is optional, because `window.location.search` is empty
    // rather than "?" when there is no query at all.
    expect(readSettingsWindowBoot("settings=1")).toEqual({ section: null });
  });

  it("SWN-FR-01: reads the main window as no settings window at all", () => {
    expect(readSettingsWindowBoot("")).toBeNull();
    expect(readSettingsWindowBoot("?")).toBeNull();
    // A value nothing writes: rendering the main shell in a window sized and
    // parented as the Settings window would be stranger than rendering nothing.
    expect(readSettingsWindowBoot("?settings=")).toBeNull();
    expect(readSettingsWindowBoot("?settings=global")).toBeNull();
    expect(readSettingsWindowBoot("?other=1")).toBeNull();
  });

  it("SWN-FR-HZWD: carries the section address the request named", () => {
    expect(readSettingsWindowBoot("?settings=1&section=global%2Fagents")).toEqual({
      section: "global/agents",
    });
    // An empty section is no section, not a section called "".
    expect(readSettingsWindowBoot("?settings=1&section=")?.section).toBeNull();
  });

  it("SWN-FR-HZWD: decodes a section the backend escaped on its way in", () => {
    // AGT-FR-06: a section address can carry an agent's id, whose shape neither
    // side chooses, so the backend percent-encodes it (`encode_query_value` in
    // `settings_window.rs`). A `&` that survived unescaped would invent a second
    // query parameter and the address would arrive truncated.
    expect(
      readSettingsWindowBoot("?settings=1&section=global%2Fagents%3Aa%26b"),
    ).toEqual({ section: "global/agents:a&b" });
  });

  it("SWN-FR-17: titles the window Settings", () => {
    expect(SETTINGS_WINDOW_TITLE).toBe("Settings");
  });
});

describe("section addresses", () => {
  it("SWN-FR-HZWD, AGT-FR-06, AGT-FR-07: names a scope and a section, or one thing within it", () => {
    expect(parseSectionAddress("global/agents")).toEqual({
      scope: "global",
      section: "agents",
      item: null,
    });
    expect(parseSectionAddress("global/agents:agent-1")).toEqual({
      scope: "global",
      section: "agents",
      item: "agent-1",
    });
    expect(parseSectionAddress(`global/agents:${NEW_AGENT_ITEM}`)).toEqual({
      scope: "global",
      section: "agents",
      item: NEW_AGENT_ITEM,
    });
  });

  it("SWN-FR-HZWD: tells a section that exists in both groups apart by its scope", () => {
    expect(parseSectionAddress("global/agents")?.scope).toBe("global");
    expect(parseSectionAddress("project/agents")?.scope).toBe("project");
    expect(parseSectionAddress("project/docker")).toEqual({
      scope: "project",
      section: "docker",
      item: null,
    });
  });

  it("SWN-FR-HZWD: reads an address that names no scope as no address", () => {
    expect(parseSectionAddress("agents")).toBeNull();
    expect(parseSectionAddress("team/agents")).toBeNull();
    expect(parseSectionAddress("global/")).toBeNull();
    expect(parseSectionAddress("")).toBeNull();
  });

  it("SWN-FR-HZWD: splits on the first colon, so an item carrying one survives", () => {
    expect(parseSectionAddress("global/agents:a:b")).toEqual({
      scope: "global",
      section: "agents",
      item: "a:b",
    });
    // An item that carries a slash cannot change the scope.
    expect(parseSectionAddress("global/agents:project/x")).toEqual({
      scope: "global",
      section: "agents",
      item: "project/x",
    });
    // A trailing colon names no item rather than an empty one.
    expect(parseSectionAddress("global/agents:")).toEqual({
      scope: "global",
      section: "agents",
      item: null,
    });
  });

  it("SWN-FR-HZWD: round-trips whatever it formats", () => {
    for (const [scope, section, item] of [
      ["global", "agents", null],
      ["global", "agents", "agent-1"],
      ["global", "agents", NEW_AGENT_ITEM],
      ["project", "graduation", null],
      ["global", "agents", "a:b"],
    ] as const) {
      expect(
        parseSectionAddress(formatSectionAddress(scope, section, item)),
      ).toEqual({ scope, section, item });
    }
  });
});
