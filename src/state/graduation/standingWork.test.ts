import { describe, expect, it } from "vitest";

import { makeRun } from "../../test/graduationFixtures";
import type {
  GraduationRun,
  StandingWorkOutcome,
  WorkStreamSummary,
} from "../../types";
import {
  DEFAULT_STANDING_WORK,
  isOccupied,
  STANDING_WORK_POSITIONS,
  pushRefusalStatement,
  standingWorkPosition,
} from "./standingWork";

/** One completed run, with whatever the standing-work step reported. */
function withOutcome(outcome: StandingWorkOutcome | null): GraduationRun {
  return { ...makeRun("r1", "completed"), standingWorkOutcome: outcome };
}

describe("the three positions", () => {
  it("GSD-FR-TBQX: they are the three the backend accepts, and committing rests", () => {
    expect(STANDING_WORK_POSITIONS.map((p) => p.value)).toEqual([
      "keep",
      "commit",
      "commit_and_push",
    ]);
    expect(DEFAULT_STANDING_WORK).toBe("commit");
  });

  it("GSD-FR-NWSC: each position says what the run does when its turn comes", () => {
    const said = Object.fromEntries(
      STANDING_WORK_POSITIONS.map((position) => [
        position.value,
        `${position.summary} ${position.description}`,
      ]),
    );
    // Each one names the act and what the run then starts from, so the choice
    // is legible without reading the other two.
    expect(said.keep).toMatch(/on top of it/);
    expect(said.keep).toMatch(/part of what the run commits/);
    expect(said.commit).toMatch(/committed under a message naming you/);
    expect(said.commit).toMatch(/starts from that commit/);
    expect(said.commit_and_push).toMatch(/goes to the project's remote/);
    expect(said.commit_and_push).toMatch(/does not stop the run/);
  });

  it("GSD-FR-TBQX: a value this build does not know reads as no position", () => {
    expect(standingWorkPosition("keep")?.summary).toMatch(/Leave it/);
    expect(
      standingWorkPosition("committed_twice" as never),
    ).toBeNull();
  });
});

describe("whether the run will wait for its stream", () => {
  /** One stream of the project, as the listing reports it. */
  const summary = (over: {
    busyRunId?: string | null;
    queuedRunCount?: number;
  }): WorkStreamSummary => ({
    stream: {
      id: "s-1",
      name: "editor-work",
      projectKey: "/Users/demo/dev/acme",
      branch: "synthesis/stream/s-1",
      baseBranch: "main",
      baseRevision: "a91bc04",
      worktreePath: "/tmp/s-1",
      createdAt: "2026-09-06T09:00:00Z",
      isMissing: false,
      busyRunId: over.busyRunId ?? null,
    },
    queuedRunCount: over.queuedRunCount ?? 0,
    aheadOfBase: 0,
    behindBase: 0,
    baseTipRevision: "a91bc04",
    missingCommits: [],
  });

  it("GSD-FR-WQPD: a stream a run holds, and one with runs queued on it, both make the run wait", () => {
    expect(isOccupied(summary({ busyRunId: "g-9" }))).toBe(true);
    expect(isOccupied(summary({ queuedRunCount: 1 }))).toBe(true);
    expect(isOccupied(summary({ busyRunId: "g-9", queuedRunCount: 3 }))).toBe(true);
  });

  it("GSD-FR-WQPD: a stream that holds neither dispatches the run at once", () => {
    expect(isOccupied(summary({}))).toBe(false);
    // And a stream that is not chosen yet is not one to decide about.
    expect(isOccupied(null)).toBe(false);
    expect(isOccupied(undefined)).toBe(false);
  });
});

describe("a push the remote did not take", () => {
  it("GRU-FR-TFEZ: the reason is stated, and so is that the run continued", () => {
    const statement = pushRefusalStatement(
      withOutcome({ commit: "a1", pushed: false, pushFailure: { code: "invalid_token" } }),
    );
    expect(statement).toMatch(/was not pushed/);
    expect(statement).toMatch(/did not take the credential/);
    expect(statement).toMatch(/not stopped by it/);
    // The code itself is not the sentence.
    expect(statement).not.toMatch(/invalid_token/);
  });

  it("GRU-FR-TFEZ: every cause the push can report reads as words rather than as a code", () => {
    // The complete set the backend's push path can return.
    const causes = [
      "no remote configured",
      "no branch is checked out",
      "invalid_token",
      "github_unreachable",
      "github_token_missing",
      "github_token_selection_required",
      "github_host_mismatch",
      "unknown_token",
      "keychain_unavailable",
      "github_identity_unresolved",
      "push_unavailable",
      "push_did_not_finish",
      "push_abandoned",
    ];
    for (const code of causes) {
      const statement = pushRefusalStatement(
        withOutcome({ pushed: false, pushFailure: { code } }),
      );
      expect(statement).toMatch(/was not pushed: /);
      expect(statement, `the code itself is not the sentence for ${code}`).not.toMatch(
        /_/,
      );
    }
  });

  it("GRU-FR-TFEZ: a code this build does not know is stated as itself rather than dropped", () => {
    expect(
      pushRefusalStatement(
        withOutcome({ pushed: false, pushFailure: { code: "something new" } }),
      ),
    ).toMatch(/was not pushed: something new\./);
  });

  it("GRU-FR-TFEZ: the detail beside a typed cause is not carried into the run region", () => {
    // A refusal may arrive as `code: detail`, and the detail is a path or a
    // remote rather than something this one line has room for.
    const statement = pushRefusalStatement(
      withOutcome({
        pushed: false,
        pushFailure: { code: "github_unreachable: https://user@example.com/acme.git" },
      }),
    );
    expect(statement).toMatch(/could not be reached/);
    expect(statement).not.toMatch(/example\.com/);
  });

  it("GRU-FR-TFEZ: a refusal with no code still says the push did not land", () => {
    const statement = pushRefusalStatement(withOutcome({ pushed: false }));
    expect(statement).toMatch(/The stream branch was not pushed\./);
    expect(statement).toMatch(/not stopped by it/);
  });

  it("GRU-FR-TFEZ: a push that landed, one nobody asked for, and a run not yet dispatched all say nothing", () => {
    expect(pushRefusalStatement(withOutcome({ commit: "a1", pushed: true }))).toBeNull();
    expect(pushRefusalStatement(withOutcome({ commit: "a1" }))).toBeNull();
    expect(pushRefusalStatement(withOutcome(null))).toBeNull();
  });
});
