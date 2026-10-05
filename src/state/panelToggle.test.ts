import { describe, expect, it } from "vitest";

import { isToggleActive, nextPanelToggleState } from "./panelToggle";

// SNV-shell-navigation.md SNV-FR-45 / SNV-FR-46: the activity bar's toggles are
// toggles, not radio buttons. Both clusters run on this one rule.

type Surface = "library" | "notes" | "changes";

describe("nextPanelToggleState (SNV-FR-45 / SNV-FR-46)", () => {
  it("switches surface when a different toggle is activated", () => {
    expect(
      nextPanelToggleState<Surface>(
        { hidden: false, surface: "library" },
        "notes",
      ),
    ).toEqual({ hidden: false, surface: "notes" });
  });

  it("hides the panel when the active surface's own toggle is activated", () => {
    expect(
      nextPanelToggleState<Surface>(
        { hidden: false, surface: "library" },
        "library",
      ),
    ).toEqual({ hidden: true, surface: "library" });
  });

  it("keeps the selected surface while hidden, so re-activating returns to it", () => {
    const hidden = nextPanelToggleState<Surface>(
      { hidden: false, surface: "changes" },
      "changes",
    );
    expect(hidden.surface).toBe("changes");
    expect(
      nextPanelToggleState<Surface>(hidden, "changes"),
    ).toEqual({ hidden: false, surface: "changes" });
  });

  it("reopens on the clicked surface, not the remembered one", () => {
    // SNV-FR-45: hiding the Library and then clicking Notes opens Notes.
    expect(
      nextPanelToggleState<Surface>(
        { hidden: true, surface: "library" },
        "notes",
      ),
    ).toEqual({ hidden: false, surface: "notes" });
  });

  it("round-trips: hide then show leaves the state it started in", () => {
    const start = { hidden: false, surface: "notes" as Surface };
    const hidden = nextPanelToggleState(start, "notes");
    expect(nextPanelToggleState(hidden, "notes")).toEqual(start);
  });
});

describe("isToggleActive (SNV-FR-45 / SNV-FR-46)", () => {
  it("marks the shown surface's toggle and no other", () => {
    const state = { hidden: false, surface: "notes" as Surface };
    expect(isToggleActive(state, "notes")).toBe(true);
    expect(isToggleActive(state, "library")).toBe(false);
  });

  it("marks nothing while the panel is hidden", () => {
    const state = { hidden: true, surface: "notes" as Surface };
    expect(isToggleActive(state, "notes")).toBe(false);
    expect(isToggleActive(state, "library")).toBe(false);
  });
});
