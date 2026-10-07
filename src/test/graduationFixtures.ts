/**
 * Runs, queues and observability records for the graduation section's tests
 * (`../../specifications/ui/GRU-graduation-runs.md`,
 * `../../specifications/ui/GRH-graduation-history.md`).
 *
 * A `GraduationRun` carries nine states, a checkpoint, a log index and an
 * observability record, and a test that builds one inline says more about the
 * shape of the type than about the requirement it is establishing. These
 * builders hold the shape so a test names only what it is about.
 */

import type {
  GraduationEscalation,
  GraduationLogIndexes,
  GraduationLogPage,
  GraduationLogPageEntry,
  GraduationLogStream,
  GraduationMergeData,
  GraduationQueue,
  GraduationRun,
  GraduationRunState,
} from "../types";
import type {
  GraduationObservability,
  PassRecord,
} from "../types/graduationObservability";

export const PROJECT_KEY = "/Users/demo/dev/acme";

/** GRS-FR-CGSP: a run's log index, healthy and empty. */
function logs() {
  const stream = (name: GraduationLogStream) => ({
    stream: name,
    latestSequence: 0,
    recordCount: 0,
    durableThroughSequence: 0,
    byteLength: 0,
    segments: [],
  });
  return {
    logStorageVersion: 1,
    activity: stream("activity"),
    structured: stream("structured"),
    persistence: {
      status: "healthy" as const,
      pendingCount: 0,
      updatedAt: "2026-09-06T09:00:00Z",
    },
  };
}

/** GRS-FR-WFWD: one segment of a stream's index. */
export interface SegmentSeed {
  phaseId: string;
  pass: number | null;
  firstSequence?: number;
  lastSequence?: number;
  recordCount?: number;
}

/**
 * GRS-FR-WFWD / GRS-FR-CGSP: a run's log indexes, with the segments a test is
 * about.
 *
 * A segment is what says a stage holds a log (GRU-FR-TQJW), and a segment whose
 * `pass` is null is what puts the run-level entry in the window's pass list
 * (GLW-FR-RVKT).
 */
export function makeLogs(over: {
  activity?: SegmentSeed[];
  structured?: SegmentSeed[];
  persistence?: Partial<GraduationLogIndexes["persistence"]>;
  logStorageVersion?: number;
} = {}): GraduationLogIndexes {
  const index = (name: GraduationLogStream, seeds: SegmentSeed[] = []) => ({
    stream: name,
    latestSequence: seeds.length,
    recordCount: seeds.length,
    durableThroughSequence: seeds.length,
    byteLength: seeds.length * 64,
    segments: seeds.map((seed, at) => ({
      phaseId: seed.phaseId,
      pass: seed.pass,
      firstSequence: seed.firstSequence ?? at + 1,
      lastSequence: seed.lastSequence ?? at + 1,
      recordCount: seed.recordCount ?? 1,
    })),
  });
  return {
    logStorageVersion: over.logStorageVersion ?? 1,
    activity: index("activity", over.activity),
    structured: index("structured", over.structured),
    persistence: {
      status: "healthy",
      pendingCount: 0,
      updatedAt: "2026-09-06T09:00:00Z",
      ...over.persistence,
    },
  };
}

/** GRS-FR-DYPS: one page a read answers with. */
export function makeLogPage(
  over: Partial<GraduationLogPage> = {},
): GraduationLogPage {
  return {
    runId: "r1",
    stream: "activity",
    phaseId: "working",
    scope: { kind: "pass", pass: 1 },
    entries: [],
    nextCursor: null,
    olderCursor: null,
    matchedTotal: 0,
    oldestReached: true,
    latestSequence: 0,
    status: "empty",
    search: "not_requested",
    ...over,
  };
}

/**
 * GRS-FR-DYPS: a page as the backend answers one read, naming the run, phase,
 * scope, and stream the read asked for, over records written for that scope
 * (GLW-FR-VRTC).
 *
 * A record that is run-level stays run-level. A test that wants a page or a
 * record of another selection builds it without this.
 */
export function pageForRead(
  request: Record<string, unknown>,
  page: GraduationLogPage,
): GraduationLogPage {
  const scope = (request.pass ?? page.scope) as GraduationLogPage["scope"];
  const runId = String(request.runId ?? page.runId);
  const phaseId = String(request.phaseId ?? page.phaseId);
  return {
    ...page,
    runId,
    phaseId,
    stream: (request.stream ?? page.stream) as GraduationLogPage["stream"],
    scope,
    entries: (page.entries ?? []).map((entry) => ({
      ...entry,
      record: {
        ...entry.record,
        run_id: runId,
        phase_id: phaseId,
        pass:
          typeof entry.record.pass === "number" && scope.kind === "pass"
            ? scope.pass
            : entry.record.pass,
      },
    })),
  };
}

/**
 * GRS-FR-JAPO: one persisted activity record, as a page entry holds it.
 *
 * The record carries the twelve persisted fields. A test can add any further
 * field through `over`, to show that the window reads four of them only
 * (GLW-FR-HGXL).
 */
