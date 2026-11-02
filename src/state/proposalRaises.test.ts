/**
 * What the shell raises and retracts for each proposal event
 * (`../../specifications/ui/DCR-draft-change-review.md` DCR-FR-18,
 * DCR-FR-QXFU; `PCR-prompt-change-review.md` PCR-FR-17, PCR-FR-26;
 * `NTF-notifications.md` NTF-FR-24).
 *
 * The decision is a pure function of the event's cause and the proposal's
 * state, so each cause is stated here once for each state.
 */
import { describe, expect, it } from "vitest";

import { draftProposalRaise, promptProposalReaction } from "./proposalRaises";
import { mintAddress } from "./notificationAddress";
import { proposal as promptFixture } from "../test/promptProposalFixtures";
import type {
  DraftChangeProposal,
  PromptChangeProposal,
  PromptProposalChangeCause,
  ProposalChangeCause,
} from "../types";

const ROOT = { projectKey: "~/dev/acme", worktree: "/w/acme" };

function draftProposal(over: Partial<DraftChangeProposal> = {}): DraftChangeProposal {
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
    hunkCount: 1,
    counts: { pending: 1, accepted: 0, rejected: 0, discussing: 0, withdrawn: 0 },
    ledger: [{ id: "h1", kind: "replace", state: "pending", edited: false, revision: 0 }],
    createdAt: "2026-01-01T00:00:00Z",
    ...over,
  };
}

function promptProposal(over: Partial<PromptChangeProposal> = {}): PromptChangeProposal {
  return promptFixture({ id: "pp1", ...over });
}

describe("a draft proposal event (DCR-FR-18, DCR-FR-QXFU)", () => {
  const draftEvent = (cause: ProposalChangeCause | undefined, state = "pending" as const) => ({
    draftId: "d1",
    proposal: draftProposal({ state }),
    cause,
  });

  it("DCR-FR-18, NTF-FR-24: a recording raises Info with the draft's key and address", () => {
    const raise = draftProposalRaise(draftEvent("recorded"), ROOT);
    expect(raise).toEqual({
      key: "draft-proposal:d1",
      level: "Info",
      title: "@arch proposed a change",
      body: "prompt.md",
      address: mintAddress(ROOT.projectKey, ROOT.worktree, { kind: "draft", draftId: "d1" }),
    });
  });

  it("DCR-FR-QXFU, NTF-FR-24: an update that leaves the proposal open raises as a recording does, with the same key", () => {
    expect(draftProposalRaise(draftEvent("updated"), ROOT)).toEqual(
      draftProposalRaise(draftEvent("recorded"), ROOT),
    );
  });

  it("DCR-FR-18: a decision or a hold that leaves the proposal open raises nothing", () => {
    expect(draftProposalRaise(draftEvent("decided"), ROOT)).toBeNull();
    expect(draftProposalRaise(draftEvent("held"), ROOT)).toBeNull();
  });

  it("DCR-FR-QXFU: an update that closes the proposal raises nothing", () => {
    expect(draftProposalRaise({ ...draftEvent("updated"), proposal: draftProposal({ state: "accepted" }) }, ROOT)).toBeNull();
    expect(draftProposalRaise({ ...draftEvent("updated"), proposal: draftProposal({ state: "rejected" }) }, ROOT)).toBeNull();
  });

  it("DCR-FR-18: a decision that closes the proposal raises nothing", () => {
    expect(draftProposalRaise({ ...draftEvent("decided"), proposal: draftProposal({ state: "accepted" }) }, ROOT)).toBeNull();
  });

  it("DCR-FR-18: an event with no cause, from an older backend, raises while the proposal is pending", () => {
    expect(draftProposalRaise(draftEvent(undefined), ROOT)?.key).toBe("draft-proposal:d1");
  });

  it("DCR-FR-18: nothing is raised with no project open", () => {
    expect(draftProposalRaise(draftEvent("recorded"), { projectKey: null, worktree: null })).toBeNull();
  });
});

describe("a prompt proposal event (PCR-FR-17, PCR-FR-26)", () => {
  const promptEvent = (
    cause: PromptProposalChangeCause | undefined,
    state: "pending" | "accepted" | "rejected" = "pending",
  ) => ({
    artifactId: "prompts/review.md",
    proposal: promptProposal({ state }),
    cause,
  });

  it("PCR-FR-17, NTF-FR-24: a recording raises Info with the proposal's own key and the file's address", () => {
    const { retract, raise } = promptProposalReaction(promptEvent("recorded"), ROOT);
    expect(retract).toBeNull();
    expect(raise).toEqual({
      key: "prompt-proposal:pp1",
      level: "Info",
      title: "@arch proposed a change to review.md",
      body: "prompts/review.md",
      address: mintAddress(ROOT.projectKey, ROOT.worktree, {
        kind: "file",
        path: "prompts/review.md",
      }),
    });
  });

  it("PCR-FR-17: an update that leaves the proposal pending raises as a recording does", () => {
    expect(promptProposalReaction(promptEvent("updated"), ROOT)).toEqual(
      promptProposalReaction(promptEvent("recorded"), ROOT),
    );
  });

  it("PCR-FR-17: a decision, a hold, or a reconciliation that leaves it pending does nothing", () => {
    for (const cause of ["decided", "held", "reconciled"] as const) {
      expect(promptProposalReaction(promptEvent(cause), ROOT)).toEqual({ retract: null, raise: null });
    }
  });

  it("PCR-FR-26: a closed proposal retracts its raise whatever the cause, and raises nothing", () => {
    for (const cause of ["decided", "reconciled", "updated", undefined] as const) {
      for (const state of ["accepted", "rejected"] as const) {
        expect(promptProposalReaction(promptEvent(cause, state), ROOT)).toEqual({
          retract: "prompt-proposal:pp1",
          raise: null,
        });
      }
    }
  });

  it("PCR-FR-26: a closed proposal retracts with no project open as well", () => {
    expect(
      promptProposalReaction(promptEvent("updated", "accepted"), { projectKey: null, worktree: null }),
    ).toEqual({ retract: "prompt-proposal:pp1", raise: null });
  });

  it("PCR-FR-17: an event with no cause, from an older backend, raises while the proposal is pending", () => {
    expect(promptProposalReaction(promptEvent(undefined), ROOT).raise?.key).toBe("prompt-proposal:pp1");
  });
});
