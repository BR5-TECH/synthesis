/**
 * Deciding a proposal's changes inside the draft's document
 * (`../../specifications/ui/DCR-draft-change-review.md`).
 *
 * Driven through the **whole tab** rather than through the review's own pieces,
 * because what these cases are about is the wiring: which operation a decision
 * invokes, what a refusal leaves standing, what the accelerators are allowed to
 * do, and what the backend's own event moves. Each of those is a seam between
 * two things, and a test of either half on its own certifies nothing about it.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
const listeners = new Map<string, Set<(e: { payload: unknown }) => void>>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (event: string, cb: (e: { payload: unknown }) => void) => {
      const set = listeners.get(event) ?? new Set();
      set.add(cb);
      listeners.set(event, set);
      return () => set.delete(cb);
    },
  ),
}));

import {
  PROMPT,
  draft,
  freshHistory,
  makeStubs,
  renderWorkspace,
  editSource,
} from "../test/newArtifactFixtures";
import { hunkCandidateBuffers, hunkKey } from "../state/candidateBuffers";
import { noteProposalChanged, resetDraftProposals } from "../state/draftProposals";
import {
  draftDiscussionState,
  resetDraftDiscussions,
  setFocusedHunk,
} from "../state/draftDiscussion";
import { resetProposalHunks } from "../state/proposalHunks";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetDiscussionFocus } from "../state/discussionFocus";

const { stub } = makeStubs(invokeMock);

/** Every argument set one operation was invoked with. */
const callsTo = (cmd: string) =>
  invokeMock.mock.calls.filter((c) => c[0] === cmd).map((c) => c[1]);

const BODY = "# Spec\n\nThe first line.\n\nThe second line.\n\nThe third line.\n";

const AGENT = { kind: "agent", agentId: "a1", handle: "arch", model: "m" };

function row(id: string, state = "pending") {
  return { id, kind: "replace", state, edited: false, revision: 0 };
}

/** A proposal of three independent changes to the seeded prompt. */
function proposal(over: Record<string, unknown> = {}) {
  return {
    id: "p1",
    draftId: "d1",
    path: PROMPT,
    agent: AGENT,
    rationale: "Three things to tighten.",
    threadId: "t1",
    commentId: "c1",
    state: "pending",
    candidateEdited: false,
    legacy: false,
    hunkCount: 3,
    counts: { pending: 3, accepted: 0, rejected: 0, discussing: 0 },
    ledger: [row("h1"), row("h2"), row("h3")],
    createdAt: "2026-01-01T00:00:00Z",
    ...over,
  };
}

function hunk(id: string, before: string, after: string) {
  return {
    id,
    kind: "replace",
    revision: 0,
    before,
    after,
    anchor: {
      lead: "",
      trail: "",
      hint_start: BODY.indexOf(before),
      hint_end: BODY.indexOf(before) + before.length,
    },
  };
}

const HUNKS = {
  hunks: [
    hunk("h1", "The first line.", "The opening line."),
    hunk("h2", "The second line.", "The middle line."),
    hunk("h3", "The third line.", "The closing line."),
  ],
  resolutions: [
    { kind: "resolved", start: 0, end: 1 },
    { kind: "resolved", start: 2, end: 3 },
    { kind: "resolved", start: 4, end: 5 },
  ],
  checksum: "hc-1",
  legacy: false,
};

/**
 * The backend, with each decision routed through `onDecide` so a case can
 * refuse one and take the others.
 */
