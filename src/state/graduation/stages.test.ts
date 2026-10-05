/**
 * The pass a surface reports, and the sentence it builds the row around
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-PNVX,
 * GRU-FR-LBPR, GRU-FR-FZCN).
 */

import { describe, expect, it } from "vitest";

import { conditionSentence, iterationLabel, passOf, retryStatement } from "./stages";
import { makeRun } from "../../test/graduationFixtures";

describe("passOf", () => {
  it("GRU-FR-PNVX: reports the pass the checkpoint holds", () => {
    const run = makeRun("r1", "working", { checkpoint: { pass: 2 } });

    expect(passOf(run)).toBe(2);
  });

  it("GRU-FR-PNVX: the checkpoint wins over the count of pass records", () => {
    // The blocked path never settles the open pass record, so a run that came
    // to rest holds one record while standing at the second pass. Counting the
    // records froze the number the author was shown, however often they
    // continued the run.
    const run = makeRun("r1", "blocked", {
      checkpoint: { pass: 2 },
      observability: {
        observabilityVersion: 1,
        currentStage: "review",
        stageCondition: "blocked",
        stageHistory: [],
        passes: [
          {
            pass: 1,
            status: "working",
            task: "Write the panel.",
            findings: [],
            startedAt: "2026-09-06T09:00:00Z",
          },
        ],
      },
    });

    expect(passOf(run)).toBe(2);
  });

  it("GRU-FR-PNVX: a record with no counted pass falls back to the pass records", () => {
    const run = makeRun("r1", "working", {
      checkpoint: { pass: 0 },
      observability: {
        observabilityVersion: 1,
        currentStage: "working",
        stageCondition: "active",
        stageHistory: [],
        passes: [
          { pass: 1, status: "passed", task: "one", findings: [], startedAt: "2026-09-06T09:00:00Z" },
          { pass: 2, status: "working", task: "two", findings: [], startedAt: "2026-09-06T10:00:00Z" },
        ],
      },
    });

    expect(passOf(run)).toBe(2);
  });

  it("GRU-FR-PNVX: a run with neither a counted pass nor a record still stands at one", () => {
    const run = makeRun("r1", "working", { checkpoint: { pass: 0 } });

    expect(passOf(run)).toBe(1);
  });
});

describe("conditionSentence", () => {
  it("GRU-FR-PNVX: the bound in the sentence is the loop's own", () => {
    expect(conditionSentence(makeRun("r1", "working", { checkpoint: { pass: 1 } }))).toBe(
      "Working · pass 1 of 2",
    );
    expect(conditionSentence(makeRun("r1", "reviewing", { checkpoint: { pass: 2 } }))).toBe(
      "Review · pass 2 of 2",
    );
  });

  it("GRU-FR-LBPR: a blocked run's row leads with the sentence, not the raw message", () => {
    const run = makeRun("r1", "blocked", {
      blocker: {
        code: "review_checkout_failed",
        message: "symlink refused: /Users/someone/.synthesis/g/g1/rv/node_modules/mdn-data",
        clearsBy: "Continue the run to try the review again.",
        attempt: 1,
      },
    });

    const sentence = conditionSentence(run);

    expect(sentence).toBe("Blocked — The review could not be given a checkout to stand in.");
    expect(sentence).not.toContain("/Users/");
    expect(sentence).not.toContain("node_modules");
  });

  it("GRU-FR-FZCN, GRL-FR-XBUE: the three shapes that rest a run for the author read apart", () => {
    const escalated = makeRun("r1", "awaiting_author", {
      escalation: {
        reason: "Which one stands?",
        questions: [{ position: 1, question: "Which one?", options: [] }],
        origin: "work",
        raisedAt: "2026-09-06T09:00:00Z",
      },
    });
    const unsettled = makeRun("r1", "awaiting_author");
    const repeated = makeRun("r1", "awaiting_author", {
      blocker: {
        code: "review_checkout_failed",
        message: "symlink refused",
        clearsBy: "Continue.",
        attempt: 2,
      },
    });

    expect(conditionSentence(escalated)).toBe("Waiting for your answer");
    // GRL-FR-XBUE: a run whose budget window is spent says what it spent, so
    // the author reads a bound rather than a run that stopped for no reason.
    const spent = makeRun("r1", "awaiting_author", {
      checkpoint: { ...unsettled.checkpoint, pass: 2, passFloor: 1, passLimit: 2 },
    });
    expect(conditionSentence(spent)).toBe("Waiting for your decision · 2 passes spent");

    const wider = makeRun("r1", "awaiting_author", {
      checkpoint: { ...unsettled.checkpoint, passFloor: 1, passLimit: 4 },
    });
    expect(conditionSentence(wider)).toBe("Waiting for your decision · 4 passes spent");

    // A window Continue moved reports its own size rather than the pass number
    // it reached, so a second budget of two reads as two.
    const continued = makeRun("r1", "awaiting_author", {
      checkpoint: { ...unsettled.checkpoint, pass: 4, passFloor: 3, passLimit: 4 },
    });
    expect(conditionSentence(continued)).toBe("Waiting for your decision · 2 passes spent");

    // A budget of one reads as one pass rather than as "1 passes".
    const single = makeRun("r1", "awaiting_author", {
      checkpoint: { ...unsettled.checkpoint, passFloor: 1, passLimit: 1 },
    });
    expect(conditionSentence(single)).toBe("Waiting for your decision · 1 pass spent");

    // A run that rested before its first dispatch has no window, so it states
    // no spend rather than one it never made.
    expect(conditionSentence(unsettled)).toBe("Waiting for your decision");
    expect(conditionSentence(repeated)).toBe("Stopped twice on the same thing");
  });
});

