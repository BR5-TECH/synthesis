/**
 * What the stage row and the log window read from a run's persisted log
 * indexes (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-TQJW,
 * `../../../specifications/ui/GLW-graduation-log-window.md` GLW-FR-CKLZ).
 *
 * Nothing here knows a stage id, a stage label, or a stage count: a phase is
 * whatever descriptor the caller passed, and every rule reads the run's own
 * data (GRU-FR-ZBMU, GLW-FR-MZUP).
 */

import type {
  GraduationLogIndexes,
  GraduationLogPassScope,
  GraduationLogStream,
  GraduationRun,
  GraduationStreamIndex,
} from "../../types";
import { isTerminalRun } from "../../types";
import { hasReadableProgress } from "../../types/graduationObservability";
import type { ProgressStage } from "../runProgress";

/** GLW-FR-CKLZ / GLW-FR-RVKT: one entry of the window's pass list. */
export interface LogScopeEntry {
  /** Stable within one phase; what a selection names. */
  key: string;
  kind: "pass" | "run_level";
  /** The pass number, or null for the run-level entry. */
  pass: number | null;
  /** GLW-FR-DDXJ: the run-level entry is never numbered as a pass. */
  label: string;
}

/** GLW-FR-PDGA: the scope a read names for one entry. */
export function scopeOf(entry: LogScopeEntry): GraduationLogPassScope {
  return entry.kind === "run_level"
    ? { kind: "run_level" }
    : { kind: "pass", pass: entry.pass ?? 1 };
}

/**
 * GLW-FR-NFLN: a stable key for one selection.
 *
 * An answer that arrives carrying another key is discarded rather than merged.
 */
export function scopeKey(
  runId: string,
  phaseId: string,
  entry: LogScopeEntry | null,
  stream: GraduationLogStream,
): string {
  return `${runId} ${phaseId} ${entry?.key ?? "none"} ${stream}`;
}

/** GRS-FR-MBED: a version this build does not recognise reads back no index. */
function indexesOf(
  logs: GraduationLogIndexes | undefined,
): GraduationStreamIndex[] {
  if (!logs || logs.logStorageVersion !== 1) return [];
  return [logs.source, logs.structured].filter(Boolean);
}

/**
 * GRU-FR-TQJW: a stage holds a log where any segment of either stream names it.
 *
 * A segment naming a pass and a segment naming `pass = null` each count, so a
 * stage whose only output is run-level is still one the author may open.
 */
export function stageHoldsLogs(
  logs: GraduationLogIndexes | undefined,
  phaseId: string,
): boolean {
  return indexesOf(logs).some((index) =>
    (index.segments ?? []).some((segment) => segment.phaseId === phaseId),
  );
}

/**
 * GRU-FR-QKSY: whether a log append for this run can change what the stage row
 * shows.
 *
 * Only the first record of a stage and pass opens the segment that makes the
 * stage activatable. Once the indexes name the run's current stage and pass,
 * later appends change nothing the row reads. A terminal run, and a run whose
 * progress or index this build cannot read, waits for nothing.
 */
export function awaitsLiveSegment(run: GraduationRun): boolean {
  if (isTerminalRun(run.state)) return false;
  if (!hasReadableProgress(run.observability)) return false;
  const indexes = indexesOf(run.logs);
  if (indexes.length === 0) return false;
  const stage = run.observability.currentStage;
  // The backend counts passes from one, whatever an older record holds.
  const pass = Math.max(1, run.checkpoint?.pass ?? 1);
  // A queue wait, a commit and a semantic merge turn write `pass = null`, so a
  // run-level segment of the stage counts as well.
  return !indexes.some((index) =>
    (index.segments ?? []).some(
      (segment) =>
        segment.phaseId === stage && (segment.pass === pass || segment.pass === null),
    ),
  );
}

/**
 * GRU-FR-ZBMU / GRU-FR-TQJW / GRU-FR-VKPD: the stage descriptors the row
 * renders, with eligibility and the sentence for a stage that holds nothing.
 *
 * The stage list is the caller's, whatever its length: this adds and drops no
 * stage and writes no id of its own.
 */
export function stagesWithLogAccess(
  stages: ProgressStage[],
  logs: GraduationLogIndexes | undefined,
): ProgressStage[] {
  return stages.map((stage) => {
    const eligible = stageHoldsLogs(logs, stage.id);
    return {
      ...stage,
      activatable: eligible,
      disabledReason: eligible
        ? null
        : `${stage.label} has written no log yet, so there is nothing to open.`,
    };
  });
}

/**
 * GLW-FR-CKLZ / GLW-FR-CQSE: the passes that entered one phase.
 *
 * Read from the run's persisted stage history and from nothing else — never
 * from log content, and never from the segments, which settle which entry opens
 * rather than which passes are listed (GLW-FR-WBTE).
 */
export function passesInPhase(run: GraduationRun, phaseId: string): number[] {
  if (!hasReadableProgress(run.observability)) return [];
  const seen = new Set<number>();
  for (const move of run.observability.stageHistory ?? []) {
    if (move.to !== phaseId) continue;
    if (typeof move.pass === "number") seen.add(move.pass);
  }
  return [...seen].sort((a, b) => a - b);
}

/**
 * GLW-FR-RVKT: whether the run-level entry stands in this phase's list.
 *
 * Both streams' indexes settle it, so the stream toggle adds and removes no
 * entry.
 */
export function hasRunLevelEntry(
  logs: GraduationLogIndexes | undefined,
  phaseId: string,
): boolean {
  return indexesOf(logs).some((index) =>
    (index.segments ?? []).some(
      (segment) => segment.phaseId === phaseId && segment.pass === null,
    ),
  );
}

/** GLW-FR-CKLZ / GLW-FR-RVKT: the whole of the window's pass list. */
export function scopeEntries(
  run: GraduationRun,
  phaseId: string,
): LogScopeEntry[] {
  const entries: LogScopeEntry[] = passesInPhase(run, phaseId).map((pass) => ({
    key: `pass-${pass}`,
    kind: "pass" as const,
    pass,
    label: `Pass ${pass}`,
  }));
  if (hasRunLevelEntry(run.logs, phaseId)) {
    entries.push({
      key: "run-level",
      kind: "run_level",
      pass: null,
      label: "Run-level",
    });
  }
  return entries;
}

/**
 * GLW-FR-ELJO / GLW-FR-YVKD / GLW-FR-XZQM: the entry the window opens on.
 *
 * The newest listed pass whose own records the selected stream's index holds;
 * where no listed pass holds one, the run-level entry where it stands, and the
 * newest listed pass otherwise. It reads the index the run record already
 * carries and makes no read of its own.
 */
export function openingEntry(
  run: GraduationRun,
  phaseId: string,
  stream: GraduationLogStream,
): LogScopeEntry | null {
  const entries = scopeEntries(run, phaseId);
  const passes = entries.filter((entry) => entry.kind === "pass");
  const runLevel = entries.find((entry) => entry.kind === "run_level") ?? null;

  const index =
    run.logs?.logStorageVersion === 1 ? run.logs[stream] : undefined;
  const segments = index?.segments ?? [];
  const holdsPass = (pass: number) =>
    segments.some(
      (segment) => segment.phaseId === phaseId && segment.pass === pass,
    );

  for (let at = passes.length - 1; at >= 0; at -= 1) {
    const pass = passes[at].pass;
    if (typeof pass === "number" && holdsPass(pass)) return passes[at];
  }
  if (runLevel) return runLevel;
  return passes.length > 0 ? passes[passes.length - 1] : null;
}
