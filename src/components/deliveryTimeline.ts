/**
 * The layout of the Dashboard's Delivery timeline (`DSH-dashboard.md`
 * DSH-FR-PLRL, DSH-FR-NZXN, DSH-FR-HVOQ, DSH-FR-EWPI).
 *
 * Pure and free of the DOM, so the axis range, the positions, the label rows,
 * and the first scroll offset are unit tests. Dates are calendar dates
 * (`YYYY-MM-DD`) and are counted as whole UTC days, so no time zone moves a
 * milestone.
 */
import type { DeliveryMilestone, FlowItemState, FlowLeafCounts } from "../types";

/** DSH-FR-EWPI: the axis gives each dated milestone at least this width. */
export const MILESTONE_MIN_WIDTH = 140;
/** DSH-FR-PLRL: half a month of margin at each end of the axis, in days. */
export const AXIS_MARGIN_DAYS = 15;
/** The space between two labels in one row. */
const LABEL_GAP = 8;
/** A label starts this far before its tick, so the glyph sits over the tick. */
const LABEL_LEAD = 6;

const DAY_MS = 86_400_000;
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** The whole UTC day number of a `YYYY-MM-DD` date, or null for a value that is not one. */
export function dayNumber(date: string): number | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(date);
  if (!match) return null;
  const [year, month, day] = [Number(match[1]), Number(match[2]) - 1, Number(match[3])];
  const at = new Date(Date.UTC(year, month, day));
  // A date that does not exist (2026-02-30) rolls over in Date.UTC: refuse it.
  if (at.getUTCFullYear() !== year || at.getUTCMonth() !== month || at.getUTCDate() !== day) return null;
  return Math.round(at.getTime() / DAY_MS);
}

/** DSH-FR-HVOQ: whether a milestone sits on the axis. A due date that is not a date counts as none. */
export function isDated(milestone: DeliveryMilestone): boolean {
  return milestone.due !== null && dayNumber(milestone.due) !== null;
}

/** The calendar date of `now` on this machine, as `YYYY-MM-DD`. */
export function localDate(now: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
}

/** DSH-FR-NNWM / FLO-FR-XAPK: a due date before today on a milestone that is not done. */
export function isPastDue(milestone: DeliveryMilestone, today: string): boolean {
  if (milestone.due === null || milestone.state === "done") return false;
  // A date that is not a date is not compared.
  const due = dayNumber(milestone.due);
  const now = dayNumber(today);
  return due !== null && now !== null && due < now;
}

/** One segment of the leaf bar. */
export interface LeafSegment {
  state: FlowItemState;
  count: number;
}

/** DSH-FR-NNWM / MDL-FR-GDUP: the leaf states in the order done, in progress, drafting, not started, dropped. */
export function leafSegments(counts: FlowLeafCounts): LeafSegment[] {
  return [
    { state: "done", count: counts.done },
    { state: "in_progress", count: counts.inProgress },
    { state: "drafting", count: counts.drafting },
    { state: "not_started", count: counts.notStarted },
    { state: "dropped", count: counts.dropped },
  ];
}

/** DSH-FR-NNWM: the total of the leaves under a milestone. */
export function leafTotal(counts: FlowLeafCounts): number {
  return counts.done + counts.inProgress + counts.drafting + counts.notStarted + counts.dropped;
}

/** DSH-FR-OUPX: the count line of the header. */
export function milestoneCountLine(milestones: DeliveryMilestone[]): string {
  if (milestones.every((m) => m.state === "done" || m.state === "dropped")) {
    return "All milestones done";
  }
  const done = milestones.filter((m) => m.state === "done").length;
  return `${done} of ${milestones.length} milestones done`;
}

/** A dated milestone on the axis. */
export interface PlacedMilestone {
  milestone: DeliveryMilestone;
  /** The x of its due date on the axis. */
  x: number;
  /** The left edge of its label. */
  labelLeft: number;
  /** DSH-FR-EWPI: 0 for the first label row, 1 for the second. */
  row: 0 | 1;
}

/** A month label below the axis. */
export interface MonthMark {
  label: string;
  x: number;
}

