/**
 * The milestone list of a run's current turn
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-WQHM,
 * GRU-FR-TBNE, GRU-FR-CYDK, GRU-FR-JDZX, GRU-FR-PZGA).
 *
 * Pure functions of a run and of the `summary` of its `milestone` records. The
 * executor wrote each summary as one marker
 * (`../../../specifications/tools/EAC-execute-agent-cli.md` EAC-FR-ZNBD). A
 * milestone is a description for the author and decides nothing about a run.
 */

import type { GraduationRun } from "../../types";

/** GRU-FR-CYDK: the four states one milestone shows. */
export type MilestoneState = "pending" | "active" | "done" | "unknown";

/** One milestone of the plan, with the state its markers give it. */
export interface Milestone {
  title: string;
  state: MilestoneState;
}

/** GRU-FR-TBNE: the phase and the pass of the turn a run's list shows. */
export interface CurrentTurn {
  phaseId: "working" | "reviewing";
  pass: number;
}

/** GRU-FR-JHVX: the one kind the list reads. */
export const MILESTONE_KIND = "milestone";

/** GRU-FR-YMMF: the one kind the progress line reads. */
export const MESSAGE_KIND = "message";

/** GRU-FR-WQHM: the largest plan the executor delivers (EAC-FR-VCBJ). */
const MAX_TITLES = 5;

/**
 * GRU-FR-TBNE: the turn whose milestones a run shows, or null where the run has
 * dispatched no turn.
 *
 * A run in `working` or `reviewing` is on the pass its checkpoint holds
 * (GRU-FR-PNVX). Any other run shows the last turn it dispatched, which is the
 * last segment of its activity index that names one of the two phases with a
 * pass number.
 */
export function currentTurnOf(run: GraduationRun): CurrentTurn | null {
  if (run.state === "working" || run.state === "reviewing") {
    const pass = run.checkpoint?.pass ?? 0;
    return pass >= 1 ? { phaseId: run.state, pass } : null;
  }
  const segments = run.logs?.activity?.segments ?? [];
  for (let at = segments.length - 1; at >= 0; at -= 1) {
    const segment = segments[at];
    if (
      (segment.phaseId === "working" || segment.phaseId === "reviewing") &&
      segment.pass !== null &&
      segment.pass >= 1
    ) {
      return { phaseId: segment.phaseId, pass: segment.pass };
    }
  }
  return null;
}

/** GRU-FR-FHLE: a run in any state but `working` or `reviewing` has ended its turn. */
export function turnHasEnded(run: GraduationRun): boolean {
  return run.state !== "working" && run.state !== "reviewing";
}

type Marker =
  | { event: "plan"; titles: string[] }
  | { event: "start"; index: number }
  | { event: "done"; index: number };

/** GRU-FR-ELDK: one summary read as a marker, or null where it is not one. */
function readMarker(summary: string): Marker | null {
  let value: unknown;
  try {
    value = JSON.parse(summary);
  } catch {
    return null;
  }
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const record = value as Record<string, unknown>;
  if (record.event === "plan") {
    const titles = record.milestones;
    if (
      !Array.isArray(titles) ||
      titles.length < 1 ||
      titles.length > MAX_TITLES ||
      !titles.every((title) => typeof title === "string" && title.trim() !== "")
    ) {
      return null;
    }
    return { event: "plan", titles: titles as string[] };
  }
  if (record.event === "start" || record.event === "done") {
    const index = record.index;
    if (typeof index !== "number" || !Number.isInteger(index) || index < 1) return null;
    return { event: record.event, index };
  }
  return null;
}

/**
 * GRU-FR-PZGA, GRU-FR-CYDK, GRU-FR-FHLE: the milestones the summaries give, in
 * plan order, or null where no valid plan stands.
 *
 * `summaries` are in the order of the records' `sequence`. When `ended` is true,
 * each milestone that is not done is unknown.
 */
export function milestonesFrom(summaries: string[], ended: boolean): Milestone[] | null {
  let titles: string[] | null = null;
  let started: boolean[] = [];
  let done: boolean[] = [];
  for (const summary of summaries) {
    const marker = readMarker(summary);
    if (marker === null) continue;
    if (marker.event === "plan") {
      // GRU-FR-PZGA: a plan after the first comes from a later call of the same
      // phase and pass, so it replaces the plan and starts every milestone over.
      titles = marker.titles;
      started = titles.map(() => false);
      done = titles.map(() => false);
      continue;
    }
    if (titles === null || marker.index > titles.length || done[marker.index - 1]) continue;
    if (marker.event === "start") {
      started[marker.index - 1] = true;
      // GRU-FR-JDZX: a start marker makes each earlier milestone done.
      for (let earlier = 0; earlier < marker.index - 1; earlier += 1) done[earlier] = true;
    } else done[marker.index - 1] = true;
  }
  if (titles === null) return null;
  return titles.map((title, at) => {
    let state: MilestoneState;
    if (done[at]) state = "done";
    else if (ended) state = "unknown";
    else state = started[at] ? "active" : "pending";
    return { title, state };
  });
}

/** GRU-FR-CYDK: the state in words, for the row and its accessible name. */
export function milestoneStateLabel(state: MilestoneState): string {
  switch (state) {
    case "pending":
      return "Pending";
    case "active":
      return "Active";
    case "done":
      return "Done";
    case "unknown":
      return "Unknown";
  }
}
