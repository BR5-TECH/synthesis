/**
 * The rail's views and its filter
 * (`../../specifications/ui/GRH-graduation-history.md`).
 *
 * Everything here is a function of the runs as the backend reports them. The
 * rail issues no read of its own: it narrows the one listing the section
 * already made.
 */

import { conditionOf, holdsStream, isTerminalRun } from "./graduation";
import type { GraduationRun } from "../types";

/** GRH-FR-WIMY: the four exclusive positions of the view selector. */
export type GraduationView = "in-flight" | "completed" | "archived" | "all";

/** GRH-FR-WIMY: In-flight is the resting position. */
export const DEFAULT_VIEW: GraduationView = "in-flight";

/**
 * GRH-FR-WIMY: the views, in the order the selector renders them.
 *
 * The shape the shell's one selector control takes (`SelectorRow`), so the
 * rail's view control and every other panel's read as one vocabulary rather
 * than as four buttons of their own.
 */
export const VIEW_POSITIONS: Array<{
  value: GraduationView;
  tag: string;
  title: string;
}> = [
  { value: "in-flight", tag: "In-flight", title: "Runs that have not ended" },
  { value: "completed", tag: "Completed", title: "Runs that completed" },
  { value: "archived", tag: "Archived", title: "Runs the author filed away" },
  { value: "all", tag: "All", title: "Every run this project has made" },
];

/** GRH-FR-QVEX: whether a held value still names one of the four positions. */
export function isGraduationView(value: unknown): value is GraduationView {
  return VIEW_POSITIONS.some((position) => position.value === value);
}

/** GRH-FR-DYNU: whether the author has filed this run away. */
export function isArchived(run: GraduationRun): boolean {
  return run.archived;
}

/**
 * GRH-FR-YMIM: whether this run is holding a work stream.
 *
 * A filed-away run that holds one stays in **In-flight** as well as in
 * **Archived**: a stream nothing else will release must never be held by a run
 * the author cannot see. An author-paused run holds nothing — it gave its
 * stream back and the run behind it started (GRD-FR-MDQZ).
 */
export function holdsAStream(run: GraduationRun): boolean {
  return holdsStream(run.state);
}

/**
 * GRH-FR-KZTP: whether this run's work has not ended.
 *
 * Module-private: "in flight" is the name of a position here, and a caller
 * outside this module asking the question means something of its own.
 */
function isInFlight(run: GraduationRun): boolean {
  return !isTerminalRun(run.state);
}

/** GRH-FR-KZTP / GRH-FR-DYNU / GRH-FR-YMIM: which runs a view admits. */
export function admits(view: GraduationView, run: GraduationRun): boolean {
  switch (view) {
    case "in-flight":
      // A run waiting in a queue is under way as much as a working one, so the
      // two are read together. The one archived run this position admits is
      // the one holding a stream (GRH-FR-YMIM).
      return isInFlight(run) && (!isArchived(run) || holdsAStream(run));
    case "completed":
      // GRH-FR-KZTP: a completion and nothing else. A run that failed and one
      // the author discarded both ended, and neither finished.
      return run.state === "completed" && !isArchived(run);
    case "archived":
      return isArchived(run);
    case "all":
      return true;
  }
}

/**
 * GRH-FR-XBWU: the position that reveals this run, given the one in force.
 *
 * A position that already admits the run is returned untouched, so revealing a
 * run the author can see costs them nothing.
 */
export function revealingView(
  run: GraduationRun,
  view: GraduationView,
): GraduationView {
  if (admits(view, run)) return view;
  return isArchived(run) ? "archived" : "all";
}

/** GRH-FR-XBWU: the text filter, cleared only where it hides the run. */
export function revealingText(
  run: GraduationRun,
  text: string,
  stateWord: (run: GraduationRun) => string,
): string {
  return matchesText(run, text, stateWord) ? text : "";
}

/**
 * GRH-FR-BDMB: the text filter matches the run's title (a draft name, or a
 * merge run's `merge.name`), the stream name and the state word together.
 *
 * The needle is compared literally: a draft named `c++ parser` is found by
 * typing `c++` rather than read as a pattern.
 */
