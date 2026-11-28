/**
 * The derivations of the review of a plan proposal (`MPV-map-proposal-review.md`,
 * `DBR-delivery-board.md`). Pure functions, so every case is a direct call.
 */
import { describe, expect, it } from "vitest";

import { createRow, draftRow, modifyRow, proposal } from "../../test/mapPlanFixtures";
import type { MapPlanRow, RowChange } from "../../types";
import {
  decidedCount,
  firstSelection,
  ghostCards,
  ghostItemId,
  mapMarks,
  nextPendingKey,
  proposalStatus,
  rowId,
  rowItemId,
  rowOp,
  rowRef,
  rowStatus,
  rowStatuses,
  summaryOf,
  type RowStatus,
} from "./derive";

const close = (reason: "completed" | "not_planned"): RowChange => ({
  field: "close",
  before: "open",
  after: "closed",
  reason,
});
const title: RowChange = { field: "title", before: "Old", after: "New" };
const NONE: ReadonlySet<string> = new Set();

describe("rowOp (MPV-FR-UXHR)", () => {
  it("MPV-FR-UXHR: maps each kind of row to its operation", () => {
    expect(rowOp(createRow("a", "New issue"))).toBe("added");
    expect(rowOp(draftRow("d", "Draft", "body"))).toBe("draft");
    expect(rowOp(modifyRow("m", 18, "Issue", [title]))).toBe("changed");
  });

  it("MPV-FR-UXHR: a close as not planned withdraws; a close as completed is a change", () => {
    expect(rowOp(modifyRow("w", 16, "Slack", [close("not_planned")]))).toBe("withdrawn");
    expect(rowOp(modifyRow("c", 16, "Slack", [close("completed")]))).toBe("changed");
    expect(rowOp(modifyRow("x", 16, "Slack", [title, close("not_planned")]))).toBe("withdrawn");
  });
});

describe("rowStatus (MPV-FR-JXFA)", () => {
  const row = (state: MapPlanRow["state"]) => createRow("r", "T", { state });

  it("MPV-FR-JXFA: maps each stored state to its status", () => {
    expect(rowStatus(row("waiting"), false)).toBe("pending");
    expect(rowStatus(row("accepted"), false)).toBe("applied");
    expect(rowStatus(row("rejected"), false)).toBe("rejected");
    expect(rowStatus(row("failed"), false)).toBe("failed");
  });

  it("MPV-FR-JXFA: a row whose decision runs is applying, whatever its stored state", () => {
    for (const state of ["waiting", "accepted", "rejected", "failed"] as const) {
      expect(rowStatus(row(state), true)).toBe("applying");
    }
  });

  it("MPV-FR-JXFA: rowStatuses reads the busy set by proposal and row key", () => {
    const p = proposal([createRow("a", "A"), createRow("b", "B")]);
    expect(rowStatuses(p, new Set([rowId("p1", "b")]))).toEqual(["pending", "applying"]);
    expect(rowStatuses(p, new Set([rowId("other", "b")]))).toEqual(["pending", "pending"]);
  });
});

describe("proposalStatus (MPV-FR-LOSO)", () => {
  const cases: [RowStatus[], RowStatus][] = [
    [["applying", "pending", "failed"], "applying"],
    [["applied", "pending", "failed"], "pending"],
    [["applied", "failed", "rejected"], "failed"],
    [["rejected", "rejected"], "rejected"],
    [["rejected", "applied"], "applied"],
    [["applied"], "applied"],
    [["pending"], "pending"],
    [["failed"], "failed"],
  ];
  for (const [rows, expected] of cases) {
    it(`MPV-FR-LOSO: ${rows.join(", ")} gives ${expected}`, () => {
      expect(proposalStatus(rows)).toBe(expected);
    });
  }

  it("MPV-FR-LOSO: applying wins over every other status, in any order", () => {
    expect(proposalStatus(["failed", "rejected", "applying"])).toBe("applying");
  });
});

describe("summary and counts (MPV-FR-WKNB, MPV-FR-GYLV)", () => {
  it("MPV-FR-WKNB: counts the rows by operation in a fixed order and leaves out zeros", () => {
    const p = proposal([
      createRow("a", "A"),
      modifyRow("w", 16, "W", [close("not_planned")]),
      modifyRow("m", 18, "M", [title]),
      modifyRow("n", 11, "N", [title]),
    ]);
    expect(summaryOf(p)).toBe("2 changed, 1 added, 1 withdrawn");
  });

  it("MPV-FR-WKNB: a draft row is counted as draft, after the other operations", () => {
    expect(summaryOf(proposal([draftRow("d", "D", "x"), createRow("a", "A")]))).toBe("1 added, 1 change");
  });

  it("MPV-FR-WKNB: a proposal of one issue row starts with its reference", () => {
    expect(summaryOf(proposal([modifyRow("m", 18, "M", [title])]))).toBe("#18, 1 changed");
  });

  it("MPV-FR-WKNB: a proposal of one added row has no reference to start with", () => {
    expect(summaryOf(proposal([createRow("a", "A")]))).toBe("1 added");
  });

  it("MPV-FR-GYLV: counts the rows that are no longer waiting", () => {
    const p = proposal([
      createRow("a", "A"),
      createRow("b", "B", { state: "accepted" }),
      createRow("c", "C", { state: "failed" }),
    ]);
    expect(decidedCount(p)).toBe(2);
  });
});

