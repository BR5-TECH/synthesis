/**
 * A diagram hunk in the draft's document, and its comparison pop-up
 * (`../../specifications/ui/DCR-draft-change-review.md` DCR-FR-JKVA to
 * DCR-FR-BYSX; `EDT-editor.md` EDT-FR-BRVQ to EDT-FR-PQAS).
 *
 * Driven through the whole tab with the real overlay and the real pop-up, so the
 * decorations, the chip, the Compare button, and the pop-up are the ones the
 * author sees. The Mermaid library is the stand-in of `../test/mermaidMock`.
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

import {
  PROMPT,
  draft,
  freshHistory,
  makeStubs,
  renderWorkspace,
} from "../test/newArtifactFixtures";
import { otherCommand } from "../test/invokeMocks";
import { resetMermaidMock } from "../test/mermaidMock";
import { letWriteLand } from "../test/autosave";
import { DiagramComparisonHost } from "./DiagramEditor";
import { resetDiagramEditorForTest } from "./DiagramEditor/store";
import { resetMermaidForTest } from "./markdownMermaidRender";
import { hunkCandidateBuffers } from "../state/candidateBuffers";
import { noteProposalChanged, resetDraftProposals } from "../state/draftProposals";
import { resetDraftDiscussions, setFocusedHunk } from "../state/draftDiscussion";
import { resetProposalHunks } from "../state/proposalHunks";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetDiscussionFocus } from "../state/discussionFocus";
import { resetLogBufferForTest } from "../logging";
import type { HunkState } from "../types";

const { stub } = makeStubs(invokeMock);

const FENCE = "```mermaid\nflowchart TD\n  A --> B\n```";
const NEW_FENCE = "```mermaid\nflowchart TD\n  A --> B\n  B --> C\n```";
const ADDED_FENCE = "```mermaid\nflowchart TD\n  X --> Y\n```";
const LEAD = "Intro text here.";
const TAIL = "Tail text goes here.";
const BODY = `## Flow\n\n${LEAD}\n\n${FENCE}\n\n${TAIL}\n`;

const AGENT = { kind: "agent" as const, agentId: "a1", handle: "arch", model: "m" };

interface Hunk {
  id: string;
  kind: "replace" | "add" | "del";
  revision: number;
  before?: string;
  after?: string;
  anchor: { lead: string; trail: string; hint_start: number; hint_end: number };
  diagram?: true;
}

const at = (text: string) => BODY.indexOf(text);

const CHANGE: Hunk = {
  id: "h1",
  kind: "replace",
  revision: 0,
  before: FENCE,
  after: NEW_FENCE,
  anchor: { lead: `${LEAD}\n\n`, trail: `\n\n${TAIL}`, hint_start: at(FENCE), hint_end: at(FENCE) },
  diagram: true,
};
const ADDITION: Hunk = {
  id: "h1",
  kind: "add",
  revision: 0,
  after: `\n\n${ADDED_FENCE}`,
  anchor: { lead: TAIL, trail: "", hint_start: at(TAIL) + TAIL.length, hint_end: at(TAIL) + TAIL.length },
  diagram: true,
};
const REMOVAL: Hunk = {
  id: "h1",
  kind: "del",
  revision: 0,
  before: `${FENCE}\n\n`,
  anchor: { lead: `${LEAD}\n\n`, trail: TAIL, hint_start: at(FENCE), hint_end: at(FENCE) },
  diagram: true,
};

function proposal(hunks: Hunk[], state: HunkState = "pending") {
  const counts = { pending: 0, accepted: 0, rejected: 0, discussing: 0, withdrawn: 0 };
  counts[state] += hunks.length;
  return {
    id: "p1",
    draftId: "d1",
    path: PROMPT,
    agent: AGENT,
    rationale: "A diagram changes.",
    threadId: "t1",
    commentId: "c1",
    state: "pending" as const,
    candidateEdited: false,
    legacy: false,
    hunkCount: hunks.length,
    counts,
    ledger: hunks.map((h) => ({
      id: h.id,
      kind: h.kind,
      state,
      edited: false,
      revision: h.revision,
      ...(h.diagram ? { diagram: true } : {}),
    })),
    createdAt: "2026-01-01T00:00:00Z",
  };
}

let refuse: ((cmd: string) => string | null) | null = null;

function backend(hunks: Hunk[], state: HunkState = "pending") {
  const record = proposal(hunks, state);
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
        return [record];
      case "load_draft_change_proposal_hunks":
        return {
          hunks,
          resolutions: hunks.map((_, i) => ({ kind: "resolved", start: i, end: i })),
          checksum: "hc-1",
          legacy: false,
        };
      case "accept_draft_change_hunk":
      case "reject_draft_change_hunk": {
        const failure = refuse?.(cmd) ?? null;
        if (failure !== null) throw failure;
        return { proposal: record, commentId: undefined, originKind: "draft_discussion" };
      }
      default:
        return otherCommand(cmd);
    }
  });
  return record;
}

beforeEach(() => {
  invokeMock.mockReset();
  refuse = null;
  hunkCandidateBuffers.clear();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  resetDraftProposals();
  resetDraftDiscussions();
  resetProposalHunks();
  resetLogBufferForTest();
  resetMermaidForTest();
  resetMermaidMock();
  resetDiagramEditorForTest();
  stub();
});

afterEach(() => {
  cleanup();
  hunkCandidateBuffers.clear();
});

const callsTo = (cmd: string) =>
  invokeMock.mock.calls.filter((c) => c[0] === cmd).map((c) => c[1]);

/** The block that a diagram hunk draws, once the review is up. */
async function block(id = "h1") {
  await screen.findByRole("region", { name: "Proposed edits" });
  return waitFor(() => {
    const el = document.querySelector<HTMLElement>(`[data-hunk="${id}"]`);
    expect(el).not.toBeNull();
    return el as HTMLElement;
  });
}

