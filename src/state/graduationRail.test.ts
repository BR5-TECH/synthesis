import { describe, expect, it } from "vitest";

import { makeRun } from "../test/graduationFixtures";
import type { GraduationRun } from "../types";
import { stateLabel } from "./graduation";
import {
  admits,
  admitted,
  clampRailFraction,
  DEFAULT_RAIL_FRACTION,
  DEFAULT_VIEW,
  fractionForDrag,
  fractionForKey,
  isGraduationView,
  MAX_RAIL_FRACTION,
  MIN_RAIL_FRACTION,
  railOrder,
  railPercent,
  revealingText,
  revealingView,
  VIEW_POSITIONS,
} from "./graduationRail";

describe("the rail's views", () => {
  /** Every state a run that has not ended stands in (GRD-FR-QJHM). */
  const IN_FLIGHT = [
    "queued",
    "working",
    "reviewing",
    "awaiting_author",
    "blocked",
    "interrupted",
  ] as const;
  /** Every state a run never leaves. */
  const ENDED = ["completed", "discarded", "failed"] as const;

  it("GRH-FR-WIMY: four positions, in order, and In-flight is the resting one", () => {
    expect(VIEW_POSITIONS.map((position) => position.value)).toEqual([
      "in-flight",
      "completed",
      "archived",
      "all",
    ]);
    // The words the author reads, which are the four the requirement names.
    expect(VIEW_POSITIONS.map((position) => position.tag)).toEqual([
      "In-flight",
      "Completed",
      "Archived",
      "All",
    ]);
    for (const position of VIEW_POSITIONS) {
      expect(position.title).not.toBe("");
    }
    expect(DEFAULT_VIEW).toBe("in-flight");
  });

  it("GRH-FR-QVEX: a held value is read back only where it names a position", () => {
    for (const position of VIEW_POSITIONS) {
      expect(isGraduationView(position.value)).toBe(true);
    }
    // "active" and "queued" were positions of an earlier selector; neither
    // names one now, so neither may be restored as though it did.
    for (const held of ["active", "queued", "", null, undefined, 3]) {
      expect(isGraduationView(held)).toBe(false);
    }
  });

  it("GRH-FR-KZTP: In-flight admits every run that has not ended, the queued ones with them", () => {
    for (const state of IN_FLIGHT) {
      expect(admits("in-flight", makeRun("r", state))).toBe(true);
    }
    for (const state of ENDED) {
      expect(admits("in-flight", makeRun("r", state))).toBe(false);
    }
  });

  it("GRH-FR-KZTP: Completed admits the completed state and no other", () => {
    expect(admits("completed", makeRun("r", "completed"))).toBe(true);
    for (const state of [...IN_FLIGHT, "discarded", "failed"] as const) {
      expect(admits("completed", makeRun("r", state))).toBe(false);
    }
  });

  it("GRH-FR-KZTP: a run that ended without completing stands in All alone", () => {
    for (const state of ["discarded", "failed"] as const) {
      const run = makeRun("r", state);
      expect(admits("in-flight", run)).toBe(false);
      expect(admits("completed", run)).toBe(false);
      expect(admits("archived", run)).toBe(false);
      expect(admits("all", run)).toBe(true);
    }
  });

  it("GRH-FR-DYNU: an archived run stands in Archived and All, and in no state position", () => {
    for (const state of ["queued", "completed", "discarded", "failed", "interrupted", "awaiting_author"] as const) {
      const run = makeRun("r", state, { archived: true });
      expect(admits("in-flight", run)).toBe(false);
      expect(admits("completed", run)).toBe(false);
      expect(admits("archived", run)).toBe(true);
      expect(admits("all", run)).toBe(true);
    }
  });

  it("GRH-FR-DYNU: Archived admits archived runs alone", () => {
    for (const state of [...IN_FLIGHT, ...ENDED] as const) {
      expect(admits("archived", makeRun("r", state))).toBe(false);
    }
  });

  it("GRH-FR-YMIM: a filed-away run holding a stream stands in In-flight and in Archived", () => {
    for (const state of ["working", "reviewing", "blocked"] as const) {
      const filed = makeRun("r", state, { archived: true });
      expect(admits("in-flight", filed)).toBe(true);
      expect(admits("archived", filed)).toBe(true);
      expect(admits("all", filed)).toBe(true);
      // The exemption defeats the archive rule and no other: a run that holds
      // a stream has not completed, so Completed still refuses it.
      expect(admits("completed", filed)).toBe(false);
      const open = makeRun("r", state);
      expect(admits("in-flight", open)).toBe(true);
      expect(admits("archived", open)).toBe(false);
    }
  });
});

