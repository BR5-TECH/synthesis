/**
 * What an interrupted run says about its stop, wherever it says it
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-BHJO,
 * GRU-FR-BLSS, GRU-FR-FJZD, and `NTF-notifications.md` NTF-FR-24).
 */

import { describe, expect, it } from "vitest";

import { makeRun } from "../../test/graduationFixtures";
import type { GraduationInterruptionReason } from "../../types/graduation";
import { raiseStatement, raisesForRun } from "./escalations";
import { conditionSentence, interruptionCause, isFailureStop } from "./stages";

function interrupted(reason: GraduationInterruptionReason) {
  return makeRun("r1", "interrupted", {
    interruption: {
      reason,
      detail: "What the backend said.",
      streamReleased: true,
      at: "2026-10-03T22:02:36Z",
    },
  });
}

const FAILURES: GraduationInterruptionReason[] = [
  "execution_timeout",
  "agent_exited",
  "agent_terminated",
  "unreadable_answer",
  "launch_failed",
  "log_persistence_failed",
  "execution_abandoned",
  "retryable_failure",
];
const DELIBERATE: GraduationInterruptionReason[] = [
  "author_pause",
  "application_shutdown",
  "project_changed",
];

describe("the headline of an interrupted run", () => {
  it.each([
    ["execution_timeout", "Stopped — the turn reached its time limit"],
    ["agent_exited", "Stopped — the agent process exited"],
    ["agent_terminated", "Stopped — the agent process was terminated"],
    ["unreadable_answer", "Stopped — the agent's answer could not be read"],
    ["launch_failed", "Stopped — the agent turn could not start"],
    ["retryable_failure", "Stopped — the cause was not recorded"],
    ["log_persistence_failed", "Stopped — its log could not be written"],
    ["execution_abandoned", "Stopped — the application closed while it was working"],
    ["author_pause", "Paused — waiting for Resume"],
    ["application_shutdown", "Stopped when the application closed"],
    ["project_changed", "Stopped when the project changed"],
  ] as [GraduationInterruptionReason, string][])(
    "GRU-FR-BHJO: %s names its own cause",
    (reason, headline) => {
      expect(conditionSentence(interrupted(reason))).toBe(headline);
    },
  );

  it("GRU-FR-BHJO: no two reasons share a headline", () => {
    const headlines = [...FAILURES, ...DELIBERATE].map((reason) =>
      conditionSentence(interrupted(reason)),
    );
    expect(new Set(headlines).size).toBe(headlines.length);
  });

  it("GRU-FR-BHJO: a run with no recorded interruption still says it waits for Continue", () => {
    expect(conditionSentence(makeRun("r1", "interrupted"))).toBe(
      "Stopped — waiting for Continue",
    );
  });
});

describe("the raise of an interrupted run", () => {
  it.each(FAILURES)("GRU-FR-BLSS, NTF-FR-24: a run stopped on %s raises", (reason) => {
    expect(isFailureStop(reason)).toBe(true);
    expect(raisesForRun(interrupted(reason))).toBe(true);
  });

  it.each(DELIBERATE)(
    "GRU-FR-BLSS, NTF-FR-24: a run stopped on %s raises nothing",
    (reason) => {
      expect(isFailureStop(reason)).toBe(false);
      expect(raisesForRun(interrupted(reason))).toBe(false);
    },
  );

  it("GRU-FR-BLSS: an interrupted run with no recorded interruption raises nothing", () => {
    expect(raisesForRun(makeRun("r1", "interrupted"))).toBe(false);
  });

  it("GRU-FR-BLSS: progress still raises nothing", () => {
    expect(raisesForRun(makeRun("r1", "working"))).toBe(false);
    expect(raisesForRun(makeRun("r1", "queued"))).toBe(false);
  });

  it("GRU-FR-FJZD: the raise names the run and the cause its headline states", () => {
    const run = interrupted("execution_timeout");
    expect(raiseStatement(run)).toBe(
      `“${run.input.draftName}” stopped: the turn reached its time limit.`,
    );
  });

  it.each(FAILURES)(
    "GRU-FR-FJZD: the raise for %s names the same cause as the headline",
    (reason) => {
      const run = interrupted(reason);
      const cause = conditionSentence(run).replace(/^Stopped — /, "");
      expect(cause).toBe(interruptionCause(reason));
      expect(raiseStatement(run)).toBe(`“${run.input.draftName}” stopped: ${cause}.`);
    },
  );

  it("GRU-FR-FJZD: an interrupted run with no recorded cause still names the run", () => {
    const run = makeRun("r1", "interrupted");
    expect(raiseStatement(run)).toBe(`“${run.input.draftName}” stopped.`);
  });
});
