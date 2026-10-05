import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

/**
 * The event bus, so a test can deliver `"artifact changed externally"` the way
 * the backend does (PCR-FR-24).
 */
const listeners = new Map<string, Set<(event: { payload: unknown }) => void>>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, handler: (e: { payload: unknown }) => void) => {
    const set = listeners.get(name) ?? new Set();
    set.add(handler);
    listeners.set(name, set);
    return () => set.delete(handler);
  }),
}));

function fireBusEvent(name: string, payload: unknown): void {
  for (const handler of listeners.get(name) ?? []) handler({ payload });
}

import { PromptChangeReview } from "./PromptChangeReview";
import {
  closePromptReview,
  openPromptReview,
  resetPromptProposals,
} from "../state/promptProposals";
import { resetDiffModes } from "../state/diffModes";
import { promptCandidateBuffers } from "../state/candidateBuffers";
import { EditSessionStore } from "../state/editSessions";
import {
  clearConversationThreads,
  publishThread,
} from "../state/conversationThreads";
import {
  publishProjectAgents,
  resetAgentRegistry,
} from "../state/agentRegistry";
import type { Discussion } from "../types";
import {
  ARCH,
  ARTIFACT,
  CURRENT,
  HUMAN,
  PROPOSED,
  ROSTER,
  addressedConversation,
  agent,
  conversation,
  message,
  proposal,
} from "../test/promptProposalFixtures";
import {
  artifactDiscussionOrigin,
} from "../test/origins";

/**
 * Tests for `PCR-prompt-change-review.md`.
 *
 * The comparison's *rendering* belongs to `DFV-diff-viewer.md` and is tested
 * with the Diff tab; the derivation is tested in `diff/localHunks.test.ts`. What
 * is tested here is the modal: what it reads, what it shows, what deciding
 * actually invokes, and what it does to the artifact's editing session.
 */


/**
 * The conversation `"read comment thread"` answers with — the decision path
 * reads it again rather than trusting what this surface was holding, so a test
 * that seeds one has to seed both.
 */
let seeded: Discussion = addressedConversation();

/** Seed the roster and the conversation the routing rule reads. */
function seedConversation(thread: Discussion = addressedConversation()) {
  seeded = thread;
  act(() => {
    publishProjectAgents(ROSTER);
    publishThread(thread);
  });
}

/** Which agents `"dispatch agent turn"` was invoked for, in order. */
function dispatched(): string[] {
  return invokeMock.mock.calls
    .filter((c) => c[0] === "dispatch_agent_turn")
    .map((c) => (c[1] as { nickname: string }).nickname);
}

/** The backend as the modal sees it, unless a test overrides a call. */
function stubBackend(over: Record<string, unknown> = {}) {
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd in over) {
      const value = over[cmd];
      if (value instanceof Error) throw value;
      return value;
    }
    switch (cmd) {
      case "list_prompt_change_proposals":
        return [proposal()];
      case "load_prompt_change_proposal_content":
        return { content: PROPOSED, checksum: "cand-1" };
      case "load_artifact_contents_by_id":
        return { body: CURRENT, checksum: "base-1" };
      case "save_prompt_change_proposal_candidate":
        return { checksum: "cand-2" };
      case "apply_prompt_change_proposal":
        return {
          proposal: proposal({ state: "accepted", decidedAt: "2026-01-02T00:00:00Z" }),
          commentId: "decision-1",
          originKind: "artifact_discussion",
        };
      case "decline_prompt_change_proposal":
        return {
          proposal: proposal({ state: "rejected", decidedAt: "2026-01-02T00:00:00Z" }),
          commentId: "decision-1",
          originKind: "artifact_discussion",
        };
      case "complete_prompt_change_decision":
        return {
          proposal: proposal({ state: "accepted", decidedAt: "2026-01-02T00:00:00Z" }),
          commentId: "decision-1",
          originKind: "artifact_discussion",
        };
      case "dispatch_agent_turn":
        return { id: "turn-1", state: "running" };
      case "read_discussion":
        return seeded;
      case "list_project_agents":
        return ROSTER;
      case "load_app_preferences":
        return {};
      default:
        return null;
    }
  });
}

let sessions: EditSessionStore;

