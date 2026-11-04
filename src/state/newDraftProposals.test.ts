/**
 * The new-draft store and its raise (`DCR-draft-change-review.md`
 * DCR-FR-RDLK, DCR-FR-QQLE).
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import {
  decompositionConfirmed,
  ensureNewDraftsLoaded,
  newDraftProposalById,
  noteNewDraftChanged,
  resetNewDraftProposals,
} from "./newDraftProposals";
import { newDraftProposalRaise } from "./proposalRaises";
import { resetLogBufferForTest } from "../logging";
import { otherCommand } from "../test/invokeMocks";
import type { NewDraftProposal } from "../types";

function proposal(over: Partial<NewDraftProposal> = {}): NewDraftProposal {
  return {
    id: "p1",
    setId: "s1",
    draftId: "d1",
    name: "relay-auth",
    position: 1,
    total: 3,
    agent: { kind: "agent", agentId: "a1", handle: "arch" },
    rationale: "why",
    threadId: "t1",
    commentId: "c1",
    state: "pending",
    revision: 1,
    createdAt: "2026-01-01T00:00:00Z",
    ...over,
  };
}

function deferred<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => otherCommand(cmd));
  resetLogBufferForTest();
  resetNewDraftProposals();
});

const reads = () => invokeMock.mock.calls.filter((c) => c[0] === "list_new_draft_proposals");

describe("the new-draft store", () => {
  it("DCR-FR-RDLK: one reading per draft, and an event folds into it", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "list_new_draft_proposals" ? Promise.resolve([proposal()]) : otherCommand(cmd),
    );
    ensureNewDraftsLoaded("d1");
    ensureNewDraftsLoaded("d1");
    await waitFor(() => expect(newDraftProposalById("p1")).toBeDefined());
    expect(reads()).toHaveLength(1);
    noteNewDraftChanged({ draftId: "d1", proposal: proposal({ state: "accepted" }), cause: "decided" });
    expect(newDraftProposalById("p1")?.state).toBe("accepted");
  });

  it("DCR-FR-RDLK: a failed reading is asked for again", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "list_new_draft_proposals" ? Promise.reject(new Error("no")) : otherCommand(cmd),
    );
    ensureNewDraftsLoaded("d1");
    await waitFor(() => expect(reads()).toHaveLength(1));
    // Asked again until the failed reading has settled and a new ask goes out.
    await waitFor(() => {
      ensureNewDraftsLoaded("d1");
      expect(reads()).toHaveLength(2);
    });
  });

  it("DCR-FR-RDLK: an event newer than a reading in flight wins over it", async () => {
    const reading = deferred<NewDraftProposal[]>();
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "list_new_draft_proposals" ? reading.promise : otherCommand(cmd),
    );
    ensureNewDraftsLoaded("d1");
    noteNewDraftChanged({ draftId: "d1", proposal: proposal({ state: "rejected" }), cause: "decided" });
    reading.resolve([proposal({ state: "pending" })]);
    await waitFor(() => expect(reads()).toHaveLength(1));
    await reading.promise;
    await Promise.resolve();
    expect(newDraftProposalById("p1")?.state).toBe("rejected");
  });

  it("DCR-FR-RDLK: a reading that lands after a reset is dropped", async () => {
    const reading = deferred<NewDraftProposal[]>();
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "list_new_draft_proposals" ? reading.promise : otherCommand(cmd),
    );
    ensureNewDraftsLoaded("d1");
    resetNewDraftProposals();
    reading.resolve([proposal()]);
    await reading.promise;
    await Promise.resolve();
    expect(newDraftProposalById("p1")).toBeUndefined();
  });

  it("DCR-FR-KNDA: a confirmation event marks the draft confirmed", () => {
    expect(decompositionConfirmed("d1")).toBe(false);
    noteNewDraftChanged({ draftId: "d1", cause: "confirmed" });
    expect(decompositionConfirmed("d1")).toBe(true);
  });
});

describe("the new-draft raise", () => {
  const root = { projectKey: "/p", worktree: "/p" };

  it("DCR-FR-QQLE: a recording and an update that stay pending raise Info with the draft's address", () => {
    for (const cause of ["recorded", "updated"] as const) {
      const raise = newDraftProposalRaise({ draftId: "d1", proposal: proposal(), cause }, root);
      expect(raise?.level).toBe("Info");
      expect(raise?.title).toBe("@arch proposed a new draft");
      expect(raise?.body).toBe("Part 1 of 3 · relay-auth");
      expect(raise?.address).toContain("d1");
    }
  });

  it("DCR-FR-QQLE: a decision, a withdraw, a confirmation, and no open project raise nothing", () => {
    expect(newDraftProposalRaise({ draftId: "d1", proposal: proposal({ state: "accepted" }), cause: "decided" }, root)).toBeNull();
    expect(newDraftProposalRaise({ draftId: "d1", proposal: proposal({ state: "withdrawn" }), cause: "updated" }, root)).toBeNull();
    expect(newDraftProposalRaise({ draftId: "d1", cause: "confirmed" }, root)).toBeNull();
    expect(newDraftProposalRaise({ draftId: "d1", proposal: proposal(), cause: "recorded" }, { projectKey: null, worktree: null })).toBeNull();
  });
});
