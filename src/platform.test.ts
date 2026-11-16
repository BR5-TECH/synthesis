import { describe, expect, it } from "vitest";

import { markPlatform, platformOf } from "./platform";

describe("the platform of the window (PJT-FR-PBQT)", () => {
  it("PJT-FR-PBQT: names macOS for a Mac platform, and other platforms as other", () => {
    expect(platformOf("MacIntel")).toBe("macos");
    expect(platformOf("Win32")).toBe("other");
    expect(platformOf("Linux x86_64")).toBe("other");
  });

  it("PJT-FR-PBQT: puts the platform on the root, where the top chrome styles read it", () => {
    const root = document.createElement("html");
    markPlatform(root, "MacIntel");
    expect(root.getAttribute("data-platform")).toBe("macos");
  });
});
