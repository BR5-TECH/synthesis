/**
 * The in-place review of a proposal to a prompt or DOC file, in its Editor tab
 * (`PCR-prompt-change-review.md` PCR-FR-QHZN, PCR-FR-WLTE, PCR-FR-SKRV,
 * PCR-FR-AXPW, PCR-FR-LGVY, PCR-FR-14, PCR-FR-16; `EDT-editor.md` EDT-FR-RWTG,
 * EDT-FR-RCHS; `ESH-editor-source-files.md` ESH-FR-SRVW; `DCR-draft-change-review.md`
 * DCR-FR-GNRL).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));
const dispatchMock = vi.fn(async (..._args: unknown[]) => {});
vi.mock("./DraftDiscussion/decisionDispatch", () => ({
  dispatchDecision: (...args: unknown[]) => dispatchMock(...args),
}));

import { Editor } from "./Editor";
import { EditSessionStore } from "../state/editSessions";
import { notePromptProposalChanged, resetPromptProposals } from "../state/promptProposals";
import { promptHunkCandidateBuffers } from "../state/candidateBuffers";
import { resetDraftDiscussions } from "../state/draftDiscussion";
import { resetProposalHunks } from "../state/proposalHunks";
import { resetLogBufferForTest } from "../logging";
import { proposal as fixture } from "../test/promptProposalFixtures";
import type {
  HunkState,
  ProposalHunk,
  ProposalHunks,
  PromptChangeProposal,
  PromptDecisionOutcome,
} from "../types";

const MD = "prompts/review.md";
const MD_BODY = "# Review\n\nThe original line.\n\nA second line.\n";
const TXT = "docs/notes.txt";
const TXT_BODY = "alpha\nbeta\ngamma\n";

function hunk(over: Partial<ProposalHunk>): ProposalHunk {
  return {
    id: "h1",
    kind: "replace",
    revision: 0,
    before: "The original line.",
    after: "A better line.",
    anchor: { lead: "# Review\n\n", trail: "", hint_start: 10, hint_end: 28 },
    ...over,
  };
}

const MD_HUNKS: ProposalHunk[] = [
  hunk({ id: "h1" }),
  hunk({
    id: "h2",
    before: "A second line.",
    after: "A sharper second line.",
    anchor: { lead: "line.\n\n", trail: "", hint_start: 30, hint_end: 44 },
  }),
];
const TXT_HUNKS: ProposalHunk[] = [
  hunk({
    id: "h1",
    before: "beta",
    after: "BETA",
    anchor: { lead: "alpha\n", trail: "", hint_start: 6, hint_end: 10 },
  }),
];

function proposalFor(
  artifactId: string,
  states: HunkState[],
  over: Partial<PromptChangeProposal> = {},
): PromptChangeProposal {
  const counts = { pending: 0, accepted: 0, rejected: 0, discussing: 0 };
  for (const s of states) counts[s] += 1;
  const undecided = counts.pending + counts.discussing > 0;
  return fixture({
    id: "prop-1",
    artifactId,
    path: artifactId,
    state: undecided ? "pending" : counts.accepted > 0 ? "accepted" : "rejected",
    hunkCount: states.length,
    counts,
    ledger: states.map((state, at) => ({
      id: `h${at + 1}`,
      kind: "replace",
      state,
      edited: false,
      revision: 0,
    })),
    ...over,
  });
}

interface Backend {
  bodies: Record<string, string>;
  proposals: PromptChangeProposal[];
  hunks: ProposalHunks;
  /** What each decision answers; a string is a typed refusal. */
  accept: (hunkId: string) => PromptDecisionOutcome | string;
  reject: (hunkId: string) => PromptDecisionOutcome | string;
}

let backend: Backend;
let version = 1;

const callsTo = (cmd: string) =>
  invokeMock.mock.calls.filter((c) => c[0] === cmd).map((c) => c[1] as Record<string, unknown>);
const order = () => invokeMock.mock.calls.map((c) => c[0] as string);

