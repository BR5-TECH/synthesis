/**
 * The layout of the Dashboard's Delivery timeline
 * (`../../specifications/ui/DSH-dashboard.md` DSH-FR-PLRL, DSH-FR-NZXN,
 * DSH-FR-NNWM, DSH-FR-OUPX, DSH-FR-HVOQ, DSH-FR-EWPI).
 */
import { describe, expect, it } from "vitest";

import {
  AXIS_MARGIN_DAYS,
  MILESTONE_MIN_WIDTH,
  dayNumber,
  initialScrollLeft,
  isDated,
  isPastDue,
  layoutTimeline,
  leafSegments,
  leafTotal,
  localDate,
  milestoneCountLine,
} from "./deliveryTimeline";
import type { DeliveryMilestone } from "../types";

function milestone(itemId: string, due: string | null, extra: Partial<DeliveryMilestone> = {}): DeliveryMilestone {
  return {
    itemId,
    name: itemId,
    due,
    state: "not_started",
    leafCounts: { notStarted: 1, drafting: 0, inProgress: 0, done: 0, dropped: 0 },
    blockedLeaves: 0,
    current: false,
    ...extra,
  };
}

const TODAY = "2026-11-14";

describe("dates", () => {
  it("DSH-FR-PLRL: a calendar date is a whole day number, and a value that is not one is null", () => {
    expect(dayNumber("2026-11-15")! - dayNumber("2026-11-14")!).toBe(1);
    expect(dayNumber("2027-01-01")! - dayNumber("2026-12-31")!).toBe(1);
    expect(dayNumber("14/11/2026")).toBeNull();
    expect(dayNumber("2026-02-30")).toBeNull();
    expect(dayNumber("")).toBeNull();
  });

  it("DSH-FR-NZXN: today is the calendar date of the local clock", () => {
    expect(localDate(new Date(2026, 10, 14, 23, 59))).toBe("2026-11-14");
    expect(localDate(new Date(2027, 0, 3, 0, 1))).toBe("2027-01-03");
  });

  it("DSH-FR-NNWM: a due date before today on a milestone that is not done is past", () => {
    expect(isPastDue(milestone("a", "2026-11-13"), TODAY)).toBe(true);
    expect(isPastDue(milestone("a", "2026-11-14"), TODAY)).toBe(false);
    expect(isPastDue(milestone("a", "2026-11-13", { state: "done" }), TODAY)).toBe(false);
    // FLO-FR-XAPK says "not done", so a dropped milestone is marked too.
    expect(isPastDue(milestone("a", "2026-11-13", { state: "dropped" }), TODAY)).toBe(true);
    expect(isPastDue(milestone("a", null), TODAY)).toBe(false);
  });
});

describe("leaves and counts", () => {
  it("DSH-FR-NNWM, MDL-FR-GDUP: the bar runs done, in progress, drafting, not started, dropped", () => {
    const counts = { notStarted: 4, drafting: 3, inProgress: 2, done: 1, dropped: 5 };
    expect(leafSegments(counts)).toEqual([
      { state: "done", count: 1 },
      { state: "in_progress", count: 2 },
      { state: "drafting", count: 3 },
      { state: "not_started", count: 4 },
      { state: "dropped", count: 5 },
    ]);
    expect(leafTotal(counts)).toBe(15);
  });

  it("DSH-FR-OUPX: the count line names done milestones, or that all are done", () => {
    const done = milestone("a", null, { state: "done" });
    const open = milestone("b", null, { state: "in_progress" });
    const dropped = milestone("c", null, { state: "dropped" });
    expect(milestoneCountLine([done, open, dropped])).toBe("1 of 3 milestones done");
    expect(milestoneCountLine([done, dropped])).toBe("All milestones done");
  });

  it("DSH-FR-OUPX: the count line ends in the sum of the blocked work items, and only when there is one", () => {
    const a = milestone("a", null, { state: "in_progress", blockedLeaves: 2 });
    const b = milestone("b", null, { blockedLeaves: 1 });
    const c = milestone("c", null, { state: "done" });
    expect(milestoneCountLine([a, b, c])).toBe("1 of 3 milestones done · 3 blocked");
    expect(milestoneCountLine([c])).toBe("All milestones done");
  });
});

