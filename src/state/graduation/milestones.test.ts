import { describe, expect, it } from "vitest";

import { makeLogs, makeRun } from "../../test/graduationFixtures";
import { currentTurnOf, milestonesFrom, turnHasEnded } from "./milestones";

const plan = (...titles: string[]) => JSON.stringify({ event: "plan", milestones: titles });
const start = (index: number) => JSON.stringify({ event: "start", index });
const done = (index: number) => JSON.stringify({ event: "done", index });

describe("the current turn of a run", () => {
  it("GRU-FR-TBNE, GRU-FR-RVEQ: a working or reviewing run is on the phase it is in and the pass of its checkpoint", () => {
    const working = makeRun("r1", "working", { checkpoint: { pass: 2 } as never });
    expect(currentTurnOf(working)).toEqual({ phaseId: "working", pass: 2 });
    const reviewing = makeRun("r1", "reviewing", { checkpoint: { pass: 1 } as never });
    expect(currentTurnOf(reviewing)).toEqual({ phaseId: "reviewing", pass: 1 });
  });

  it("GRU-FR-TBNE, GRU-FR-SXAC: any other run is on the last working or reviewing segment of its activity index", () => {
    const ended = makeRun("r1", "completed", {
      logs: makeLogs({
        activity: [
          { phaseId: "queued", pass: null },
          { phaseId: "working", pass: 1 },
          { phaseId: "reviewing", pass: 1 },
          { phaseId: "working", pass: 2 },
          { phaseId: "reviewing", pass: 2 },
          { phaseId: "done", pass: null },
        ],
      }),
    });
    expect(currentTurnOf(ended)).toEqual({ phaseId: "reviewing", pass: 2 });
    expect(currentTurnOf(makeRun("r2", "queued"))).toBeNull();
  });

  it("GRU-FR-FHLE: only a working or reviewing run is inside its turn", () => {
    expect(turnHasEnded(makeRun("r1", "working"))).toBe(false);
    expect(turnHasEnded(makeRun("r1", "reviewing"))).toBe(false);
    for (const state of ["interrupted", "awaiting_author", "blocked", "completed"] as const) {
      expect(turnHasEnded(makeRun("r1", state))).toBe(true);
    }
  });
});

describe("the states of the milestones", () => {
  it("GRU-FR-NKDU: no valid plan gives no list", () => {
    expect(milestonesFrom([], false)).toBeNull();
    expect(milestonesFrom([start(1), done(1)], false)).toBeNull();
    expect(milestonesFrom([plan()], false)).toBeNull();
  });

  it("GRU-FR-CYDK: a milestone is pending, active, or done", () => {
    const states = milestonesFrom([plan("A", "B", "C"), start(1), done(1), start(2)], false);
    expect(states).toEqual([
      { title: "A", state: "done" },
      { title: "B", state: "active" },
      { title: "C", state: "pending" },
    ]);
  });

  it("GRU-FR-CYDK: a done marker without a start marker makes the milestone done", () => {
    expect(milestonesFrom([plan("A", "B"), done(2)], false)?.map((m) => m.state)).toEqual([
      "pending",
      "done",
    ]);
  });

  it("GRU-FR-FHLE: once the turn has ended, each milestone that is not done is unknown", () => {
    const states = milestonesFrom([plan("A", "B", "C"), start(1), done(1), start(2)], true);
    expect(states?.map((m) => m.state)).toEqual(["done", "unknown", "unknown"]);
  });

  it("GRU-FR-PZGA, GRU-FR-BXMS, GRU-FR-ELDK: a bad record is ignored and a later plan starts the list over", () => {
    const states = milestonesFrom(
      [
        start(1),
        "not json",
        "[]",
        JSON.stringify({ event: "plan", milestones: [] }),
        JSON.stringify({ event: "start", index: 0 }),
        JSON.stringify({ event: "start", index: "1" }),
        plan("A", "B"),
        plan("Other"),
        done(7),
        done(1),
        start(1),
      ],
      false,
    );
    expect(states).toEqual([{ title: "Other", state: "done" }]);
  });

  it("GRU-FR-PZGA: a plan of a later call replaces the plan and resets every milestone to pending", () => {
    const states = milestonesFrom(
      [plan("A", "B"), start(1), done(1), plan("C", "D", "E"), start(2)],
      false,
    );
    expect(states).toEqual([
      { title: "C", state: "pending" },
      { title: "D", state: "active" },
      { title: "E", state: "pending" },
    ]);
  });

  it("GRU-FR-BXMS: a start marker for a milestone that is already done changes nothing", () => {
    const states = milestonesFrom([plan("A"), done(1), start(1)], false);
    expect(states).toEqual([{ title: "A", state: "done" }]);
  });
});
