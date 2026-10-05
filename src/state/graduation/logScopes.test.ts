import { describe, expect, it } from "vitest";

import { makeLogs, makeObservability, makeRun } from "../../test/graduationFixtures";
import {
  awaitsLiveSegment,
  hasRunLevelEntry,
  openingEntry,
  passesInPhase,
  scopeEntries,
  scopeKey,
  scopeOf,
  stageHoldsLogs,
  stagesWithLogAccess,
} from "./logScopes";
import type { ProgressStage } from "../runProgress";

/** A run whose stage history entered three stages over two passes. */
function run(over: Parameters<typeof makeRun>[2] = {}) {
  return makeRun("r1", "working", {
    observability: makeObservability({
      stageHistory: [
        { from: "queued", to: "queued", pass: 1, at: "1", reason: "enqueued" },
        { from: "queued", to: "working", pass: 1, at: "2", reason: "work_started" },
        { from: "working", to: "review", pass: 1, at: "3", reason: "review_started" },
        { from: "review", to: "working", pass: 2, at: "4", reason: "review_revision" },
      ],
    }),
    ...over,
  });
}

describe("stage eligibility", () => {
  it("GRU-FR-TQJW: a stage a pass-scoped segment names holds a log", () => {
    const logs = makeLogs({ structured: [{ phaseId: "working", pass: 1 }] });
    expect(stageHoldsLogs(logs, "working")).toBe(true);
    expect(stageHoldsLogs(logs, "review")).toBe(false);
  });

  it("GRU-FR-TQJW: a stage whose only segment names `pass = null` holds a log", () => {
    const logs = makeLogs({ source: [{ phaseId: "queued", pass: null }] });
    expect(stageHoldsLogs(logs, "queued")).toBe(true);
  });

  it("GRU-FR-TQJW: a segment of either stream makes the stage eligible", () => {
    const logs = makeLogs({ structured: [{ phaseId: "done", pass: null }] });
    expect(stageHoldsLogs(logs, "done")).toBe(true);
  });

  it("GRS-FR-MBED: a storage version this build does not know makes no stage eligible", () => {
    const logs = makeLogs({
      logStorageVersion: 2,
      source: [{ phaseId: "working", pass: 1 }],
    });
    expect(stageHoldsLogs(logs, "working")).toBe(false);
  });

  it("GRU-FR-ZBMU, GRU-FR-VKPD: any stage list at all, and a sentence for one that holds nothing", () => {
    // Seven stages of ids this feature has never met, to establish that no
    // stage id, label, or count is written into the rule.
    const stages: ProgressStage[] = ["alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta"].map(
      (id) => ({ id, label: id.toUpperCase() }),
    );
    const logs = makeLogs({ source: [{ phaseId: "epsilon", pass: null }] });
    const decorated = stagesWithLogAccess(stages, logs);
    expect(decorated).toHaveLength(7);
    expect(decorated.map((stage) => stage.id)).toEqual(stages.map((s) => s.id));
    expect(decorated.find((stage) => stage.id === "epsilon")?.activatable).toBe(true);
    const beta = decorated.find((stage) => stage.id === "beta");
    expect(beta?.activatable).toBe(false);
    expect(beta?.disabledReason).toContain("no log yet");
  });
});

describe("the window's pass list", () => {
  it("GLW-FR-CKLZ, GLW-FR-CQSE: the passes that entered the stage, from the persisted history", () => {
    expect(passesInPhase(run(), "working")).toEqual([1, 2]);
    expect(passesInPhase(run(), "review")).toEqual([1]);
    expect(passesInPhase(run(), "done")).toEqual([]);
  });

  it("GLW-FR-RVKT: the run-level entry stands exactly where a `pass = null` segment does", () => {
    const withRunLevel = run({
      logs: makeLogs({ structured: [{ phaseId: "queued", pass: null }] }),
    });
    expect(hasRunLevelEntry(withRunLevel.logs, "queued")).toBe(true);
    expect(hasRunLevelEntry(withRunLevel.logs, "working")).toBe(false);
  });

  it("GLW-FR-RVKT: both streams settle it, so the stream toggle adds and removes no entry", () => {
    const logs = makeLogs({ source: [{ phaseId: "done", pass: null }] });
    expect(hasRunLevelEntry(logs, "done")).toBe(true);
  });

  it("GLW-FR-CKLZ, GLW-FR-DDXJ: the run-level entry follows the passes and is not numbered as one", () => {
    const entries = scopeEntries(
      run({
        logs: makeLogs({
          structured: [
            { phaseId: "working", pass: 1 },
            { phaseId: "working", pass: null },
          ],
        }),
      }),
      "working",
    );
    expect(entries.map((entry) => entry.label)).toEqual([
      "Pass 1",
      "Pass 2",
      "Run-level",
    ]);
    expect(entries[2].pass).toBeNull();
    expect(entries[2].kind).toBe("run_level");
  });

  it("GLW-FR-PDGA: an entry names its own read scope and no other", () => {
    const entries = scopeEntries(
      run({ logs: makeLogs({ source: [{ phaseId: "working", pass: null }] }) }),
      "working",
    );
    expect(scopeOf(entries[0])).toEqual({ kind: "pass", pass: 1 });
    expect(scopeOf(entries[entries.length - 1])).toEqual({ kind: "run_level" });
  });
});