describe("the axis (DSH-FR-PLRL / DSH-FR-EWPI)", () => {
  it("DSH-FR-PLRL: runs from the earliest to the latest of the dues and today, with half a month at each end", () => {
    const layout = layoutTimeline(
      [milestone("beta", "2027-03-31"), milestone("ga", "2027-06-30")],
      TODAY,
      1000,
    );
    const start = dayNumber(TODAY)! - AXIS_MARGIN_DAYS;
    const end = dayNumber("2027-06-30")! + AXIS_MARGIN_DAYS;
    const x = (date: string) => ((dayNumber(date)! - start) / (end - start)) * 1000;
    expect(layout.width).toBe(1000);
    expect(layout.todayX).toBeCloseTo(x(TODAY));
    expect(layout.placed.map((p) => p.x)).toEqual([x("2027-03-31"), x("2027-06-30")]);
    // The axis ends half a month after the latest date.
    expect(layout.width - layout.placed[1].x).toBeCloseTo((AXIS_MARGIN_DAYS / (end - start)) * 1000);
  });

  it("DSH-FR-PLRL: today after every due date still sits on the axis", () => {
    const layout = layoutTimeline([milestone("alpha", "2026-09-30")], TODAY, 600);
    expect(layout.placed[0].x).toBeLessThan(layout.todayX);
    expect(layout.todayX).toBeLessThan(layout.width);
  });

  it("DSH-FR-EWPI: the axis gives each dated milestone at least 140px", () => {
    const four = ["2027-01-31", "2027-02-28", "2027-03-31", "2027-04-30"].map((d) => milestone(d, d));
    expect(layoutTimeline(four, TODAY, 300).width).toBe(4 * MILESTONE_MIN_WIDTH);
    expect(layoutTimeline(four, TODAY, 900).width).toBe(900);
  });

  it("DSH-FR-PLRL: month labels sit at each first of the month, with the year on the first and on January", () => {
    const layout = layoutTimeline([milestone("beta", "2027-03-31")], TODAY, 1000);
    expect(layout.months.map((m) => m.label)).toEqual(["Nov 2026", "Dec", "Jan 2027", "Feb", "Mar", "Apr"]);
    const xs = layout.months.map((m) => m.x);
    expect([...xs].sort((a, b) => a - b)).toEqual(xs);
  });

  it("DSH-FR-EWPI: a label that would overlap the one before it takes the second row", () => {
    const layout = layoutTimeline(
      [
        milestone("a", "2027-03-01"),
        milestone("b", "2027-03-03"),
        milestone("c", "2027-06-30"),
      ],
      TODAY,
      600,
    );
    expect(layout.placed.map((p) => [p.milestone.itemId, p.row])).toEqual([
      ["a", 0],
      ["b", 1],
      ["c", 0],
    ]);
    expect(layout.rows).toBe(2);
  });

  it("DSH-FR-EWPI: labels far apart stay in one row, and rows go by date, not by plan order", () => {
    const layout = layoutTimeline(
      [milestone("late", "2027-06-30"), milestone("early", "2027-01-15")],
      TODAY,
      800,
    );
    expect(layout.rows).toBe(1);
    // Plan order is kept in the result.
    expect(layout.placed.map((p) => p.milestone.itemId)).toEqual(["late", "early"]);
  });

  it("DSH-FR-EWPI: no label leaves the axis at either end", () => {
    // GA sits near the right end of a 300px axis, so its label must be moved in.
    const layout = layoutTimeline([milestone("ga", "2027-06-30")], TODAY, 300);
    expect(layout.placed[0].x + MILESTONE_MIN_WIDTH).toBeGreaterThan(layout.width);
    for (const p of layout.placed) {
      expect(p.labelLeft).toBeGreaterThanOrEqual(0);
      expect(p.labelLeft + MILESTONE_MIN_WIDTH).toBeLessThanOrEqual(layout.width);
    }
  });

  it("DSH-FR-HVOQ: milestones without a due date leave the axis, in plan order", () => {
    const layout = layoutTimeline(
      [milestone("v3", null), milestone("beta", "2027-03-31"), milestone("v2", null)],
      TODAY,
      600,
    );
    expect(layout.placed.map((p) => p.milestone.itemId)).toEqual(["beta"]);
    expect(layout.undated.map((m) => m.itemId)).toEqual(["v3", "v2"]);
  });

  it("DSH-FR-HVOQ: a due date that is not a date puts the milestone in the No date group", () => {
    const layout = layoutTimeline([milestone("odd", "2027/03/31"), milestone("beta", "2027-03-31")], TODAY, 600);
    expect(layout.placed.map((p) => p.milestone.itemId)).toEqual(["beta"]);
    expect(layout.undated.map((m) => m.itemId)).toEqual(["odd"]);
    expect(isDated(milestone("odd", "2027/03/31"))).toBe(false);
    expect(isDated(milestone("beta", "2027-03-31"))).toBe(true);
  });

  it("DSH-FR-EWPI: three labels on one date use two rows and no more", () => {
    const same = ["a", "b", "c"].map((id) => milestone(id, "2027-03-31"));
    const layout = layoutTimeline(same, TODAY, 600);
    expect(layout.rows).toBe(2);
    expect(layout.placed.every((p) => p.row === 0 || p.row === 1)).toBe(true);
  });

  it("DSH-FR-PLRL: an axis that starts on the 1st of a month labels that month", () => {
    // 2026-12-16 less 15 days is 2026-12-01.
    const layout = layoutTimeline([milestone("a", "2027-02-15")], "2026-12-16", 600);
    expect(layout.months[0]).toEqual({ label: "Dec 2026", x: 0 });
  });

  it("DSH-FR-NZXN: with no dated milestone the axis still holds today", () => {
    const layout = layoutTimeline([milestone("v2", null)], TODAY, 500);
    expect(layout.placed).toEqual([]);
    expect(layout.width).toBe(500);
    expect(layout.todayX).toBeCloseTo(250);
  });
});

