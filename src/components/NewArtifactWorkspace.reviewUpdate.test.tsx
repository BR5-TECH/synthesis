/**
 * A withdrawn change and an update of the open proposal, in the draft's own
 * (ProseMirror) document (`../../specifications/ui/DCR-draft-change-review.md`
 * DCR-FR-07, DCR-FR-11, DCR-FR-BQNL, DCR-FR-OZVU, DCR-FR-WRJP).
 *
 * Driven through the whole tab with the real overlay, so the decorations, the
 * gutter map, and the chip are the ones the author sees. The event is folded in
 * through `noteProposalChanged`, which is what the shell's subscriber to
 * `draft-change-proposals-changed` calls with the payload (DCR-FR-18).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, screen, waitFor, within } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import {
  PROMPT,
  draft,
  freshHistory,
  makeStubs,
  renderWorkspace,
} from "../test/newArtifactFixtures";
import { otherCommand } from "../test/invokeMocks";
import { hunkCandidateBuffers, hunkKey } from "../state/candidateBuffers";
import { noteProposalChanged, resetDraftProposals } from "../state/draftProposals";
import { resetDraftDiscussions, setFocusedHunk } from "../state/draftDiscussion";
import { resetProposalHunks } from "../state/proposalHunks";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetDiscussionFocus } from "../state/discussionFocus";
import { resetLogBufferForTest } from "../logging";
import type { HunkState } from "../types";

const { stub } = makeStubs(invokeMock);

const BODY =
  "# Spec\n\nThe first line.\n\nThe second line.\n\nThe third line.\n\nThe fourth line.\n";

const AGENT = { kind: "agent" as const, agentId: "a1", handle: "arch", model: "m" };

interface Hunk {
  id: string;
  kind: "replace" | "add";
  revision: number;
  before: string;
  after: string;
  anchor: { lead: string; trail: string; hint_start: number; hint_end: number };
}

function replacement(id: string, before: string, after: string, revision = 0): Hunk {
  const at = BODY.indexOf(before);
  return {
    id,
    kind: "replace",
    revision,
    before,
    after,
    anchor: { lead: "", trail: "", hint_start: at, hint_end: at + before.length },
  };
}

const H1 = replacement("h1", "The first line.", "The opening line.");
const H2 = replacement("h2", "The second line.", "The middle line.");
const H3 = replacement("h3", "The third line.", "The closing line.");
const H4 = replacement("h4", "The fourth line.", "The final line.");

function proposal(rows: { id: string; state: HunkState; revision?: number }[]) {
  const counts = { pending: 0, accepted: 0, rejected: 0, discussing: 0, withdrawn: 0 };
  for (const r of rows) counts[r.state] += 1;
  return {
    id: "p1",
    draftId: "d1",
    path: PROMPT,
    agent: AGENT,
    rationale: "Three things to tighten.",
    threadId: "t1",
    commentId: "c1",
    state: "pending" as const,
    candidateEdited: false,
    legacy: false,
    hunkCount: rows.length,
    counts,
    ledger: rows.map((r) => ({
      id: r.id,
      kind: "replace" as const,
      state: r.state,
      edited: false,
      revision: r.revision ?? 0,
    })),
    createdAt: "2026-01-01T00:00:00Z",
  };
}

/** What the backend holds now; a case moves it on for an update. */
let stored: { proposal: ReturnType<typeof proposal>; hunks: Hunk[] };

function backend() {
  invokeMock.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case "open_draft":
        return draft();
      case "list_draft_history":
        return freshHistory();
      case "load_draft_file_contents":
        return { body: BODY, checksum: "c1" };
      case "save_draft_file_contents":
        return { checksum: "c2" };
      case "list_draft_change_proposals":
        return [stored.proposal];
      case "load_draft_change_proposal_hunks":
        return {
          hunks: stored.hunks,
          resolutions: stored.hunks.map((_, i) => ({ kind: "resolved", start: i, end: i })),
          checksum: "hc-1",
          legacy: false,
        };
      default:
        return otherCommand(cmd);
    }
  });
}

beforeEach(() => {
  invokeMock.mockReset();
  hunkCandidateBuffers.clear();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  resetDraftProposals();
  resetDraftDiscussions();
  resetProposalHunks();
  resetLogBufferForTest();
  stub();
});

afterEach(() => {
  cleanup();
  hunkCandidateBuffers.clear();
});

const decorated = (id: string) => document.querySelector(`[data-hunk="${id}"]`);
const gutterMarks = () =>
  within(screen.getByTestId("hunk-gutter-map")).getAllByRole("button");

