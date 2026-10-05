/**
 * The sentences an update's row and window are both written from
 * (`../../../specifications/ui/WSS-work-stream-selector.md` WSS-FR-GTQL,
 * WSS-FR-PMYA), and the two enablement rules the row reads (WSS-FR-HZVQ,
 * WSS-FR-XRHT).
 *
 * Pure functions, tested directly: each of the record's seven states reads back
 * differently, and rendering covers only the states a row can reach.
 */

import { describe, expect, it } from "vitest";

import { updateStateSentence, updateSubject } from "./subject";
import {
  mergeRunLink,
  summary,
  updateAskedOnce,
  updateRecord,
} from "../../test/streamFixtures";
import { canUpdateStream, isReconciling } from "../../types";
import type { StreamUpdateState } from "../../types";

describe("what an update rests on, in words (WSS-FR-GTQL)", () => {
  it("WSS-FR-GTQL: every state reads back as its own sentence", () => {
    const said = (
      [
        "running",
        "updated",
        "nothing_to_update",
        "conflicted",
        "escalated",
        "cancelled",
        "failed",
      ] as StreamUpdateState[]
    ).map((state) => updateStateSentence(updateRecord(state)));

    expect(new Set(said).size).toBe(said.length);
    for (const sentence of said) {
      expect(sentence.length).toBeGreaterThan(0);
      // The state's own name is never what the author reads.
      expect(sentence).not.toMatch(/nothing_to_update/);
    }
  });

  it("WSS-FR-GTQL: a settled update names the source branch it brought work in from", () => {
    const said = updateStateSentence(
      updateRecord("updated", { updatedPaths: ["a.ts", "b.ts"], semanticTurns: 1 }),
    );

    expect(said).toMatch(/2 paths/);
    expect(said).toMatch(/from main/);
    expect(said).toMatch(/1 agent turn/);
  });

  it("WSS-FR-GTQL: a running update names the strategy the author chose", () => {
    expect(updateStateSentence(updateRecord("running"))).toMatch(
      /merge of the source/,
    );
    expect(
      updateStateSentence(updateRecord("running", { strategy: "rebase_source" })),
    ).toMatch(/rebase/);
  });

  it("WSS-FR-GTQL: a cancelled update says both branches are unchanged", () => {
    expect(updateStateSentence(updateRecord("cancelled"))).toMatch(
      /Neither branch was changed/,
    );
  });
});

describe("the record the resolution window is opened against (WSS-FR-PMYA)", () => {
  it("WSS-FR-PMYA: an update's view carries its stream, its base branch and its escalation", () => {
    const subject = updateSubject(updateAskedOnce());

    expect(subject.streamId).toBe("w1");
    expect(subject.baseBranch).toBe("main");
    expect(subject.escalation?.questions).toHaveLength(1);
    expect(subject.running).toBe(false);
    // An escalated record is answered or cleared, never retried.
    expect(subject.retryable).toBe(false);
  });

  it("WSS-FR-PMYA: a conflicted, cancelled or failed update is retryable and a running one is not", () => {
    expect(updateSubject(updateRecord("conflicted")).retryable).toBe(true);
    expect(updateSubject(updateRecord("cancelled")).retryable).toBe(true);
    expect(updateSubject(updateRecord("failed")).retryable).toBe(true);
    expect(updateSubject(updateRecord("running")).running).toBe(true);
    expect(updateSubject(updateRecord("running")).retryable).toBe(false);
  });
});

describe("when the row may offer Update (WSS-FR-HZVQ, WSS-FR-XRHT)", () => {
  it("WSS-FR-HZVQ: only a stream behind its base with no queued and no active run", () => {
    expect(canUpdateStream(summary({}, { behindBase: 2 }))).toBe(true);
    expect(canUpdateStream(summary({}, { behindBase: 0 }))).toBe(false);
    expect(
      canUpdateStream(summary({}, { behindBase: 2, queuedRunCount: 1 })),
    ).toBe(false);
    expect(
      canUpdateStream(summary({ busyRunId: "r-1" }, { behindBase: 2 })),
    ).toBe(false);
    expect(
      canUpdateStream(summary({ isMissing: true }, { behindBase: 2 })),
    ).toBe(false);
  });

  it("WSS-FR-XRHT: a running update or a merge run that holds the stream makes it reconciling", () => {
    expect(isReconciling(summary({}, { behindBase: 2 }))).toBe(false);
    for (const state of [
      "queued",
      "working",
      "reviewing",
      "blocked",
      "interrupted",
      "awaiting_author",
    ] as const) {
      expect(
        isReconciling(
          summary({}, { behindBase: 2, mergeRun: mergeRunLink(state) }),
        ),
      ).toBe(true);
    }
    // A merge run that has completed or failed lets go of the stream.
    for (const state of ["completed", "failed"] as const) {
      expect(
        isReconciling(
          summary({}, { behindBase: 2, mergeRun: mergeRunLink(state) }),
        ),
      ).toBe(false);
    }
    expect(
      isReconciling(
        summary({}, { behindBase: 2, update: updateRecord("running") }),
      ),
    ).toBe(true);
    expect(
      isReconciling(
        summary({}, { behindBase: 2, update: updateRecord("conflicted") }),
      ),
    ).toBe(false);
  });
});
