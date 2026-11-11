/**
 * The words of an apply run at rest on a plan rest
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-FZCN,
 * GRU-FR-TCSM, GRU-FR-TVDF, GRU-FR-PZJY, GRU-FR-BPNJ, GRU-FR-KWQX;
 * `../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-YRZK).
 */
import { describe, expect, it } from "vitest";

import { makeRun } from "../../test/graduationFixtures";
import type { GraduationRest, GraduationRun, ReviewFinding } from "../../types";
import {
  PLAN_REST_STRIP_WORDS,
  conditionSentence,
  continueRefusalMessage,
  graduationErrorMessage,
  planContinueMessage,
  planContinueRefusal,
  planFindingCount,
  planRestOf,
  planRestStatement,
  planThreadOf,
  planUpdateCauseSentence,
  raiseLevel,
  raiseStatement,
  raisesForRun,
} from ".";

const AT = "2026-11-08T09:00:00Z";

function finding(kind: ReviewFinding["kind"]): ReviewFinding {
  return { severity: "major", description: "d", affectedFiles: [], correction: "c", kind };
}

function applyRun(rest: GraduationRest, state: GraduationRun["state"] = "awaiting_author"): GraduationRun {
  return makeRun("r1", state, {
    // A spent budget window, so a test can see that a plan rest does not say it.
    checkpoint: { pass: 2, passLimit: 2, passFloor: 1 },
    openspec: { phase: "apply", change: "add-logout" },
    input: { draftId: "", draftName: "", prompt: "p", promptChecksum: "c", capturedAt: AT },
    rest,
  });
}

const UPDATE: GraduationRest = {
  reason: "plan_update",
  at: AT,
  origin: "work",
  threadId: "pt-1",
  proposalId: "p-1",
  setId: "qs-1",
  planReason: "The plan is wrong.",
};
const FINDINGS: GraduationRest = {
  reason: "plan_findings",
  at: AT,
  origin: "review",
  threadId: "pt-2",
  findings: [finding("plan"), finding("work"), finding("plan"), finding(undefined)],
};
const INVALID: GraduationRest = { reason: "plan_update_invalid", at: AT, cause: "plan_deletes_file" };

