/**
 * The plan update of an apply run in the change workspace
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-QSZP,
 * OCW-FR-YRZK, OCW-FR-ZPDC, OCW-FR-XJMR, OCW-FR-KZDG;
 * `../../../specifications/ui/DQA-discussion-question-answering.md`
 * DQA-FR-XFEE).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
const listeners = new Map<string, Set<(e: { payload: unknown }) => void>>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, cb: (e: { payload: unknown }) => void) => {
    const set = listeners.get(event) ?? new Set();
    set.add(cb);
    listeners.set(event, set);
    return () => set.delete(cb);
  }),
}));

/**
 * PCR chips as a stand-in, as in `OpenSpecChange.review.test.tsx`: the real
 * chip is placed from the rendered box of its hunk, and jsdom renders no box.
 */
vi.mock("../Editor/ReviewOverlay", async () => {
  const { createElement } = await import("react");
  type Review = import("../Editor/props").EditorReview;
  return {
    ReviewOverlay: ({ review }: { review: Review | undefined }) => {
      if (review === undefined || review.focused === null) return null;
      if (!review.hunks.some((h) => h.id === review.focused)) return null;
      const id = review.focused;
      return createElement(
        "div",
        { role: "group", "aria-label": `Actions for ${id}` },
        createElement("button", { type: "button", disabled: review.busy, onClick: () => review.onAccept(id) }, "Accept"),
        createElement("button", { type: "button", disabled: review.busy, onClick: () => review.onReject(id) }, "Reject"),
        createElement("button", { type: "button", disabled: review.busy, onClick: () => review.onDiscuss(id) }, "Discuss"),
      );
    },
  };
});

import { flushLogs } from "../../logging";
import { configureRunRoute } from "../../state/graduation/runRoute";
import { rereadChangeRunQueue } from "../../state/openspec/changeRunQueue";
import { publishSet } from "../../state/questionSets";
import { projectAgent } from "../../test/editorAgentsFixtures";
import { makeRun, type RunOverrides } from "../../test/graduationFixtures";
import {
  CHANGE,
  STREAM_ID,
  TARGET,
  changeBackend,
  changeDiscussions,
  changeProposal,
  drawChange,
  resetChangeWorkspaceStores,
  wireChangeBackend,
  type ChangeBackend,
} from "../../test/changeWorkspaceFixtures";
import type {
  Discussion,
  GraduationRest,
  GraduationRun,
  PendingQuestionSet,
} from "../../types";

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

const AT = "2026-01-02T00:00:00Z";
const SYNTHESIS = { kind: "agent", agentId: "synthesis", handle: "Synthesis", title: "" } as const;
const PLAN_THREAD = "pt-1";
const SET_ID = "qs-plan";

/** OPU-FR-QXUE: the plan thread, with the reason and the plan proposal comment. */
function planThread(): Discussion {
  return {
    id: PLAN_THREAD,
    target: TARGET,
    fragmentTarget: null,
    resolved: false,
    createdAt: AT,
    updatedAt: AT,
    comments: [
      { id: "pt-1-c1", author: SYNTHESIS, body: "The plan names no header.", quotes: [], attachments: [], createdAt: AT },
      {
        id: "pt-1-c2",
        author: SYNTHESIS,
        body: "The plan names no header.",
        quotes: [],
        attachments: [{ kind: "changeProposal", proposalId: "p1", streamId: STREAM_ID, change: CHANGE }],
        createdAt: AT,
      },
    ],
  };
}

/** OPU-FR-VXVO: the set of the plan update. The second question has no option. */
function planSet(): PendingQuestionSet {
  return {
    setId: SET_ID,
    discussionId: PLAN_THREAD,
    askedBy: SYNTHESIS,
    askedAt: AT,
    questions: [
      {
        position: 1,
        text: "Where does the entry stand?",
        options: [
          { position: 1, value: "In the shared header" },
          { position: 2, value: "On every page" },
        ],
      },
      { position: 2, text: "What does the notice say?", options: [] },
    ],
  };
}

