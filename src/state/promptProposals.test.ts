import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import {
  closePromptReview,
  configurePromptReviewRoute,
  ensureLoaded,
  notePromptProposalChanged,
  openPromptReview,
  openPromptReviewFor,
  pendingOf,
  proposalsOf,
  readingStatusOf,
  referenceTo,
  requestProposalReading,
  resetPromptProposals,
  reviewingPromptProposal,
} from "./promptProposals";
import { promptCandidateBuffers } from "./candidateBuffers";
import type { PromptChangeProposal } from "../types";

/**
 * Tests for the reading every surface of `PCR-prompt-change-review.md` renders
 * from (PCR-FR-16, PCR-FR-27, PCR-FR-28) and for CTA-FR-SACG's three answers.
 *
 * Driven directly rather than through a component, because the cases that matter
 * most are the ones a component test cannot stage: a reading that failed, an
 * event landing while one is in flight, and a further ask that settles.
 */

const ARTIFACT = "prompts/review.md";

function proposal(over: Partial<PromptChangeProposal> = {}): PromptChangeProposal {
  return {
    id: "prop-1",
    artifactId: ARTIFACT,
    path: ARTIFACT,
    agent: { kind: "agent", agentId: "a1", handle: "arch", model: "m" },
    rationale: "The instructions bury the important step.",
    threadId: "t1",
    commentId: "c1",
    state: "pending",
    candidateEdited: false,
    commentOwed: false,
    createdAt: "2026-01-01T00:00:00Z",
    ...over,
  };
}

const lists = () =>
  invokeMock.mock.calls.filter((c) => c[0] === "list_prompt_change_proposals");

/** Let the reading's promise settle. */
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async () => [proposal()]);
  resetPromptProposals();
  promptCandidateBuffers.clear();
  configurePromptReviewRoute(null);
});

afterEach(() => {
  closePromptReview();
  configurePromptReviewRoute(null);
});

// ---------------------------------------------------------------------------
// PCR-FR-27 — one reading per artifact (PCR-FR-27)
// ---------------------------------------------------------------------------

describe("the one reading per artifact (PCR-FR-27)", () => {
  it("takes one reading however many surfaces ask for it", async () => {
    ensureLoaded(ARTIFACT);
    ensureLoaded(ARTIFACT);
    ensureLoaded(ARTIFACT);
    expect(lists()).toHaveLength(1);
    await settle();
    expect(proposalsOf(ARTIFACT)).toHaveLength(1);
    expect(readingStatusOf(ARTIFACT)).toBe("read");
    // A reading that answered is not asked for again.
    ensureLoaded(ARTIFACT);
    expect(lists()).toHaveLength(1);
  });

  it("asks again after a reading that failed, and stops at the bound", async () => {
    invokeMock.mockImplementation(async () => {
      throw new Error("no_project_open");
    });
    for (let n = 0; n < 6; n += 1) {
      ensureLoaded(ARTIFACT);
      await settle();
    }
    // CTA-FR-LOOE: a backend that refuses every read costs the surface three calls
    // rather than one per render.
    expect(lists()).toHaveLength(3);
    // PCR-FR-27: a reading that failed is not the session's answer, so nothing
    // reports the absence of a proposal from it.
    expect(readingStatusOf(ARTIFACT)).toBe("failed");
    expect(referenceTo(ARTIFACT, "prop-1")).toEqual({ kind: "unresolved" });
  });

  it("says a proposal is gone only after a completed reading and one further ask", async () => {
    invokeMock.mockImplementation(async () => []);
    ensureLoaded(ARTIFACT);
    await settle();
    // A completed reading that did not return it is not yet an answer: the
    // further ask of CTA-FR-XVNC is what settles it.
    expect(referenceTo(ARTIFACT, "prop-9")).toEqual({ kind: "unresolved" });

    requestProposalReading(ARTIFACT, "prop-9");
    await settle();
    expect(lists()).toHaveLength(2);
    expect(referenceTo(ARTIFACT, "prop-9")).toEqual({ kind: "gone" });

    // …and it is made once per reference, so a proposal that is genuinely gone
    // settles rather than being asked after for the rest of the session.
    requestProposalReading(ARTIFACT, "prop-9");
    await settle();
    expect(lists()).toHaveLength(2);
  });

  it("keeps a proposal an event delivered while a reading was in flight", async () => {
    let release: (value: PromptChangeProposal[]) => void = () => {};
    invokeMock.mockImplementation(
      () => new Promise<PromptChangeProposal[]>((resolve) => (release = resolve)),
    );
    ensureLoaded(ARTIFACT);
    notePromptProposalChanged({
      artifactId: ARTIFACT,
      proposal: proposal({ id: "prop-late" }),
    });
    release([proposal({ id: "prop-1" })]);
    await settle();

    const ids = proposalsOf(ARTIFACT).map((p) => p.id).sort();
    expect(ids).toEqual(["prop-1", "prop-late"]);
  });
});