function outcome(
  states: HunkState[],
  resolved: boolean,
  artifactId = MD,
): PromptDecisionOutcome {
  return {
    proposal: proposalFor(artifactId, states),
    resolved,
    commentId: resolved ? "c-decision" : undefined,
    originKind: "artifact_comment",
  };
}

function answer(value: unknown): Promise<unknown> {
  return typeof value === "string" ? Promise.reject(value) : Promise.resolve(value);
}

function wire() {
  invokeMock.mockImplementation((cmd: string, args: Record<string, unknown> = {}) => {
    switch (cmd) {
      case "append_log_records":
        return Promise.resolve();
      case "load_artifact_contents_by_id": {
        const id = String(args.id);
        return Promise.resolve({ body: backend.bodies[id] ?? "", checksum: `ck-${version}` });
      }
      case "save_artifact_contents":
        backend.bodies[String(args.id)] = String(args.body);
        version += 1;
        return Promise.resolve({ checksum: `ck-${version}` });
      case "list_prompt_change_proposals":
        return Promise.resolve(backend.proposals);
      case "load_prompt_change_proposal_hunks":
        return Promise.resolve(backend.hunks);
      case "accept_prompt_change_hunk":
        return answer(backend.accept(String(args.hunkId)));
      case "reject_prompt_change_hunk":
        return answer(backend.reject(String(args.hunkId)));
      case "decline_prompt_change_proposal":
        return Promise.resolve(outcome(backend.proposals[0].ledger.map(() => "rejected"), true));
      case "edit_prompt_change_hunk":
        return Promise.resolve({ checksum: "hk-2" });
      case "complete_prompt_change_decision":
        return Promise.resolve(outcome(["accepted"], true));
      case "list_comment_threads":
      case "list_discussions":
      case "list_agent_turns":
      case "list_recoverable_agent_turn_failures":
      case "list_personal_agents":
      case "list_project_agents":
        return Promise.resolve([]);
      case "load_app_preferences":
        return Promise.resolve({});
      default:
        return Promise.reject(new Error(`unexpected command ${cmd}`));
    }
  });
}

function renderTab(artifactId: string, sessions: EditSessionStore) {
  return render(
    <Editor
      artifactId={artifactId}
      artifactName={artifactId.split("/").pop()}
      sessions={sessions}
      showComments={false}
      showActions={false}
    />,
  );
}

let sessions: EditSessionStore;

beforeEach(() => {
  invokeMock.mockReset();
  dispatchMock.mockClear();
  resetLogBufferForTest();
  resetPromptProposals();
  resetProposalHunks();
  resetDraftDiscussions();
  promptHunkCandidateBuffers.clear();
  version = 1;
  backend = {
    bodies: { [MD]: MD_BODY, [TXT]: TXT_BODY },
    proposals: [],
    hunks: { hunks: MD_HUNKS, resolutions: [], checksum: "hk-1", legacy: false },
    accept: () => outcome(["accepted", "pending"], false),
    reject: () => outcome(["rejected", "pending"], false),
  };
  sessions = new EditSessionStore();
  wire();
});

afterEach(() => {
  cleanup();
  promptHunkCandidateBuffers.clear();
});

function withMd(states: HunkState[] = ["pending", "pending"]) {
  backend.proposals = [proposalFor(MD, states)];
  backend.hunks = { hunks: MD_HUNKS, resolutions: [], checksum: "hk-1", legacy: false };
}

function withTxt(legacy = false) {
  backend.proposals = [proposalFor(TXT, ["pending"])];
  backend.hunks = { hunks: TXT_HUNKS, resolutions: [], checksum: "hk-1", legacy };
}