describe("revealing a run the filters hide", () => {
  const word = (run: GraduationRun) => stateLabel(run.state);

  it("GRH-FR-XBWU: a position that already admits the run is left alone", () => {
    const working = makeRun("r1", "working");
    expect(revealingView(working, "in-flight")).toBe("in-flight");
    expect(revealingView(working, "all")).toBe("all");
    const filed = makeRun("r2", "completed", { archived: true });
    expect(revealingView(filed, "archived")).toBe("archived");
  });

  it("GRH-FR-XBWU: a run the position hides moves it to All, or to Archived where it is filed", () => {
    expect(revealingView(makeRun("r1", "completed"), "in-flight")).toBe("all");
    expect(revealingView(makeRun("r2", "failed"), "completed")).toBe("all");
    expect(
      revealingView(makeRun("r3", "completed", { archived: true }), "in-flight"),
    ).toBe("archived");
  });

  it("GRH-FR-XBWU: the text filter is cleared only where it excludes the run", () => {
    const run = makeRun("r1", "working", {
      input: { ...makeRun("r1").input, draftName: "editor-scroll" },
    });
    expect(revealingText(run, "editor", word)).toBe("editor");
    expect(revealingText(run, "", word)).toBe("");
    expect(revealingText(run, "registry", word)).toBe("");
  });
});

describe("the rail's text filter", () => {
  /** GRH-FR-BDMB: the state word the filter reads, as the rail passes it. */
  const word = (run: GraduationRun) => stateLabel(run.state);

  it("GRH-FR-BDMB: it narrows the view in force rather than replacing it", () => {
    const filed = makeRun("r1", "completed", {
      archived: true,
      input: { ...makeRun("r1").input, draftName: "editor-scroll" },
    });
    expect(admitted([filed], "in-flight", "editor", word)).toEqual([]);
    expect(admitted([filed], "archived", "editor", word)).toEqual([filed]);
  });

  it("GRH-FR-BDMB: it matches the draft name, the stream name and the state word", () => {
    const run = makeRun("r1", "completed", {
      streamName: "editor-work",
      input: { ...makeRun("r1").input, draftName: "Scroll position" },
    });
    for (const needle of ["scroll", "editor-work", "completed"]) {
      expect(admitted([run], "all", needle, word)).toEqual([run]);
    }
    expect(admitted([run], "all", "registry", word)).toEqual([]);
  });

  it("GRH-FR-BDMB: the needle is literal, trimmed and read without case", () => {
    const run = makeRun("r1", "queued", {
      input: { ...makeRun("r1").input, draftName: "c++ parser" },
    });
    expect(admitted([run], "all", "c++", word)).toEqual([run]);
    expect(admitted([run], "all", "C++", word)).toEqual([run]);
    expect(admitted([run], "all", "  parser  ", word)).toEqual([run]);
    expect(admitted([run], "all", "", word)).toEqual([run]);
    // Read as a pattern, `c+` would match "cc parser". It is not one.
    const other = makeRun("r2", "queued", {
      input: { ...makeRun("r2").input, draftName: "cc parser" },
    });
    expect(admitted([other], "all", "c+", word)).toEqual([]);
  });

  it("GRH-FR-NCJK: the rail sorts by nothing", () => {
    // The project's own order, and the two orders a sort would produce, all
    // disagree: by time this is [r2, r1] and by name it is [r1, r2].
    const oldest = makeRun("r1", "queued", {
      createdAt: "2026-01-01T00:00:00Z",
      input: { ...makeRun("r1").input, draftName: "Alpha" },
    });
    const newest = makeRun("r2", "queued", {
      createdAt: "2026-09-06T12:00:00Z",
      input: { ...makeRun("r2").input, draftName: "Zulu" },
    });
    const order = [newest, oldest];
    expect(railOrder(order).map((run) => run.id)).toEqual(["r2", "r1"]);
    expect(admitted(order, "all", "", word).map((run) => run.id)).toEqual([
      "r2",
      "r1",
    ]);
  });
});