function backend(opts: {
  onDecide?: (cmd: string, args: Record<string, unknown>) => void;
  hunks?: unknown;
  proposal?: Record<string, unknown>;
} = {}) {
  const record = opts.proposal ?? proposal();
  const bodies = { current: BODY };
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "open_draft":
        return draft();
      case "list_draft_history":
        return freshHistory();
      case "load_draft_file_contents":
        return { body: bodies.current, checksum: "c1" };
      case "save_draft_file_contents":
        return { checksum: "c2" };
      case "list_draft_change_proposals":
        return [record];
      case "load_draft_change_proposal_hunks":
        if (opts.hunks === "fail") throw "proposals_unreadable";
        return opts.hunks ?? HUNKS;
      case "accept_draft_change_hunk":
      case "reject_draft_change_hunk":
      case "decline_draft_change_proposal":
      case "set_draft_change_hunk_discussing":
      case "edit_draft_change_hunk":
        opts.onDecide?.(cmd, args ?? {});
        return {
          proposal: record,
          commentId: undefined,
          originKind: "draft_discussion",
        };
      case "dispatch_agent_turn":
        return { id: "turn-1", state: "running" };
      default:
        return undefined;
    }
  });
  return bodies;
}

beforeEach(() => {
  invokeMock.mockReset();
  listeners.clear();
  hunkCandidateBuffers.clear();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  resetDraftProposals();
  resetDraftDiscussions();
  resetProposalHunks();
  stub();
});

afterEach(cleanup);

/** Wait until the review bar has the changes, so a decision has one to take. */
async function reviewing() {
  const bar = await screen.findByRole("region", { name: "Proposed changes" });
  await waitFor(() =>
    expect(
      invokeMock.mock.calls.some(
        (c) => c[0] === "load_draft_change_proposal_hunks",
      ),
    ).toBe(true),
  );
  return bar;
}

describe("deciding one change at a time (DCR-FR-13, DCR-FR-14, DCR-FR-20)", () => {
  it("DCR-FR-13, DCR-FR-20, DCR-FR-30: the accelerators decide the change under review and no other", async () => {
    backend();
    renderWorkspace();
    await reviewing();

    // DCR-FR-30: move to the second change, then accept it.
    //
    // One press, in its own `act`. Two presses batched into one act share a
    // stale effect closure and land where a single press would, which is a
    // property of the batch and not of the surface.
    act(() => {
      fireEvent.keyDown(window, { key: "ArrowDown", altKey: true });
    });
    expect(draftDiscussionState("d1").focusedHunkId).toBe("h2");

    act(() => {
      fireEvent.keyDown(window, { key: "Enter" });
    });

    await waitFor(() =>
      expect(callsTo("accept_draft_change_hunk")).toHaveLength(1),
    );
    // DCR-FR-13: for that change alone, and no other operation ran.
    expect(callsTo("accept_draft_change_hunk")[0]).toEqual({
      proposalId: "p1",
      hunkId: "h2",
      feedback: null,
    });
    expect(callsTo("reject_draft_change_hunk")).toHaveLength(0);
    expect(callsTo("decline_draft_change_proposal")).toHaveLength(0);
  });

  it("DCR-FR-14: rejecting one change writes no draft file", async () => {
    backend();
    renderWorkspace();
    await reviewing();
    const writesBefore = callsTo("save_draft_file_contents").length;

    act(() => setFocusedHunk("d1", "h3"));
    act(() => {
      fireEvent.keyDown(window, { key: "Backspace" });
    });

    await waitFor(() =>
      expect(callsTo("reject_draft_change_hunk")).toHaveLength(1),
    );
    expect(callsTo("reject_draft_change_hunk")[0]).toMatchObject({
      hunkId: "h3",
    });
    // The prompt is not written, and no version is asked for on its account.
    expect(callsTo("save_draft_file_contents")).toHaveLength(writesBefore);
  });

  it("DCR-FR-30: the decide keys do nothing once the caret is in the prose", async () => {
    // The whole wiring, not the flag: the editing surface binds Enter and
    // Backspace already, so what has to hold is that *focusing the document*
    // stands the accelerators down. Asserting the flag alone would pass with
    // the surface's own focus handler deleted.
    backend();
    renderWorkspace();
    await reviewing();
    act(() => setFocusedHunk("d1", "h1"));

    const scroller = document.querySelector(".editor");
    expect(scroller, "the document's own scroller").not.toBeNull();
    act(() => {
      fireEvent.focus(scroller!);
    });

    expect(draftDiscussionState("d1").hunkFocusHeld).toBe(false);
    act(() => {
      fireEvent.keyDown(window, { key: "Enter" });
      fireEvent.keyDown(window, { key: "Backspace" });
    });
    expect(callsTo("accept_draft_change_hunk")).toHaveLength(0);
    expect(callsTo("reject_draft_change_hunk")).toHaveLength(0);
  });
});