function mount() {
  const view = renderWorkspace();
  render(<DiagramComparisonHost />);
  return view;
}

/** The diagram source that a pane shows, read from the stand-in's drawing. */
async function paneSource(name: string): Promise<string> {
  const pane = screen.getByRole("region", { name });
  const svg = await waitFor(() => {
    const found = pane.querySelector(".mermaid-diagram svg");
    expect(found).not.toBeNull();
    return found as SVGElement;
  });
  return svg.getAttribute("data-source") ?? svg.textContent ?? "";
}

describe("a diagram hunk that changes a diagram (DCR-FR-JKVA, DCR-FR-XNPE)", () => {
  it("DCR-FR-JKVA, DCR-FR-XNPE, DCR-FR-BYSX: the new diagram stands in place of the old fence, which is hidden and unchanged", async () => {
    backend([CHANGE]);
    mount();
    const drawn = await block();
    expect(drawn).toHaveClass("hunk--add", "hunk--diagram");
    expect(drawn).toHaveAccessibleName(/^Edit 1 of 1, diagram edit proposed by .*arch/);
    await waitFor(() => expect(drawn.querySelector("svg")).not.toBeNull());
    expect(drawn.querySelector("svg")?.getAttribute("data-source")).toContain("B --> C");
    // The old fence stays in the document, and is only hidden.
    expect(document.querySelector(".hunk--diagram-old")).not.toBeNull();
    // DCR-FR-JKVA: the diagram is the whole of the hunk. No proposed text.
    expect(document.querySelector(".hunk__doc")).toBeNull();
    // DCR-FR-WTCQ: no editing surface and no edit write.
    expect(drawn.querySelector("[contenteditable='true'], textarea")).toBeNull();
    // DCR-FR-BYSX: counted as any hunk is.
    expect(screen.getByTestId("review-counter")).toHaveTextContent("reviewing edit 1 of 1");
    await letWriteLand();
    expect(callsTo("save_draft_file_contents")).toEqual([]);
    expect(callsTo("edit_draft_change_hunk")).toEqual([]);
  });

  it("DCR-FR-XNPE: a new source that does not render shows its source and the message, as text", async () => {
    backend([{ ...CHANGE, after: "```mermaid\nINVALID <b>x</b>\n```" }]);
    mount();
    const drawn = await block();
    const failure = await waitFor(() => {
      const el = drawn.querySelector(".mermaid-error");
      expect(el).not.toBeNull();
      return el as HTMLElement;
    });
    expect(failure).toHaveTextContent("Diagram error");
    expect(drawn.querySelector(".hunk__diagram-source")).toHaveTextContent("INVALID <b>x</b>");
    // The message and the source are text, never markup.
    expect(drawn.querySelector("b")).toBeNull();
  });
});

