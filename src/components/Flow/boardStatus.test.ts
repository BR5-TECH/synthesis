/** The status words and the status dot of the board (`DBR-delivery-board.md`). */
import { describe, expect, it } from "vitest";

import type { TrackerFacts } from "../../types";
import { boardDot, boardStatusWord, KIND_CHIPS, milestoneBadgeClass, NO_STATUS_NAMES } from "./boardStatus";

function issue(status: string | null, state: "open" | "closed" = "open"): TrackerFacts {
  return {
    itemId: "t1",
    ref: { kind: "issue", number: 5, url: "https://github.com/acme/app/issues/5" },
    state,
    closeReason: null,
    updatedAt: "2026-11-01T00:00:00Z",
    status,
    claimEligible: false,
    externalBlockers: [],
  };
}

describe("the status dot of a GitHub plan (DBR-FR-TCPE)", () => {
  it("DBR-FR-TCPE: the default names give their dots, compared without case", () => {
    expect(boardDot({ tracker: issue("Ready") }, NO_STATUS_NAMES)).toBe("ready");
    expect(boardDot({ tracker: issue("in PROGRESS") }, NO_STATUS_NAMES)).toBe("in progress");
    expect(boardDot({ tracker: issue("In review") }, NO_STATUS_NAMES)).toBe("in review");
    expect(boardDot({ tracker: issue("DONE") }, NO_STATUS_NAMES)).toBe("done");
  });

  it("DBR-FR-TCPE: the Ready and In Progress status names of the settings give ready and in progress", () => {
    const names = { ready: "Up next", inProgress: "Doing" };
    expect(boardDot({ tracker: issue("up next") }, names)).toBe("ready");
    expect(boardDot({ tracker: issue("Doing") }, names)).toBe("in progress");
    // The default names still give their dots.
    expect(boardDot({ tracker: issue("Ready") }, names)).toBe("ready");
  });

  it("DBR-FR-TCPE: a closed issue is done, whatever its Status", () => {
    expect(boardDot({ tracker: issue("Backlog", "closed") }, NO_STATUS_NAMES)).toBe("done");
    expect(boardDot({ tracker: issue(null, "closed") }, NO_STATUS_NAMES)).toBe("done");
  });

  it("DBR-FR-TCPE: every other name, and no Status, gives backlog; the delivery state is not read", () => {
    expect(boardDot({ tracker: issue("Todo"), state: "in_progress" }, NO_STATUS_NAMES)).toBe("backlog");
    expect(boardDot({ tracker: issue(null) }, NO_STATUS_NAMES)).toBe("backlog");
    expect(boardDot({ tracker: issue("") }, NO_STATUS_NAMES)).toBe("backlog");
  });
});

describe("the status dot of a Flow file (DBR-FR-GHDZ)", () => {
  it("DBR-FR-GHDZ: each delivery state gives its dot, and no state gives no dot", () => {
    expect(boardDot({ state: "drafting" }, NO_STATUS_NAMES)).toBe("ready");
    expect(boardDot({ state: "in_progress" }, NO_STATUS_NAMES)).toBe("in progress");
    expect(boardDot({ state: "done" }, NO_STATUS_NAMES)).toBe("done");
    expect(boardDot({ state: "not_started" }, NO_STATUS_NAMES)).toBe("backlog");
    expect(boardDot({ state: "dropped" }, NO_STATUS_NAMES)).toBe("backlog");
    expect(boardDot({}, NO_STATUS_NAMES)).toBeNull();
  });
});

describe("the status word (DBR-FR-HWHO)", () => {
  it("DBR-FR-HWHO: the Status of an issue, in lowercase, and no status where it has none", () => {
    expect(boardStatusWord({ tracker: issue("In Progress") })).toBe("in progress");
    expect(boardStatusWord({ tracker: issue(null) })).toBe("no status");
  });

  it("DBR-FR-HWHO: the name of the delivery state of a Flow file item, in lowercase, and nothing with no state", () => {
    expect(boardStatusWord({ state: "not_started" })).toBe("not started");
    expect(boardStatusWord({})).toBeNull();
  });
});

describe("the kind chip and the milestone badge (DBR-FR-DZSU, DBR-FR-BYQX)", () => {
  it("DBR-FR-DZSU: each kind of a card or a group has its three-letter code", () => {
    expect([KIND_CHIPS.epic, KIND_CHIPS.feature, KIND_CHIPS.task, KIND_CHIPS.defect]).toEqual(["EPC", "FTR", "TSK", "DEF"]);
  });

  it("DBR-FR-BYQX: a milestone in progress takes the live badge, a done one the ok badge, and the rest the neutral badge", () => {
    expect(milestoneBadgeClass("in_progress")).toBe("badge badge--live");
    expect(milestoneBadgeClass("done")).toBe("badge badge--ok");
    expect(milestoneBadgeClass("not_started")).toBe("badge");
  });
});