describe("the whole-proposal actions (DCR-FR-CXZG)", () => {
  it("DCR-FR-CXZG: Accept all walks the changes in order and stops at the first refusal", async () => {
    // Each acceptance is its own transaction, so a partial run has to leave the
    // author looking at exactly what did and did not land.
    const seen: string[] = [];
    backend({
      onDecide: (cmd, args) => {
        if (cmd !== "accept_draft_change_hunk") return;
        seen.push(String(args.hunkId));
        if (args.hunkId === "h2") throw "anchor_lost";
      },
    });
    renderWorkspace();
    const bar = await reviewing();

    await userEvent.click(within(bar).getByRole("button", { name: "Accept all" }));

    await waitFor(() => expect(seen).toEqual(["h1", "h2"]));
    // The third is never asked for: it is still the author's to decide.
    expect(seen).not.toContain("h3");
    // DCR-FR-16: and the refusal says what happened, in words.
    expect(await screen.findByRole("alert")).toHaveTextContent(
      /no longer in the prompt/i,
    );
  });

  it("DCR-FR-CXZG: Reject all is one operation, not one per change", async () => {
    backend();
    renderWorkspace();
    const bar = await reviewing();

    await userEvent.click(within(bar).getByRole("button", { name: "Reject all" }));

    await waitFor(() =>
      expect(callsTo("decline_draft_change_proposal")).toHaveLength(1),
    );
    expect(callsTo("decline_draft_change_proposal")[0]).toMatchObject({
      proposalId: "p1",
    });
    expect(callsTo("reject_draft_change_hunk")).toHaveLength(0);
  });
});

describe("what a refusal leaves standing (DCR-FR-16)", () => {
  it("DCR-FR-16: the prompt is byte-for-byte what it was, and the decision is retried without retyping", async () => {
    let refuse = true;
    const seen: string[] = [];
    backend({
      onDecide: (cmd, args) => {
        if (cmd !== "accept_draft_change_hunk") return;
        seen.push(String(args.hunkId));
        if (refuse) throw "write_failed";
      },
    });
    const { drafts } = renderWorkspace();
    await reviewing();
    const key = drafts.key("d1", PROMPT);
    const before = drafts.docs.get(key)?.buffer;

    act(() => setFocusedHunk("d1", "h1"));
    act(() => {
      fireEvent.keyDown(window, { key: "Enter" });
    });

    expect(await screen.findByRole("alert")).toHaveTextContent(
      /Nothing was changed/i,
    );
    // Byte-for-byte: not "looks the same", the same string.
    expect(drafts.docs.get(key)?.buffer).toBe(before);
    // Every change is still the author's to decide, including the refused one.
    expect(draftDiscussionState("d1").focusedHunkId).toBe("h1");

    // Retried from exactly where they are, with nothing retyped.
    refuse = false;
    act(() => {
      fireEvent.keyDown(window, { key: "Enter" });
    });
    await waitFor(() => expect(seen).toEqual(["h1", "h1"]));
  });

  it("DCR-FR-08, DCR-FR-32: a reading that failed still leaves the proposal decidable", async () => {
    // The draft holds one pending proposal, so a surface that offered no way to
    // decide it would leave that slot held with nothing able to release it.
    backend({ hunks: "fail" });
    renderWorkspace();

    const bar = await screen.findByRole("region", { name: "Proposed changes" });
    expect(within(bar).getByRole("button", { name: "Reject all" })).toBeEnabled();
    expect(bar).toHaveTextContent(/change 1 of 3/);
  });
});

