import { describe, expect, it } from "vitest";

import {
  NEW_AGENT_ITEM,
  formatSectionAddress,
  parseSectionAddress,
  readSettingsWindowBoot,
  settingsWindowTitle,
} from "./settingsWindow";

/**
 * Which window a webview is, and where inside it to land
 * (`../specifications/ui/SWN-settings-windows.md` SWN-FR-01, SWN-FR-13,
 * SWN-FR-17).
 *
 * All three windows boot one HTML entry point, so this decision is what stands
 * between the author and a settings window rendering the whole shell inside an
 * 800 × 600 frame. Pure, so it is checkable without a desktop.
 */

describe("which window this is (SWN-FR-01)", () => {
  it("recognises each settings window by the query the backend built it with", () => {
    expect(readSettingsWindowBoot("?settings=global")).toEqual({
      kind: "global",
      section: null,
    });
    expect(readSettingsWindowBoot("?settings=project")).toEqual({
      kind: "project",
      section: null,
    });
    // The leading `?` is optional, because `window.location.search` is empty
    // rather than "?" when there is no query at all.
    expect(readSettingsWindowBoot("settings=global")?.kind).toBe("global");
  });

  it("reads the main window as no settings window at all", () => {
    expect(readSettingsWindowBoot("")).toBeNull();
    expect(readSettingsWindowBoot("?")).toBeNull();
    // A value nothing writes: rendering the main shell in a window sized and
    // parented as a settings window would be stranger than rendering nothing.
    expect(readSettingsWindowBoot("?settings=")).toBeNull();
    expect(readSettingsWindowBoot("?settings=appearance")).toBeNull();
    expect(readSettingsWindowBoot("?other=global")).toBeNull();
  });

  it("carries the section the request named (SWN-FR-13)", () => {
    expect(readSettingsWindowBoot("?settings=global&section=agents")).toEqual({
      kind: "global",
      section: "agents",
    });
    // An empty section is no section, not a section called "".
    expect(
      readSettingsWindowBoot("?settings=project&section=")?.section,
    ).toBeNull();
  });

  it("decodes a section the backend escaped on its way in", () => {
    // AGT-FR-06: a section address can carry an agent's id, whose shape neither
    // side chooses, so the backend percent-encodes it (`encode_query_value` in
    // `settings_window.rs`). A `&` that survived unescaped would invent a second
    // query parameter and the address would arrive truncated.
    expect(
      readSettingsWindowBoot("?settings=global&section=agents%3Aa%26b"),
    ).toEqual({ kind: "global", section: "agents:a&b" });
  });

  it("titles each window as the backend does (SWN-FR-17)", () => {
    expect(settingsWindowTitle("global")).toBe("Global settings");
    expect(settingsWindowTitle("project")).toBe("Project settings");
  });
});

describe("section addresses (SWN-FR-13 / AGT-FR-06 / AGT-FR-07)", () => {
  it("names a section alone, or one thing within it", () => {
    expect(parseSectionAddress("agents")).toEqual({
      section: "agents",
      item: null,
    });
    expect(parseSectionAddress("agents:agent-1")).toEqual({
      section: "agents",
      item: "agent-1",
    });
    expect(parseSectionAddress(`agents:${NEW_AGENT_ITEM}`)).toEqual({
      section: "agents",
      item: NEW_AGENT_ITEM,
    });
  });

  it("splits on the first colon, so an item carrying one survives", () => {
    expect(parseSectionAddress("agents:a:b")).toEqual({
      section: "agents",
      item: "a:b",
    });
    // A trailing colon names no item rather than an empty one.
    expect(parseSectionAddress("agents:")).toEqual({
      section: "agents",
      item: null,
    });
  });

  it("round-trips whatever it formats", () => {
    for (const [section, item] of [
      ["agents", null],
      ["agents", "agent-1"],
      ["agents", NEW_AGENT_ITEM],
      ["github", null],
      ["agents", "a:b"],
    ] as const) {
      expect(parseSectionAddress(formatSectionAddress(section, item))).toEqual({
        section,
        item,
      });
    }
  });
});
