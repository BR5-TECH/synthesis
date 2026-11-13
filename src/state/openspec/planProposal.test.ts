/**
 * The plan proposal of an apply run at rest on a plan rest
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-XJMR;
 * `../../../specifications/core/OPU-openspec-plan-updates.md` OPU-FR-EVBZ,
 * OPU-FR-UOYJ, OPU-FR-WOKJ).
 */
import { describe, expect, it } from "vitest";

import { makeRun } from "../../test/graduationFixtures";
import type { GraduationRest, GraduationRun, PendingQuestionSet } from "../../types";
import { planRestForProposal, planSetStands, type PlanProposalRest } from "./planProposal";

const AT = "2026-11-08T09:00:00Z";
const STREAM = "s-1";
const CHANGE = "add-logout";
const SET_ID = "qs-1";

const UPDATE: GraduationRest = {
  reason: "plan_update",
  at: AT,
  origin: "work",
  threadId: "pt-1",
  proposalId: "p-1",
  setId: SET_ID,
};

/** An apply run of `change` on `streamId`, at rest on `rest`. */
function applyRun(
  rest: GraduationRest,
  over: { state?: GraduationRun["state"]; streamId?: string; change?: string; id?: string } = {},
): GraduationRun {
  return makeRun(over.id ?? "r1", over.state ?? "awaiting_author", {
    streamId: over.streamId ?? STREAM,
    openspec: { phase: "apply", change: over.change ?? CHANGE },
    input: { draftId: "", draftName: "", prompt: "p", promptChecksum: "c", capturedAt: AT },
    rest,
  });
}

function found(rest: GraduationRest, state: GraduationRun["state"] = "awaiting_author"): PlanProposalRest {
  const run = applyRun(rest, { state });
  const hit = planRestForProposal([run], STREAM, CHANGE, rest.proposalId ?? null);
  if (!hit) throw new Error("the fixture names no plan proposal");
  return hit;
}

function set(setId: string): PendingQuestionSet {
  return {
    setId,
    discussionId: "pt-1",
    askedBy: { kind: "agent", agentId: "synthesis", handle: "Synthesis", title: "" },
    askedAt: AT,
    questions: [{ position: 1, text: "Which header?", options: [] }],
  };
}

describe("planRestForProposal (OPU-FR-UOYJ, OPU-FR-WOKJ)", () => {
  it("OPU-FR-UOYJ: the run whose plan rest names the proposal on this stream and change is found", () => {
    const run = applyRun(UPDATE);
    const hit = planRestForProposal([run], STREAM, CHANGE, "p-1");
    expect(hit?.run).toBe(run);
    expect(hit?.rest.reason).toBe("plan_update");
  });

  it("OPU-FR-UOYJ, OPU-FR-WOKJ: a run on another stream or of another change does not name the proposal", () => {
    const otherStream = applyRun(UPDATE, { streamId: "s-2", id: "r2" });
    const otherChange = applyRun(UPDATE, { change: "add-login", id: "r3" });
    expect(planRestForProposal([otherStream, otherChange], STREAM, CHANGE, "p-1")).toBeNull();
    // The same runs are found for their own stream and change.
    expect(planRestForProposal([otherStream, otherChange], "s-2", CHANGE, "p-1")?.run).toBe(otherStream);
    expect(planRestForProposal([otherStream, otherChange], STREAM, "add-login", "p-1")?.run).toBe(
      otherChange,
    );
  });

  it("OPU-FR-WOKJ: no proposal id, another proposal id, or a rest that is no plan rest finds nothing", () => {
    const run = applyRun(UPDATE);
    expect(planRestForProposal([run], STREAM, CHANGE, null)).toBeNull();
    expect(planRestForProposal([run], STREAM, CHANGE, "p-other")).toBeNull();
    const checks = applyRun({ reason: "checks_not_passed", at: AT, proposalId: "p-1" });
    expect(planRestForProposal([checks], STREAM, CHANGE, "p-1")).toBeNull();
  });
});

describe("planSetStands (OCW-FR-XJMR, OPU-FR-EVBZ)", () => {
  it("OCW-FR-XJMR, OPU-FR-EVBZ: before the read answers, a run that waits with no answers holds the proposal", () => {
    expect(planSetStands(found(UPDATE), undefined)).toBe(true);
    expect(planSetStands(found({ ...UPDATE, answers: [] }), undefined)).toBe(true);
  });

  it("OCW-FR-XJMR, OPU-FR-EVBZ: before the read answers, a run whose set was answered does not hold the proposal", () => {
    const answered: GraduationRest = {
      ...UPDATE,
      answers: [{ questionPosition: 1, ownAnswer: "The shared header." }],
    };
    expect(planSetStands(found(answered), undefined)).toBe(false);
  });

  it("OCW-FR-XJMR: before the read answers, a run that no longer waits on the author does not hold the proposal", () => {
    expect(planSetStands(found(UPDATE, "working"), undefined)).toBe(false);
  });

  it("OCW-FR-XJMR, OPU-FR-EVBZ: the set of the rest holds the proposal, and a set of another id or no set does not", () => {
    expect(planSetStands(found(UPDATE), set(SET_ID))).toBe(true);
    expect(planSetStands(found(UPDATE), set("qs-other"))).toBe(false);
    expect(planSetStands(found(UPDATE), null)).toBe(false);
  });

  it("OCW-FR-XJMR: a plan_update rest with no set id holds nothing", () => {
    expect(planSetStands(found({ ...UPDATE, setId: null }), undefined)).toBe(false);
    expect(planSetStands(found({ ...UPDATE, setId: null }), set(SET_ID))).toBe(false);
  });

  it("OCW-FR-XJMR: a plan_findings rest holds nothing, also while a set stands in its thread", () => {
    const findings: GraduationRest = {
      reason: "plan_findings",
      at: AT,
      origin: "review",
      threadId: "pt-1",
      proposalId: "p-1",
      setId: SET_ID,
    };
    expect(planSetStands(found(findings), undefined)).toBe(false);
    expect(planSetStands(found(findings), set(SET_ID))).toBe(false);
  });
});