describe("a diagram hunk that adds or removes a diagram (DCR-FR-JKVA)", () => {
  it("DCR-FR-JKVA, DCR-FR-TSNW: an addition draws the new diagram after its lead", async () => {
    backend([ADDITION]);
    mount();
    const drawn = await block();
    expect(drawn).toHaveClass("hunk--add", "hunk--diagram");
    expect(drawn).toHaveAccessibleName(/diagram addition proposed by/);
    await waitFor(() => expect(drawn.querySelector("svg")).not.toBeNull());
    const tail = Array.from(document.querySelectorAll("p")).find((p) => p.textContent === TAIL)!;
    expect(tail.compareDocumentPosition(drawn) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(document.querySelector(".hunk--diagram-old")).toBeNull();
  });

  it("DCR-FR-JKVA, DCR-FR-BYSX: a removal keeps the old diagram, marks it as removed, and says so in words", async () => {
    backend([REMOVAL]);
    mount();
    const drawn = await block();
    // The old fence is the one marked; its foot holds the words and the chip.
    const marked = document.querySelector<HTMLElement>(".hunk--diagram-marked");
    expect(marked).not.toBeNull();
    expect(marked).toHaveAttribute("data-hunk", "h1");
    expect(document.querySelector(".hunk--diagram-old")).toBeNull();
    const foot = document.querySelector<HTMLElement>(".hunk--diagram-foot");
    expect(foot).toHaveTextContent("Diagram removed");
    expect(foot).toHaveAccessibleName(/diagram removal proposed by/);
    expect(drawn).toBeInTheDocument();
  });

  it("DCR-FR-JKVA, DCR-FR-07: a rejected change shows the document's own diagram in the held tone and no new diagram", async () => {
    backend([CHANGE], "rejected");
    mount();
    await block();
    // A rejected change is visited, not undecided, so the review is put on it.
    act(() => setFocusedHunk("d1", "h1"));
    expect(document.querySelector(".hunk--diagram-old")).toBeNull();
    expect(document.querySelector(".hunk--diagram.hunk--add")).toBeNull();
    const marked = document.querySelector<HTMLElement>(".hunk--diagram-marked");
    expect(marked).toHaveClass("hunk--rejected");
    expect(document.querySelector(".hunk--diagram-foot")).toHaveTextContent(
      "Diagram edit rejected",
    );
    // DCR-FR-HBTM: Accept, Discuss, and Compare, and no Reject.
    expect(await screen.findByRole("button", { name: "Compare diagrams" })).toBeInTheDocument();
    const chip = screen.getByTestId("hunk-chip");
    expect(within(chip).getByRole("button", { name: "Accept" })).toBeInTheDocument();
    expect(within(chip).getByRole("button", { name: /Discuss/ })).toBeInTheDocument();
    expect(within(chip).queryByRole("button", { name: "Reject" })).toBeNull();
  });
});

describe("a diagram hunk the document cannot place (DCR-FR-BYSX, DCR-FR-31)", () => {
  it("DCR-FR-BYSX, DCR-FR-31: it is counted and decidable, and the bar says it is not drawn", async () => {
    backend([{ ...CHANGE, before: "```mermaid\nflowchart TD\n  Q --> R\n```" }]);
    renderWorkspace();
    await screen.findByRole("region", { name: "Proposed edits" });
    expect(await screen.findByText(/not shown in the document/i)).toBeInTheDocument();
    expect(document.querySelector("[data-hunk]")).toBeNull();
    expect(screen.getByRole("button", { name: /accept all/i })).toBeEnabled();
    expect(screen.getByRole("button", { name: /reject all/i })).toBeEnabled();
  });
});

describe("the Compare button and the comparison pop-up (DCR-FR-HBTM, DCR-FR-ESIY)", () => {
  it("DCR-FR-HBTM: an undecided diagram hunk's chip holds Accept, Reject, Discuss, and Compare", async () => {
    backend([CHANGE]);
    mount();
    await block();
    const compare = await screen.findByRole("button", { name: "Compare diagrams" });
    expect(compare).toHaveAttribute("title", "Compare diagrams");
    expect(compare).toHaveTextContent("Compare");
    const chip = screen.getByTestId("hunk-chip");
    expect(within(chip).getByRole("button", { name: "Accept" })).toBeInTheDocument();
    expect(within(chip).getByRole("button", { name: "Reject" })).toBeInTheDocument();
    expect(within(chip).getByRole("button", { name: /Discuss/ })).toBeInTheDocument();
  });

  it("DCR-FR-ESIY, EDT-FR-NZLC: Compare opens a modal with the old diagram on the left and the new one on the right", async () => {
    backend([CHANGE]);
    mount();
    await block();
    fireEvent.click(await screen.findByRole("button", { name: "Compare diagrams" }));

    const dialog = screen.getByRole("dialog", { name: "Compare Mermaid diagrams" });
    expect(dialog).toHaveAttribute("aria-modal", "true");
    const left = within(dialog).getByRole("region", { name: "Current diagram" });
    const right = within(dialog).getByRole("region", { name: "Proposed diagram" });
    expect(left).toHaveTextContent("Current");
    expect(right).toHaveTextContent("Proposed");
    expect(await paneSource("Current diagram")).toContain("A --> B");
    expect(await paneSource("Current diagram")).not.toContain("B --> C");
    expect(await paneSource("Proposed diagram")).toContain("B --> C");
    // DCR-FR-OLDZ, EDT-FR-HCWP: the first decision control takes the focus.
    expect(within(dialog).getByRole("button", { name: "Accept" })).toHaveFocus();
  });

  it("DCR-FR-ESIY, EDT-FR-NZLC: an addition has no diagram yet on the left", async () => {
    backend([ADDITION]);
    mount();
    await block();
    fireEvent.click(await screen.findByRole("button", { name: "Compare diagrams" }));
    expect(
      within(screen.getByRole("region", { name: "Current diagram" })).getByText("No diagram yet"),
    ).toBeInTheDocument();
    expect(await paneSource("Proposed diagram")).toContain("X --> Y");
  });

  it("DCR-FR-ESIY, EDT-FR-NZLC: a removal has the diagram removed on the right", async () => {
    backend([REMOVAL]);
    mount();
    await block();
    fireEvent.click(await screen.findByRole("button", { name: "Compare diagrams" }));
    expect(
      within(screen.getByRole("region", { name: "Proposed diagram" })).getByText("Diagram removed"),
    ).toBeInTheDocument();
    expect(await paneSource("Current diagram")).toContain("A --> B");
  });

  it("DCR-FR-ESIY, EDT-FR-JMTD: a source that does not render fails in its own pane, and the other pane is not affected", async () => {
    backend([{ ...CHANGE, after: "```mermaid\nINVALID\n```" }]);
    mount();
    await block();
    fireEvent.click(await screen.findByRole("button", { name: "Compare diagrams" }));
    const right = screen.getByRole("region", { name: "Proposed diagram" });
    expect(await within(right).findByRole("alert")).toHaveTextContent("Diagram error");
    expect(right).toHaveTextContent("INVALID");
    expect(await paneSource("Current diagram")).toContain("A --> B");
  });

  it("DCR-FR-OLDZ, EDT-FR-HCWP, DCR-FR-UKMP: Escape and Close decide nothing, write nothing, and return the focus to Compare", async () => {
    backend([CHANGE]);
    mount();
    await block();
    const compare = await screen.findByRole("button", { name: "Compare diagrams" });
    fireEvent.click(compare);
    screen.getByRole("dialog", { name: "Compare Mermaid diagrams" });

    fireEvent.keyDown(
      within(screen.getByRole("dialog")).getByRole("button", { name: "Accept" }),
      { key: "Escape" },
    );
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(compare).toHaveFocus();

    fireEvent.click(compare);
    fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(compare).toHaveFocus();

    await letWriteLand();
    expect(callsTo("accept_draft_change_hunk")).toEqual([]);
    expect(callsTo("reject_draft_change_hunk")).toEqual([]);
    // DCR-FR-UKMP: view state alone. No write of the draft.
    expect(callsTo("save_draft_file_contents")).toEqual([]);
  });

  it("DCR-FR-OLDZ, DCR-FR-13: Accept in the pop-up accepts that hunk and closes the pop-up", async () => {
    backend([CHANGE]);
    mount();
    await block();
    fireEvent.click(await screen.findByRole("button", { name: "Compare diagrams" }));
    fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Accept" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(callsTo("accept_draft_change_hunk")).toEqual([
      { proposalId: "p1", hunkId: "h1", feedback: null },
    ]);
    expect(callsTo("reject_draft_change_hunk")).toEqual([]);
  });

  it("DCR-FR-OLDZ, DCR-FR-14: Reject in the pop-up rejects that hunk and closes the pop-up", async () => {
    backend([CHANGE]);
    mount();
    await block();
    fireEvent.click(await screen.findByRole("button", { name: "Compare diagrams" }));
    fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Reject" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(callsTo("reject_draft_change_hunk")).toEqual([
      { proposalId: "p1", hunkId: "h1", feedback: null },
    ]);
    expect(callsTo("accept_draft_change_hunk")).toEqual([]);
  });

  it("DCR-FR-OLDZ, DCR-FR-16: a refused decision shows the typed error in the pop-up and keeps it open", async () => {
    backend([CHANGE]);
    refuse = (cmd) => (cmd === "accept_draft_change_hunk" ? "write_failed" : null);
    mount();
    await block();
    fireEvent.click(await screen.findByRole("button", { name: "Compare diagrams" }));
    const dialog = screen.getByRole("dialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Accept" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(/Nothing was changed/i);
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    // The decision can be tried again from the pop-up.
    await waitFor(() => expect(within(dialog).getByRole("button", { name: "Accept" })).toBeEnabled());
  });

  it("DCR-FR-GAFN: a hunk decided elsewhere closes the pop-up", async () => {
    const record = backend([CHANGE]);
    mount();
    await block();
    fireEvent.click(await screen.findByRole("button", { name: "Compare diagrams" }));
    screen.getByRole("dialog");

    act(() =>
      noteProposalChanged({
        draftId: "d1",
        proposal: { ...proposal([CHANGE], "accepted"), id: record.id },
        cause: "decided",
      } as never),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(callsTo("accept_draft_change_hunk")).toEqual([]);
  });

  it("DCR-FR-GAFN: an update that changes the hunk draws the new source in the open pop-up", async () => {
    backend([CHANGE]);
    mount();
    await block();
    fireEvent.click(await screen.findByRole("button", { name: "Compare diagrams" }));
    expect(await paneSource("Proposed diagram")).toContain("B --> C");
    const accept = within(screen.getByRole("dialog")).getByRole("button", { name: "Accept" });
    expect(accept).toHaveFocus();

    const revised: Hunk = {
      ...CHANGE,
      revision: 1,
      after: "```mermaid\nflowchart TD\n  A --> B\n  B --> D\n```",
    };
    backend([revised]);
    act(() =>
      noteProposalChanged({
        draftId: "d1",
        proposal: proposal([revised]),
        cause: "updated",
      } as never),
    );
    await waitFor(async () => expect(await paneSource("Proposed diagram")).toContain("B --> D"));
    // The pop-up stays open and keeps the focus where it was.
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(accept).toHaveFocus();
  });
});