describe("a withdrawn change in the draft's document (DCR-FR-OZVU)", () => {
  it("DCR-FR-07, DCR-FR-BQNL, DCR-FR-OZVU: it has no decoration, no gutter mark, and no chip", async () => {
    stored = {
      proposal: proposal([
        { id: "h1", state: "pending" },
        { id: "h2", state: "withdrawn" },
        { id: "h3", state: "pending" },
      ]),
      hunks: [H1, H2, H3],
    };
    backend();
    renderWorkspace();
    await screen.findByRole("region", { name: "Proposed changes" });
    await waitFor(() => {
      expect(decorated("h1")).not.toBeNull();
      expect(decorated("h3")).not.toBeNull();
    });
    expect(decorated("h2")).toBeNull();
    expect(document.body).not.toHaveTextContent("The middle line.");
    // DCR-FR-BQNL: one mark per change that is drawn, and none for it.
    expect(gutterMarks()).toHaveLength(2);
    expect(screen.getByTestId("review-counter")).toHaveTextContent("of 2");

    // DCR-FR-12: a focus set on it from elsewhere puts the review, and so the
    // chip, on a change the review shows, never on it. (jsdom renders no box,
    // so the chip itself is not placed here; what it would follow is.)
    act(() => setFocusedHunk("d1", "h2"));
    await waitFor(() =>
      expect(document.querySelector('[data-focused="true"]')?.getAttribute("data-hunk")).toBe("h1"),
    );
    expect(screen.getByTestId("review-counter")).toHaveTextContent("reviewing change 1 of 2");
    for (const chip of screen.queryAllByTestId("hunk-chip")) {
      expect(chip).not.toHaveAccessibleName(/second|middle/i);
    }
  });
});

describe("an update of the open proposal (DCR-FR-WRJP)", () => {
  it("DCR-FR-WRJP, DCR-FR-11: the withdrawn change leaves, the added one is drawn and counted, and an unwritten edit of the changed one is dropped", async () => {
    stored = {
      proposal: proposal([
        { id: "h1", state: "pending" },
        { id: "h2", state: "pending" },
      ]),
      hunks: [H1, H2],
    };
    backend();
    renderWorkspace();
    await screen.findByRole("region", { name: "Proposed changes" });
    await waitFor(() => {
      expect(decorated("h1")).not.toBeNull();
      expect(decorated("h2")).not.toBeNull();
    });
    expect(screen.getByTestId("review-counter")).toHaveTextContent("of 2");

    const key = hunkKey("p1", "h1");
    // The agent changes h1, withdraws h2, and adds h3 and h4.
    stored = {
      proposal: proposal([
        { id: "h1", state: "pending", revision: 1 },
        { id: "h2", state: "withdrawn" },
        { id: "h3", state: "pending" },
        { id: "h4", state: "pending" },
      ]),
      hunks: [replacement("h1", "The first line.", "The revised line.", 1), H2, H3, H4],
    };
    act(() => {
      // An edit of h1 that is still resting: nothing has written it, and the
      // update arrives before its rest ends, with no await between the two.
      hunkCandidateBuffers.adopt(key, H1.after, "hc-1");
      hunkCandidateBuffers.edit(key, "The author's own line.");
      expect(hunkCandidateBuffers.get(key)?.dirty).toBe(true);
      noteProposalChanged({ draftId: "d1", proposal: stored.proposal, cause: "updated" });
    });

    // The unwritten edit is gone, and no write of it is still scheduled.
    expect(hunkCandidateBuffers.get(key)).toBeUndefined();
    expect(hunkCandidateBuffers.hasPendingWrite(key)).toBe(false);

    await waitFor(() => {
      expect(decorated("h3")).not.toBeNull();
      expect(decorated("h4")).not.toBeNull();
    });
    await waitFor(() => expect(decorated("h2")).toBeNull());
    // DCR-FR-11: counted in proposal order, the withdrawn one not counted.
    expect(screen.getByTestId("review-counter")).toHaveTextContent("of 3");
    expect(gutterMarks()).toHaveLength(3);
    act(() => setFocusedHunk("d1", "h4"));
    expect(screen.getByTestId("review-counter")).toHaveTextContent("reviewing change 3 of 3");
    // The changed hunk shows the agent's new text, not the dropped edit.
    await waitFor(() => expect(document.body).toHaveTextContent("The revised line."));
    expect(document.body).not.toHaveTextContent("The author's own line.");
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "edit_draft_change_hunk"),
    ).toHaveLength(0);
  });
});
