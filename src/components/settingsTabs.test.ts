import { describe, expect, it } from "vitest";

import { settingsTabStyle } from "./settingsTabs";

describe("settingsTabStyle", () => {
  it("AII-FR-OTQY, SET-FR-GVED: a tab that is not implemented keeps a grayed label, also when selected", () => {
    expect(settingsTabStyle(false, true).color).toBe("var(--fg-4)");
    expect(settingsTabStyle(true, true).color).toBe("var(--fg-4)");
    // The underline still shows which tab is open.
    expect(settingsTabStyle(true, true).borderBottom).toBe(
      "2px solid var(--accent)",
    );
  });

  it("AII-FR-OTQY: an implemented tab keeps its usual label colors", () => {
    expect(settingsTabStyle(true).color).toBe("var(--fg-1)");
    expect(settingsTabStyle(false).color).toBeUndefined();
  });
});