const UPDATE: GraduationRest = {
  reason: "plan_update",
  at: AT,
  origin: "work",
  threadId: PLAN_THREAD,
  proposalId: "p1",
  setId: SET_ID,
  planReason: "The plan names no header.",
};

/** An apply run of the change, at rest on `rest`. */
function restingRun(rest: GraduationRest, over: RunOverrides = {}): GraduationRun {
  return makeRun("g-1", "awaiting_author", {
    streamId: STREAM_ID,
    streamName: "auth-work",
    input: { draftId: "", draftName: "", prompt: "p", promptChecksum: "c", capturedAt: AT },
    openspec: { phase: "apply", change: CHANGE },
    rest,
    ...over,
  });
}

let backend: ChangeBackend;

beforeEach(() => {
  resetChangeWorkspaceStores();
  listeners.clear();
  configureRunRoute(() => {});
  invokeMock.mockReset();
  backend = changeBackend({
    proposals: [changeProposal(["pending", "pending"], { threadId: PLAN_THREAD, commentId: "pt-1-c2" })],
    discussions: [...changeDiscussions(), planThread()],
    runs: [restingRun(UPDATE)],
    questionSets: { [PLAN_THREAD]: planSet() },
  });
  wireChangeBackend(invokeMock, backend);
});
afterEach(() => {
  configureRunRoute(null);
  cleanup();
  expect(backend.unknown).toEqual([]);
});

const strip = () => screen.getByTestId("openspec-run-strip");
const fileArticle = (path: string) =>
  screen.getAllByTestId("openspec-file").find((a) => a.getAttribute("data-path") === path)!;

async function openedStrip() {
  drawChange({ file: "proposal.md" });
  await screen.findByTestId("openspec-run-strip");
  return strip();
}

/** The plan thread is the third discussion of the change (DiscussionColumn's tab title). */
async function planTab() {
  const column = await screen.findByTestId("openspec-change-discussion");
  await waitFor(() => expect(within(column).getAllByRole("tab")).toHaveLength(3));
  return within(column).getByTitle("Discussion 3");
}

