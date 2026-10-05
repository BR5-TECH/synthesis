import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

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

import { PromptChangeReview } from "./PromptChangeReview";
import {
  closePromptReview,
  notePromptProposalChanged,
  openPromptReview,
  resetPromptProposals,
} from "../state/promptProposals";
import { resetDiffModes } from "../state/diffModes";
import { promptCandidateBuffers } from "../state/candidateBuffers";
import { sealBurst } from "../state/editHistory";
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
  ARTIFACT,
  CURRENT,
  PROPOSED,
  ROSTER,
  addressedConversation,
  agent,
  conversation,
  proposal,
} from "../test/promptProposalFixtures";

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
// PCR-FR-22 / PCR-FR-21, PCR-FR-25 / PCR-FR-22 — the candidate (PCR-FR-20 … PCR-FR-23)
// ---------------------------------------------------------------------------

describe("the candidate (PCR-FR-21, PCR-FR-22, PCR-FR-23)", () => {
  /**
   * `open()` waits on a `findBy`, which polls on real timers — so a test that
   * fakes the clock renders and settles by advancing it instead.
   */
  async function openFaked(id = "prop-1") {
    render(<PromptChangeReview artifactId={ARTIFACT} sessions={sessions} />);
    await act(async () => {
      openPromptReview(id);
      await vi.advanceTimersByTimeAsync(0);
    });
  }

  it("writes the candidate and no file of the project, and says which is which", async () => {
    vi.useFakeTimers();
    try {
      await openFaked();
      await act(async () => {
        promptCandidateBuffers.edit("prop-1", "# Review\n\nMy own version.\n");
        await vi.advanceTimersByTimeAsync(0);
      });

      // PCR-FR-23: the standing line says the candidate has been edited **and
      // that the file has not changed**, in words.
      expect(
        screen.getByText(/You have edited this candidate/),
      ).toHaveTextContent(/has not changed the file/);

      await act(async () => {
        await vi.advanceTimersByTimeAsync(promptCandidateBuffers.delay + 50);
      });

      expect(calls("save_prompt_change_proposal_candidate")).toHaveLength(1);
      expect(calls("save_prompt_change_proposal_candidate")[0][1]).toEqual({
        proposalId: "prop-1",
        content: "# Review\n\nMy own version.\n",
        baselineChecksum: "cand-1",
      });
      // PCR-FR-22: not a change to the artifact.
      expect(calls("save_artifact_contents")).toHaveLength(0);
      expect(calls("apply_prompt_change_proposal")).toHaveLength(0);
      expect(calls("decline_prompt_change_proposal")).toHaveLength(0);
    } finally {
      vi.useRealTimers();
    }
  });

  it("brings an outstanding candidate write forward before a decision", async () => {
    vi.useFakeTimers();
    try {
      vi.spyOn(sessions, "resetRestored").mockResolvedValue();
      await openFaked();
      await act(async () => {
        promptCandidateBuffers.edit("prop-1", "# Review\n\nMine.\n");
        await vi.advanceTimersByTimeAsync(0);
      });
      expect(calls("save_prompt_change_proposal_candidate")).toHaveLength(0);

      await act(async () => {
        fireEvent.click(screen.getByRole("button", { name: "Accept" }));
        await vi.advanceTimersByTimeAsync(0);
      });

      // PCR-FR-10 / PCR-FR-22: what lands is the candidate as the author last
      // edited it rather than as the agent composed it.
      expect(calls("save_prompt_change_proposal_candidate")).toHaveLength(1);
      expect(calls("apply_prompt_change_proposal")).toHaveLength(1);
    } finally {
      vi.useRealTimers();
    }
  });

  it("keeps the edited candidate across a dismissal and a reopening", async () => {
    vi.useFakeTimers();
    try {
      await openFaked();
      await act(async () => {
        promptCandidateBuffers.edit("prop-1", "# Review\n\nMine.\n");
        await vi.advanceTimersByTimeAsync(0);
      });
      await act(async () => {
        fireEvent.keyDown(window, { key: "Escape" });
        await vi.advanceTimersByTimeAsync(0);
      });
      // PCR-FR-22: the outstanding write is brought forward by the dismissal.
      expect(calls("save_prompt_change_proposal_candidate")).toHaveLength(1);

      await act(async () => {
        openPromptReview("prop-1");
        await vi.advanceTimersByTimeAsync(0);
      });
      // PCR-FR-21: the buffer survives, so the reopening renders the author's
      // text rather than the agent's original.
      expect(promptCandidateBuffers.get("prop-1")?.text).toBe(
        "# Review\n\nMine.\n",
      );
    } finally {
      vi.useRealTimers();
    }
  });

  it("disables both decisions while a candidate write has failed", async () => {
    vi.useFakeTimers();
    try {
      stubBackend({
        save_prompt_change_proposal_candidate: new Error("write_failed"),
      });
      await openFaked();
      await act(async () => {
        promptCandidateBuffers.edit("prop-1", "# Review\n\nMine.\n");
        await vi.advanceTimersByTimeAsync(promptCandidateBuffers.delay + 50);
      });

      // PCR-FR-22: every edit stays in the buffer, and nothing is retried on a
      // timer.
      expect(promptCandidateBuffers.get("prop-1")?.text).toBe(
        "# Review\n\nMine.\n",
      );
      const after = calls("save_prompt_change_proposal_candidate").length;
      await act(async () => {
        await vi.advanceTimersByTimeAsync(promptCandidateBuffers.delay * 5);
      });
      expect(calls("save_prompt_change_proposal_candidate")).toHaveLength(after);

      const accept = screen.getByRole("button", { name: "Accept" });
      const reject = screen.getByRole("button", { name: "Reject" });
      expect(accept).toBeDisabled();
      expect(reject).toBeDisabled();
      expect(accept.getAttribute("title")).toMatch(/could not be saved/);
      expect(calls("apply_prompt_change_proposal")).toHaveLength(0);
    } finally {
      vi.useRealTimers();
    }
  });

  it("states a stale candidate as a conflict and chooses neither side", async () => {
    vi.useFakeTimers();
    try {
      stubBackend({
        save_prompt_change_proposal_candidate: new Error("candidate_stale"),
      });
      await openFaked();
      await act(async () => {
        promptCandidateBuffers.edit("prop-1", "# Review\n\nMine.\n");
        await vi.advanceTimersByTimeAsync(promptCandidateBuffers.delay + 50);
      });

      // PCR-FR-22: an explicit conflict over the **candidate**, offering both
      // resolutions and choosing neither.
      expect(
        screen.getByText(/rewritten somewhere else since you opened it/i),
      ).toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: "Keep what is on screen" }),
      ).toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: "Take the stored version" }),
      ).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Accept" })).toBeDisabled();
      expect(screen.getByRole("button", { name: "Reject" })).toBeDisabled();
    } finally {
      vi.useRealTimers();
    }
  });
});

