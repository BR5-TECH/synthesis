/**
 * The ledger helpers with a `withdrawn` change in the ledger, and what an
 * update does to an author's unwritten edits
 * (`../../specifications/ui/DCR-draft-change-review.md` DCR-FR-11,
 * DCR-FR-OZVU, DCR-FR-QPRV, DCR-FR-WRJP; `../core/DCP-draft-change-proposals.md`
 * DCP-FR-PWSF).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import {
  firstUndecidedHunk,
  hunkPosition,
  nextUndecidedAfter,
  noteProposalChanged,
  resetDraftProposals,
  reviewedHunkTotal,
  revisedHunkIds,
  undecidedHunks,
  visitedHunks,
} from "./draftProposals";
import { hunkCandidateBuffers, hunkKey } from "./candidateBuffers";
import { resetLogBufferForTest } from "../logging";
import { isUndecided } from "../types";
import type { DraftChangeProposal, HunkLedgerRow, HunkState } from "../types";

function row(id: string, state: HunkState, revision = 0): HunkLedgerRow {
  return { id, kind: "replace", state, edited: false, revision };
}

function proposal(ledger: HunkLedgerRow[], over: Partial<DraftChangeProposal> = {}): DraftChangeProposal {
  return {
    id: "p1",
    draftId: "d1",
    path: "prompt.md",
    agent: { kind: "agent", agentId: "a1", handle: "arch" },
    rationale: "why",
    threadId: "t1",
    commentId: "c1",
    state: "pending",
    candidateEdited: false,
    legacy: false,
    hunkCount: ledger.length,
    counts: { pending: 0, accepted: 0, rejected: 0, discussing: 0, withdrawn: 0 },
    ledger,
    createdAt: "2026-01-01T00:00:00Z",
    ...over,
  };
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "append_log_records") return undefined;
    throw new Error(`unexpected command ${cmd}`);
  });
  resetLogBufferForTest();
  resetDraftProposals();
  hunkCandidateBuffers.clear();
});

afterEach(() => {
  vi.useRealTimers();
  hunkCandidateBuffers.clear();
});

describe("a withdrawn change in the ledger (DCR-FR-OZVU)", () => {
  const ledger = [
    row("h1", "pending"),
    row("h2", "withdrawn"),
    row("h3", "rejected"),
    row("h4", "discussing"),
  ];

  it("DCP-FR-PWSF, DCR-FR-OZVU: is neither undecided nor visited", () => {
    expect(isUndecided("withdrawn")).toBe(false);
    expect(undecidedHunks(proposal(ledger)).map((r) => r.id)).toEqual(["h1", "h4"]);
    // DCR-FR-QPRV: a move visits the undecided and the rejected, never it.
    expect(visitedHunks(proposal(ledger)).map((r) => r.id)).toEqual(["h1", "h3", "h4"]);
  });

  it("DCR-FR-11, DCR-FR-OZVU: is not counted, and the changes after it keep a place among the rest", () => {
    const p = proposal(ledger);
    expect(reviewedHunkTotal(p)).toBe(3);
    expect(hunkPosition(p, "h1")).toBe(1);
    expect(hunkPosition(p, "h3")).toBe(2);
    expect(hunkPosition(p, "h4")).toBe(3);
    // It has no place of its own.
    expect(hunkPosition(p, "h2")).toBe(0);
  });

  it("DCR-FR-11: a record with no ledger counts its own hunk count", () => {
    expect(reviewedHunkTotal({ hunkCount: 2, ledger: null })).toBe(2);
  });

  it("DCR-FR-12, DCR-FR-OZVU: the review does not move onto it after a decision", () => {
    const p = proposal([row("h1", "pending"), row("h2", "withdrawn"), row("h3", "pending")]);
    expect(nextUndecidedAfter(p, "h1")).toBe("h3");
    expect(firstUndecidedHunk(proposal([row("h1", "withdrawn"), row("h2", "pending")]))?.id).toBe("h2");
  });
});

describe("an update of the open proposal (DCR-FR-WRJP)", () => {
  it("DCR-FR-WRJP: names the changes whose revision went up and the ones that became withdrawn", () => {
    const before = proposal([row("h1", "pending"), row("h2", "rejected"), row("h3", "pending")]);
    const after = proposal([
      row("h1", "pending", 1),
      row("h2", "pending", 1),
      row("h3", "withdrawn"),
      row("h4", "pending"),
    ]);
    expect(revisedHunkIds(before, after)).toEqual(["h1", "h2", "h3"]);
    expect(revisedHunkIds(undefined, after)).toEqual([]);
  });

  it("DCR-FR-WRJP, DCP-FR-TQBO: an unwritten edit of a changed or withdrawn hunk is dropped, and others are kept", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    const before = proposal([row("h1", "pending"), row("h2", "pending"), row("h3", "pending")]);
    noteProposalChanged({ draftId: "d1", proposal: before, cause: "recorded" });
    for (const id of ["h1", "h2", "h3"]) {
      hunkCandidateBuffers.adopt(hunkKey("p1", id), "agent text", "hc-1");
      hunkCandidateBuffers.edit(hunkKey("p1", id), "the author's text");
    }

    noteProposalChanged({
      draftId: "d1",
      cause: "updated",
      proposal: proposal([row("h1", "pending", 1), row("h2", "withdrawn"), row("h3", "pending")]),
    });

    expect(hunkCandidateBuffers.get(hunkKey("p1", "h1"))).toBeUndefined();
    expect(hunkCandidateBuffers.get(hunkKey("p1", "h2"))).toBeUndefined();
    expect(hunkCandidateBuffers.get(hunkKey("p1", "h3"))?.text).toBe("the author's text");
    // The dropped edits are not written when their rest ends; the kept one is.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "append_log_records") return undefined;
      if (cmd === "edit_draft_change_hunk") return { checksum: "hc-2" };
      throw new Error(`unexpected command ${cmd}`);
    });
    await vi.runOnlyPendingTimersAsync();
    vi.useRealTimers();
    const writes = () =>
      invokeMock.mock.calls
        .filter((c) => c[0] === "edit_draft_change_hunk")
        .map((c) => (c[1] as { hunkId: string }).hunkId);
    await vi.waitFor(() => expect(writes()).toEqual(["h3"]));
  });

  it("DCR-FR-WRJP: a decision event, which bumps no revision, drops no edit", () => {
    const before = proposal([row("h1", "pending"), row("h2", "pending")]);
    noteProposalChanged({ draftId: "d1", proposal: before, cause: "recorded" });
    hunkCandidateBuffers.adopt(hunkKey("p1", "h2"), "agent text", "hc-1");
    hunkCandidateBuffers.edit(hunkKey("p1", "h2"), "the author's text");

    noteProposalChanged({
      draftId: "d1",
      cause: "decided",
      proposal: proposal([row("h1", "accepted"), row("h2", "pending")]),
    });

    expect(hunkCandidateBuffers.get(hunkKey("p1", "h2"))?.text).toBe("the author's text");
  });
});