describe("the opening selection", () => {
  it("GLW-FR-ELJO: the newest listed pass whose own records the selected stream holds", () => {
    const target = run({
      logs: makeLogs({
        source: [
          { phaseId: "working", pass: 1 },
          { phaseId: "working", pass: 2 },
        ],
      }),
    });
    expect(openingEntry(target, "working", "source")?.key).toBe("pass-2");
  });

  it("GLW-FR-ELJO: an older pass wins where the newest one printed nothing", () => {
    const target = run({
      logs: makeLogs({ source: [{ phaseId: "working", pass: 1 }] }),
    });
    expect(openingEntry(target, "working", "source")?.key).toBe("pass-1");
  });

  it("GLW-FR-YVKD: a stage holding run-level records alone opens on the run-level entry", () => {
    const target = run({
      logs: makeLogs({ source: [{ phaseId: "review", pass: null }] }),
    });
    expect(openingEntry(target, "review", "source")?.key).toBe("run-level");
  });

  it("GLW-FR-ELJO: the run-level entry stays available but never displaces the newest pass", () => {
    const target = run({
      logs: makeLogs({
        source: [
          { phaseId: "working", pass: 2 },
          { phaseId: "working", pass: null },
        ],
      }),
    });
    expect(openingEntry(target, "working", "source")?.key).toBe("pass-2");
    expect(
      scopeEntries(target, "working").some((entry) => entry.kind === "run_level"),
    ).toBe(true);
  });

  it("GLW-FR-XZQM: the selected stream's own index settles it", () => {
    const target = run({
      logs: makeLogs({
        source: [{ phaseId: "working", pass: 1 }],
        structured: [{ phaseId: "working", pass: 2 }],
      }),
    });
    expect(openingEntry(target, "working", "source")?.key).toBe("pass-1");
    expect(openingEntry(target, "working", "structured")?.key).toBe("pass-2");
  });

  it("GLW-FR-WBTE: a pass that wrote nothing is still listed and still selectable", () => {
    const target = run({
      logs: makeLogs({ source: [{ phaseId: "working", pass: 1 }] }),
    });
    expect(scopeEntries(target, "working").map((e) => e.key)).toEqual([
      "pass-1",
      "pass-2",
    ]);
  });
});

describe("scope identity", () => {
  it("GLW-FR-NFLN: a key names one run, stage, entry, and stream", () => {
    const entry = { key: "pass-1", kind: "pass" as const, pass: 1, label: "Pass 1" };
    const base = scopeKey("r1", "working", entry, "source");
    expect(scopeKey("r2", "working", entry, "source")).not.toBe(base);
    expect(scopeKey("r1", "review", entry, "source")).not.toBe(base);
    expect(scopeKey("r1", "working", entry, "structured")).not.toBe(base);
    expect(
      scopeKey(
        "r1",
        "working",
        { key: "run-level", kind: "run_level", pass: null, label: "Run-level" },
        "source",
      ),
    ).not.toBe(base);
  });
});

describe("whether a log append can open a stage", () => {
  type Seeds = Parameters<typeof makeLogs>[0];
  const at = (
    state: Parameters<typeof makeRun>[1],
    stage: "queued" | "working" | "review" | "done",
    pass: number,
    logs: Seeds | undefined,
  ) =>
    makeRun("r1", state, {
      checkpoint: { pass },
      observability: makeObservability({ currentStage: stage }),
      logs: logs === undefined ? undefined : makeLogs(logs),
    });

  it.each([
    ["no segment names the current stage", at("working", "working", 1, {})],
    [
      "only another stage holds a segment",
      at("working", "working", 1, { source: [{ phaseId: "review", pass: 1 }] }),
    ],
    [
      "only an earlier pass of the stage holds a segment",
      at("working", "working", 2, { source: [{ phaseId: "working", pass: 1 }] }),
    ],
    ["the run rests interrupted with nothing written", at("interrupted", "working", 1, {})],
    ["the run rests blocked with nothing written", at("blocked", "working", 1, {})],
    ["the run waits for its author with nothing written", at("awaiting_author", "review", 1, {})],
  ])("GRU-FR-QKSY: an append reloads where %s", (_label, run) => {
    expect(awaitsLiveSegment(run)).toBe(true);
  });

  it.each([
    [
      "the source stream names the current stage and pass",
      at("working", "working", 1, { source: [{ phaseId: "working", pass: 1 }] }),
    ],
    [
      "only the structured stream names the current stage and pass",
      at("working", "working", 1, { structured: [{ phaseId: "working", pass: 1 }] }),
    ],
    [
      "a run-level segment names the current stage",
      at("working", "done", 1, { source: [{ phaseId: "done", pass: null }] }),
    ],
    [
      "a pass-less record names the queued stage",
      at("queued", "queued", 1, { structured: [{ phaseId: "queued", pass: null }] }),
    ],
    [
      "an older record holds pass 0, which the backend counts as pass 1",
      at("working", "working", 0, { source: [{ phaseId: "working", pass: 1 }] }),
    ],
    ["the run completed", at("completed", "done", 1, {})],
    ["the run was discarded", at("discarded", "working", 1, {})],
    ["the run failed", at("failed", "working", 1, {})],
    ["the run carries no log indexes", at("working", "working", 1, undefined)],
    [
      "the log storage version is not one this build reads",
      at("working", "working", 1, { logStorageVersion: 2 }),
    ],
  ])("GRU-FR-QKSY: an append reloads nothing where %s", (_label, run) => {
    expect(awaitsLiveSegment(run)).toBe(false);
  });

  it("GRU-FR-QKSY, GRU-FR-CKOB: an observability version this build cannot read waits for nothing", () => {
    const run = makeRun("r1", "working", {
      observability: makeObservability({ observabilityVersion: 2 as 1 }),
    });
    expect(awaitsLiveSegment(run)).toBe(false);
  });
});