/** Render the modal with `id` under review, waiting for both revisions. */
async function open(id = "prop-1") {
  render(<PromptChangeReview artifactId={ARTIFACT} sessions={sessions} />);
  await act(async () => {
    openPromptReview(id);
  });
  return screen.findByTestId("prompt-change-review");
}

/** Activate a mode toggle by its label, as the author would. */
async function activate(label: string): Promise<void> {
  await act(async () => {
    fireEvent.click(screen.getByRole("radio", { name: label }));
  });
}

function calls(cmd: string): unknown[][] {
  return invokeMock.mock.calls.filter((c) => c[0] === cmd);
}

beforeEach(() => {
  invokeMock.mockReset();
  listeners.clear();
  resetPromptProposals();
  resetDiffModes();
  // PCR-FR-21: a candidate buffer outlives every surface by design, which is
  // exactly what makes it carry from one test into the next.
  promptCandidateBuffers.clear();
  sessions = new EditSessionStore();
  resetAgentRegistry();
  clearConversationThreads();
  stubBackend();
  // PCR-FR-14: every decision routes by the conversation's active agents, so
  // every test needs one. The default is the ordinary case — the author
  // addressed `@arch` and `@arch` proposed — and a test wanting another seeds it.
  seedConversation();
});

afterEach(() => {
  cleanup();
  act(() => closePromptReview());
});

// ---------------------------------------------------------------------------
// PCR-FR-16, PCR-FR-26 / PCR-FR-03 / PCR-FR-09 — what opens it (PCR-FR-03, PCR-FR-09)
// ---------------------------------------------------------------------------