export interface TimelineLayout {
  /** The width of the axis. */
  width: number;
  /** DSH-FR-NZXN: the x of today. */
  todayX: number;
  months: MonthMark[];
  /** The dated milestones, in plan order. */
  placed: PlacedMilestone[];
  /** DSH-FR-HVOQ: the milestones with no due date, in plan order. */
  undated: DeliveryMilestone[];
  /** The number of label rows, 1 or 2. */
  rows: 1 | 2;
  /** DSH-FR-EWPI: the x the first scroll shows: the current milestone, else today. */
  focusX: number;
  /** The id of the milestone the first scroll shows, or null for today. */
  focusItemId: string | null;
}

/**
 * DSH-FR-PLRL / DSH-FR-NZXN / DSH-FR-HVOQ / DSH-FR-EWPI: place the milestones.
 *
 * The axis runs from the earliest to the latest of the due dates and today,
 * with AXIS_MARGIN_DAYS at each end. It is as wide as the viewport, and never
 * narrower than MILESTONE_MIN_WIDTH for each dated milestone. A label that
 * would overlap the label before it in the first row goes to the second row.
 */
export function layoutTimeline(
  milestones: DeliveryMilestone[],
  today: string,
  viewportWidth: number,
): TimelineLayout {
  const todayDay = dayNumber(today) ?? 0;
  const dated: { milestone: DeliveryMilestone; day: number }[] = [];
  const undated: DeliveryMilestone[] = [];
  for (const milestone of milestones) {
    const day = milestone.due === null ? null : dayNumber(milestone.due);
    if (day === null) undated.push(milestone);
    else dated.push({ milestone, day });
  }

  const days = [todayDay, ...dated.map((d) => d.day)];
  const start = Math.min(...days) - AXIS_MARGIN_DAYS;
  const end = Math.max(...days) + AXIS_MARGIN_DAYS;
  const span = end - start;
  const width = Math.max(Math.floor(viewportWidth), dated.length * MILESTONE_MIN_WIDTH, MILESTONE_MIN_WIDTH);
  const xOf = (day: number) => ((day - start) / span) * width;

  const rowEnds = [-Infinity, -Infinity];
  const placed: PlacedMilestone[] = [...dated]
    // Rows are given out from left to right; the result keeps plan order.
    .map((d, index) => ({ ...d, index }))
    .sort((a, b) => a.day - b.day || a.index - b.index)
    .map(({ milestone, day, index }) => {
      const x = xOf(day);
      const labelLeft = Math.min(Math.max(x - LABEL_LEAD, 0), width - MILESTONE_MIN_WIDTH);
      let row: 0 | 1 = 0;
      if (labelLeft < rowEnds[0] + LABEL_GAP) {
        row = labelLeft >= rowEnds[1] + LABEL_GAP || rowEnds[1] < rowEnds[0] ? 1 : 0;
      }
      rowEnds[row] = labelLeft + MILESTONE_MIN_WIDTH;
      return { placed: { milestone, x, labelLeft, row }, index };
    })
    .sort((a, b) => a.index - b.index)
    .map((p) => p.placed);

  const months: MonthMark[] = [];
  const first = new Date(start * DAY_MS);
  let year = first.getUTCFullYear();
  // The first label is the first 1st of a month on the axis, the start included.
  let month = first.getUTCMonth() + (first.getUTCDate() === 1 ? 0 : 1);
  for (;;) {
    if (month > 11) {
      month = 0;
      year += 1;
    }
    const day = Math.round(Date.UTC(year, month, 1) / DAY_MS);
    if (day > end) break;
    const label = months.length === 0 || month === 0 ? `${MONTHS[month]} ${year}` : MONTHS[month];
    months.push({ label, x: xOf(day) });
    month += 1;
  }

  const current = placed.find((p) => p.milestone.current) ?? null;
  return {
    width,
    todayX: xOf(todayDay),
    months,
    placed,
    undated,
    rows: placed.some((p) => p.row === 1) ? 2 : 1,
    focusX: current ? current.x : xOf(todayDay),
    focusItemId: current ? current.milestone.itemId : null,
  };
}

/**
 * DSH-FR-EWPI: the scroll offset that shows the focus x, a third of the way
 * into the viewport, and never past either end of the axis.
 */
export function initialScrollLeft(layout: TimelineLayout, viewportWidth: number): number {
  const max = Math.max(layout.width - viewportWidth, 0);
  return Math.min(Math.max(layout.focusX - viewportWidth / 3, 0), max);
}