describe("selection (MPV-FR-CAQL)", () => {
  const p = proposal([createRow("a", "A"), createRow("b", "B"), createRow("c", "C"), createRow("d", "D")]);

  it("MPV-FR-CAQL: selects the next pending row after the decided one", () => {
    expect(nextPendingKey(p, ["applied", "pending", "pending", "pending"], "a")).toBe("b");
    expect(nextPendingKey(p, ["pending", "applied", "rejected", "pending"], "b")).toBe("d");
  });

  it("MPV-FR-CAQL: wraps to the first pending row when none follows", () => {
    expect(nextPendingKey(p, ["pending", "applied", "applied", "applied"], "d")).toBe("a");
  });

  it("MPV-FR-CAQL: keeps the row when no row is pending", () => {
    expect(nextPendingKey(p, ["applied", "applied", "rejected", "failed"], "c")).toBe("c");
  });

  it("MPV-FR-CAQL: opens on the first pending row, else on the first row", () => {
    expect(firstSelection(p, ["applied", "rejected", "pending", "pending"])).toBe("c");
    expect(firstSelection(p, ["applied", "applied", "applied", "applied"])).toBe("a");
    expect(firstSelection(proposal([]), [])).toBeNull();
  });
});

describe("references (MPV-FR-ENVX)", () => {
  it("MPV-FR-ENVX: an issue row shows its number, an added row `new`, a draft nothing", () => {
    expect(rowRef(modifyRow("m", 18, "M", [title]))).toBe("#18");
    expect(rowRef(createRow("a", "A"))).toBe("new");
    expect(rowRef(draftRow("d", "D", ""))).toBeNull();
  });

  it("MPV-FR-ENVX: an applied added row shows the number it created", () => {
    const row = createRow("a", "A", { state: "accepted", result: { issueNumber: 32 } });
    expect(rowRef(row)).toBe("#32");
  });
});

describe("board marks (DBR-FR-AGIN, DBR-FR-WKNI, DBR-FR-PMGS)", () => {
  const p = proposal([
    modifyRow("m", 18, "M", [title]),
    modifyRow("w", 16, "W", [close("not_planned")]),
    modifyRow("done", 11, "Done", [title], { state: "accepted" }),
    modifyRow("no", 12, "No", [title], { state: "rejected" }),
    createRow("a", "Added", {}, { milestone: { number: 2, title: "M2" }, type: "Feature" }),
    createRow("b", "Loose"),
    createRow("gone", "Gone", { state: "rejected" }),
    draftRow("d", "Draft", "x"),
  ]);

  it("DBR-FR-AGIN: a pending changed row marks `proposed` and a withdrawn row marks `withdrawn`", () => {
    const marks = mapMarks([p], NONE);
    expect(marks.get("issue-18")).toBe("proposed");
    expect(marks.get("issue-16")).toBe("withdrawn");
  });

  it("DBR-FR-AGIN: an applied row and a rejected row mark nothing", () => {
    const marks = mapMarks([p], NONE);
    expect(marks.has("issue-11")).toBe(false);
    expect(marks.has("issue-12")).toBe(false);
    expect([...marks.keys()].sort()).toEqual(["issue-16", "issue-18"]);
  });

  it("DBR-FR-AGIN: an applying row keeps its mark", () => {
    const decided = proposal([modifyRow("m", 18, "M", [title], { state: "accepted" })]);
    expect(mapMarks([decided], new Set([rowId("p1", "m")])).get("issue-18")).toBe("proposed");
  });

  it("DBR-FR-WKNI: each pending added row gives one ghost card in its milestone, or in none", () => {
    expect(ghostCards([p], NONE)).toEqual([
      {
        id: ghostItemId("p1", "a"),
        proposalId: "p1",
        rowKey: "a",
        title: "Added",
        type: "Feature",
        milestoneId: "milestone-2",
      },
      { id: ghostItemId("p1", "b"), proposalId: "p1", rowKey: "b", title: "Loose", type: null, milestoneId: null },
    ]);
  });

  it("DBR-FR-WKNI: an applied or rejected added row gives no ghost card", () => {
    const decided = proposal([createRow("a", "A", { state: "accepted", result: { issueNumber: 40 } })]);
    expect(ghostCards([decided], NONE)).toEqual([]);
  });

  it("DBR-FR-PMGS: the item of a row is its issue, or its ghost card until it is applied", () => {
    expect(rowItemId("p1", modifyRow("m", 18, "M", [title]))).toBe("issue-18");
    expect(rowItemId("p1", createRow("a", "A"))).toBe(ghostItemId("p1", "a"));
    expect(rowItemId("p1", createRow("a", "A", { state: "accepted", result: { issueNumber: 40 } }))).toBe("issue-40");
    expect(rowItemId("p1", draftRow("d", "D", ""))).toBeNull();
  });
});