describe("plan rests", () => {
  it("GRU-FR-FZCN, GRU-FR-TCSM, GRU-FR-TVDF, GRU-FR-PZJY: the condition line names the plan rest and no spent pass budget", () => {
    expect(conditionSentence(applyRun(UPDATE))).toBe("Waiting for your plan decisions");
    expect(conditionSentence(applyRun(FINDINGS))).toBe("Waiting for you to settle the plan findings");
    expect(conditionSentence(applyRun(INVALID))).toBe("Stopped: the plan update was refused");
    for (const rest of [UPDATE, FINDINGS, INVALID]) {
      expect(conditionSentence(applyRun(rest))).not.toMatch(/spent/);
    }
  });

  it("GRU-FR-TCSM: a plan_update rest says that the run asks to change the plan and where the author answers it", () => {
    const statement = planRestStatement(planRestOf(applyRun(UPDATE))!);
    expect(statement).toContain("The run asks to change the plan of its change.");
    expect(statement).toContain("Answer its questions and decide its edits in the change discussion");
  });

  it("GRU-FR-TVDF: a plan_findings rest counts the plan findings alone, a finding with no kind being work", () => {
    expect(planFindingCount(FINDINGS)).toBe(2);
    expect(planRestStatement(planRestOf(applyRun(FINDINGS))!)).toContain(
      "The review found that the plan of the change is wrong: 2 plan findings.",
    );
    const one = { ...FINDINGS, findings: [finding("plan")] };
    expect(planRestStatement(planRestOf(applyRun(one))!)).toContain("1 plan finding.");
    expect(planFindingCount({ ...FINDINGS, findings: null })).toBe(0);
  });

  it("GRU-FR-PZJY: a plan_update_invalid rest says why, with one sentence for each cause", () => {
    expect(planRestStatement(planRestOf(applyRun(INVALID))!)).toBe(
      "The run gave a plan update that the application refused. It proposed to delete a file of the change, and a plan update cannot delete a file.",
    );
    const causes = ["plan_path_outside_change", "plan_file_not_text", "plan_deletes_file", "plan_update_empty"] as const;
    expect(new Set(causes.map(planUpdateCauseSentence)).size).toBe(causes.length);
    expect(planUpdateCauseSentence(undefined)).toBe("The cause was not recorded.");
  });

  it("GRU-FR-TCSM, GRU-FR-TVDF: the plan thread is the thread of a plan_update and a plan_findings rest, and a plan_update_invalid rest has none", () => {
    expect(planThreadOf(applyRun(UPDATE))).toBe("pt-1");
    expect(planThreadOf(applyRun(FINDINGS))).toBe("pt-2");
    expect(planThreadOf(applyRun(INVALID))).toBeNull();
    expect(planThreadOf(applyRun({ ...UPDATE, threadId: null }))).toBeNull();
  });

  it("GRU-FR-TCSM: a run that is not in awaiting_author reads no plan rest", () => {
    expect(planRestOf(applyRun(UPDATE, "working"))).toBeNull();
    expect(planThreadOf(applyRun(UPDATE, "completed"))).toBeNull();
    expect(planRestOf(applyRun({ reason: "checks_not_passed", at: AT }))).toBeNull();
  });

  it("OCW-FR-YRZK: the strip words of each plan rest", () => {
    expect(PLAN_REST_STRIP_WORDS).toEqual({
      plan_update: "waiting for plan decisions",
      plan_findings: "plan findings to settle",
      plan_update_invalid: "plan update refused",
    });
  });

  it("GRU-FR-BPNJ: a refused Continue of a plan rest is stated in words, and another code is not a plan refusal", () => {
    expect(planContinueMessage("plan_questions_pending")).toBe(
      "Answer the questions in the change discussion first, then continue the run.",
    );
    expect(planContinueMessage("proposal_pending")).toBe(
      "Decide the plan edits in the change discussion first, or their commit is still owed. Then continue the run.",
    );
    expect(planContinueMessage("openspec_validation_unavailable: cli_failed")).toBe(
      "OpenSpec could not be read (cli_failed), so the run did not continue.",
    );
    expect(planContinueMessage("Error: openspec_validation_unavailable")).toBe(
      "OpenSpec could not be read, so the run did not continue.",
    );
    expect(planContinueRefusal("stream_busy", null)).toBeNull();
  });

  it("GRU-FR-BPNJ: the plan words are for a plan rest only, and every other run keeps its words", () => {
    // The general words do not know the plan codes, so they render the code as itself.
    expect(graduationErrorMessage("proposal_pending")).toBe("proposal_pending");
    expect(graduationErrorMessage("plan_questions_pending")).toBe("plan_questions_pending");
    expect(graduationErrorMessage("write_failed")).toBe("write_failed");
    const atPlanRest = applyRun(UPDATE);
    const working = applyRun(UPDATE, "working");
    const atChecksRest = applyRun({ reason: "checks_not_passed", at: AT });
    expect(continueRefusalMessage(atPlanRest, "Error: write_failed")).toBe(
      "The run could not be made ready to continue. Nothing changed; try Continue again.",
    );
    expect(continueRefusalMessage(atPlanRest, "proposal_pending")).toBe(
      "Decide the plan edits in the change discussion first, or their commit is still owed. Then continue the run.",
    );
    for (const run of [working, atChecksRest]) {
      expect(continueRefusalMessage(run, "proposal_pending")).toBe("proposal_pending");
      expect(continueRefusalMessage(run, "Error: unknown_run")).toBe("That run is not there any more.");
    }
    // A code that is no plan refusal keeps its words at a plan rest too.
    expect(continueRefusalMessage(atPlanRest, "unknown_run")).toBe("That run is not there any more.");
  });

  it("GRU-FR-KWQX, GRU-FR-BLSS: a plan rest raises at Warn, and its body names the change and where the author decides it", () => {
    for (const rest of [UPDATE, FINDINGS, INVALID]) {
      const run = applyRun(rest);
      expect(raisesForRun(run)).toBe(true);
      expect(raiseLevel(run)).toBe("Warn");
      expect(raiseStatement(run)).toContain("add-logout");
    }
    expect(raiseStatement(applyRun(UPDATE))).toBe(
      "“Apply add-logout” asks to change the plan of add-logout. Decide it in the change discussion.",
    );
    expect(raiseStatement(applyRun(FINDINGS))).toBe(
      "The review of “Apply add-logout” found the plan of add-logout wrong. Decide it in the change discussion.",
    );
    expect(raiseStatement(applyRun(INVALID))).toBe(
      "“Apply add-logout” gave a plan update for add-logout that was refused.",
    );
    expect(raiseStatement(applyRun(INVALID))).not.toContain("change discussion");
  });
});

describe("GRU-FR-BPNJ: write_failed and the plan Continue message", () => {
  it("GRU-FR-BPNJ: a write_failed Continue says that the run could not be made ready and that Continue can be tried again", () => {
    const words = planContinueRefusal("write_failed", null);
    expect(words).toMatch(/could not be made ready to continue/);
    expect(words).toMatch(/try Continue again/);
    expect(planContinueMessage("Error: write_failed")).toBe(words);
    expect(planContinueMessage("openspec_validation_unavailable:cli_timed_out")).toBe(
      "OpenSpec could not be read (cli_timed_out), so the run did not continue.",
    );
    expect(planContinueMessage("unknown_run")).toBe("That run is not there any more.");
  });
});
