/** MPV-FR-SNCR: the kind chip of a new issue, from the Type mapping. */
import { describe, expect, it } from "vitest";

import type { DeliverySettings } from "../../types";
import { kindOfType } from "./kinds";

const SETTINGS: DeliverySettings = {
  source: { kind: "github" },
  createTypes: { epic: "Epic", feature: "Feature", task: "Task", defect: "Bug" },
  readEntries: [
    { type: "Story", kind: "feature" },
    { type: "Spike", kind: "exclude" },
  ],
  untyped: "task",
};

describe("kindOfType (MPV-FR-SNCR)", () => {
  it("MPV-FR-SNCR: a read entry wins, matched without regard to case", () => {
    expect(kindOfType(SETTINGS, "story")).toBe("feature");
  });

  it("MPV-FR-SNCR: an excluded Type has no kind", () => {
    expect(kindOfType(SETTINGS, "Spike")).toBeNull();
  });

  it("MPV-FR-SNCR: a create Type maps back to its kind", () => {
    expect(kindOfType(SETTINGS, "Bug")).toBe("defect");
    expect(kindOfType(SETTINGS, "Task")).toBe("task");
  });

  it("MPV-FR-SNCR: no Type takes the untyped kind; an excluded untyped kind gives none", () => {
    expect(kindOfType(SETTINGS, null)).toBe("task");
    expect(kindOfType({ ...SETTINGS, untyped: "exclude" }, null)).toBeNull();
    expect(kindOfType({ ...SETTINGS, untyped: null }, null)).toBeNull();
  });

  it("MPV-FR-SNCR: an unknown Type, or no settings, gives no kind", () => {
    expect(kindOfType(SETTINGS, "Chore")).toBeNull();
    expect(kindOfType(null, "Task")).toBeNull();
  });
});