describe("the rail's width", () => {
  it("GRH-FR-MCHQ: an absent or unusable stored width opens at the default", () => {
    for (const held of [undefined, null, NaN, Infinity]) {
      expect(clampRailFraction(held)).toBe(DEFAULT_RAIL_FRACTION);
    }
  });

  it("GRH-FR-MCHQ: the width is held inside its bounds", () => {
    expect(clampRailFraction(0.01)).toBeCloseTo(MIN_RAIL_FRACTION, 5);
    expect(clampRailFraction(0.9)).toBeCloseTo(MAX_RAIL_FRACTION, 5);
    expect(clampRailFraction(0.17)).toBeCloseTo(0.17, 5);
  });

  it("GRH-FR-MCHQ: the announced percentage is a reading of the stored fraction", () => {
    expect(railPercent(0.174)).toBe(17);
    expect(railPercent(0.175)).toBe(18);
    // Clamped before it is rounded, so nothing announces a width nothing uses.
    expect(railPercent(2)).toBe(railPercent(MAX_RAIL_FRACTION));
  });

  it("GRH-FR-MCHQ: a pointer inside the section puts the boundary where it is", () => {
    expect(fractionForDrag(100, 400)).toBeCloseTo(0.25, 5);
    expect(fractionForDrag(-50, 400)).toBeCloseTo(MIN_RAIL_FRACTION, 5);
    expect(fractionForDrag(900, 400)).toBeCloseTo(MAX_RAIL_FRACTION, 5);
    for (const width of [0, -1, NaN]) {
      expect(fractionForDrag(100, width)).toBe(DEFAULT_RAIL_FRACTION);
    }
  });

  it("GRH-FR-OFHS: the arrow keys move the width and stop at the bounds", () => {
    expect(fractionForKey("ArrowLeft", 0.2)).toBeCloseTo(0.19, 5);
    expect(fractionForKey("ArrowRight", 0.2)).toBeCloseTo(0.21, 5);
    expect(fractionForKey("ArrowLeft", MIN_RAIL_FRACTION)).toBeCloseTo(
      MIN_RAIL_FRACTION,
      5,
    );
    expect(fractionForKey("ArrowRight", MAX_RAIL_FRACTION)).toBeCloseTo(
      MAX_RAIL_FRACTION,
      5,
    );
    expect(fractionForKey("Home", 0.2)).toBeCloseTo(MIN_RAIL_FRACTION, 5);
    expect(fractionForKey("End", 0.2)).toBeCloseTo(MAX_RAIL_FRACTION, 5);
  });

  it("GRH-FR-MCHQ: a key step tidies a width a drag left between points", () => {
    expect(fractionForKey("ArrowRight", 0.174)).toBeCloseTo(0.18, 5);
  });

  it("GRH-FR-OFHS: a key this control does not answer to is left alone", () => {
    for (const key of ["ArrowUp", "a", "Enter"]) {
      expect(fractionForKey(key, 0.2)).toBeNull();
    }
  });
});