export function makeActivityEntry(
  sequence: number,
  summary: string,
  over: Record<string, unknown> = {},
): GraduationLogPageEntry {
  const pass = "pass" in over ? (over.pass as number | null) : 1;
  return {
    record: {
      schema_version: 1,
      record_id: `rec-${sequence}`,
      sequence,
      at: "2026-09-06T09:04:31Z",
      run_id: "r1",
      phase_id: "working",
      pass,
      origin: "executor",
      producer: "work_turn",
      channel: "stdout",
      kind: "message",
      summary,
      ...over,
    },
    presentation: { runLevel: pass === null },
  };
}

/** GOB-FR-XYCY: one pass, whole. */
export function makePass(over: Partial<PassRecord> = {}): PassRecord {
  return {
    pass: 1,
    status: "working",
    task: "Make the editor keep its scroll position.",
    findings: [],
    startedAt: "2026-09-06T09:02:00Z",
    ...over,
  };
}

/** GOB-FR-XMDU: the record, at the version this build renders. */
export function makeObservability(
  over: Partial<GraduationObservability> = {},
): GraduationObservability {
  return {
    observabilityVersion: 1,
    currentStage: "working",
    stageCondition: "active",
    stageHistory: [],
    passes: [makePass()],
    ...over,
  };
}

/** GXD-FR-HGSU: what a run in `awaiting_author` is waiting for. */
export function makeEscalation(
  over: Partial<GraduationEscalation> = {},
): GraduationEscalation {
  return {
    reason: "The log window can page by record or by segment.",
    origin: "work",
    raisedAt: "2026-09-06T09:15:00Z",
    questions: [
      {
        position: 1,
        question: "Which unit should paging move by?",
        options: [
          {
            answer: "record",
            summary: "By record",
            description: "A page is a fixed number of records.",
          },
          {
            answer: "segment",
            summary: "By segment",
            description: "A page is one phase and pass.",
          },
        ],
      },
    ],
    ...over,
  };
}

export type RunOverrides = Omit<Partial<GraduationRun>, "checkpoint"> & {
  /** Written over the checkpoint's own defaults. */
  checkpoint?: Partial<GraduationRun["checkpoint"]>;
};

/** One run, whole, at whatever state the test is about. */
export function makeRun(
  id: string,
  state: GraduationRunState = "working",
  over: RunOverrides = {},
): GraduationRun {
  const { checkpoint, ...rest } = over;
  return {
    id,
    streamId: "s-1",
    streamName: "editor-work",
    projectKey: PROJECT_KEY,
    state,
    // GRD-FR-HQPD: the choice every run carries, at its resting position.
    standingWork: "commit",
    input: {
      draftId: `draft-${id}`,
      draftName: `Run ${id}`,
      prompt: "Fix the thing that is broken.",
      promptChecksum: "sha-1",
      capturedAt: "2026-09-06T09:00:00Z",
    },
    baseCommit: "a91bc04",
    commits: [],
    autoStart: true,
    archived: false,
    workTurns: 1,
    reviewTurns: 0,
    checkpoint: {
      pass: 1,
      changedPaths: [],
      hiddenPaths: [],
      hiddenPathsOmitted: 0,
      pendingEscalationAnswers: [],
      verdictRefusals: 0,
      blockAttempts: 0,
      ...checkpoint,
    },
    logs: logs(),
    observability: makeObservability(),
    createdAt: "2026-09-06T09:00:00Z",
    updatedAt: "2026-09-06T09:10:00Z",
    ...rest,
  };
}

/** GRD-FR-LGDV: the project's whole run order. */
export function makeQueue(
  runs: GraduationRun[],
  projectKey = PROJECT_KEY,
): GraduationQueue {
  return { projectKey, runs };
}

/** GRD-FR-KZPT: what a merge run carries, with one path Git could not settle. */
export function makeMergeData(
  over: Partial<GraduationMergeData> = {},
): GraduationMergeData {
  return {
    name: "Merge editor-work",
    streamBranch: "synthesis/stream/editor-work",
    baseBranch: "main",
    baseTip: "b".repeat(40),
    streamTip: "c".repeat(40),
    mergeBase: "a".repeat(40),
    snapshotCommit: "d".repeat(40),
    publication: { kind: "uncommitted" },
    changedPaths: [
      "specifications/core/GRD-graduation.md",
      "specifications/ui/GRU-graduation-runs.md",
      "src/state/graduation/stages.ts",
    ],
    unresolvedPaths: ["specifications/core/GRD-graduation.md"],
    conflicts: [
      {
        path: "specifications/core/GRD-graduation.md",
        baseChange: "updated",
        streamChange: "updated",
      },
    ],
    result: null,
    ...over,
  };
}

/**
 * GRD-FR-VCTH: one merge run, whole. It has no draft: its draft id and draft
 * name are empty, and its title is `merge.name`.
 */
export function makeMergeRun(
  id: string,
  state: GraduationRunState = "working",
  over: Omit<RunOverrides, "merge"> & { merge?: Partial<GraduationMergeData> } = {},
): GraduationRun {
  const { merge, ...rest } = over;
  const run = makeRun(id, state, {
    standingWork: "keep",
    baseCommit: "d".repeat(40),
    input: {
      draftId: "",
      draftName: "",
      prompt: "Merge the work stream editor-work into main.",
      promptChecksum: "sha-merge",
      capturedAt: "2026-09-06T09:00:00Z",
    },
    ...rest,
  });
  return { ...run, merge: makeMergeData(merge) };
}