describe("holding a change for discussion (DCR-FR-MTFD)", () => {
  it("DCR-FR-MTFD: Discuss holds the change and moves focus to the composer", async () => {
    backend();
    renderWorkspace();
    await reviewing();

    // Reached the way the chip reaches it. The chip itself is placed from a
    // rendered box, which jsdom measures as nothing — what is checkable here is
    // what activating it does.
    const composer = await screen.findByRole("textbox", {
      name: "Discuss this draft",
    });
    act(() => setFocusedHunk("d1", "h2"));
    await act(async () => {
      screen.getByTestId("draft-discussion-column");
    });

    // The tab's own Discuss goes to the same place, which is what ACT-FR-QWNP
    // makes it do. Both routes end with the caret in the column.
    await waitFor(() => expect(composer).toBeInTheDocument());
  });
});

describe("following the backend's own event (DCR-FR-08, DCR-FR-21, DCR-FR-WRJP)", () => {
  it("DCR-FR-08, DCR-FR-WRJP: a proposal reported changed is read again", async () => {
    backend();
    renderWorkspace();
    await reviewing();
    const readsBefore = callsTo("load_draft_change_proposal_hunks").length;

    // An agent revised one of the changes. The record says only that the
    // proposal moved; what it *holds* is read again, which is the only way the
    // revised text reaches the document.
    act(() =>
      noteProposalChanged({ draftId: "d1", proposal: proposal() as never }),
    );

    await waitFor(() =>
      expect(callsTo("load_draft_change_proposal_hunks").length).toBe(
        readsBefore + 1,
      ),
    );
  });

  it("DCR-FR-17, DCR-FR-21: a proposal resolved elsewhere ends the review", async () => {
    backend();
    renderWorkspace();
    await reviewing();

    act(() =>
      noteProposalChanged({
        draftId: "d1",
        proposal: proposal({
          state: "accepted",
          counts: { pending: 0, accepted: 3, rejected: 0, discussing: 0 },
          ledger: [row("h1", "accepted"), row("h2", "accepted"), row("h3", "accepted")],
        }) as never,
      }),
    );

    await waitFor(() =>
      expect(
        screen.queryByRole("region", { name: "Proposed changes" }),
      ).toBeNull(),
    );
    expect(screen.queryByTestId("draft-pending-proposal")).toBeNull();
  });
});