describe("the strip of a plan rest (OCW-FR-YRZK, OCW-FR-ZPDC)", () => {
  it.each([
    ["plan_update", UPDATE, "waiting for plan decisions", true],
    [
      "plan_findings",
      { reason: "plan_findings", at: AT, origin: "review", threadId: PLAN_THREAD, findings: [] } as GraduationRest,
      "plan findings to settle",
      true,
    ],
    [
      "plan_update_invalid",
      { reason: "plan_update_invalid", at: AT, cause: "plan_update_empty" } as GraduationRest,
      "plan update refused",
      false,
    ],
  ])("OCW-FR-YRZK: a rest on %s states its words, and Show thread stands where the rest has a plan thread", async (_reason, rest, words, thread) => {
    backend.runs = [restingRun(rest)];
    const shown = await openedStrip();
    expect(within(shown).getByTestId("openspec-run-strip-state")).toHaveTextContent(words);
    expect(within(shown).queryByRole("button", { name: "Show thread" }) !== null).toBe(thread);
    // OCW-FR-ZPDC: every plan rest offers Continue.
    expect(within(shown).getByRole("button", { name: "Continue" })).toBeInTheDocument();
  });

  it("OCW-FR-YRZK: Show thread selects the plan thread in the discussion column", async () => {
    const shown = await openedStrip();
    const tab = await planTab();
    expect(tab).toHaveAttribute("aria-selected", "false");
    await userEvent.click(within(shown).getByRole("button", { name: "Show thread" }));
    await waitFor(() => expect(tab).toHaveAttribute("aria-selected", "true"));
  });

  it("OCW-FR-ZPDC: Continue invokes continue_graduation_run with the run", async () => {
    backend.continueRun = () => restingRun(UPDATE, { state: "working", rest: null });
    const shown = await openedStrip();
    const lists = calls("list_graduation_queue").length;
    await userEvent.click(within(shown).getByRole("button", { name: "Continue" }));
    await waitFor(() => expect(calls("continue_graduation_run")).toHaveLength(1));
    expect(calls("continue_graduation_run")[0][1]).toEqual({ runId: "g-1" });
    // The runs are read again, so the strip follows the run.
    await waitFor(() => expect(calls("list_graduation_queue").length).toBeGreaterThan(lists));
    expect(screen.queryByTestId("openspec-run-strip-refusal")).toBeNull();
  });

  it("OCW-FR-ZPDC: a second press of Continue while the first is in flight invokes nothing more", async () => {
    let release: (run: GraduationRun) => void = () => {};
    const wired = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) =>
      cmd === "continue_graduation_run"
        ? new Promise<GraduationRun>((resolve) => (release = resolve))
        : wired(cmd, args),
    );
    const shown = await openedStrip();
    const button = within(shown).getByRole("button", { name: "Continue" });
    await userEvent.click(button);
    await waitFor(() => expect(button).toHaveAttribute("aria-disabled", "true"));
    await userEvent.click(button);
    expect(calls("continue_graduation_run")).toHaveLength(1);
    release(restingRun(UPDATE));
    await waitFor(() => expect(button).not.toHaveAttribute("aria-disabled"));
    expect(calls("continue_graduation_run")).toHaveLength(1);
    expect(screen.queryByTestId("openspec-run-strip-refusal")).toBeNull();
  });

  it("OCW-FR-ZPDC: a Continue refused by keyboard keeps the focus on Continue", async () => {
    backend.continueRun = () => ({ reject: "proposal_pending" });
    const shown = await openedStrip();
    const button = within(shown).getByRole("button", { name: "Continue" });
    button.focus();
    await userEvent.keyboard("{Enter}");
    const refusal = await within(shown).findByTestId("openspec-run-strip-refusal");
    expect(refusal.textContent).toBe(
      "Decide the plan edits in the change discussion first, or their commit is still owed. Then continue the run.",
    );
    expect(button).not.toHaveAttribute("aria-disabled");
    expect(document.activeElement).toBe(button);
  });

  it("OCW-FR-ZPDC, GRU-FR-BPNJ: a refused Continue is stated in the strip in words, and the rest stays", async () => {
    backend.continueRun = () => ({ reject: "plan_questions_pending" });
    const shown = await openedStrip();
    await userEvent.click(within(shown).getByRole("button", { name: "Continue" }));
    const refusal = await within(shown).findByTestId("openspec-run-strip-refusal");
    expect(refusal).toHaveAttribute("role", "alert");
    expect(refusal).toHaveTextContent(
      "Answer the questions in the change discussion first, then continue the run.",
    );
    expect(within(shown).getByTestId("openspec-run-strip-state")).toHaveTextContent(
      "waiting for plan decisions",
    );
  });
});

