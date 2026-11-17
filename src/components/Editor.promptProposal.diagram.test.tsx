/**
 * A diagram hunk in the review of a file in its Editor tab, and its comparison
 * pop-up (`PCR-prompt-change-review.md` PCR-FR-DMXE, PCR-FR-TNQA, PCR-FR-GZSV;
 * `DCR-draft-change-review.md` DCR-FR-JKVA to DCR-FR-BYSX).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";

vi.mock("mermaid", async () => (await import("../test/mermaidMock")).mermaidModule);

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
import { MainBranchWriteHost } from "./MainBranchWrite";
import { DiagramComparisonHost } from "./DiagramEditor";
import { closeDiagramEditor, resetDiagramEditorForTest } from "./DiagramEditor/store";
import { setMainBranchWriteOpening } from "../state/mainBranchWrite/requests";
import { resetMermaidForTest } from "./markdownMermaidRender";
import { resetMermaidMock } from "../test/mermaidMock";
import { mainGuard, scriptedReads } from "../test/mainBranchWriteFixtures";
import { EditSessionStore } from "../state/editSessions";
import { resetPromptProposals } from "../state/promptProposals";
import { promptHunkCandidateBuffers } from "../state/candidateBuffers";
import { resetDraftDiscussions } from "../state/draftDiscussion";
import { resetProposalHunks } from "../state/proposalHunks";
import { resetLogBufferForTest } from "../logging";
import { proposal as fixture } from "../test/promptProposalFixtures";
import type {
  ProposalHunk,
  ProposalHunks,
  PromptChangeProposal,
  PromptDecisionOutcome,
} from "../types";

const MD = "prompts/flow.md";
const FENCE = "```mermaid\nflowchart TD\n  A --> B\n```";
const NEW_FENCE = "```mermaid\nflowchart TD\n  A --> B\n  B --> C\n```";
const MD_BODY = `# Flow\n\nThe original line.\n\n${FENCE}\n`;
const TXT = "docs/flow.txt";
const TXT_BODY = `intro\n${FENCE}\noutro\n`;

function diagramHunk(body: string, over: Partial<ProposalHunk> = {}): ProposalHunk {
  const at = body.indexOf(FENCE);
  return {
    id: "h1",
    kind: "replace",
    revision: 0,
    before: FENCE,
    after: NEW_FENCE,
    anchor: { lead: "", trail: "", hint_start: at, hint_end: at + FENCE.length },
    diagram: true,
    ...over,
  };
}

function proposalFor(artifactId: string, state: "pending" | "accepted" = "pending") {
  return fixture({
    id: "prop-1",
    artifactId,
    path: artifactId,
    state: state === "pending" ? "pending" : "accepted",
    hunkCount: 1,
    counts: {
      pending: state === "pending" ? 1 : 0,
      accepted: state === "accepted" ? 1 : 0,
      rejected: 0,
      discussing: 0,
      withdrawn: 0,
    },
    ledger: [{ id: "h1", kind: "replace", state, edited: false, revision: 0, diagram: true }],
  });
}

interface Backend {
  bodies: Record<string, string>;
  proposal: PromptChangeProposal;
  hunks: ProposalHunks;
  accept: () => PromptDecisionOutcome | string;
}
let backend: Backend;
let sessions: EditSessionStore;

const callsTo = (cmd: string) =>
  invokeMock.mock.calls.filter((c) => c[0] === cmd).map((c) => c[1] as Record<string, unknown>);

function wire() {
  invokeMock.mockImplementation((cmd: string, args: Record<string, unknown> = {}) => {
    switch (cmd) {
      case "append_log_records":
      case "list_comment_threads":
      case "list_discussions":
      case "list_agent_turns":
      case "list_recoverable_agent_turn_failures":
      case "list_personal_agents":
      case "list_project_agents":
        return Promise.resolve(cmd === "append_log_records" ? undefined : []);
      case "load_artifact_contents_by_id":
        return Promise.resolve({ body: backend.bodies[String(args.id)] ?? "", checksum: "ck-1" });
      case "save_artifact_contents":
        backend.bodies[String(args.id)] = String(args.body);
        return Promise.resolve({ checksum: "ck-2" });
      case "list_prompt_change_proposals":
        return Promise.resolve([backend.proposal]);
      case "load_prompt_change_proposal_hunks":
        return Promise.resolve(backend.hunks);
      case "accept_prompt_change_hunk": {
        const answer = backend.accept();
        return typeof answer === "string" ? Promise.reject(answer) : Promise.resolve(answer);
      }
      case "load_app_preferences":
        return Promise.resolve({});
      default:
        return Promise.reject(new Error(`unexpected command ${cmd}`));
    }
  });
}

function open(artifactId: string) {
  backend.proposal = proposalFor(artifactId);
  backend.hunks = {
    hunks: [diagramHunk(backend.bodies[artifactId])],
    resolutions: [],
    checksum: "hk-1",
    legacy: false,
  };
  render(
    <Editor
      artifactId={artifactId}
      artifactName={artifactId.split("/").pop()}
      sessions={sessions}
      showComments={false}
      showActions={false}
    />,
  );
  render(<DiagramComparisonHost />);
  render(<MainBranchWriteHost />);
}

beforeEach(() => {
  invokeMock.mockReset();
  dispatchMock.mockClear();
  resetLogBufferForTest();
  resetPromptProposals();
  resetProposalHunks();
  resetDraftDiscussions();
  resetMermaidForTest();
  resetMermaidMock();
  resetDiagramEditorForTest();
  promptHunkCandidateBuffers.clear();
  backend = {
    bodies: { [MD]: MD_BODY, [TXT]: TXT_BODY },
    proposal: proposalFor(MD),
    hunks: { hunks: [], resolutions: [], checksum: "hk-1", legacy: false },
    accept: () => ({
      proposal: proposalFor(MD, "accepted"),
      resolved: true,
      commentId: "c-decision",
      originKind: "artifact_comment",
    }),
  };
  sessions = new EditSessionStore();
  wire();
});

afterEach(() => {
  cleanup();
  setMainBranchWriteOpening(null);
  promptHunkCandidateBuffers.clear();
});

async function compare() {
  await screen.findByRole("region", { name: "Proposed changes" });
  fireEvent.click(await screen.findByRole("button", { name: "Compare diagrams" }));
  return screen.getByRole("dialog", { name: "Compare Mermaid diagrams" });
}

describe("a diagram hunk on the rich surface of a file (PCR-FR-DMXE)", () => {
  it("PCR-FR-DMXE, DCR-FR-JKVA, DCR-FR-HBTM: the new diagram is drawn and the chip holds Compare", async () => {
    open(MD);
    await screen.findByRole("region", { name: "Proposed changes" });
    const drawn = await waitFor(() => {
      const el = document.querySelector<HTMLElement>(".hunk--diagram[data-hunk='h1']");
      expect(el).not.toBeNull();
      return el as HTMLElement;
    });
    await waitFor(() => expect(drawn.querySelector("svg")).not.toBeNull());
    expect(document.querySelector(".hunk--diagram-old")).not.toBeNull();
    expect(await screen.findByRole("button", { name: "Compare diagrams" })).toBeInTheDocument();
  });

  it("PCR-FR-DMXE, DCR-FR-WTCQ: the file takes no edit and no hunk edit is written", async () => {
    open(MD);
    await screen.findByRole("region", { name: "Proposed changes" });
    await waitFor(() => expect(document.querySelector(".hunk--diagram")).not.toBeNull());
    expect(document.querySelector(".hunk--diagram [contenteditable='true'], .hunk--diagram textarea")).toBeNull();
    expect(callsTo("edit_prompt_change_hunk")).toEqual([]);
    expect(callsTo("save_artifact_contents")).toEqual([]);
  });

  it("PCR-FR-TNQA: Accept in the pop-up accepts the hunk through the file's command and closes the pop-up", async () => {
    open(MD);
    const dialog = await compare();
    fireEvent.click(within(dialog).getByRole("button", { name: "Accept" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(callsTo("accept_prompt_change_hunk")).toEqual([
      { proposalId: "prop-1", hunkId: "h1", feedback: null },
    ]);
  });

  it("PCR-FR-TNQA: a failure shows the typed error in the pop-up, beside the hunk, and keeps the pop-up open", async () => {
    backend.accept = () => "anchor_lost";
    open(MD);
    const dialog = await compare();
    fireEvent.click(within(dialog).getByRole("button", { name: "Accept" }));
    const message = "The text this change alters is no longer in the file.";
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(message);
    expect(screen.getByRole("dialog", { name: "Compare Mermaid diagrams" })).toBeInTheDocument();
    // Beside the hunk too: the review bar says it.
    expect(
      screen.getAllByRole("alert").some((el) => el.closest("[role='dialog']") === null),
    ).toBe(true);
  });

  it("PCR-FR-TNQA, MBW-FR-DBGA: the pop-up stays open while the confirmation shows, a cancel keeps it open, and a confirm accepts and closes it", async () => {
    const reads = scriptedReads(mainGuard());
    // The shell closes the floating overlays when a dialog of the window opens.
    setMainBranchWriteOpening(() => closeDiagramEditor("other-overlay"));
    open(MD);
    const dialog = await compare();

    fireEvent.click(within(dialog).getByRole("button", { name: "Accept" }));
    const confirmation = await screen.findByTestId("main-branch-write-dialog");
    expect(callsTo("accept_prompt_change_hunk")).toEqual([]);
    expect(screen.getByRole("dialog", { name: "Compare Mermaid diagrams" })).toBeInTheDocument();

    // The confirmation takes the keys while it shows: Escape is its cancel.
    fireEvent.keyDown(within(confirmation).getByRole("button", { name: "Cancel" }), {
      key: "Escape",
    });
    await waitFor(() => expect(screen.queryByTestId("main-branch-write-dialog")).toBeNull());
    expect(screen.getByRole("dialog", { name: "Compare Mermaid diagrams" })).toBeInTheDocument();
    expect(callsTo("accept_prompt_change_hunk")).toEqual([]);

    fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Accept" }));
    fireEvent.click(await screen.findByRole("button", { name: "Confirm write to main" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(callsTo("accept_prompt_change_hunk")).toHaveLength(1);
    expect(reads.confirms).toEqual(["main"]);
  });
});

describe("a diagram hunk on the source surface (PCR-FR-GZSV)", () => {
  it("PCR-FR-GZSV, DCR-FR-WTCQ: the hunk is drawn as the fence text, is not editable, and the chip holds Compare", async () => {
    open(TXT);
    const surface = await screen.findByTestId("source-review");
    const block = await waitFor(() => {
      const el = surface.querySelector<HTMLElement>(".hunk--add[data-hunk='h1']");
      expect(el).not.toBeNull();
      return el as HTMLElement;
    });
    expect(block).toHaveTextContent("B --> C");
    expect(block.querySelector("textarea")).toBeNull();
    expect(block).toHaveAccessibleName(/diagram change proposed by/);
    expect(await screen.findByRole("button", { name: "Compare diagrams" })).toBeInTheDocument();
    expect(callsTo("edit_prompt_change_hunk")).toEqual([]);
  });

  it("PCR-FR-GZSV, EDT-FR-NZLC: Compare shows both diagrams rendered, as on the rich surface", async () => {
    open(TXT);
    const dialog = await compare();
    const left = within(dialog).getByRole("region", { name: "Current diagram" });
    const right = within(dialog).getByRole("region", { name: "Proposed diagram" });
    await waitFor(() => expect(left.querySelector(".mermaid-diagram svg")).not.toBeNull());
    await waitFor(() => expect(right.querySelector(".mermaid-diagram svg")).not.toBeNull());
    expect(right.querySelector(".mermaid-diagram svg")?.getAttribute("data-source")).toContain(
      "B --> C",
    );
  });

  it("PCR-FR-GZSV, DCR-FR-GAFN: the end of the review closes the pop-up", async () => {
    open(TXT);
    await compare();
    // The proposal is resolved elsewhere: the review ends.
    const { notePromptProposalChanged } = await import("../state/promptProposals");
    await act(async () => {
      notePromptProposalChanged({ artifactId: TXT, proposal: proposalFor(TXT, "accepted") });
    });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });
});