describe("opening (PCR-FR-03, PCR-FR-09)", () => {
  it("renders nothing until a proposal is under review, and reads no document", async () => {
    render(<PromptChangeReview artifactId={ARTIFACT} sessions={sessions} />);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "list_prompt_change_proposals",
        expect.anything(),
      ),
    );
    expect(screen.queryByTestId("prompt-change-review")).toBeNull();
    // PCR-FR-09: an artifact carrying a pending proposal costs its tab the
    // indication and no read of any document.
    expect(calls("load_prompt_change_proposal_content")).toHaveLength(0);
    expect(calls("load_artifact_contents_by_id")).toHaveLength(0);
  });

  it("renders nothing for a proposal belonging to another artifact", async () => {
    render(
      <PromptChangeReview artifactId="prompts/elsewhere.md" sessions={sessions} />,
    );
    await act(async () => {
      openPromptReview("prop-1");
    });
    expect(screen.queryByTestId("prompt-change-review")).toBeNull();
  });

  it("leaves an open review standing when a route names a different proposal", async () => {
    await open();
    await act(async () => {
      openPromptReview("prop-2");
    });
    // PCR-FR-03: what is showing is the proposal the author is deciding, and
    // swapping it out under their pointer would decide it by accident.
    expect(await screen.findByText(ARTIFACT)).toBeInTheDocument();
    expect(
      screen.getByText("The instructions bury the important step."),
    ).toBeInTheDocument();
  });

  it("reads each revision once however many modes are cycled through", async () => {
    // PCR-FR-09: switching among the six mode combinations
    // re-renders from held text without a further round-trip.
    await open();
    await screen.findByText(ARTIFACT);
    expect(calls("load_prompt_change_proposal_content")).toHaveLength(1);
    expect(calls("load_artifact_contents_by_id")).toHaveLength(1);

    for (const rendering of ["Source", "Rich"]) {
      for (const visualization of ["Unified", "Side-by-side", "Final"]) {
        await activate(visualization);
        await activate(rendering);
      }
    }
    expect(calls("load_prompt_change_proposal_content")).toHaveLength(1);
    expect(calls("load_artifact_contents_by_id")).toHaveLength(1);
  });

  it("reads the comparison in the Diff tab's own modes and writes the shared preference", async () => {
    // PCR-FR-06 / PCR-FR-08 / PCR-FR-06, PCR-FR-08: the two groups are the Diff
    // tab's, and the choice is the same user-global one, so activating a toggle
    // here writes it for every open Diff tab and for the draft review.
    await open();
    await screen.findByText(ARTIFACT);
    for (const label of ["Unified", "Side-by-side", "Final", "Source", "Rich"]) {
      expect(screen.getByRole("radio", { name: label })).toBeEnabled();
    }

    await activate("Final");
    await waitFor(() => expect(calls("save_app_preferences")).toHaveLength(1));
    expect(screen.getByRole("radio", { name: "Final" })).toBeChecked();
  });

  it("disables the rendering group for a prompt that is not Markdown", async () => {
    // PCR-FR-07: the disablement is display-only, so the stored
    // preference is untouched and the comparison is still readable.
    const txt = "prompts/review.txt";
    stubBackend({
      list_prompt_change_proposals: [proposal({ artifactId: ARTIFACT, path: txt })],
    });
    await open();
    await screen.findByText(txt);

    expect(screen.getByRole("radio", { name: "Source" })).toBeDisabled();
    expect(screen.getByRole("radio", { name: "Rich" })).toBeDisabled();
    expect(screen.getByRole("radio", { name: "Source" })).toBeChecked();
    // The visualizations are unaffected, and nothing was written.
    expect(screen.getByRole("radio", { name: "Side-by-side" })).toBeEnabled();
    expect(calls("save_app_preferences")).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// PCR-FR-02 / PCR-FR-10 — the head and the base (PCR-FR-04, PCR-FR-05)
// ---------------------------------------------------------------------------

describe("the comparison (PCR-FR-04, PCR-FR-05)", () => {
  it("names the file, the agent, its title and the rationale", async () => {
    await open();
    expect(await screen.findByText(ARTIFACT)).toBeInTheDocument();
    expect(screen.getByText(/@arch/)).toBeInTheDocument();
    expect(screen.getByTestId("prompt-review-agent-title")).toHaveTextContent(
      "Architect",
    );
    expect(
      screen.getByText("The instructions bury the important step."),
    ).toBeInTheDocument();
  });

  it("says the file has changed rather than moving the base, and keeps Accept enabled", async () => {
    await open();
    await screen.findByText(ARTIFACT);
    expect(screen.queryByTestId("prompt-review-file-changed")).toBeNull();

    await act(async () => {
      fireBusEvent("artifact-changed-externally", {
        artifactId: ARTIFACT,
        checksum: "somebody-elses-bytes",
      });
    });

    // PCR-FR-05 / PCR-FR-24: the event moves nothing — it puts up the standing
    // line and does no more.
    expect(
      await screen.findByTestId("prompt-review-file-changed"),
    ).toHaveTextContent(/Accept replaces it whole/);
    expect(screen.getByRole("button", { name: "Accept" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Reject" })).toBeEnabled();
    // …and the comparison was not re-derived from a fresh read.
    expect(calls("load_artifact_contents_by_id")).toHaveLength(1);
  });

  it("raises no external-change modal over the tab while it is showing", async () => {
    // The store's divergence watch is the Editor's, mounted beside this modal
    // in the real tab (EXC-FR-QCNO) — so the test mounts one too, and seeds the
    // session the event has to find.
    const release = sessions.watchExternalChanges();
    sessions.get(ARTIFACT);
    sessions.update(ARTIFACT, { baseline: "base-1", loaded: true });
    await open();
    await screen.findByText(ARTIFACT);

    await act(async () => {
      fireBusEvent("artifact-changed-externally", {
        artifactId: ARTIFACT,
        checksum: "somebody-elses-bytes",
      });
    });
    // EDT-FR-84 / PCR-FR-24: the divergence is recorded but no modal is raised,
    // so every tab bound to that artifact stays interactive.
    expect(sessions.get(ARTIFACT)?.conflict).toBe(false);
    expect(sessions.get(ARTIFACT)?.pending).toBe("somebody-elses-bytes");
    release();
  });
});

// ---------------------------------------------------------------------------
// PCR-FR-11 … PCR-FR-14 — deciding (PCR-FR-10 … PCR-FR-15)
// ---------------------------------------------------------------------------

describe("deciding (PCR-FR-10, PCR-FR-11, PCR-FR-12, PCR-FR-14)", () => {
  it("quiesces the session, applies, resets it, and dispatches one turn", async () => {
    const quiesce = vi.spyOn(sessions, "quiesce");
    const reset = vi.spyOn(sessions, "resetRestored").mockResolvedValue();
    await open();
    await screen.findByText(ARTIFACT);

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    });

    await waitFor(() => expect(calls("apply_prompt_change_proposal")).toHaveLength(1));
    expect(quiesce).toHaveBeenCalledWith([ARTIFACT]);
    // EXC-FR-UVJY: reset on success, so every tab bound to the artifact renders
    // the accepted text at once without a reload.
    expect(reset).toHaveBeenCalledWith(ARTIFACT);
    expect(calls("apply_prompt_change_proposal")[0][1]).toMatchObject({
      proposalId: "prop-1",
      feedback: null,
    });
    // PCR-FR-14: one fresh turn per active agent, naming the decision's comment.
    // Here the conversation is addressed to `@arch` alone.
    expect(calls("dispatch_agent_turn")).toHaveLength(1);
    expect(calls("dispatch_agent_turn")[0][1]).toMatchObject({
      nickname: "arch",
      origin: artifactDiscussionOrigin("t1", ARTIFACT),
      triggerCommentId: "decision-1",
    });
    await waitFor(() =>
      expect(screen.queryByTestId("prompt-change-review")).toBeNull(),
    );
  });

  it("PCR-FR-14, AGC-FR-04, AGC-FR-29, CMT-FR-79, CTA-FR-LCFU, CTA-FR-QUXJ: dispatches once per active agent, the decision being an untagged human comment", async () => {
    // The proposer is reached because it is still one of the agents the author
    // is talking to, not because it proposed — and `@sec` is reached on exactly
    // the same terms.
    vi.spyOn(sessions, "resetRestored").mockResolvedValue();
    seedConversation(
      conversation([
        message("c0", HUMAN, "@arch @sec please review"),
        message("c1", ARCH, "Here is a change I would make."),
      ]),
    );
    await open();
    await screen.findByText(ARTIFACT);

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    });

    await waitFor(() => expect(dispatched()).toEqual(["arch", "sec"]));
    for (const nickname of ["arch", "sec"]) {
      expect(invokeMock).toHaveBeenCalledWith("dispatch_agent_turn", {
        nickname,
        origin: artifactDiscussionOrigin("t1", ARTIFACT),
        triggerCommentId: "decision-1",
      });
    }
  });

  it("PCR-FR-14, AGC-FR-04, AGC-FR-29, CMT-FR-79, CTA-FR-LCFU, CTA-FR-QUXJ: a refused dispatch neither suppresses the other nor repeats itself", async () => {
    // AGC-FR-04 / CMT-FR-79: the outcomes are independent. The refusal is not
    // surfaced and does not undo the decision, so what a test can witness is
    // that both agents were still asked exactly once and the modal still closed.
    vi.spyOn(sessions, "resetRestored").mockResolvedValue();
    let seen = 0;
    stubBackend({
      get dispatch_agent_turn() {
        seen += 1;
        return seen === 1
          ? new Error("agent_unreachable")
          : { id: "turn-2", state: "running" };
      },
    });
    seedConversation(
      conversation([
        message("c0", HUMAN, "@arch @sec please review"),
        message("c1", ARCH, "Here is a change I would make."),
      ]),
    );
    await open();
    await screen.findByText(ARTIFACT);

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    });

    await waitFor(() => expect(dispatched()).toEqual(["arch", "sec"]));
    await waitFor(() =>
      expect(screen.queryByTestId("prompt-change-review")).toBeNull(),
    );
    expect(dispatched()).toHaveLength(2);
  });

  it("reads the conversation rather than deciding on what it happened to hold", async () => {
    // PCR-FR-14 / PCR-FR-18: a review opened from a notification can be decided
    // before anything has published the conversation. Routing off an empty cache
    // would silently send the decision to nobody.
    vi.spyOn(sessions, "resetRestored").mockResolvedValue();
    clearConversationThreads();
    act(() => publishProjectAgents(ROSTER));
    seeded = conversation([
      message("c0", HUMAN, "@arch @sec please review"),
      message("c1", ARCH, "Here is a change I would make."),
    ]);
    await open();
    await screen.findByText(ARTIFACT);

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    });

    await waitFor(() => expect(dispatched()).toEqual(["arch", "sec"]));
  });

  it("reads the roster rather than deciding on an enrolment nothing has published", async () => {
    // `useProjectAgents` loads nothing itself, and an empty roster resolves no
    // tag at all.
    vi.spyOn(sessions, "resetRestored").mockResolvedValue();
    seedConversation();
    resetAgentRegistry();
    await open();
    await screen.findByText(ARTIFACT);

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    });

    await waitFor(() => expect(dispatched()).toEqual(["arch"]));
    expect(calls("list_project_agents")).not.toHaveLength(0);
  });

  it("PCR-FR-14, CTA-FR-LCFU, CTA-FR-QUXJ, CTA-FR-JQDM: does not reach the proposer where the author has turned to somebody else", async () => {
    // There is no route by which a proposal's own agent is dispatched to for
    // having proposed (per CTA-FR-LCFU, CTA-FR-SSUZ, CTA-FR-DMNI).
    vi.spyOn(sessions, "resetRestored").mockResolvedValue();
    seedConversation(
      conversation([
        message("c0", HUMAN, "@arch please review"),
        message("c1", ARCH, "Here is a change I would make."),
        message("c2", HUMAN, "@sec take this over"),
      ]),
    );
    await open();
    await screen.findByText(ARTIFACT);

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    });

    await waitFor(() => expect(dispatched()).toEqual(["sec"]));
  });

  it("PCR-FR-14, PCR-FR-13, CTA-FR-LCFU, CTA-FR-QUXJ: dispatches nothing where no human comment names an enrolled agent", async () => {
    // The decision's comment is appended and the decision is not refused or
    // retried on that account.
    vi.spyOn(sessions, "resetRestored").mockResolvedValue();
    seedConversation(
      conversation([
        message("c0", HUMAN, "leaving this here"),
        message("c1", ARCH, "Here is a change I would make."),
      ]),
    );
    await open();
    await screen.findByText(ARTIFACT);

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    });

    await waitFor(() =>
      expect(screen.queryByTestId("prompt-change-review")).toBeNull(),
    );
    expect(dispatched()).toEqual([]);
  });

  it("PCR-FR-14, PCR-FR-13, CTA-FR-LCFU, CTA-FR-QUXJ: reaches exactly the agents the feedback names", async () => {
    // PCR-FR-13: a decision carrying feedback that names agents is a tagged
    // comment like any other.
    vi.spyOn(sessions, "resetRestored").mockResolvedValue();
    await open();
    await screen.findByText(ARTIFACT);

    await act(async () => {
      fireEvent.change(screen.getByLabelText("Feedback"), {
        target: { value: "@scribe please re-read this" },
      });
    });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    });

    await waitFor(() => expect(dispatched()).toEqual(["scribe"]));
  });

  it("declines with the feedback typed, writing no file and asking no confirmation", async () => {
    const quiesce = vi.spyOn(sessions, "quiesce");
    await open();
    await screen.findByText(ARTIFACT);

    const field = screen.getByLabelText("Feedback");
    await act(async () => {
      fireEvent.change(field, { target: { value: "Too terse." } });
    });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Reject" }));
    });

    await waitFor(() =>
      expect(calls("decline_prompt_change_proposal")).toHaveLength(1),
    );
    expect(calls("decline_prompt_change_proposal")[0][1]).toMatchObject({
      proposalId: "prop-1",
      feedback: "Too terse.",
    });
    // PCR-FR-12: a decline writes nothing into the project, so nothing about the
    // artifact's session is touched.
    expect(quiesce).not.toHaveBeenCalled();
    expect(calls("save_artifact_contents")).toHaveLength(0);
    expect(calls("apply_prompt_change_proposal")).toHaveLength(0);
    await waitFor(() =>
      expect(screen.queryByTestId("prompt-change-review")).toBeNull(),
    );
  });

  it("renders a refused acceptance inline, lifts the quiesce, and keeps everything", async () => {
    const unquiesce = vi.spyOn(sessions, "unquiesce");
    const reset = vi.spyOn(sessions, "resetRestored").mockResolvedValue();
    stubBackend({ apply_prompt_change_proposal: new Error("write_failed") });
    await open();
    await screen.findByText(ARTIFACT);
    const field = screen.getByLabelText("Feedback");
    await act(async () => {
      fireEvent.change(field, { target: { value: "Keep the second sentence." } });
    });

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    });

    // PCR-FR-15: a `write_failed` is a refusal and says the file has not
    // changed, which is true of every one of them.
    const stated = await screen.findByRole("alert");
    expect(stated).toHaveTextContent(/file has not changed/i);
    expect(screen.getByTestId("prompt-change-review")).toBeInTheDocument();
    // PCR-FR-11: the quiesce is lifted and nothing is reset.
    expect(unquiesce).toHaveBeenCalledWith(ARTIFACT);
    expect(reset).not.toHaveBeenCalled();
    // …and the feedback is still in the field, so the decision is retried
    // without retyping.
    expect(screen.getByLabelText("Feedback")).toHaveValue(
      "Keep the second sentence.",
    );
    expect(calls("dispatch_agent_turn")).toHaveLength(0);
  });

  it("raises the external change it passed over once a failed acceptance lifts the quiesce", async () => {
    // EDT-FR-81, EXC-FR-QCNO / EDT-FR-84: the replacement the event was passed over for did
    // not happen, so the ordinary modal is raised for it.
    const release = sessions.watchExternalChanges();
    sessions.get(ARTIFACT);
    sessions.update(ARTIFACT, { baseline: "base-1", loaded: true });
    stubBackend({ apply_prompt_change_proposal: new Error("write_failed") });
    await open();
    await screen.findByText(ARTIFACT);

    await act(async () => {
      fireBusEvent("artifact-changed-externally", {
        artifactId: ARTIFACT,
        checksum: "somebody-elses-bytes",
      });
    });
    expect(sessions.get(ARTIFACT)?.conflict).toBe(false);

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    });
    await waitFor(() => expect(sessions.get(ARTIFACT)?.conflict).toBe(true));
    release();
  });

  it("disables Accept with the reason stated when the target no longer resolves", async () => {
    stubBackend({ apply_prompt_change_proposal: new Error("artifact_not_found") });
    await open();
    await screen.findByText(ARTIFACT);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    });

    // PCR-FR-15: Accept disabled with the reason stated, Reject still enabled —
    // declining is the one thing still left to do with such a proposal.
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Accept" })).toBeDisabled(),
    );
    expect(screen.getByRole("button", { name: "Reject" })).toBeEnabled();
    expect(
      screen.getByText(/no longer in the project, so this change cannot be applied/i),
    ).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// PCR-FR-15 — every answer the backend can give (PCR-FR-15)
