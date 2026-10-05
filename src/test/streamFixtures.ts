/**
 * The work stream shapes every selector test builds from
 * (`../../specifications/core/WKS-work-streams.md`).
 *
 * Shared rather than copied, on the model of `graduationFixtures.ts`: two test
 * files render the same surface, and a fixture that drifted between them would
 * let one of them pass against a listing the backend never sends.
 */

import type {
  GraduationRunState,
  StreamMergeResult,
  StreamMergeRunLink,
  StreamUpdateCommit,
  StreamUpdateRecord,
  StreamUpdateState,
  StreamUpdateStrategy,
  WorkStreamSummary,
} from "../types";

export function stream(over: Partial<WorkStreamSummary["stream"]> = {}) {
  return {
    id: "w1",
    projectKey: "p",
    name: "editor work",
    branch: "synthesis/stream/editor-work",
    worktreePath: "/data/w/w1",
    baseBranch: "main",
    baseRevision: "abc",
    createdAt: "2026-05-01T00:00:00Z",
    busyRunId: null,
    isMissing: false,
    ...over,
  };
}

export function summary(
  over: Partial<WorkStreamSummary["stream"]> = {},
  counts: {
    aheadOfBase?: number;
    behindBase?: number;
    baseTipRevision?: string;
    missingCommits?: StreamUpdateCommit[];
    queuedRunCount?: number;
    mergeRun?: StreamMergeRunLink | null;
    update?: StreamUpdateRecord | null;
  } = {},
): WorkStreamSummary {
  const behindBase = counts.behindBase ?? 0;
  return {
    stream: stream(over),
    aheadOfBase: counts.aheadOfBase ?? 0,
    behindBase,
    // WKS-FR-KFVJ: the revision an Update confirmation pins. A stream standing
    // behind its base always has one, so the fixture carries one wherever the
    // caller says the stream is behind.
    baseTipRevision:
      counts.baseTipRevision ?? (behindBase > 0 ? "9f2c1ab0000000000000000000000000000000ba" : "abc"),
    missingCommits:
      counts.missingCommits ??
      Array.from({ length: behindBase }, (_, index) => missingCommit(index)),
    queuedRunCount: counts.queuedRunCount ?? 0,
    // WKS-FR-KHJS: the listing carries `mergeRun` as null where the stream has
    // none; a fixture omits the key, which a reader treats the same way.
    ...(counts.mergeRun ? { mergeRun: counts.mergeRun } : {}),
    ...(counts.update ? { update: counts.update } : {}),
  };
}

/** WKS-FR-UBGX: one commit the stream is missing from its base branch. */
export function missingCommit(index = 0): StreamUpdateCommit {
  return {
    revision: `${index}`.padStart(40, "c"),
    summary: `base commit ${index + 1}`,
    author: "Test Author",
    committedAt: "2026-05-01T00:00:00Z",
  };
}

/** WKS-FR-ZKUP: one stream's update record, as the listing carries it. */
export function updateRecord(
  state: StreamUpdateState,
  over: Partial<StreamUpdateRecord> = {},
): StreamUpdateRecord {
  const strategy: StreamUpdateStrategy = "merge_source";
  return {
    streamId: "w1",
    projectKey: "p",
    state,
    strategy,
    attemptId: "u1",
    baseBranch: "main",
    baseRevision: "9f2c1ab0000000000000000000000000000000ba",
    semanticTurns: 0,
    reason: "",
    failure: "",
    requestedAt: "2026-05-01T00:00:00Z",
    updatedAt: "2026-05-01T00:01:00Z",
    missingCommits: [missingCommit()],
    escalation: null,
    conflicts: [],
    updatedPaths: [],
    decisions: [],
    ...over,
  };
}

/** An update escalation with one question and two proposed responses. */
export function updateAskedOnce(): StreamUpdateRecord {
  return updateRecord("escalated", {
    reason: "The two branches answered the same requirement differently.",
    conflicts: [
      { path: "README.md", baseChange: "updated", streamChange: "updated" },
    ],
    escalation: {
      reason: "The two branches answered the same requirement differently.",
      raisedAt: "2026-05-01T00:01:00Z",
      origin: "semantic_merge",
      questions: [
        {
          position: 1,
          question: "Which design stands?",
          options: [
            {
              answer: "base",
              summary: "Keep the base branch's",
              description: "Drops the stream's.",
            },
            {
              answer: "stream",
              summary: "Keep the stream's",
              description: "Drops the base branch's.",
            },
          ],
        },
      ],
    },
  });
}

/** WKS-FR-KHJS: a stream's merge run, as the listing links it. */
export function mergeRunLink(
  state: GraduationRunState,
  over: Partial<StreamMergeRunLink> = {},
): StreamMergeRunLink {
  return {
    runId: "run-merge-1",
    name: "Merge editor work",
    state,
    ...over,
  };
}

/** WKS-FR-QNHF: what a merge call answers when Git settled it with no run. */
export function mergedResult(
  over: Partial<Extract<StreamMergeResult, { kind: "merged" }>> = {},
): StreamMergeResult {
  return {
    kind: "merged",
    mergedPaths: ["README.md", "src/a.ts"],
    commit: null,
    ...over,
  };
}

/** WKS-FR-ZLWT: what a merge call answers when a path conflicts. */
export function conflictedResult(
  over: Partial<Extract<StreamMergeResult, { kind: "conflicted" }>> = {},
): StreamMergeResult {
  return {
    kind: "conflicted",
    runId: "run-merge-1",
    conflictedPaths: ["README.md", "docs/a.md"],
    ...over,
  };
}