describe("a diagram hunk among the hunks of a proposal (DCR-FR-BYSX)", () => {
  const TEXT: Hunk = {
    id: "h2",
    kind: "replace",
    revision: 0,
    before: TAIL,
    after: "Tail text ends here.",
    anchor: { lead: "", trail: "", hint_start: at(TAIL), hint_end: at(TAIL) + TAIL.length },
  };

  it("DCR-FR-BYSX, DCR-FR-BQNL: it is counted and marked in the gutter by its kind, as a text hunk is", async () => {
    backend([CHANGE, TEXT]);
    mount();
    await block("h1");
    await block("h2");
    expect(screen.getByTestId("review-counter")).toHaveTextContent("reviewing edit 1 of 2");
    const marks = within(screen.getByTestId("hunk-gutter-map")).getAllByRole("button");
    expect(marks).toHaveLength(2);
    expect(marks[0]).toHaveAttribute("data-kind", "replace");
    expect(marks[0]).toHaveAccessibleName(/diagram edit/);
    expect(marks[1]).toHaveAccessibleName(/replacement/);
    // Only the diagram hunk has Compare, and only text is editable.
    expect(document.querySelector(".hunk--add[data-hunk='h2'] .hunk__doc")).not.toBeNull();
    expect(document.querySelector(".hunk--diagram .hunk__doc")).toBeNull();
  });

  it("DCR-FR-BYSX, DCR-FR-CXZG: Accept all decides the diagram hunk as it decides a text hunk", async () => {
    backend([CHANGE, TEXT]);
    mount();
    await block("h1");
    fireEvent.click(await screen.findByRole("button", { name: "Accept all" }));
    await waitFor(() => expect(callsTo("accept_draft_change_hunk")).toHaveLength(2));
    expect(callsTo("accept_draft_change_hunk")).toEqual([
      { proposalId: "p1", hunkId: "h1", feedback: null },
      { proposalId: "p1", hunkId: "h2", feedback: null },
    ]);
  });
});