describe("PCR-FR-QHZN: the review renders in place", () => {
  it("PCR-FR-QHZN, DCR-FR-GNRL: renders the draft's review bar in the document column, with no dialog and no scrim", async () => {
    withMd();
    const { container } = renderTab(MD, sessions);
    const bar = await screen.findByRole("region", { name: "Proposed changes" });
    expect(container.querySelector(".editor__column, .editor-tab")?.contains(bar)).toBe(true);
    expect(screen.getByTestId("review-counter")).toHaveTextContent("reviewing change 1 of 2");
    // DCR-FR-27: said about the file, in the draft's words.
    expect(screen.getByTestId("review-standing")).toHaveTextContent("The file has not changed.");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByRole("button", { name: "Accept all" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Reject all" })).toBeEnabled();
  });

  it("PCR-FR-16: a proposal that arrives in an open tab enters review with nothing to activate", async () => {
    renderTab(MD, sessions);
    await waitFor(() => expect(callsTo("list_prompt_change_proposals")).toHaveLength(1));
    expect(screen.queryByRole("region", { name: "Proposed changes" })).toBeNull();
    await act(async () => {
      notePromptProposalChanged({ artifactId: MD, proposal: proposalFor(MD, ["pending", "pending"]) });
    });
    expect(await screen.findByRole("region", { name: "Proposed changes" })).toBeInTheDocument();
    expect(screen.getByTestId("editor-prompt-proposal-indication")).toBeInTheDocument();
  });

  it("PCR-FR-QHZN: the review goes when no change is undecided", async () => {
    withMd();
    renderTab(MD, sessions);
    await screen.findByRole("region", { name: "Proposed changes" });
    await act(async () => {
      notePromptProposalChanged({ artifactId: MD, proposal: proposalFor(MD, ["accepted", "rejected"]) });
    });
    await waitFor(() =>
      expect(screen.queryByRole("region", { name: "Proposed changes" })).toBeNull(),
    );
    expect(screen.queryByTestId("editor-prompt-proposal-indication")).toBeNull();
  });
});

describe("PCR-FR-AXPW: deciding one change", () => {
  it("PCR-FR-AXPW: Accept all accepts each change through accept_prompt_change_hunk, then resets the session to the file", async () => {
    withMd();
    backend.accept = (hunkId) => {
      backend.bodies[MD] = MD_BODY.replace("The original line.", "A better line.");
      return hunkId === "h1"
        ? outcome(["accepted", "pending"], false)
        : outcome(["accepted", "accepted"], true);
    };
    renderTab(MD, sessions);
    await screen.findByRole("region", { name: "Proposed changes" });
    const loadsBefore = callsTo("load_artifact_contents_by_id").length;

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Accept all" }));
    });

    await waitFor(() =>
      expect(callsTo("accept_prompt_change_hunk")).toEqual([
        { proposalId: "prop-1", hunkId: "h1", feedback: null },
        { proposalId: "prop-1", hunkId: "h2", feedback: null },
      ]),
    );
    // EXC-FR-UVJY: the session takes what the file now holds.
    await waitFor(() =>
      expect(callsTo("load_artifact_contents_by_id").length).toBeGreaterThan(loadsBefore),
    );
    await waitFor(() => expect(sessions.get(MD)?.buffer).toContain("A better line."));
    expect(sessions.get(MD)?.dirty).toBe(false);
  });

  it("PCR-FR-AXPW, EDT-FR-81: an unsaved edit is written before the acceptance is asked for", async () => {
    withMd();
    renderTab(MD, sessions);
    await screen.findByRole("region", { name: "Proposed changes" });
    await act(async () => {
      const session = sessions.ensure(MD);
      session.buffer = `${MD_BODY}An edit of the author's.\n`;
      sessions.update(MD, { dirty: true });
    });

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Accept all" }));
    });
    await waitFor(() => expect(callsTo("accept_prompt_change_hunk").length).toBeGreaterThan(0));
    const calls = order();
    expect(calls.indexOf("save_artifact_contents")).toBeGreaterThan(-1);
    expect(calls.indexOf("save_artifact_contents")).toBeLessThan(
      calls.indexOf("accept_prompt_change_hunk"),
    );
  });

  it("DCR-FR-CXZG, PCR-FR-AXPW: Accept all stops at the first refusal, and the refusal is said in the file's words", async () => {
    withMd();
    backend.accept = () => "anchor_lost";
    renderTab(MD, sessions);
    await screen.findByRole("region", { name: "Proposed changes" });

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Accept all" }));
    });

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The text this change alters is no longer in the file.",
    );
    expect(callsTo("accept_prompt_change_hunk")).toHaveLength(1);
    // A refusal resets nothing: the file is read no second time for it.
    expect(sessions.get(MD)?.quiesced).toBe(false);
  });

  it("PCR-FR-QHZN: Reject all declines the proposal and writes nothing", async () => {
    withMd();
    renderTab(MD, sessions);
    await screen.findByRole("region", { name: "Proposed changes" });

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Reject all" }));
    });

    await waitFor(() =>
      expect(callsTo("decline_prompt_change_proposal")).toEqual([
        { proposalId: "prop-1", feedback: null },
      ]),
    );
    expect(callsTo("save_artifact_contents")).toHaveLength(0);
    expect(callsTo("accept_prompt_change_hunk")).toHaveLength(0);
  });

  it("PCR-FR-14: a decision that leaves a change undecided dispatches nothing; the resolving one dispatches", async () => {
    withMd();
    backend.accept = (hunkId) =>
      hunkId === "h1" ? outcome(["accepted", "pending"], false) : outcome(["accepted", "accepted"], true);
    renderTab(MD, sessions);
    await screen.findByRole("region", { name: "Proposed changes" });

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Accept all" }));
    });

    await waitFor(() => expect(callsTo("accept_prompt_change_hunk")).toHaveLength(2));
    await waitFor(() => expect(dispatchMock).toHaveBeenCalledTimes(1));
    const [dispatched, decision] = dispatchMock.mock.calls[0] as [
      PromptChangeProposal,
      PromptDecisionOutcome,
    ];
    expect(dispatched.id).toBe("prop-1");
    expect(decision.commentId).toBe("c-decision");
  });

  it("PCR-FR-15: an acceptance whose comment is owed says so at the foot of the column and retries the comment alone", async () => {
    withMd(["pending"]);
    backend.hunks = { hunks: [MD_HUNKS[0]], resolutions: [], checksum: "hk-1", legacy: false };
    backend.accept = () => "acceptance_incomplete";
    renderTab(MD, sessions);
    await screen.findByRole("region", { name: "Proposed changes" });

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Accept all" }));
    });
    // The record the backend reports past the commit point.
    await act(async () => {
      notePromptProposalChanged({
        artifactId: MD,
        proposal: proposalFor(MD, ["accepted"], { commentOwed: true }),
      });
    });

    const owed = await screen.findByRole("region", { name: "Decision comment owed" });
    expect(owed).toHaveTextContent("The change was applied to this file.");
    await act(async () => {
      fireEvent.click(within(owed).getByRole("button", { name: "Try again" }));
    });
    await waitFor(() =>
      expect(callsTo("complete_prompt_change_decision")).toEqual([{ proposalId: "prop-1" }]),
    );
    expect(callsTo("accept_prompt_change_hunk")).toHaveLength(1);
    await waitFor(() => expect(dispatchMock).toHaveBeenCalledTimes(1));
  });
});