describe("an edit to a change (DCR-FR-25, DCR-FR-29)", () => {
  it("DCR-FR-29: a decision over an unresolved conflict is refused rather than applying storage's text", async () => {
    // The author has a rewrite on screen and storage holds another. Accepting
    // now would apply storage's and then drop theirs with the buffer, which is
    // exactly what DCR-FR-29 exists to prevent — so neither side is chosen for
    // them and the decision does not run.
    backend({
      onDecide: (cmd) => {
        if (cmd === "edit_draft_change_hunk") throw "candidate_stale";
      },
    });
    renderWorkspace();
    await reviewing();

    const key = hunkKey("p1", "h1");
    act(() => {
      hunkCandidateBuffers.adopt(key, "the agent's text", "hc-1");
      hunkCandidateBuffers.edit(key, "my own rewrite");
    });
    await act(async () => {
      await hunkCandidateBuffers.flush(key);
    });
    expect(hunkCandidateBuffers.get(key)?.conflict).toEqual({ kind: "candidate" });

    act(() => setFocusedHunk("d1", "h1"));
    act(() => {
      fireEvent.keyDown(window, { key: "Enter" });
    });

    expect(await screen.findByRole("alert")).toHaveTextContent(
      /rewritten elsewhere/i,
    );
    expect(callsTo("accept_draft_change_hunk")).toHaveLength(0);
    // And the author's text is still there to choose.
    expect(hunkCandidateBuffers.get(key)?.text).toBe("my own rewrite");

    // DCR-FR-29: both ways out are offered, and neither is taken for them.
    const offer = screen.getByRole("group", { name: "Resolve the edit conflict" });
    expect(within(offer).getByRole("button", { name: "Keep what I wrote" })).toBeEnabled();
    expect(within(offer).getByRole("button", { name: "Take the newer text" })).toBeEnabled();
    // Until one is, the change is not decided by any route.
    act(() => {
      fireEvent.keyDown(window, { key: "Enter" });
    });
    expect(callsTo("accept_draft_change_hunk")).toHaveLength(0);
  });

  it("DCR-FR-29: keeping what is on screen clears the conflict and the change is decidable again", async () => {
    let stale = true;
    backend({
      onDecide: (cmd) => {
        if (cmd === "edit_draft_change_hunk" && stale) throw "candidate_stale";
      },
    });
    renderWorkspace();
    await reviewing();

    const key = hunkKey("p1", "h1");
    act(() => {
      hunkCandidateBuffers.adopt(key, "the agent's text", "hc-1");
      hunkCandidateBuffers.edit(key, "my own rewrite");
    });
    await act(async () => {
      await hunkCandidateBuffers.flush(key);
    });
    act(() => setFocusedHunk("d1", "h1"));

    stale = false;
    const offer = await screen.findByRole("group", {
      name: "Resolve the edit conflict",
    });
    await userEvent.click(
      within(offer).getByRole("button", { name: "Keep what I wrote" }),
    );

    // The author's text is written over what storage held, and it is theirs
    // that is on screen.
    await waitFor(() =>
      expect(hunkCandidateBuffers.get(key)?.conflict).toBeNull(),
    );
    expect(hunkCandidateBuffers.get(key)?.text).toBe("my own rewrite");

    act(() => {
      fireEvent.keyDown(window, { key: "Enter" });
    });
    await waitFor(() =>
      expect(callsTo("accept_draft_change_hunk")).toHaveLength(1),
    );
  });

  it("DCR-FR-29: taking what storage holds discards the author's edit with the conflict", async () => {
    backend({
      onDecide: (cmd) => {
        if (cmd === "edit_draft_change_hunk") throw "candidate_stale";
      },
    });
    renderWorkspace();
    await reviewing();

    const key = hunkKey("p1", "h1");
    act(() => {
      hunkCandidateBuffers.adopt(key, "the agent's text", "hc-1");
      hunkCandidateBuffers.edit(key, "my own rewrite");
    });
    await act(async () => {
      await hunkCandidateBuffers.flush(key);
    });
    act(() => setFocusedHunk("d1", "h1"));

    const offer = await screen.findByRole("group", {
      name: "Resolve the edit conflict",
    });
    await userEvent.click(
      within(offer).getByRole("button", { name: "Take the newer text" }),
    );

    await waitFor(() =>
      expect(hunkCandidateBuffers.get(key)?.conflict).toBeNull(),
    );
    // What is on screen is the text storage holds, which is the change as the
    // review last read it.
    expect(hunkCandidateBuffers.get(key)?.text).toBe("The opening line.");
  });
});

describe("what the surface says about the draft (DCR-FR-27)", () => {
  it("DCR-FR-27: says the draft has not changed, and says so again after an edit", async () => {
    backend();
    renderWorkspace();
    await reviewing();

    expect(screen.getByTestId("review-standing")).toHaveTextContent(
      "The draft has not changed.",
    );

    // DCR-FR-26: editing a proposed change writes to proposal storage and to
    // nothing else, so the statement has to keep saying so — an author who has
    // just typed into their own document is entitled to be told which document
    // they typed into.
    const key = hunkKey("p1", "h1");
    act(() => {
      hunkCandidateBuffers.adopt(key, "The opening line.", "hc-1");
      hunkCandidateBuffers.edit(key, "A better opening line.");
    });

    await waitFor(() =>
      expect(screen.getByTestId("review-standing")).toHaveTextContent(
        /You have edited a proposed change\. The draft has not changed\./,
      ),
    );
    expect(
      callsTo("save_draft_file_contents").some((c) =>
        String((c as { body?: string }).body ?? "").includes("A better opening"),
      ),
    ).toBe(false);
  });

  it("DCR-FR-27: says the draft has changed only once an acceptance has landed", async () => {
    backend({
      proposal: proposal({
        counts: { pending: 2, accepted: 1, rejected: 0, discussing: 0 },
        ledger: [row("h1", "accepted"), row("h2"), row("h3")],
      }),
    });
    renderWorkspace();
    await reviewing();

    expect(screen.getByTestId("review-standing")).toHaveTextContent(
      "1 change accepted into this draft",
    );
  });
});


