import { describe, expect, it } from "vitest";

import { makeRun } from "../../test/graduationFixtures";
import {
  runsOfStream,
  streamWorkIsDone,
  temporaryMergeMessage,
} from "./temporaryStream";

const stream = { id: "s-1", worktreePath: "/data/w/s-1" };

describe("the work of a temporary stream", () => {
  it("WSS-FR-PRVK: a stream with runs that are all completed is done", () => {
    const runs = [makeRun("a", "completed"), makeRun("b", "completed")];
    expect(streamWorkIsDone(runs, stream)).toBe(true);
  });

  it("WSS-FR-PRVK: a stream with no run is not done", () => {
    expect(streamWorkIsDone([], stream)).toBe(false);
    expect(
      streamWorkIsDone([makeRun("x", "completed", { streamId: "other" })], stream),
    ).toBe(false);
  });

  it.each(["failed", "discarded", "awaiting_author", "queued", "working"] as const)(
    "WSS-FR-GBTD: a %s run keeps the work from being done",
    (state) => {
      const runs = [makeRun("a", "completed"), makeRun("b", state)];
      expect(streamWorkIsDone(runs, stream)).toBe(false);
    },
  );

  it("WKS-FR-NDSV: a direct run that targets the working copy belongs to the stream", () => {
    const direct = makeRun("d", "working", {
      streamId: "",
      directTarget: { worktreePath: "/data/w/s-1", worktreeName: "s-1", branch: "b" },
    });
    expect(runsOfStream([direct], stream)).toHaveLength(1);
    expect(streamWorkIsDone([makeRun("a", "completed"), direct], stream)).toBe(false);
  });
});

describe("the default merge message of a temporary stream", () => {
  it("WSS-FR-GQXN: one line for each run, in run order, each line starting with a dash", () => {
    const runs = [
      makeRun("a", "completed", { input: { ...makeRun("a").input, draftName: "Fix tab order" } }),
      makeRun("z", "completed", { streamId: "other" }),
      makeRun("b", "completed", { input: { ...makeRun("b").input, draftName: "Add run logs" } }),
    ];
    expect(temporaryMergeMessage(runs, stream)).toBe("- Fix tab order\n- Add run logs");
  });

  it("WSS-FR-GQXN: a run with no draft name has no line", () => {
    const merge = makeRun("m", "completed", {
      input: { ...makeRun("m").input, draftName: "" },
    });
    const runs = [makeRun("a", "completed"), merge];
    expect(temporaryMergeMessage(runs, stream)).toBe("- Run a");
  });
});
