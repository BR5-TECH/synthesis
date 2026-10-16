/**
 * Which changes the review visits and where a decision moves it
 * (`DCR-draft-change-review.md` DCR-FR-QPRV, DCR-FR-12).
 */
import { describe, expect, it } from "vitest";

import { nextUndecidedAfter, visitedHunks } from "./draftProposals";
import type { DraftChangeProposal, HunkState } from "../types";

function proposal(states: HunkState[]): DraftChangeProposal {
  return {
    id: "p1",
    draftId: "d1",
    path: "ui/spec.md",
    agent: { kind: "agent", agentId: "a1", handle: "arch", model: "m" },
    rationale: "why",
    threadId: "t1",
    commentId: "c1",
    state: "pending",
    candidateEdited: false,
    legacy: false,
    hunkCount: states.length,
    counts: { pending: 0, accepted: 0, rejected: 0, discussing: 0 },
    ledger: states.map((state, at) => ({
      id: `h${at + 1}`,
      kind: "replace",
      state,
      edited: false,
      revision: 0,
    })),
    createdAt: "2026-01-01T00:00:00Z",
  };
}

describe("DCR-FR-QPRV: the changes a move visits", () => {
  it("DCR-FR-QPRV: visits every undecided and every rejected change, in proposal order", () => {
    const p = proposal(["accepted", "rejected", "pending", "discussing", "accepted"]);
    expect(visitedHunks(p).map((row) => row.id)).toEqual(["h2", "h3", "h4"]);
  });

  it("DCR-FR-QPRV: never visits an accepted change", () => {
    const p = proposal(["accepted", "accepted"]);
    expect(visitedHunks(p)).toEqual([]);
  });
});

describe("DCR-FR-12: where a decision moves the review", () => {
  it("DCR-FR-12: moves to the next undecided change after the decided one", () => {
    const p = proposal(["pending", "pending", "pending"]);
    expect(nextUndecidedAfter(p, "h2")).toBe("h3");
  });

  it("DCR-FR-12: rounds to the first undecided change where none follows", () => {
    const p = proposal(["pending", "pending", "pending"]);
    expect(nextUndecidedAfter(p, "h3")).toBe("h1");
  });

  it("DCR-FR-12: passes over a rejected change, which is visited but not undecided", () => {
    const p = proposal(["pending", "rejected", "pending"]);
    expect(nextUndecidedAfter(p, "h1")).toBe("h3");
  });

  it("DCR-FR-12: moves on from where an accepted rejected change stood", () => {
    // The author takes back a rejection on h2: the review goes on to h3, not
    // back to the start.
    const p = proposal(["pending", "rejected", "pending"]);
    expect(nextUndecidedAfter(p, "h2")).toBe("h3");
  });

  it("DCR-FR-12: goes nowhere once nothing is undecided", () => {
    const p = proposal(["accepted", "pending", "rejected"]);
    expect(nextUndecidedAfter(p, "h2")).toBeNull();
  });
});
