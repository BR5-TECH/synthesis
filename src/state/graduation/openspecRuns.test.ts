/**
 * The words of an OpenSpec change run
 * (`../../../specifications/core/GOB-graduation-observability.md` GOB-FR-CAXV,
 * GOB-FR-TEAY; `../../../specifications/ui/GRU-graduation-runs.md`
 * GRU-FR-SOFY, GRU-FR-HABG).
 */
import { describe, expect, it } from "vitest";

import { makeObservability, makeRun } from "../../test/graduationFixtures";
import type { GraduationRun } from "../../types";
import { runTitle } from "./merge";
import { actionsFor } from "./runState";
import { conditionOf, conditionSentence, stageDescriptorsFor, stageOf } from "./stages";
import { changeRunOutcome, changeRunTitle, reworkBlockerSentence, tasksSentence } from "./openspecRuns";

const run = (phase: "propose" | "apply" | "rework", over: Partial<GraduationRun> = {}) =>
  makeRun("r1", "working", { openspec: { phase, change: "add-logout" }, ...over });

describe("OpenSpec change run words", () => {
  it("GOB-FR-CAXV: the labels are selected by the phase of the openspec data, and a propose run keeps a draft run's", () => {
    expect(stageDescriptorsFor(run("apply")).map((s) => s.label)).toEqual(["Queued", "Tasks", "Verify", "Landed"]);
    expect(stageDescriptorsFor(run("rework")).map((s) => s.label)).toEqual([
      "Queued",
      "Reworking",
      "Reviewing",
      "Proposed",
    ]);
    expect(stageDescriptorsFor(run("propose")).map((s) => s.label)).toEqual(["Queued", "Working", "Review", "Done"]);
  });

  it("GOB-FR-CAXV: a stage with another condition keeps the label of its stage, with the condition of the record", () => {
    const blocked = run("apply", {
      state: "blocked",
      observability: makeObservability({ currentStage: "working", stageCondition: "blocked" }),
    });
    expect(stageDescriptorsFor(blocked).find((s) => s.id === "working")?.label).toBe("Tasks");
    expect(conditionOf(blocked)).toBe("blocked");
  });

  it("GOB-FR-TEAY: a rework run blocked at its finish stands at Reviewing with the condition blocked", () => {
    const finish = run("rework", {
      state: "blocked",
      observability: makeObservability({ currentStage: "review", stageCondition: "blocked" }),
    });
    expect(stageOf(finish)).toBe("review");
    expect(conditionOf(finish)).toBe("blocked");
    expect(stageDescriptorsFor(finish).find((s) => s.id === "review")?.label).toBe("Reviewing");
  });

  it("GOB-FR-CAXV: the review sentence of an apply run says Verify", () => {
    expect(conditionSentence(run("apply", { state: "reviewing" }))).toMatch(/^Verify · pass 1 of/);
  });

  it("GRU-FR-HABG, OCR-FR-FZVF: the title is Apply or Rework and the change, where a draft run names its draft", () => {
    expect(runTitle(run("apply"))).toBe("Apply add-logout");
    expect(runTitle(run("rework"))).toBe("Rework add-logout");
    expect(changeRunTitle(run("propose"))).toBeNull();
    expect(runTitle(run("propose"))).toBe("Run r1");
  });

  it("GRU-FR-HABG, GRU-FR-WDWB: an OpenSpec change run offers no Open change, and Restart once discarded", () => {
    expect(actionsFor(run("apply"))).not.toContain("open-draft");
    expect(actionsFor(run("rework", { state: "discarded" }))).toEqual(["restart"]);
  });

  it("GRU-FR-PXCW: the Tasks sentence leaves out a next task where none is open", () => {
    const base = { state: "all_done" as const, total: 4, complete: 4, missingArtifacts: [] };
    expect(tasksSentence({ ...base, nextTask: null })).toBe("4 of 4 tasks");
    expect(tasksSentence({ ...base, nextTask: "  " })).toBe("4 of 4 tasks");
  });

  it("GRU-FR-IKEQ, GRU-FR-NGQE, PVC-FR-NEXL: the words of a rework run name an OpenSpec change, never a bare change", () => {
    const proposed = run("rework", {
      state: "completed",
      openspec: { phase: "rework", change: "add-logout", proposalId: "p1" },
    });
    expect(changeRunOutcome(proposed)).toBe("The result is a proposal in the discussion of the OpenSpec change.");
    expect(changeRunOutcome(run("rework", { state: "discarded" }))).toBe(
      "Discarded. Nothing was written to the OpenSpec change.",
    );
    expect(reworkBlockerSentence(run("rework", { state: "blocked" }), "proposal_pending")).toBe(
      "The OpenSpec change holds a pending edit proposal, so the result of the rework could not be recorded. " +
        "Decide that proposal first, then continue the run.",
    );
  });
});