describe("the first scroll (DSH-FR-EWPI)", () => {
  it("DSH-FR-EWPI: shows the current milestone", () => {
    const layout = layoutTimeline(
      [
        milestone("alpha", "2026-09-30", { state: "done" }),
        milestone("beta", "2027-03-31", { current: true }),
        milestone("ga", "2027-06-30"),
      ],
      TODAY,
      200,
    );
    expect(layout.focusItemId).toBe("beta");
    expect(layout.focusX).toBe(layout.placed[1].x);
    const left = initialScrollLeft(layout, 200);
    // A third of the way into the viewport, inside both ends of the axis.
    const third = layout.focusX - 200 / 3;
    expect(third).toBeGreaterThan(0);
    expect(third).toBeLessThan(layout.width - 200);
    expect(left).toBeCloseTo(third);
  });

  it("DSH-FR-EWPI: shows today where no milestone is current, or the current one has no date", () => {
    const none = layoutTimeline([milestone("a", "2027-03-31"), milestone("b", "2027-06-30")], TODAY, 200);
    expect(none.focusItemId).toBeNull();
    expect(none.focusX).toBe(none.todayX);
    const undated = layoutTimeline(
      [milestone("a", "2027-03-31"), milestone("b", null, { current: true })],
      TODAY,
      200,
    );
    expect(undated.focusItemId).toBeNull();
    expect(undated.focusX).toBe(undated.todayX);
  });

  it("DSH-FR-EWPI: never scrolls past either end of the axis", () => {
    const layout = layoutTimeline(
      ["2027-01-31", "2027-02-28", "2027-03-31", "2027-12-31"].map((d) =>
        milestone(d, d, { current: d === "2027-12-31" }),
      ),
      TODAY,
      200,
    );
    expect(initialScrollLeft(layout, 200)).toBe(layout.width - 200);
    expect(initialScrollLeft({ ...layout, focusX: 0 }, 200)).toBe(0);
    expect(initialScrollLeft(layout, layout.width + 100)).toBe(0);
  });
});