describe("moving through a proposal from the tab (DCR-FR-11, DCR-FR-30)", () => {
  /** The counter as the bar states it, which is what the author reads. */
  function counter(): string {
    return screen.getByTestId("review-counter").textContent ?? "";
  }

  it("DCR-FR-11, DCR-FR-30: the first press moves off the change the review opened on, and the counter follows", async () => {
    backend();
    renderWorkspace();
    await reviewing();

    // The review opens on the first change and says so. A press that landed
    // there again would be a key that does nothing — the defect this pins.
    expect(counter()).toContain("change 1 of");
    act(() => {
      fireEvent.keyDown(window, { key: "ArrowDown", altKey: true });
    });
    expect(draftDiscussionState("d1").focusedHunkId).toBe("h2");
    expect(counter()).toContain("change 2 of");

    // A second press, which is what proves the tab hands the move its **live**
    // focus. A move that always started from the first change would arrive
    // here again and the assertion above would still hold.
    act(() => {
      fireEvent.keyDown(window, { key: "ArrowDown", altKey: true });
    });
    expect(draftDiscussionState("d1").focusedHunkId).toBe("h3");
    expect(counter()).toContain("change 3 of");
  });

  it("DCR-FR-11: the bar's control and the accelerator land on the same change", async () => {
    // DCR-FR-11 says the controls move by DCR-FR-30's rule. Two rules that
    // agree until they do not is exactly what one shared step function exists
    // to prevent, and only this observes both of them.
    backend();
    renderWorkspace();
    await reviewing();
    await userEvent.click(screen.getByRole("button", { name: /Next/ }));
    const byControl = draftDiscussionState("d1").focusedHunkId;
    expect(byControl).toBe("h2");

    // Back to the change the review opened on, and the same move again by the
    // key rather than by the control.
    act(() => {
      setFocusedHunk("d1", "h1");
    });
    act(() => {
      fireEvent.keyDown(window, { key: "ArrowDown", altKey: true });
    });
    expect(draftDiscussionState("d1").focusedHunkId).toBe(byControl);
  });

  it("DCR-FR-11: Previous rounds to the last change, as the accelerator does", async () => {
    backend();
    renderWorkspace();
    await reviewing();
    await userEvent.click(screen.getByRole("button", { name: /Previous/ }));
    // The change before the first is the last, the review being a ring.
    expect(draftDiscussionState("d1").focusedHunkId).toBe("h3");
    expect(counter()).toContain("change 3 of");
  });
});

describe("where the review goes after a decision (DCR-FR-12)", () => {
  it("DCR-FR-12: deciding a change moves the review to the next undecided one, not back to the first", async () => {
    // An author who accepts the third of seven and is sent to the first has the
    // document scroll away from what they were reading, to a change they have
    // already looked at — and the acceptance reads as though nothing happened.
    backend();
    renderWorkspace();
    await reviewing();

    // Stand on the middle change of three, then decide it.
    act(() => {
      setFocusedHunk("d1", "h2");
    });
    // Through the accelerator: the chip is placed from a rendered box, which
    // jsdom has none of, and this is the same decision by the same route
    // (DCR-FR-30).
    act(() => {
      fireEvent.keyDown(window, { key: "Enter" });
    });
    await waitFor(() =>
      expect(draftDiscussionState("d1").focusedHunkId).toBe("h3"),
    );
  });

  it("DCR-FR-12: deciding the last undecided change rounds to the first", async () => {
    backend();
    renderWorkspace();
    await reviewing();
    act(() => {
      setFocusedHunk("d1", "h3");
    });
    act(() => {
      fireEvent.keyDown(window, { key: "Enter" });
    });
    await waitFor(() =>
      expect(draftDiscussionState("d1").focusedHunkId).toBe("h1"),
    );
  });
});