// ---------------------------------------------------------------------------
// PCR-FR-16 — arrival (PCR-FR-16)
// ---------------------------------------------------------------------------

describe("arrival (PCR-FR-16)", () => {
  it("opens no review for an artifact with no consumer mounted", () => {
    notePromptProposalChanged({ artifactId: ARTIFACT, proposal: proposal() });
    // PCR-FR-16: the subscriber is the shell's and hears about every artifact in
    // the worktree, so a proposal for a file with no tab open is left to the
    // tab's indication and to the notification.
    expect(reviewingPromptProposal()).toBeNull();
    expect(pendingOf(ARTIFACT)?.id).toBe("prop-1");
  });

  it("does not reopen on a second event about a proposal already seen", () => {
    notePromptProposalChanged({ artifactId: ARTIFACT, proposal: proposal() });
    openPromptReview("prop-1");
    closePromptReview();
    notePromptProposalChanged({ artifactId: ARTIFACT, proposal: proposal() });
    expect(reviewingPromptProposal()).toBeNull();
  });

  it("drops an unwritten candidate edit when the proposal is decided", () => {
    promptCandidateBuffers.adopt("prop-1", "the agent's text\n", "c1");
    promptCandidateBuffers.edit("prop-1", "mine\n");
    notePromptProposalChanged({
      artifactId: ARTIFACT,
      proposal: proposal({ state: "accepted" }),
    });
    // PCR-FR-24: there is nothing left to decide, so nothing left to edit.
    expect(promptCandidateBuffers.get("prop-1")).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// PCR-FR-03 / PCR-FR-17 — the routes into a review (PCR-FR-03, PCR-FR-18)
// ---------------------------------------------------------------------------

describe("the routes into a review (PCR-FR-03, PCR-FR-18)", () => {
  it("focuses the same instance rather than creating a second", () => {
    openPromptReview("prop-1");
    openPromptReview("prop-1");
    expect(reviewingPromptProposal()).toBe("prop-1");
  });

  it("leaves an open review standing when a route names a different proposal", () => {
    openPromptReview("prop-1");
    openPromptReview("prop-2");
    expect(reviewingPromptProposal()).toBe("prop-1");
  });

  it("opens or focuses the artifact's tab before showing the review over it", () => {
    const opened: string[] = [];
    configurePromptReviewRoute((id) => opened.push(id));
    openPromptReviewFor(ARTIFACT, "prop-1");
    // PCR-FR-18: the review is never rendered anywhere but over the tab for its
    // own file.
    expect(opened).toEqual([ARTIFACT]);
    expect(reviewingPromptProposal()).toBe("prop-1");
  });
});

// ---------------------------------------------------------------------------
// PCR-FR-25 — the content root (PCR-FR-28)
// ---------------------------------------------------------------------------

describe("the content root (PCR-FR-28)", () => {
  it("discards every reading, dismisses the review, and drops the candidate", async () => {
    ensureLoaded(ARTIFACT);
    await settle();
    openPromptReview("prop-1");
    promptCandidateBuffers.adopt("prop-1", "the agent's text\n", "c1");
    promptCandidateBuffers.edit("prop-1", "mine\n");

    resetPromptProposals();

    expect(proposalsOf(ARTIFACT)).toHaveLength(0);
    expect(readingStatusOf(ARTIFACT)).toBe("unread");
    expect(reviewingPromptProposal()).toBeNull();
    // PCR-FR-21: the candidate is dropped when the project closes and when the
    // active worktree changes. One carried across would hold a baseline
    // checksum naming a file the application has left.
    expect(promptCandidateBuffers.get("prop-1")).toBeUndefined();
  });

  it("does not fold a reading issued against the outgoing tree", async () => {
    let release: (value: PromptChangeProposal[]) => void = () => {};
    invokeMock.mockImplementation(
      () => new Promise<PromptChangeProposal[]>((resolve) => (release = resolve)),
    );
    ensureLoaded(ARTIFACT);
    resetPromptProposals();
    release([proposal()]);
    await settle();

    expect(proposalsOf(ARTIFACT)).toHaveLength(0);
    // …and the artifact is left needing a fresh reading rather than counting as
    // read from the outgoing one.
    expect(readingStatusOf(ARTIFACT)).toBe("unread");
  });
});
