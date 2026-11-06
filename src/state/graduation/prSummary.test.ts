/**
 * The pure rules a PR summary run is rendered from
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-WEBM,
 * GRU-FR-AQKZ, GRU-FR-DQSA, GRU-FR-LJUY).
 */

import { beforeEach, describe, expect, it, vi } from "vitest";

import { invoke } from "@tauri-apps/api/core";

import { startPullRequestSummary } from "../../api/streams";
import { resetLogBufferForTest } from "../../logging";
import { makePrSummaryRun, makeQueue, makeRun } from "../../test/graduationFixtures";
import { raiseStatement } from "./escalations";
import {
  actionsFor,
  canRestart,
  conditionSentence,
  holdsItsQueue,
  isPrSummaryRun,
  iterationLabel,
  outcomeStatement,
  prSummaryResultOf,
  runQueueKey,
  runsRemovedByDiscard,
  runTitle,
  stageDescriptorsFor,
  targetName,
} from ".";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (command: string) => {
    if (command === "append_log_records") return undefined;
    throw new Error(`unexpected command ${command}`);
  });
  resetLogBufferForTest();
});

describe("a PR summary run (GRU-FR-WEBM, GRU-FR-AQKZ, GRU-FR-DQSA, GRU-FR-LJUY)", () => {
  it("GRU-FR-WEBM: its title is `Summarize <head>`, and it names its head branch and queue", () => {
    const run = makePrSummaryRun("p1");
    expect(isPrSummaryRun(run)).toBe(true);
    expect(isPrSummaryRun(makeRun("r1"))).toBe(false);
    expect(runTitle(run)).toBe("Summarize feature/panel");
    expect(targetName(run)).toBe("feature/panel");
    expect(runQueueKey(run)).toBe("pr:feature/panel");
  });

  it("GRU-FR-AQKZ: three stages, and no pass in what the row says", () => {
    const run = makePrSummaryRun("p1", "working", { checkpoint: { pass: 2, passLimit: 2 } });
    expect(stageDescriptorsFor(run).map((stage) => stage.label)).toEqual([
      "Queued",
      "Summarizing",
      "Done",
    ]);
    expect(conditionSentence(run)).toBe("Summarizing");
    expect(iterationLabel(run)).toBeNull();
  });

  it("GRU-FR-DQSA: the result is read in `completed` alone", () => {
    const result = { title: "t", description: "d" };
    expect(prSummaryResultOf(makePrSummaryRun("p1", "completed", { prSummary: { result } }))).toEqual(
      result,
    );
    for (const state of ["working", "failed", "interrupted", "discarded"] as const) {
      expect(prSummaryResultOf(makePrSummaryRun("p1", state, { prSummary: { result } }))).toBeNull();
    }
    const failed = makePrSummaryRun("p1", "failed", {
      failure: { code: "unreadable_answer", message: "Not readable.", retryable: false },
    });
    expect(outcomeStatement(failed)).toBe("Not readable.");
  });

  it("GRU-FR-LJUY: Pause, Continue and Discard on the terms of every run; no Open draft, no Restart, no Revert", () => {
    expect(actionsFor(makePrSummaryRun("p1", "working"))).toEqual(["pause", "discard"]);
    expect(actionsFor(makePrSummaryRun("p1", "interrupted"))).toEqual(["continue", "discard"]);
    expect(actionsFor(makePrSummaryRun("p1", "discarded"))).toEqual([]);
    expect(actionsFor(makePrSummaryRun("p1", "failed"))).toEqual(["discard"]);
    // A record that somehow names commits still offers no Revert.
    expect(actionsFor(makePrSummaryRun("p1", "interrupted", { commits: ["abc"] }))).not.toContain(
      "revert",
    );
    expect(canRestart(makePrSummaryRun("p1", "discarded"))).toBe(false);
  });

  it("GRD-FR-CCHK: a failed PR summary run holds no queue, so its discard removes no run", () => {
    const failed = makePrSummaryRun("p1", "failed", { dispatched: true });
    const behind = makePrSummaryRun("p2", "queued");
    expect(holdsItsQueue(failed)).toBe(false);
    expect(runsRemovedByDiscard(makeQueue([failed, behind]), failed)).toEqual([]);
    // A failed draft run still holds its queue.
    expect(holdsItsQueue(makeRun("r1", "failed"))).toBe(true);
  });

  it("GRU-FR-DQSA: the notification of a completed run says it wrote the summary, and names no branch it committed on", () => {
    const run = makePrSummaryRun("p1", "completed");
    expect(raiseStatement(run)).toBe("“Summarize feature/panel” wrote the pull request summary.");
    expect(raiseStatement(makePrSummaryRun("p1", "failed"))).toBe("“Summarize feature/panel” failed.");
  });

  it("GRU-FR-WEBM: a queued run waits in the queue of its head branch", () => {
    const run = makePrSummaryRun("p1", "queued");
    expect(conditionSentence(run, 0)).toBe("Next in the head branch's queue");
    expect(conditionSentence(run)).toBe("Waiting to start");
    expect(actionsFor(run)).toEqual(["discard"]);
    expect(actionsFor(makePrSummaryRun("p1", "completed"))).toEqual([]);
  });

  it("GRD-FR-HMDW: the start names the head, the base and the mode", async () => {
    vi.mocked(invoke).mockImplementationOnce(async () => makePrSummaryRun("p1", "queued"));
    await startPullRequestSummary("feature/panel", "main", "combine");
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("start_pull_request_summary", {
      head: "feature/panel",
      base: "main",
      mode: "combine",
    });
  });
});