// ---------------------------------------------------------------------------

describe("every refusal it can render (PCR-FR-15)", () => {
  /**
   * PCR-FR-15: every answer is either a refusal that truthfully says the file
   * has not changed, or a success — with or without its comment owed. There is
   * no third answer, and none of these may ever read as "accepted".
   */
  const refusals: ReadonlyArray<[string, RegExp]> = [
    ["write_failed", /file has not changed/i],
    ["already_decided", /already been decided/i],
    ["artifact_not_found", /no longer in the project/i],
    ["not_a_prompt_artifact", /no longer in the project as a prompt/i],
    ["discussion_locked", /conversation is locked/i],
    ["acceptance_in_progress", /still being applied/i],
    ["proposal_not_found", /no longer available/i],
  ];

  it.each(refusals)("states %s in terms the author can act on", async (
    error,
    reads,
  ) => {
    vi.spyOn(sessions, "resetRestored").mockResolvedValue();
    stubBackend({ apply_prompt_change_proposal: new Error(error) });
    await open();
    await screen.findByText(ARTIFACT);

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    });

    const stated = await screen.findByText(reads);
    expect(stated).toBeInTheDocument();
    // …and never as an acceptance: the modal stays open with the comparison
    // rendered, and no turn is dispatched.
    expect(screen.getByTestId("prompt-change-review")).toBeInTheDocument();
    expect(screen.queryByTestId("prompt-review-comment-owed")).toBeNull();
    expect(calls("dispatch_agent_turn")).toHaveLength(0);
    // PCR-FR-15: none of them says the change was applied.
    expect(screen.queryByText(/change was applied/i)).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// PCR-FR-14, PCP-FR-14 / PCR-FR-10 / PCR-FR-15 — the owed comment (PCR-FR-15)
// ---------------------------------------------------------------------------

describe("an acceptance whose comment is owed (PCR-FR-15)", () => {
  it("states that the change was applied and retries through the completion command", async () => {
    vi.spyOn(sessions, "resetRestored").mockResolvedValue();
    stubBackend({
      apply_prompt_change_proposal: new Error("acceptance_incomplete"),
    });
    await open();
    await screen.findByText(ARTIFACT);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    });

    const stated = await screen.findByTestId("prompt-review-comment-owed");
    expect(stated).toHaveTextContent(/change was applied to this file/i);
    expect(stated).toHaveTextContent(/has not yet been told/i);
    // It never states that the file is unchanged, because it is not.
    expect(screen.queryByText(/file has not changed/i)).toBeNull();
    // …and no turn is dispatched, there being nothing yet for one to answer.
    expect(calls("dispatch_agent_turn")).toHaveLength(0);

    stubBackend();
    // PCR-FR-15, PCR-FR-14, PCP-FR-14: one turn **for each distinct agent then active**, so a rule
    // dispatching to the proposer alone would show up as one.
    seedConversation(
      conversation([
        message("c0", HUMAN, "@arch @sec please review"),
        message("c1", ARCH, "Here is a change I would make."),
      ]),
    );
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    });

    // PCR-FR-15: the retry invokes the completion command and never a second
    // acceptance, which would be a second decision rather than the completion of
    // one.
    await waitFor(() =>
      expect(calls("complete_prompt_change_decision")).toHaveLength(1),
    );
    expect(calls("apply_prompt_change_proposal")).toHaveLength(1);
    await waitFor(() => expect(dispatched()).toEqual(["arch", "sec"]));
    await waitFor(() =>
      expect(screen.queryByTestId("prompt-change-review")).toBeNull(),
    );
  });

  it("PCR-FR-15, PCR-FR-14, PCP-FR-14: the retry routes by the comment the completion appended", async () => {
    // PCR-FR-14 / PCR-FR-15: the owed comment's body was journalled when the
    // acceptance landed and carries the feedback the author wrote then — which
    // this modal may no longer hold, the owed state outliving it. So the retry
    // reads the conversation again and routes by the appended comment itself
    // rather than by whatever the field says now.
    vi.spyOn(sessions, "resetRestored").mockResolvedValue();
    stubBackend({
      apply_prompt_change_proposal: new Error("acceptance_incomplete"),
    });
    await open();
    await screen.findByText(ARTIFACT);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    });
    await screen.findByTestId("prompt-review-comment-owed");
    expect(dispatched()).toEqual([]);

    stubBackend({
      // What the completion appended: the journalled body, carrying feedback
      // this surface never had.
      read_discussion: conversation([
        message("c0", HUMAN, "@arch please review"),
        message("c1", ARCH, "Here is a change I would make."),
        message(
          "decision-1",
          HUMAN,
          "Accepted the proposed change to `prompts/review.md`.\n\n@scribe please re-read this",
        ),
      ]),
    });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    });

    await waitFor(() =>
      expect(calls("complete_prompt_change_decision")).toHaveLength(1),
    );
    await waitFor(() => expect(calls("read_discussion")).not.toHaveLength(0));
    // `@scribe` — read off the comment the completion appended, and not `@arch`,
    // which is who this surface's own copy of the conversation still named.
    await waitFor(() => expect(dispatched()).toEqual(["scribe"]));
  });

  it("leaves the statement standing when the retry fails again", async () => {
    vi.spyOn(sessions, "resetRestored").mockResolvedValue();
    stubBackend({
      apply_prompt_change_proposal: new Error("acceptance_incomplete"),
      complete_prompt_change_decision: {
        proposal: proposal({ state: "accepted", commentOwed: true }),
        originKind: "artifact_discussion",
      },
    });
    await open();
    await screen.findByText(ARTIFACT);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    });
    await screen.findByTestId("prompt-review-comment-owed");

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    });

    expect(
      await screen.findByTestId("prompt-review-comment-owed"),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Try again" })).toBeInTheDocument();
    expect(calls("dispatch_agent_turn")).toHaveLength(0);
    expect(screen.queryByText(/file has not changed/i)).toBeNull();
  });
});
