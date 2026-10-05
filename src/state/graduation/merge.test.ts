/**
 * What a merge run says about itself, as pure functions
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-HDPQ,
 * GRU-FR-NWEC, GRU-FR-YSTV, GRU-FR-ELFZ, GRU-FR-WDWB, GRU-FR-DVWY,
 * GRU-FR-DBUS, GRU-FR-QMWX, GRU-FR-BLSS).
 *
 * The component tests render the whole section; these pin the rules the
 * rendering reads, so a rule that changed does not hide behind a surface that
 * happens not to draw it.
 */

import { describe, expect, it } from "vitest";

import { makeMergeRun, makeQueue, makeRun } from "../../test/graduationFixtures";
import { matchesText } from "../graduationRail";
import { blockerStatement } from "./blockers";
import { defaultCommitMessage } from "./standingWork";
import {
  failureStatement,
  fateByPath,
  isMergeRun,
  mergeProvenanceLine,
  mergeResultSentence,
  MERGE_BRANCH_MOVED_STATEMENT,
  publicationSentence,
  runTitle,
  shortCommit,
} from "./merge";
import { positionOf, pauseAnnouncement } from "./arrangement";
import { raiseStatement, raiseTitle } from "./escalations";
import { graduationErrorMessage } from "./messages";
import { canRestart } from "./restart";
import { actionsFor, stateLabelOf } from "./runState";
import {
  conditionSentence,
  GRADUATION_STAGES,
  MERGE_STAGES,
  outcomeStatement,
  stageDescriptorsFor,
} from "./stages";

describe("the title of a run (GRU-FR-HDPQ)", () => {
  it("GRU-FR-HDPQ: a merge run's title is merge.name and a draft run's is its draft's name", () => {
    expect(isMergeRun(makeMergeRun("m1"))).toBe(true);
    expect(isMergeRun(makeRun("r1"))).toBe(false);
    expect(runTitle(makeMergeRun("m1"))).toBe("Merge editor-work");
    expect(runTitle(makeRun("r1"))).toBe("Run r1");
  });

  it("GRU-FR-HDPQ: the text filter matches the merge title", () => {
    const run = makeMergeRun("m1", "working", { merge: { name: "Merge docs-pass" } });
    const word = () => "Working";
    expect(matchesText(run, "docs-pass", word)).toBe(true);
    expect(matchesText(run, "merge docs", word)).toBe(true);
    expect(matchesText(run, "nothing like it", word)).toBe(false);
  });

  it("GRU-FR-HDPQ, GRU-FR-ZKYU: a merge run's announcements name it by its title, never a draft", () => {
    const run = makeMergeRun("m1", "queued");
    expect(pauseAnnouncement(run, true)).toContain("Merge editor-work");
    expect(positionOf(makeQueue([run]), "m1")).toBe(0);
  });

  it("GRU-FR-HDPQ: the commit message default of a stream skips a merge run, which has no draft name", () => {
    const runs = [makeRun("r1", "queued"), makeMergeRun("m1", "queued")];
    expect(defaultCommitMessage(runs, "s-1")).toBe("Run r1");
  });
});

describe("provenance and publication (GRU-FR-NWEC)", () => {
  it("GRU-FR-NWEC: the provenance names the stream, its branch and the base branch", () => {
    const line = mergeProvenanceLine(makeMergeRun("m1"));
    expect(line).toContain("editor-work");
    expect(line).toContain("synthesis/stream/editor-work");
    expect(line).toContain("“main”");
  });

  it("GRU-FR-NWEC: each publication choice reads in words", () => {
    expect(publicationSentence({ kind: "uncommitted" })).toMatch(
      /left uncommitted in the base worktree/,
    );
    expect(publicationSentence({ kind: "commit", message: "x" })).toMatch(
      /committed under the author's message/,
    );
  });
});

