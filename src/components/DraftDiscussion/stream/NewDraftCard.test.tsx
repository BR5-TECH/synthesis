/**
 * The new-draft card and the What next? row
 * (`../../../../specifications/ui/DCR-draft-change-review.md`).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { NewDraftCard } from "./NewDraftCard";
import { noteNewDraftChanged, resetNewDraftProposals } from "../../../state/newDraftProposals";
import {
  configureDecompositionRoutes,
  registerDiscussRoute,
  showNewDraftCard,
} from "../../../state/decompositionRoutes";
import { resetLogBufferForTest } from "../../../logging";
import {
  clearAllDiscussionSessions,
  getDiscussionSession,
  setDiscussionScroll,
} from "../../../state/discussionSession";
import { otherCommand } from "../../../test/invokeMocks";
import { MainBranchWriteHost } from "../../MainBranchWrite";
import { mainGuard, scriptedReads } from "../../../test/mainBranchWriteFixtures";
import type {
  AgentTurn,
  Comment,
  DraftSummary,
  NewDraftProposal,
  NewDraftProposalReference,
} from "../../../types";

const AGENT = { kind: "agent" as const, agentId: "a1", handle: "arch" };
const HUMAN = { kind: "human" as const, login: "raver119" };

function proposal(over: Partial<NewDraftProposal> = {}): NewDraftProposal {
  return {
    id: "p1",
    setId: "s1",
    draftId: "d1",
    name: "relay-auth",
    position: 1,
    total: 3,
    agent: AGENT,
    rationale: "why",
    threadId: "t1",
    commentId: "c1",
    state: "pending",
    revision: 1,
    createdAt: "2026-01-01T00:00:01Z",
    ...over,
  };
}

function ref(id = "p1", update?: "change" | "withdraw"): NewDraftProposalReference {
  return { kind: "newDraftProposal", proposalId: id, draftId: "d1", update };
}

function comment(id: string, attachments: Comment["attachments"], author = AGENT as Comment["author"]): Comment {
  return { id, author, body: "Why.", quotes: [], attachments, createdAt: "2026-01-01T00:00:00Z" };
}

let summary: Partial<DraftSummary> = {};
let listed: NewDraftProposal[] = [];

function routes() {
  const opened: string[] = [];
  configureDecompositionRoutes({
    openDraft: () => {},
    draftOf: (id) =>
      id === "d1" ? ({ id: "d1", name: "relay", status: "active", updatedAt: "", ...summary } as DraftSummary) : undefined,
    openPreview: (id, label) => opened.push(`${id}:${label}`),
    closePreview: () => {},
  });
  return opened;
}

function renderCard(
  props: { comments?: Comment[]; reference?: NewDraftProposalReference; turns?: AgentTurn[] } = {},
) {
  const c = comment("c1", [ref()]);
  return render(
    <NewDraftCard
      reference={props.reference ?? ref()}
      comment={props.comments?.[0] ?? c}
      comments={props.comments ?? [c]}
      pendingTurns={props.turns ?? []}
    />,
  );
}

beforeEach(() => {
  clearAllDiscussionSessions();
  summary = {};
  listed = [proposal()];
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
    if (cmd === "list_new_draft_proposals") return Promise.resolve(listed);
    if (cmd === "accept_new_draft_proposal" || cmd === "reject_new_draft_proposal") {
      const state = cmd === "accept_new_draft_proposal" ? "accepted" : "rejected";
      return Promise.resolve({
        proposal: proposal({ state, decidedAt: "2026-01-02T00:00:00Z" }),
        commentId: "decision-1",
        originKind: "draft_discussion",
        feedbackSeen: args?.feedback,
      });
    }
    if (cmd === "read_discussion") {
      return Promise.resolve({
        id: "t1",
        target: { kind: "draft", draftId: "d1" },
        fragmentTarget: null,
        resolved: false,
        createdAt: "",
        updatedAt: "",
        comments: [
          { ...comment("c0", [], HUMAN), body: "@arch please decompose" },
          comment("c1", [ref()]),
          comment("decision-1", [], HUMAN),
        ],
      });
    }
    if (cmd === "list_project_agents") {
      return Promise.resolve([
        {
          agent: { id: "a1", nickname: "arch" },
          scope: "project",
          availability: "ready",
        },
      ]);
    }
    if (cmd === "dispatch_agent_turn") return Promise.resolve({ id: "turn-1" });
    if (cmd === "confirm_draft_decomposition") return Promise.resolve({ id: "d1" });
    return otherCommand(cmd);
  });
  resetLogBufferForTest();
  resetNewDraftProposals();
  routes();
});

let release: (() => void) | null = null;

afterEach(() => {
  cleanup();
  configureDecompositionRoutes(null);
  release?.();
  release = null;
});

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

describe("the new-draft card", () => {
  it("DCR-FR-RDLK, DCR-FR-QWXP, DCR-FR-LFDJ, DCR-FR-FKNQ: the card reads its proposal and names its place, name, and state", async () => {
    renderCard();
    const card = await screen.findByTestId("new-draft-card");
    expect(card).toHaveAccessibleName("Proposed draft, part 1 of 3, relay-auth, Waiting for you");
    expect(card).toHaveTextContent("Part 1 of 3 · relay-auth");
    expect(card).toHaveTextContent("Waiting for you");
    expect(card).toHaveTextContent("NEW");
    expect(calls("list_new_draft_proposals")).toHaveLength(1);
  });

  it("DCR-FR-QWXP: an accepted card says so in words and names the created draft", async () => {
    listed = [proposal({ state: "accepted", createdDraftId: "n1", decidedAt: "x" })];
    renderCard({ turns: [{ id: "t" } as AgentTurn] });
    const card = await screen.findByTestId("new-draft-card");
    expect(card).toHaveTextContent("Accepted");
    expect(card).toHaveTextContent("Created draft relay-auth");
    expect(screen.queryByRole("button", { name: "Accept" })).toBeNull();
  });

  it("DCR-FR-MGMZ: an earlier comment of the same proposal is one line that names its update", async () => {
    const first = comment("c1", [ref()]);
    const second = comment("c2", [ref("p1", "change")]);
    render(<NewDraftCard reference={ref()} comment={first} comments={[first, second]} pendingTurns={[]} />);
    const line = await screen.findByTestId("new-draft-reference");
    expect(line).toHaveTextContent("Proposed a draft · Part 1 of 3 · relay-auth");
    expect(screen.queryByRole("button", { name: "Accept" })).toBeNull();
  });

  it("DCR-FR-UTKG, DCR-FR-NMJH: Accept with feedback sends it and dispatches one turn on the decision comment", async () => {
    renderCard();
    fireEvent.change(await screen.findByLabelText("Feedback for the agent (optional)"), {
      target: { value: "Keep it small." },
    });
    fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    await waitFor(() => expect(calls("dispatch_agent_turn")).toHaveLength(1));
    expect(calls("accept_new_draft_proposal")[0][1]).toEqual({ proposalId: "p1", feedback: "Keep it small." });
    expect(calls("dispatch_agent_turn")[0][1]).toMatchObject({ nickname: "arch", triggerCommentId: "decision-1" });
    // DCR-FR-UTKG: the card reads Accepted and names the created draft.
    expect(await screen.findByText("Created draft relay-auth")).toBeInTheDocument();
    expect(screen.getByTestId("new-draft-card")).toHaveTextContent("Accepted");
  });

  it("DCR-FR-FMRQ, DCR-FR-NMJH: Reject without feedback sends none and dispatches nothing", async () => {
    renderCard();
    fireEvent.click(await screen.findByRole("button", { name: "Reject" }));
    await waitFor(() => expect(calls("reject_new_draft_proposal")).toHaveLength(1));
    expect(calls("reject_new_draft_proposal")[0][1]).toEqual({ proposalId: "p1", feedback: null });
    // DCR-FR-FMRQ: the card reads Rejected, and the controls are gone.
    await waitFor(() => expect(screen.getByTestId("new-draft-card")).toHaveTextContent("Rejected"));
    expect(screen.queryByRole("button", { name: "Reject" })).toBeNull();
    expect(calls("dispatch_agent_turn")).toHaveLength(0);
  });

  it("DCR-FR-TOOT: a refused decision shows its error and keeps the feedback", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_new_draft_proposals") return Promise.resolve(listed);
      if (cmd === "accept_new_draft_proposal") return Promise.reject("discussion_resolved");
      if (cmd === "read_discussion" || cmd === "list_project_agents") return new Promise(() => {});
      return otherCommand(cmd);
    });
    renderCard();
    const field = await screen.findByLabelText("Feedback for the agent (optional)");
    fireEvent.change(field, { target: { value: "Keep." } });
    fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("discussion_resolved");
    expect(field).toHaveValue("Keep.");
    expect(screen.getByTestId("new-draft-card")).toHaveTextContent("Waiting for you");
  });

  it("CVP-FR-CUKV: a decision on a proposed draft makes its discussion follow again", async () => {
    setDiscussionScroll("t1", 100, false);
    renderCard();
    fireEvent.click(await screen.findByRole("button", { name: "Reject" }));
    await waitFor(() => expect(getDiscussionSession("t1").footRequest).toBe(1));
    expect(getDiscussionSession("t1").atTail).toBe(true);
  });

  it("CVP-FR-CUKV: a refused decision on a proposed draft leaves its discussion where it is", async () => {
    setDiscussionScroll("t1", 100, false);
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_new_draft_proposals") return Promise.resolve(listed);
      if (cmd === "accept_new_draft_proposal") return Promise.reject("discussion_resolved");
      if (cmd === "read_discussion" || cmd === "list_project_agents") return new Promise(() => {});
      return otherCommand(cmd);
    });
    renderCard();
    fireEvent.click(await screen.findByRole("button", { name: "Accept" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("discussion_resolved");
    expect(getDiscussionSession("t1")).toMatchObject({ atTail: false, footRequest: 0 });
  });

  it("DCR-FR-PBDD: an archived original disables Accept and Reject with the reason, and keeps Discuss and Preview", async () => {
    summary = { status: "archived" };
    renderCard();
    expect(await screen.findByRole("button", { name: "Accept" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Reject" })).toBeDisabled();
    expect(screen.getByTestId("new-draft-card")).toHaveTextContent("This draft is archived");
    expect(screen.getByRole("button", { name: "Discuss →" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Preview ↗" })).toBeEnabled();
  });

  it("DCR-FR-HWLA: Preview opens the preview tab named by the proposed name", async () => {
    const opened = routes();
    renderCard();
    fireEvent.click(await screen.findByRole("button", { name: "Preview ↗" }));
    expect(opened).toEqual(["p1:relay-auth"]);
  });

  it("DCR-FR-AMLR: Discuss takes the draft tab's Discuss route and decides nothing", async () => {
    const asked: string[] = [];
    release = registerDiscussRoute("d1", (id) => asked.push(id));
    renderCard();
    fireEvent.click(await screen.findByRole("button", { name: "Discuss →" }));
    expect(asked).toEqual(["p1"]);
    expect(calls("accept_new_draft_proposal")).toHaveLength(0);
  });

  it("DCR-FR-THXH: a decision from another window updates the card and dispatches nothing", async () => {
    renderCard();
    await screen.findByText("Waiting for you", { exact: false });
    noteNewDraftChanged({ draftId: "d1", proposal: proposal({ state: "rejected", decidedAt: "x" }), cause: "decided" });
    expect(await screen.findByText("Rejected", { exact: false })).toBeInTheDocument();
    expect(calls("dispatch_agent_turn")).toHaveLength(0);
  });
});

describe("the card a route asks for", () => {
  it("CMT-FR-MPKL, DCR-FR-QQLE: a route that asks for the card opens the draft and puts the card in focus", async () => {
    const opened: string[] = [];
    configureDecompositionRoutes({
      openDraft: (id) => opened.push(id),
      draftOf: () => undefined,
      openPreview: () => {},
      closePreview: () => {},
    });
    renderCard();
    const card = await screen.findByTestId("new-draft-card");
    showNewDraftCard("d1", "p1");
    await waitFor(() => expect(card).toHaveFocus());
    expect(opened).toEqual(["d1"]);
  });
});

describe("the What next? row", () => {
  it("DCR-FR-KNDA, DCR-FR-WRQT: no row while a proposal is pending; a decided set with an accepted one offers both actions", async () => {
    listed = [proposal()];
    const { unmount } = renderCard();
    await screen.findByTestId("new-draft-card");
    expect(screen.queryByTestId("decomposition-next-step")).toBeNull();
    unmount();
    resetNewDraftProposals();
    listed = [proposal({ state: "accepted", decidedAt: "2026-01-02T00:00:00Z", createdDraftId: "n1" })];
    renderCard();
    const row = await screen.findByTestId("decomposition-next-step");
    expect(row).toHaveTextContent("What next?");
    expect(screen.getByRole("button", { name: "Ask for next draft" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Confirm decomposition done" })).toBeInTheDocument();
  });

  it("DCR-FR-WRQT: with every proposal rejected, Confirm is not offered", async () => {
    listed = [proposal({ state: "rejected", decidedAt: "x" })];
    renderCard();
    await screen.findByTestId("decomposition-next-step");
    expect(screen.queryByRole("button", { name: "Confirm decomposition done" })).toBeNull();
  });

  it("DCR-FR-KNDA: no row while a turn of the conversation runs, or after the confirmation", async () => {
    listed = [proposal({ state: "accepted", decidedAt: "x", createdDraftId: "n1" })];
    const { unmount } = renderCard({ turns: [{ id: "t" } as AgentTurn] });
    await screen.findByTestId("new-draft-card");
    expect(screen.queryByTestId("decomposition-next-step")).toBeNull();
    unmount();
    summary = { superseded: { setId: "s1", confirmedAt: "x", drafts: [{ id: "n1", name: "relay-auth" }] } };
    renderCard();
    expect(await screen.findByTestId("decomposition-confirmed")).toHaveTextContent("Decomposition confirmed.");
    expect(screen.queryByTestId("decomposition-next-step")).toBeNull();
  });

  it("DCR-FR-BELO: Confirm invokes the confirmation, asks for nothing, and dispatches no turn", async () => {
    listed = [proposal({ state: "accepted", decidedAt: "x", createdDraftId: "n1" })];
    renderCard();
    fireEvent.click(await screen.findByRole("button", { name: "Confirm decomposition done" }));
    await waitFor(() => expect(calls("confirm_draft_decomposition")).toHaveLength(1));
    expect(calls("confirm_draft_decomposition")[0][1]).toEqual({ draftId: "d1" });
    noteNewDraftChanged({ draftId: "d1", cause: "confirmed" });
    expect(await screen.findByTestId("decomposition-confirmed")).toHaveTextContent("relay-auth");
    expect(calls("dispatch_agent_turn")).toHaveLength(0);
  });

  it("DCR-FR-KYJS: a confirmation by tool replaces the row with the statement, with no click", async () => {
    listed = [proposal({ state: "accepted", decidedAt: "x", createdDraftId: "n1" })];
    renderCard();
    await screen.findByTestId("decomposition-next-step");
    noteNewDraftChanged({ draftId: "d1", cause: "confirmed" });
    expect(await screen.findByTestId("decomposition-confirmed")).toHaveTextContent("relay-auth");
    expect(screen.queryByTestId("decomposition-next-step")).toBeNull();
    expect(calls("confirm_draft_decomposition")).toHaveLength(0);
    expect(calls("dispatch_agent_turn")).toHaveLength(0);
  });

  it("DCR-FR-KYJS: a decision by tool changes the card as the card's own decision does", async () => {
    renderCard();
    await screen.findByText("Waiting for you", { exact: false });
    noteNewDraftChanged({
      draftId: "d1",
      proposal: proposal({ state: "accepted", decidedAt: "x", createdDraftId: "n1" }),
      cause: "decided",
    });
    expect(await screen.findByText("Accepted", { exact: false })).toBeInTheDocument();
    expect(await screen.findByTestId("decomposition-next-step")).toBeInTheDocument();
    expect(calls("accept_new_draft_proposal")).toHaveLength(0);
  });

  it("DCR-FR-UVMY: Ask for next draft dispatches one turn on the newest human comment and records nothing", async () => {
    listed = [proposal({ state: "rejected", decidedAt: "x" })];
    renderCard();
    fireEvent.click(await screen.findByRole("button", { name: "Ask for next draft" }));
    await waitFor(() => expect(calls("dispatch_agent_turn")).toHaveLength(1));
    expect(calls("dispatch_agent_turn")[0][1]).toMatchObject({ nickname: "arch", triggerCommentId: "decision-1" });
    expect(calls("reject_new_draft_proposal")).toHaveLength(0);
  });

  it("MBW-FR-DBGA, MBW-FR-XBJS: Accept on a main branch waits for the confirmation, and a cancel creates no draft", async () => {
    const reads = scriptedReads(mainGuard());
    renderCard();
    render(<MainBranchWriteHost />);
    fireEvent.click(await screen.findByRole("button", { name: "Accept" }));

    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("You asked to create a draft.");
    expect(calls("accept_new_draft_proposal")).toHaveLength(0);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(calls("accept_new_draft_proposal")).toHaveLength(0);
    expect(reads.confirms).toEqual([]);
    expect(await screen.findByRole("button", { name: "Accept" })).toBeEnabled();
  });

  it("MBW-FR-DBGA, MBW-FR-ILFN: Accept on a main branch runs after Confirm, and Reject asks for no confirmation", async () => {
    const reads = scriptedReads(mainGuard());
    renderCard();
    render(<MainBranchWriteHost />);
    fireEvent.click(await screen.findByRole("button", { name: "Accept" }));
    fireEvent.click(await screen.findByRole("button", { name: "Confirm write to main" }));

    await waitFor(() => expect(calls("accept_new_draft_proposal")).toHaveLength(1));
    expect(reads.confirms).toEqual(["main"]);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("MBW-FR-DBGA: Reject writes nothing, so it shows no confirmation on a main branch", async () => {
    const reads = scriptedReads(mainGuard());
    renderCard();
    render(<MainBranchWriteHost />);
    fireEvent.click(await screen.findByRole("button", { name: "Reject" }));

    await waitFor(() => expect(calls("reject_new_draft_proposal")).toHaveLength(1));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(reads.reads).toEqual([]);
  });
});