describe("a refused Continue that is an OpenSpec failure (OCW-FR-ZPDC, GRU-FR-BPNJ)", () => {
  /** The OFN-FR-LKAT records sent so far. */
  const noticeRecords = () => {
    flushLogs();
    return calls("append_log_records")
      .flatMap((c) => (c[1] as { records: Array<{ message: string; fields: Record<string, unknown> }> }).records)
      .filter((r) => r.message === "an openspec failure notice appeared");
  };

  it("OCW-FR-ZPDC, GRU-FR-BPNJ, OFN-FR-QZEC: a wrapper with a cause and a detail states the CLI state of its cause, not the validation", async () => {
    backend.continueRun = () => ({
      reject: {
        code: "openspec_validation_unavailable",
        cause: "cli_unavailable",
        detail: "failed",
        diagnostics: [],
      },
    });
    const shown = await openedStrip();
    await userEvent.click(within(shown).getByRole("button", { name: "Continue" }));
    const refusal = await within(shown).findByTestId("openspec-run-strip-refusal");
    expect(refusal.textContent).toBe(
      "The OpenSpec CLI on this machine does not start or does not report its version. The run did not continue.",
    );
    expect(refusal).not.toHaveTextContent("validation could not run");
  });

  it("OFN-FR-LKAT: the short form of a refused Continue emits one WARN record with the surface, the code, the cause, and the class", async () => {
    backend.continueRun = () => ({
      reject: { code: "openspec_validation_unavailable", cause: "cli_timed_out", change: CHANGE, diagnostics: [{ message: "secret" }] },
    });
    const shown = await openedStrip();
    await userEvent.click(within(shown).getByRole("button", { name: "Continue" }));
    await within(shown).findByTestId("openspec-run-strip-refusal");
    const records = noticeRecords();
    expect(records).toHaveLength(1);
    expect(records[0].fields).toEqual({
      surface: "change_run_strip_continue",
      code: "openspec_validation_unavailable",
      cause: "cli_timed_out",
      class: "transient",
    });
    expect(JSON.stringify(records)).not.toContain("secret");
  });

  it("OFN-FR-LKAT, GRU-FR-LBPR: a blocked run states the short form of its OpenSpec blocker, and the strip logs it once", async () => {
    backend.runs = [
      restingRun(UPDATE, {
        state: "blocked",
        rest: null,
        blocker: { code: "cli_timed_out", message: "x", clearsBy: "", attempt: 1, diagnostics: [{ message: "secret" }] },
      }),
    ];
    const shown = await openedStrip();
    await waitFor(() =>
      expect(shown).toHaveTextContent("The OpenSpec CLI did not finish in time, also after one more try."),
    );
    await waitFor(() => expect(noticeRecords()).toHaveLength(1));
    // A read again gives a new run object with the same blocker: the same notice.
    const reads = calls("list_graduation_queue").length;
    await act(async () => {
      await rereadChangeRunQueue();
    });
    expect(calls("list_graduation_queue").length).toBeGreaterThan(reads);
    const records = noticeRecords();
    expect(records).toHaveLength(1);
    expect(records[0].fields).toEqual({
      surface: "change_run_strip_blocker",
      code: "cli_timed_out",
      cause: "",
      class: "transient",
    });
  });

  it("OCW-FR-ZPDC: a typed refusal that is not an OpenSpec failure keeps its words and emits no notice record", async () => {
    backend.continueRun = () => ({ reject: "plan_questions_pending" });
    const shown = await openedStrip();
    await userEvent.click(within(shown).getByRole("button", { name: "Continue" }));
    await within(shown).findByTestId("openspec-run-strip-refusal");
    expect(noticeRecords()).toHaveLength(0);
  });
});