export function matchesText(
  run: GraduationRun,
  text: string,
  stateWord: (run: GraduationRun) => string,
): boolean {
  const needle = text.trim().toLowerCase();
  if (needle === "") return true;
  return [
    // GRU-FR-HDPQ: a merge run's title is `merge.name`, and the filter matches
    // it where it matches a draft run's draft name.
    run.merge?.name ?? run.input.draftName,
    run.streamName,
    run.directTarget?.worktreeName ?? "",
    run.directTarget?.branch ?? "",
    stateWord(run),
  ]
    .join(" ")
    .toLowerCase()
    .includes(needle);
}

/**
 * GRH-FR-BDMB: the listing, narrowed by the view in force and then by the text.
 *
 * The filter narrows the view rather than replacing it, so a filed-away run
 * whose name matches stays absent from In-flight.
 */
export function admitted(
  runs: GraduationRun[],
  view: GraduationView,
  text: string,
  stateWord: (run: GraduationRun) => string,
): GraduationRun[] {
  return runs.filter(
    (run) => admits(view, run) && matchesText(run, text, stateWord),
  );
}

/** GRH-FR-NCJK: the rail lists in the project's own run order. */
export function railOrder(runs: GraduationRun[]): GraduationRun[] {
  return runs;
}

/** The condition a row renders its run in. */
export { conditionOf };

/**
 * GRH-FR-MCHQ: the rail's width, as a fraction of the section's own width.
 *
 * A fraction rather than a pixel count, so a rail sized in a wide window keeps
 * its share of a narrow one. The bounds are what keep both regions usable: a
 * rail below the floor holds no run name, and one above the ceiling takes the
 * room the run region needs for its stage row.
 */
export const MIN_RAIL_FRACTION = 0.05;
export const MAX_RAIL_FRACTION = 0.3;

/** GRH-FR-MCHQ: what a rail nobody has sized takes. */
export const DEFAULT_RAIL_FRACTION = 0.2;

/** GRH-FR-MCHQ: what one arrow key press moves the width by. */
export const RAIL_STEP = 0.01;

/**
 * GRH-FR-MCHQ: a stored or dragged value, brought inside the bounds.
 *
 * A value that is absent or is not a number reads as the default: a record
 * written before the preference existed holds none, and the rail must open at
 * a width rather than at nothing.
 */
export function clampRailFraction(fraction: number | undefined | null): number {
  if (typeof fraction !== "number" || !Number.isFinite(fraction)) {
    return DEFAULT_RAIL_FRACTION;
  }
  return Math.min(MAX_RAIL_FRACTION, Math.max(MIN_RAIL_FRACTION, fraction));
}

/**
 * GRH-FR-MCHQ: the width as the percentage the divider announces.
 *
 * Rounded to a whole number, which is what the keyboard steps move in and what
 * a reader is told; the stored fraction keeps whatever precision a drag gave
 * it, so announcing is a reading of the value rather than a rounding of it.
 */
export function railPercent(fraction: number): number {
  return Math.round(clampRailFraction(fraction) * 100);
}

/** GRH-FR-MCHQ: where a pointer at `x` inside a section of `width` puts it. */
export function fractionForDrag(x: number, width: number): number {
  if (!Number.isFinite(width) || width <= 0) return DEFAULT_RAIL_FRACTION;
  return clampRailFraction(x / width);
}

/**
 * GRH-FR-MCHQ / GRH-FR-OFHS: what a key press does to the width.
 *
 * Left and Right move it by one percentage point in the leading and trailing
 * directions, Home sets the floor and End the ceiling, each stopping at the
 * bounds. `null` for a key this control does not answer to, so the caller
 * leaves the event alone rather than swallowing it.
 *
 * Stepping is computed from the announced percentage rather than from the raw
 * fraction, so a width a drag left at 17.4 % steps to 18 % rather than to
 * 18.4 % and the author's own arrow keys are what tidy it up.
 */
export function fractionForKey(
  key: string,
  fraction: number,
): number | null {
  switch (key) {
    case "ArrowLeft":
      return clampRailFraction((railPercent(fraction) - 1) / 100);
    case "ArrowRight":
      return clampRailFraction((railPercent(fraction) + 1) / 100);
    case "Home":
      return MIN_RAIL_FRACTION;
    case "End":
      return MAX_RAIL_FRACTION;
    default:
      return null;
  }
}