describe("stages and state words (GRU-FR-YSTV)", () => {
  it("GRU-FR-YSTV, GRU-FR-ZBMU: both descriptor lists hold the same ids in the same order, and differ in their labels alone", () => {
    expect(MERGE_STAGES.map((s) => s.id)).toEqual(GRADUATION_STAGES.map((s) => s.id));
    expect(MERGE_STAGES.map((s) => s.label)).toEqual([
      "Queued",
      "Reconciling",
      "Reviewing",
      "Merged",
    ]);
    expect(stageDescriptorsFor(makeMergeRun("m1"))).toBe(MERGE_STAGES);
    expect(stageDescriptorsFor(makeRun("r1"))).toBe(GRADUATION_STAGES);
  });

  it("GRU-FR-YSTV, GOB-FR-PLTB: a merge run that ended without applying its merge labels its last stage Not merged", () => {
    const base = makeMergeRun("m2", "failed");
    const run = makeMergeRun("m2", "failed", {
      observability: {
        ...base.observability,
        currentStage: "done",
        stageCondition: "stopped",
      },
    });
    const labels = stageDescriptorsFor(run).map((stage) => stage.label);
    expect(labels).toEqual(["Queued", "Reconciling", "Reviewing", "Not merged"]);
  });

  it("GRU-FR-YSTV: working reads Reconciling, reviewing reads Reviewing, completed reads Merged", () => {
    expect(stateLabelOf(makeMergeRun("m1", "working"))).toBe("Reconciling");
    expect(stateLabelOf(makeMergeRun("m1", "reviewing"))).toBe("Reviewing");
    expect(stateLabelOf(makeMergeRun("m1", "completed"))).toBe("Merged");
    expect(stateLabelOf(makeMergeRun("m1", "blocked"))).toBe("Blocked");
    expect(stateLabelOf(makeRun("r1", "working"))).toBe("Working");
    expect(stateLabelOf(makeRun("r1", "completed"))).toBe("Completed");
  });

  it("GRU-FR-YSTV, GRU-FR-PNVX: the pass sentence is the checkpoint's own", () => {
    const working = makeMergeRun("m1", "working", { checkpoint: { pass: 2, passLimit: 4 } });
    expect(conditionSentence(working)).toBe("Reconciling · pass 2 of 4");
    expect(conditionSentence(makeMergeRun("m1", "reviewing"))).toMatch(/^Reviewing · pass 1 of 2/);
  });

  it("GRU-FR-JRMA, GRU-FR-QMWX: a merge run's outcome line says it merged, or why it failed", () => {
    expect(outcomeStatement(makeMergeRun("m1", "completed"))).toBe("Merged into main.");
    expect(outcomeStatement(makeMergeRun("m1", "discarded"))).toMatch(
      /Nothing was written to either branch/,
    );
    expect(
      outcomeStatement(
        makeMergeRun("m1", "failed", {
          failure: { code: "merge_branch_moved", message: "raw", retryable: false },
        }),
      ),
    ).toBe(MERGE_BRANCH_MOVED_STATEMENT);
  });
});

describe("what a merge run offers (GRU-FR-ELFZ, GRU-FR-WDWB, GRU-FR-DVWY, GRU-FR-TOKG)", () => {
  it("GRU-FR-ELFZ: it offers no Open draft in any state", () => {
    for (const state of [
      "queued",
      "working",
      "reviewing",
      "blocked",
      "interrupted",
      "awaiting_author",
      "completed",
      "discarded",
      "failed",
    ] as const) {
      expect(actionsFor(makeMergeRun("m1", state))).not.toContain("open-draft");
    }
  });

  it("GRU-FR-WDWB, GRU-FR-DVWY: it offers no Restart and no Revert, whatever it published", () => {
    const discarded = makeMergeRun("m1", "discarded", { commits: ["abc"] });
    expect(actionsFor(discarded)).not.toContain("restart");
    expect(actionsFor(discarded)).not.toContain("revert");
    expect(canRestart(discarded)).toBe(false);
    // A draft run that made a commit still does.
    const draft = makeRun("r1", "discarded", { commits: ["abc"] });
    expect(actionsFor(draft)).toEqual(expect.arrayContaining(["restart", "revert"]));
  });

  it("GRU-FR-TOKG: it offers the controls every run offers in its state", () => {
    expect(actionsFor(makeMergeRun("m1", "working"))).toEqual(["pause", "discard"]);
    expect(actionsFor(makeMergeRun("m1", "interrupted"))).toEqual(["continue", "discard"]);
    expect(actionsFor(makeMergeRun("m1", "blocked"))).toEqual(["continue", "discard"]);
    expect(actionsFor(makeMergeRun("m1", "failed"))).toEqual(["discard"]);
    expect(actionsFor(makeMergeRun("m1", "completed"))).toEqual([]);
  });
});

