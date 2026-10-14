import { describe, expect, it } from "vitest";

import {
  formatDurationMs,
  formatPasses,
  GRADUATION_LOOP_SETTINGS,
  loopRefusalCode,
  loopRefusalMessage,
} from "./graduationLoopSettings";

describe("graduation loop settings", () => {
  it("SET-FR-RTDF: durations read in hours and minutes", () => {
    expect(formatDurationMs(1_800_000)).toBe("30 min");
    expect(formatDurationMs(10_800_000)).toBe("3 h");
    expect(formatDurationMs(12_600_000)).toBe("3 h 30 min");
    expect(formatDurationMs(172_800_000)).toBe("48 h");
    expect(formatDurationMs(345_600_000)).toBe("96 h");
    expect(formatDurationMs(0)).toBe("0 min");
    expect(formatDurationMs(20_000)).toBe("less than 1 min");
    expect(formatDurationMs(90_000)).toBe("2 min");
  });

  it("SET-FR-RTDF: passes read with their count", () => {
    expect(formatPasses(1)).toBe("1 pass");
    expect(formatPasses(2)).toBe("2 passes");
    expect(formatPasses(10)).toBe("10 passes");
  });

  it("SET-FR-PSLD, SET-FR-RTDF: the sliders' order, bounds, steps and defaults", () => {
    expect(
      GRADUATION_LOOP_SETTINGS.map((s) => [s.label, s.min, s.max, s.step, s.defaultValue]),
    ).toEqual([
      ["Passes per run", 1, 10, 1, 2],
      ["Time per turn", 1_800_000, 172_800_000, 1_800_000, 10_800_000],
      ["Total time per run", 1_800_000, 345_600_000, 1_800_000, 18_000_000],
    ]);
  });

  it("SET-FR-WTOS: a typed refusal reads in words, and a log names its code alone", () => {
    const [pass, stage, total] = GRADUATION_LOOP_SETTINGS;
    expect(loopRefusalMessage(pass, "graduation_pass_budget_invalid: 11")).toBe(pass.refusal);
    expect(loopRefusalMessage(stage, new Error("graduation_stage_time_limit_invalid"))).toBe(
      stage.refusal,
    );
    expect(loopRefusalMessage(total, "graduation_total_time_limit_invalid: x")).toBe(
      total.refusal,
    );
    expect(loopRefusalMessage(total, "disk full")).toBe("The setting was not saved: disk full");
    expect(loopRefusalCode("graduation_pass_budget_invalid: 11")).toBe(
      "graduation_pass_budget_invalid",
    );
    expect(loopRefusalCode("")).toBe("unknown");
  });
});