describe("iterationLabel", () => {
  it("GRU-FR-PNVX: the label carries the bound the pass is counted against", () => {
    const run = makeRun("r1", "working", {
      checkpoint: { pass: 2 },
      observability: {
        observabilityVersion: 1,
        currentStage: "working",
        stageCondition: "active",
        stageHistory: [],
        passes: [],
      },
    });

    expect(iterationLabel(run)).toBe("pass 2 of 2");
  });

  it("GRU-FR-PNVX, GXD-FR-PWYD: the bound is the run's own window, not a constant", () => {
    // A project that configured a wider pass budget bounds its runs by it, and
    // the run carries that bound rather than the surface assuming one.
    const run = makeRun("r1", "working", {
      checkpoint: { pass: 3, passFloor: 1, passLimit: 5 },
      observability: {
        observabilityVersion: 1,
        currentStage: "working",
        stageCondition: "active",
        stageHistory: [],
        passes: [],
      },
    });

    expect(iterationLabel(run)).toBe("pass 3 of 5");
    expect(conditionSentence(run)).toBe("Working · pass 3 of 5");
  });
});

describe("retryStatement", () => {
  function withRetries(count: number) {
    return makeRun("r1", "blocked", {
      observability: {
        observabilityVersion: 1,
        currentStage: "review",
        stageCondition: "blocked",
        stageHistory: Array.from({ length: count }, () => ({
          from: "review" as const,
          to: "queued" as const,
          pass: 2,
          at: "2026-09-06T09:00:00Z",
          reason: "blocked_retry" as const,
        })),
        passes: [],
      },
    });
  }

  it("GRU-FR-LBPR: says how many times the author sent the run round again", () => {
    expect(retryStatement(withRetries(1))).toBe("Sent round again once.");
    expect(retryStatement(withRetries(3))).toBe("Sent round again 3 times.");
  });

  it("GRU-FR-LBPR: a run that never looped says nothing", () => {
    expect(retryStatement(withRetries(0))).toBeNull();
  });

  it("GRU-FR-LBPR: a review's own revision is a backward move but not a retry", () => {
    const run = makeRun("r1", "working", {
      observability: {
        observabilityVersion: 1,
        currentStage: "working",
        stageCondition: "active",
        stageHistory: [
          {
            from: "review",
            to: "working",
            pass: 2,
            at: "2026-09-06T09:00:00Z",
            reason: "review_revision",
          },
        ],
        passes: [],
      },
    });

    expect(retryStatement(run)).toBeNull();
  });

  it("GRU-FR-LBPR: a forward move is not a retry", () => {
    const run = makeRun("r1", "working", {
      observability: {
        observabilityVersion: 1,
        currentStage: "working",
        stageCondition: "active",
        stageHistory: [
          {
            from: "queued",
            to: "working",
            pass: 1,
            at: "2026-09-06T09:00:00Z",
            reason: "work_started",
          },
        ],
        passes: [],
      },
    });

    expect(retryStatement(run)).toBeNull();
  });
});