describe("paths, results and failures (GRU-FR-AJGM, GRU-FR-JRMA, GRU-FR-DBUS, GRU-FR-QMWX)", () => {
  it("GRU-FR-AJGM: what each side did to a path reads from the run, with the base branch named", () => {
    const run = makeMergeRun("m1", "working", {
      merge: {
        baseBranch: "dev",
        conflicts: [{ path: "a.md", baseChange: "deleted", streamChange: "" }],
      },
    });
    expect(fateByPath(run.merge!)).toEqual({
      "a.md": "dev: deleted · stream: unknown",
    });
  });

  it("GRU-FR-JRMA: the short commit is seven characters", () => {
    expect(shortCommit("9f2c1ab4d5e6f708")).toBe("9f2c1ab");
  });

  it("GRU-FR-JRMA: the result sentence stands only for a completed run that holds a result", () => {
    const result = { published: "commit" as const, commit: "abc1234ff", mergedPaths: [] };
    expect(mergeResultSentence(makeMergeRun("m1", "completed", { merge: { result } }))).toMatch(
      /as a commit/,
    );
    expect(mergeResultSentence(makeMergeRun("m1", "working", { merge: { result } }))).toBeNull();
    expect(mergeResultSentence(makeMergeRun("m1", "completed"))).toBeNull();
  });

  it("GRU-FR-DBUS: each apply blocker says its cause and the three facts every one carries", () => {
    for (const code of ["merge_dirty_side", "merge_guard_held", "merge_apply_failed"]) {
      const statement =
        blockerStatement(
          makeMergeRun("m1", "blocked", {
            blocker: { code, message: "raw", clearsBy: "backend", attempt: 2 },
          }),
        ) ?? "";
      expect(statement).toMatch(/Nothing was written to either branch/);
      expect(statement).toMatch(/No new pass is spent/);
      expect(statement).toMatch(/Continue retries the apply/);
      expect(statement).toMatch(/stopped here 2 times in a row/);
      expect(statement).not.toContain(code);
    }
  });

  it("GRU-FR-QMWX, GRU-FR-EMNV: merge_branch_moved reads the same as a failure and as a refusal", () => {
    const failed = makeMergeRun("m1", "failed", {
      failure: { code: "merge_branch_moved", message: "raw", retryable: false },
    });
    expect(failureStatement(failed)).toBe(MERGE_BRANCH_MOVED_STATEMENT);
    expect(graduationErrorMessage("merge_branch_moved")).toBe(MERGE_BRANCH_MOVED_STATEMENT);
    expect(graduationErrorMessage("Error: merge_branch_moved")).toBe(
      MERGE_BRANCH_MOVED_STATEMENT,
    );
    // A draft run's failure keeps the backend's own message.
    expect(
      failureStatement(
        makeRun("r1", "failed", { failure: { code: "x", message: "own words", retryable: true } }),
      ),
    ).toBe("own words");
  });
});

describe("the notification a merge run raises (GRU-FR-BLSS)", () => {
  it("GRU-FR-BLSS, GRU-FR-ZKYU: it names the run by its title and never calls it a graduation", () => {
    const run = makeMergeRun("m1", "awaiting_author");
    expect(raiseTitle(run, "Waiting on you")).toBe("Merge · Waiting on you");
    expect(raiseTitle(makeRun("r1"), "Working")).toBe("Graduation · Working");
    expect(raiseStatement(run)).toBe("“Merge editor-work” is waiting for your decision.");
  });

  it("GRU-FR-BLSS: a completed merge says where it merged, and a failed one says it failed", () => {
    expect(raiseStatement(makeMergeRun("m1", "completed"))).toBe(
      "“Merge editor-work” is merged into main.",
    );
    expect(raiseStatement(makeMergeRun("m1", "failed"))).toBe("“Merge editor-work” failed.");
    expect(raiseStatement(makeMergeRun("m1", "blocked"))).toBe("“Merge editor-work” is blocked.");
    expect(raiseStatement(makeMergeRun("m1", "discarded"))).toBe(
      "“Merge editor-work” was discarded.",
    );
  });

  it("GRU-FR-BLSS: a draft run's statements are unchanged", () => {
    expect(raiseStatement(makeRun("r1", "completed"))).toBe(
      "“Run r1” is committed on editor-work.",
    );
  });
});
