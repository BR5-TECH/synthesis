import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import {
  configurePromptReviewRoute,
  ensureLoaded,
  notePromptProposalChanged,
  pendingOf,
  proposalsOf,
  readingStatusOf,
  referenceTo,
  requestProposalReading,
  resetPromptProposals,
  showPromptHunk,
  showPromptProposal,
} from "./promptProposals";
import { hunkKey, promptHunkCandidateBuffers } from "./candidateBuffers";
import {
  artifactReviewKey,
  draftDiscussionState,
  resetDraftDiscussions,
} from "./draftDiscussion";
import { resetLogBufferForTest } from "../logging";
import { proposal as fixture } from "../test/promptProposalFixtures";
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
  return fixture(over);
}

const lists = () =>
  invokeMock.mock.calls.filter((c) => c[0] === "list_prompt_change_proposals");

/** Let the reading's promise settle. */
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async () => [proposal()]);
  resetLogBufferForTest();
  resetPromptProposals();
  resetDraftDiscussions();
  promptHunkCandidateBuffers.clear();
  configurePromptReviewRoute(null);
});

afterEach(() => {
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
  it("PCR-FR-16: holds an arriving proposal as the artifact's pending one, opening nothing", () => {
    notePromptProposalChanged({ artifactId: ARTIFACT, proposal: proposal() });
    expect(pendingOf(ARTIFACT)?.id).toBe("prop-1");
  });

  it("PCR-FR-QHZN: drops an unwritten hunk edit when the proposal is decided", () => {
    const key = hunkKey("prop-1", "h1");
    promptHunkCandidateBuffers.adopt(key, "the agent's text\n", "c1");
    promptHunkCandidateBuffers.edit(key, "mine\n");
    notePromptProposalChanged({
      artifactId: ARTIFACT,
      proposal: proposal({ state: "accepted" }),
    });
    expect(promptHunkCandidateBuffers.get(key)).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// PCR-FR-RFJC / PCR-FR-18 — the routes into the review
// ---------------------------------------------------------------------------

describe("the routes into the review (PCR-FR-RFJC, PCR-FR-18)", () => {
  it("PCR-FR-RFJC: opens or focuses the tab, then scrolls to the first undecided change", () => {
    notePromptProposalChanged({
      artifactId: ARTIFACT,
      proposal: proposal({
        hunkCount: 2,
        ledger: [
          { id: "h1", kind: "replace", state: "accepted", edited: false, revision: 0 },
          { id: "h2", kind: "add", state: "pending", edited: false, revision: 0 },
        ],
      }),
    });
    const opened: string[] = [];
    configurePromptReviewRoute((id) => opened.push(id));
    showPromptProposal(ARTIFACT, "prop-1");
    expect(opened).toEqual([ARTIFACT]);
    const state = draftDiscussionState(artifactReviewKey(ARTIFACT));
    expect(state.focusedHunkId).toBe("h2");
    expect(state.hunkReveal).toBe(1);
  });

  it("PCR-FR-RFJC: a hunk route scrolls to that change", () => {
    const opened: string[] = [];
    configurePromptReviewRoute((id) => opened.push(id));
    showPromptHunk(ARTIFACT, "prop-1", "h3");
    expect(opened).toEqual([ARTIFACT]);
    expect(draftDiscussionState(artifactReviewKey(ARTIFACT)).focusedHunkId).toBe("h3");
  });
});

// ---------------------------------------------------------------------------
// PCR-FR-28 — the content root
// ---------------------------------------------------------------------------

describe("the content root (PCR-FR-28)", () => {
  it("PCR-FR-28: discards every reading and drops the hunk edits", async () => {
    ensureLoaded(ARTIFACT);
    await settle();
    const key = hunkKey("prop-1", "h1");
    promptHunkCandidateBuffers.adopt(key, "the agent's text\n", "c1");
    promptHunkCandidateBuffers.edit(key, "mine\n");

    resetPromptProposals();

    expect(proposalsOf(ARTIFACT)).toHaveLength(0);
    expect(readingStatusOf(ARTIFACT)).toBe("unread");
    expect(promptHunkCandidateBuffers.get(key)).toBeUndefined();
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