describe("the plan proposal while the plan questions stand (OCW-FR-XJMR)", () => {
  it("OCW-FR-XJMR: the chips and Reject all are unavailable, the review bar says why, and the proposed text takes no edit", async () => {
    drawChange({ file: "proposal.md" });
    const chip = await screen.findByRole("group", { name: "Actions for h1" });
    const shown = await openedStripAfterDraw();
    await waitFor(() =>
      expect(within(shown).getByTestId("openspec-run-strip-state")).toHaveTextContent(
        "waiting for plan decisions",
      ),
    );
    await waitFor(() =>
      expect(within(chip).getByRole("button", { name: "Accept" })).toBeDisabled(),
    );
    expect(within(chip).getByRole("button", { name: "Reject" })).toBeDisabled();
    expect(within(chip).getByRole("button", { name: "Discuss" })).toBeDisabled();
    const bar = screen.getByTestId("openspec-review-bar");
    expect(within(bar).getByTestId("openspec-review-held")).toHaveTextContent(
      "Answer the questions in the plan thread first.",
    );
    expect(within(bar).getByRole("button", { name: "Reject all" })).toBeDisabled();
    // The held sentence follows the count, and Reject all keeps its place at the end.
    const order = [
      within(bar).getByTestId("openspec-review-count"),
      within(bar).getByTestId("openspec-review-held"),
      within(bar).getByRole("button", { name: "Reject all" }),
    ];
    expect(order[0].compareDocumentPosition(order[1]) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(order[1].compareDocumentPosition(order[2]) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    // The edit of a hunk is the field of its proposed text, which is not offered.
    expect(
      within(fileArticle("proposal.md")).queryByRole("textbox", { name: /Proposed text/ }),
    ).toBeNull();
    expect(calls("accept_change_edit_hunk")).toHaveLength(0);
  });

  it("OCW-FR-XJMR: once the set is gone, the chips, the edit, and Reject all are available again", async () => {
    drawChange({ file: "proposal.md" });
    const chip = await screen.findByRole("group", { name: "Actions for h1" });
    await waitFor(() => expect(within(chip).getByRole("button", { name: "Accept" })).toBeDisabled());
    // CMS-FR-BQEN: the set was submitted, so the thread holds none.
    publishSet(PLAN_THREAD, null);
    await waitFor(() => expect(within(chip).getByRole("button", { name: "Accept" })).toBeEnabled());
    expect(screen.queryByTestId("openspec-review-held")).toBeNull();
    expect(within(screen.getByTestId("openspec-review-bar")).getByRole("button", { name: "Reject all" })).toBeEnabled();
    expect(
      within(fileArticle("proposal.md")).getByRole("textbox", { name: /Proposed text/ }),
    ).toBeInTheDocument();
  });

  it("OCW-FR-XJMR: a proposal that no plan rest names keeps its chips while another thread holds a set", async () => {
    backend.runs = [restingRun({ ...UPDATE, proposalId: "p-other" })];
    drawChange({ file: "proposal.md" });
    const chip = await screen.findByRole("group", { name: "Actions for h1" });
    // The tab knows the run only once the strip states its plan rest.
    const shown = await openedStripAfterDraw();
    await waitFor(() =>
      expect(within(shown).getByTestId("openspec-run-strip-state")).toHaveTextContent(
        "waiting for plan decisions",
      ),
    );
    expect(within(chip).getByRole("button", { name: "Accept" })).toBeEnabled();
    expect(screen.queryByTestId("openspec-review-held")).toBeNull();
  });

  it("OCW-FR-XJMR: a decision refused with plan_questions_pending is stated at the hunk", async () => {
    // The tab does not know the run, so it offers the chips; the backend refuses.
    backend.runs = [];
    backend.decide = () => ({ reject: "plan_questions_pending" });
    drawChange({ file: "proposal.md" });
    const chip = await screen.findByRole("group", { name: "Actions for h1" });
    await userEvent.click(within(chip).getByRole("button", { name: "Accept" }));
    const refusal = await within(fileArticle("proposal.md")).findByTestId("openspec-hunk-refusal");
    expect(refusal).toHaveTextContent("Answer the questions in the plan thread first. Nothing was changed.");
  });
});

describe("the plan thread (OCW-FR-QSZP, OCW-FR-KZDG, DQA-FR-XFEE)", () => {
  it("OCW-FR-QSZP, DQA-FR-XFEE: the plan thread shows the set of Synthesis, and Submit invokes the submission alone", async () => {
    drawChange({ file: "proposal.md" });
    await userEvent.click(within(await openedStripAfterDraw()).getByRole("button", { name: "Show thread" }));
    const block = await screen.findByLabelText("Questions from Synthesis");
    await userEvent.click(within(block).getAllByRole("radio")[0]);
    await userEvent.click(within(block).getByTestId("discussion-questions-next"));
    await userEvent.type(
      within(block).getByRole("textbox", { name: "Your own answer to question 2 of 2" }),
      "Your session ended.",
    );
    await userEvent.click(within(block).getByTestId("discussion-questions-submit"));
    await waitFor(() => expect(calls("submit_discussion_question_answers")).toHaveLength(1));
    expect(calls("submit_discussion_question_answers")[0][1]).toEqual({
      discussionId: PLAN_THREAD,
      setId: SET_ID,
      answers: [
        { questionPosition: 1, optionPosition: 1, optionValue: "In the shared header" },
        { questionPosition: 2, ownAnswer: "Your session ended." },
      ],
    });
    // The block invokes no operation of the run.
    expect(calls("continue_graduation_run")).toHaveLength(0);
    expect(calls("answer_graduation_escalation")).toHaveLength(0);
  });

  it("OPU-FR-GYHC, DQA-FR-CIRK: a plan thread that addresses no agent dispatches no turn on its submission", async () => {
    // An enrolled agent that the plan thread does not address.
    const wired = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) =>
      cmd === "list_project_agents" ? Promise.resolve([projectAgent("arch", "a-1")]) : wired(cmd, args),
    );
    drawChange({ file: "proposal.md" });
    await userEvent.click(within(await openedStripAfterDraw()).getByRole("button", { name: "Show thread" }));
    const block = await screen.findByLabelText("Questions from Synthesis");
    await userEvent.click(within(block).getAllByRole("radio")[0]);
    await userEvent.click(within(block).getByTestId("discussion-questions-next"));
    await userEvent.type(
      within(block).getByRole("textbox", { name: "Your own answer to question 2 of 2" }),
      "Your session ended.",
    );
    await userEvent.click(within(block).getByTestId("discussion-questions-submit"));
    await waitFor(() => expect(calls("submit_discussion_question_answers")).toHaveLength(1));
    // The submission has settled once it records that it reached no agent.
    await waitFor(() => {
      flushLogs();
      const messages = calls("append_log_records").flatMap((c) =>
        (c[1] as { records: { message: string; fields: Record<string, unknown> }[] }).records,
      );
      expect(messages).toContainEqual(
        expect.objectContaining({
          message: "a question submission reached no agent",
          fields: expect.objectContaining({ threadId: PLAN_THREAD, enrolled: 1 }),
        }),
      );
    });
    expect(calls("dispatch_agent_turn")).toHaveLength(0);
    expect(calls("dispatch_agent_group")).toHaveLength(0);
  });

  it("OCW-FR-KZDG: once the plan thread holds no set, one hint line stands under the pending plan proposal", async () => {
    backend.questionSets = {};
    drawChange({ file: "proposal.md" });
    await userEvent.click(within(await openedStripAfterDraw()).getByRole("button", { name: "Show thread" }));
    const hint = await screen.findByTestId("dds-plan-hint");
    expect(hint).toHaveTextContent("Decide these edits with the chips in the change, or by voice or comment");
    expect(hint).toHaveTextContent("“@arch accept the first two edits”");
    expect(screen.getAllByTestId("dds-plan-hint")).toHaveLength(1);
  });

  it("OCW-FR-KZDG: while the set stands, the plan thread shows no hint", async () => {
    drawChange({ file: "proposal.md" });
    await userEvent.click(within(await openedStripAfterDraw()).getByRole("button", { name: "Show thread" }));
    await screen.findByLabelText("Questions from Synthesis");
    expect(screen.queryByTestId("dds-plan-hint")).toBeNull();
  });

  it("OCW-FR-KZDG: a pending proposal that is not a plan proposal shows no hint", async () => {
    backend.runs = [];
    backend.questionSets = {};
    drawChange({ file: "proposal.md" });
    const column = await screen.findByTestId("openspec-change-discussion");
    await waitFor(() => expect(within(column).getAllByRole("tab")).toHaveLength(3));
    await userEvent.click(within(column).getByTitle("Discussion 3"));
    await within(column).findAllByTestId("dds-change-row");
    expect(screen.queryByTestId("dds-plan-hint")).toBeNull();
  });
});

/** The strip, after `drawChange` was called by the test itself. */
async function openedStripAfterDraw() {
  await screen.findByTestId("openspec-run-strip");
  return strip();
}