// ---------------------------------------------------------------------------
// PCR-FR-22, PCR-FR-23 / PCR-FR-20 / PCR-FR-19 — target-only editing (PCR-FR-20)
// ---------------------------------------------------------------------------

/** The rows a keystroke may reach — the candidate's, and no others. */
const candidateCells = (): HTMLElement[] =>
  Array.from(document.querySelectorAll<HTMLElement>('[data-target="true"]'));

const candidateCell = (text: string): HTMLElement => {
  const found = candidateCells().find((n) => n.textContent === text);
  if (!found) {
    throw new Error(
      `no editable candidate row reading ${JSON.stringify(text)}; the editable rows are ${JSON.stringify(
        candidateCells().map((n) => n.textContent),
      )}`,
    );
  }
  return found;
};

const typeInto = (cell: HTMLElement, text: string) => {
  cell.textContent = text;
  fireEvent.input(cell);
};

describe("editing the candidate (PCR-FR-20)", () => {
  it("takes a keystroke in the candidate and none in the base", async () => {
    // PCR-FR-19, PCR-FR-20 / DFV-FR-41: the candidate is the target and the base is the
    // original, so a keystroke reaches one and never the other.
    await open();
    await screen.findByText(ARTIFACT);

    // The base's own rows are announced read-only and carry no editable flag.
    const base = Array.from(
      document.querySelectorAll<HTMLElement>(".diff-line__text"),
    ).filter((n) => n.getAttribute("aria-readonly") === "true");
    expect(base.length).toBeGreaterThan(0);
    for (const row of base) {
      expect(row.getAttribute("data-target")).not.toBe("true");
      expect(row.getAttribute("contenteditable")).not.toBe("true");
    }

    const cell = candidateCell("A better line.");
    // DCR-FR-28 / PCR-FR-20: the name says which revision the keystrokes reach
    // and names the agent that composed it, so a screen-reader user landing in
    // it knows it belongs to a proposal rather than to the file.
    expect(cell.getAttribute("aria-label")).toContain("proposal candidate");
    expect(cell.getAttribute("aria-label")).toContain("@arch");
    typeInto(cell, "A better line, corrected.");

    // PCR-FR-20 / PCR-FR-22: the edit reached the **candidate** and no file of
    // the project.
    expect(promptCandidateBuffers.get("prop-1")?.text).toContain(
      "A better line, corrected.",
    );
    expect(calls("save_artifact_contents")).toHaveLength(0);
  });

  it("traverses the candidate's own history and never the artifact's", async () => {
    // PCR-FR-20 / EDT-FR-84, EDT-FR-22, PCR-FR-22: the candidate holds its own undo history, so one
    // undo issued here reverses the candidate's last edit and nothing of the
    // artifact's.
    await open();
    await screen.findByText(ARTIFACT);
    typeInto(candidateCell("A better line."), "one");
    act(() => {
      sealBurst(promptCandidateBuffers.get("prop-1")!.history);
    });
    typeInto(candidateCell("one"), "two");
    expect(promptCandidateBuffers.get("prop-1")?.text).toContain("two");

    await act(async () => {
      // The accelerator is claimed on the modal itself, in the capture phase,
      // exactly as a Diff tab claims it (DFV-FR-50).
      fireEvent.keyDown(screen.getByRole("dialog"), { key: "z", metaKey: true });
    });

    expect(promptCandidateBuffers.get("prop-1")?.text).toContain("one");
    // …and the artifact's own session was never touched by any of it.
    expect(sessions.has(ARTIFACT)).toBe(false);
    expect(calls("save_artifact_contents")).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// PCR-FR-10, PCR-FR-11 / PCR-FR-22, PCR-FR-12 — an empty candidate (PCR-FR-02, PCR-FR-20)
// ---------------------------------------------------------------------------

describe("an empty candidate (PCR-FR-02)", () => {
  it("says what accepting would do and leaves Accept enabled", async () => {
    // PCR-FR-02, PCR-FR-10, PCR-FR-11, PCR-FR-20: an empty candidate is a change the author is entitled to accept
    // rather than a state to be corrected first — which is where this surface
    // departs from the draft review's own empty-candidate statement.
    stubBackend({
      load_prompt_change_proposal_content: { content: "", checksum: "cand-0" },
    });
    await open();
    await screen.findByText(ARTIFACT);

    expect(
      await screen.findByText(/would leave the file empty/i),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Accept" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Reject" })).toBeEnabled();
  });

  it("keeps both decisions enabled when the author empties the candidate", async () => {
    // PCR-FR-20, PCR-FR-22, PCR-FR-12, PCR-FR-10: the file is byte-for-byte what it was, and the write carried no
    // bytes.
    vi.useFakeTimers();
    try {
      render(<PromptChangeReview artifactId={ARTIFACT} sessions={sessions} />);
      await act(async () => {
        openPromptReview("prop-1");
        await vi.advanceTimersByTimeAsync(0);
      });
      await act(async () => {
        promptCandidateBuffers.edit("prop-1", "");
        await vi.advanceTimersByTimeAsync(promptCandidateBuffers.delay + 50);
      });

      expect(calls("save_prompt_change_proposal_candidate")).toHaveLength(1);
      expect(
        (calls("save_prompt_change_proposal_candidate")[0][1] as { content: string })
          .content,
      ).toBe("");
      expect(screen.getByRole("button", { name: "Accept" })).toBeEnabled();
      expect(screen.getByRole("button", { name: "Reject" })).toBeEnabled();
      expect(calls("save_artifact_contents")).toHaveLength(0);
    } finally {
      vi.useRealTimers();
    }
  });
});

// ---------------------------------------------------------------------------
// PCR-FR-22 — resolving a candidate conflict (PCR-FR-22)
// ---------------------------------------------------------------------------

describe("resolving a candidate conflict (PCR-FR-22)", () => {
  /** Drive a candidate edit into the stale refusal the conflict comes from. */
  async function intoConflict() {
    stubBackend({
      save_prompt_change_proposal_candidate: new Error("candidate_stale"),
    });
    render(<PromptChangeReview artifactId={ARTIFACT} sessions={sessions} />);
    await act(async () => {
      openPromptReview("prop-1");
      await vi.advanceTimersByTimeAsync(0);
    });
    await act(async () => {
      promptCandidateBuffers.edit("prop-1", "# Review\n\nMine.\n");
      await vi.advanceTimersByTimeAsync(promptCandidateBuffers.delay + 50);
    });
  }

  it("keeps what is on screen, against the baseline the refusal was about", async () => {
    vi.useFakeTimers();
    try {
      await intoConflict();
      stubBackend({
        load_prompt_change_proposal_content: {
          content: "# Review\n\nSomebody else's.\n",
          checksum: "moved-on",
        },
      });

      await act(async () => {
        fireEvent.click(screen.getByRole("button", { name: "Keep what is on screen" }));
        await vi.advanceTimersByTimeAsync(promptCandidateBuffers.delay + 50);
      });

      expect(promptCandidateBuffers.get("prop-1")?.text).toBe(
        "# Review\n\nMine.\n",
      );
      // Written against the checksum the refusal was about rather than the stale
      // one, so it is not refused again.
      const saves = calls("save_prompt_change_proposal_candidate");
      const last = saves[saves.length - 1][1] as { baselineChecksum: string };
      expect(last.baselineChecksum).toBe("moved-on");
      // …and both decisions are offered again.
      expect(screen.getByRole("button", { name: "Accept" })).toBeEnabled();
    } finally {
      vi.useRealTimers();
    }
  });

  it("takes the stored version, discarding the edits with the conflict", async () => {
    vi.useFakeTimers();
    try {
      await intoConflict();
      stubBackend({
        load_prompt_change_proposal_content: {
          content: "# Review\n\nSomebody else's.\n",
          checksum: "moved-on",
        },
      });

      await act(async () => {
        fireEvent.click(screen.getByRole("button", { name: "Take the stored version" }));
        await vi.advanceTimersByTimeAsync(promptCandidateBuffers.delay + 50);
      });

      expect(promptCandidateBuffers.get("prop-1")?.text).toBe(
        "# Review\n\nSomebody else's.\n",
      );
      expect(screen.getByRole("button", { name: "Reject" })).toBeEnabled();
    } finally {
      vi.useRealTimers();
    }
  });
});

// ---------------------------------------------------------------------------
// PCR-FR-24 / PCR-FR-20 — a decided proposal (PCR-FR-24, PCR-FR-02)
// ---------------------------------------------------------------------------

describe("a decided proposal (PCR-FR-24)", () => {
  it("replaces the controls with the decided statement when another window decides", async () => {
    await open();
    await screen.findByText(ARTIFACT);
    act(() => {
      promptCandidateBuffers.edit("prop-1", "# Review\n\nUnwritten.\n");
    });

    await act(async () => {
      notePromptProposalChanged({
        artifactId: ARTIFACT,
        proposal: proposal({ state: "accepted", decidedAt: "2026-01-02T00:00:00Z" }),
      });
    });

    expect(await screen.findByTestId("prompt-review-decided")).toHaveTextContent(
      /Accepted/,
    );
    expect(screen.queryByRole("button", { name: "Accept" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Reject" })).toBeNull();
    // …and the unwritten candidate edit is dropped with them.
    expect(promptCandidateBuffers.get("prop-1")).toBeUndefined();
  });

  it("opens a decided proposal for reading with nothing to decide", async () => {
    stubBackend({
      list_prompt_change_proposals: [
        proposal({ state: "accepted", decidedAt: "2026-01-02T00:00:00Z" }),
      ],
    });
    await open();
    expect(await screen.findByTestId("prompt-review-decided")).toBeInTheDocument();
    expect(screen.queryByLabelText("Feedback")).toBeNull();
    expect(screen.queryByRole("button", { name: "Accept" })).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// PCR-FR-20 — what the surface does not offer (PCR-FR-19)
// ---------------------------------------------------------------------------

describe("what it does not offer (PCR-FR-19)", () => {
  it("carries no staging control and exactly two text inputs' worth of writing", async () => {
    await open();
    const modal = await screen.findByTestId("prompt-change-review");
    for (const name of [/stage/i, /apply hunk/i, /accept hunk/i, /revert hunk/i]) {
      expect(screen.queryByRole("button", { name })).toBeNull();
    }
    // The feedback field is the only `textarea` the foot renders; the
    // candidate's editing surface is the comparison's own (PCR-FR-20).
    const areas = modal.querySelectorAll("textarea");
    expect(areas).toHaveLength(1);
    expect(areas[0]).toHaveAccessibleName("Feedback");
  });

  it("dismisses on a pointer-down on the scrim without invoking anything", async () => {
    await open();
    const scrim = await screen.findByTestId("prompt-change-review");
    await act(async () => {
      await userEvent.click(scrim);
    });
    expect(screen.queryByTestId("prompt-change-review")).toBeNull();
    expect(calls("apply_prompt_change_proposal")).toHaveLength(0);
    expect(calls("decline_prompt_change_proposal")).toHaveLength(0);
  });
});