describe("PCR-FR-WLTE: the rich surface only", () => {
  it("PCR-FR-WLTE, EDT-FR-RCHS: a Markdown file showing raw text switches to rich", async () => {
    withMd();
    sessions.ensure(MD);
    sessions.update(MD, { mode: "text" });
    renderTab(MD, sessions);
    await screen.findByRole("region", { name: "Proposed changes" });
    await waitFor(() => expect(sessions.get(MD)?.mode).toBe("wysiwyg"));
  });

  it("PCR-FR-WLTE, EDT-FR-RWTG: the raw toggle is disabled while a proposal is pending, and states why", async () => {
    withMd();
    renderTab(MD, sessions);
    await screen.findByRole("region", { name: "Proposed changes" });
    const toggle = screen.getByTestId("editor-mode-toggle");
    expect(toggle).toBeDisabled();
    expect(toggle).toHaveAttribute(
      "title",
      "A proposed change is under review. Decide it before you switch to raw text.",
    );

    await act(async () => {
      notePromptProposalChanged({ artifactId: MD, proposal: proposalFor(MD, ["rejected", "rejected"]) });
    });
    await waitFor(() => expect(screen.getByTestId("editor-mode-toggle")).toBeEnabled());
  });
});

describe("PCR-FR-SKRV: the source surface", () => {
  it("PCR-FR-SKRV, ESH-FR-SRVW: a .txt file shows the review on its source surface, read-only, with the draft's decorations", async () => {
    withTxt();
    renderTab(TXT, sessions);
    const surface = await screen.findByTestId("source-review");
    expect(surface).toHaveTextContent("This file is read-only while a proposed change is under review.");
    // The editable textarea of the file is not offered while the change stands.
    expect(screen.queryByRole("textbox", { name: "Source" })).toBeNull();
    const struck = await waitFor(() => {
      const el = surface.querySelector(".hunk--del");
      expect(el).not.toBeNull();
      return el as HTMLElement;
    });
    expect(struck).toHaveTextContent("beta");
    const block = within(surface).getByRole("group", { name: /Change 1 of 1/ });
    expect(block).toHaveClass("hunk", "hunk--add");
    expect(screen.getByTestId("hunk-chip")).toBeInTheDocument();
    expect(screen.getByTestId("hunk-gutter-map")).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Proposed changes" })).toBeInTheDocument();
    // ESH-FR-LKNM: a source file has no toggle to disable.
    expect(screen.queryByTestId("editor-mode-toggle")).toBeNull();
  });

  it("PCR-FR-SKRV, PCP-FR-EDTW: the proposed text is edited in a plain field, and the edit is written before the decision", async () => {
    withTxt();
    backend.accept = () => outcome(["accepted"], true, TXT);
    renderTab(TXT, sessions);
    const field = await screen.findByRole("textbox", { name: /^Proposed text of Change 1 of 1/ });
    expect(field).toHaveValue("BETA");

    await act(async () => {
      fireEvent.change(field, { target: { value: "Beta, edited" } });
    });
    await act(async () => {
      fireEvent.click(within(screen.getByTestId("hunk-chip")).getByRole("button", { name: "Accept" }));
    });

    await waitFor(() =>
      expect(callsTo("edit_prompt_change_hunk")).toEqual([
        { proposalId: "prop-1", hunkId: "h1", after: "Beta, edited", baselineChecksum: "hk-1" },
      ]),
    );
    const calls = order();
    expect(calls.indexOf("edit_prompt_change_hunk")).toBeLessThan(
      calls.indexOf("accept_prompt_change_hunk"),
    );
  });

  it("PCR-FR-QHZN: Reject on the chip rejects the one change and writes nothing", async () => {
    withTxt();
    backend.reject = () => outcome(["rejected"], true, TXT);
    renderTab(TXT, sessions);
    const chip = await screen.findByTestId("hunk-chip");
    await act(async () => {
      fireEvent.click(within(chip).getByRole("button", { name: "Reject" }));
    });
    await waitFor(() =>
      expect(callsTo("reject_prompt_change_hunk")).toEqual([
        { proposalId: "prop-1", hunkId: "h1", feedback: null },
      ]),
    );
    expect(callsTo("save_artifact_contents")).toHaveLength(0);
    // The resolving rejection dispatches (PCR-FR-14).
    await waitFor(() => expect(dispatchMock).toHaveBeenCalledTimes(1));
  });

  it("PCR-FR-LGVY: a legacy proposal accepts no edit to its text", async () => {
    withTxt(true);
    renderTab(TXT, sessions);
    const surface = await screen.findByTestId("source-review");
    await waitFor(() => expect(surface.querySelector(".hunk--add")).not.toBeNull());
    expect(within(surface).queryByRole("textbox")).toBeNull();
    expect(surface.querySelector(".hunk--add")).toHaveTextContent("BETA");
  });
});
