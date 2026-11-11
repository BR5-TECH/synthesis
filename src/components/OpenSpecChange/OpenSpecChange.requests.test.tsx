/**
 * The request card of a change action request
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-QCPB,
 * OCW-FR-CTWP, OCW-FR-GDZT).
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

import { originKind } from "../../state/discussionOrigin";
import { configureRunRoute } from "../../state/graduation/runRoute";
import { makeRun } from "../../test/graduationFixtures";
import { projectAgent } from "../../test/newArtifactFixtures";
import {
  AGENT,
  CHANGE,
  ME,
  STREAM_ID,
  TARGET,
  actionRequest,
  changeBackend,
  drawChange,
  resetChangeWorkspaceStores,
  wireChangeBackend,
  type ChangeBackend,
} from "../../test/changeWorkspaceFixtures";
import type { ChangeActionRequest, ConversationOrigin, Discussion, GraduationRun } from "../../types";

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

function emit(event: string, payload: unknown) {
  for (const cb of listeners.get(event) ?? []) cb({ payload });
}

let backend: ChangeBackend;
const openRun = vi.fn();
/** The comments the request thread holds after a decision. */
let decided: Discussion["comments"];

/** A thread whose agent comment carries the request. */
function requestThread(action: "apply" | "rework" = "apply"): Discussion {
  const at = "2026-01-03T00:00:00Z";
  return {
    id: "t1",
    target: TARGET,
    fragmentTarget: null,
    resolved: false,
    createdAt: at,
    updatedAt: at,
    comments: [
      { id: "t1-c1", author: ME, body: "@arch is it ready?", quotes: [], attachments: [], createdAt: at },
      {
        id: "t1-c9",
        author: AGENT,
        body: "The tasks are ready.",
        quotes: [],
        attachments: [{ kind: "changeActionRequest", requestId: "q1", streamId: STREAM_ID, change: CHANGE, action }],
        createdAt: at,
      },
      ...decided,
    ],
  };
}

function runOf(id: string): GraduationRun {
  return makeRun(id, "working", {
    streamId: STREAM_ID,
    openspec: { phase: "apply", change: CHANGE },
    input: { draftId: "", draftName: "", prompt: "p", promptChecksum: "c", capturedAt: "2026-01-01T00:00:00Z" },
  });
}

beforeEach(() => {
  resetChangeWorkspaceStores();
  listeners.clear();
  openRun.mockReset();
  configureRunRoute(openRun);
  decided = [];
  invokeMock.mockReset();
  backend = changeBackend({ proposals: [], discussions: [requestThread()], requests: [actionRequest()] });
  wireChangeBackend(invokeMock, backend);
  const rest = invokeMock.getMockImplementation()!;
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    if (cmd === "list_project_agents") return [projectAgent("arch")];
    if (cmd === "dispatch_agent_turn") return { turnId: "turn-1" };
    if (cmd === "read_discussion") return { ...requestThread(), comments: requestThread().comments };
    return rest(cmd, args);
  });
});
afterEach(() => {
  configureRunRoute(null);
  cleanup();
  expect(backend.unknown).toEqual([]);
});

/** The arguments of the dispatch at `at`. */
function dispatched(at: number): { nickname: string; triggerCommentId: string; origin: ConversationOrigin } {
  return calls("dispatch_agent_turn")[at][1] as {
    nickname: string;
    triggerCommentId: string;
    origin: ConversationOrigin;
  };
}

async function card(): Promise<HTMLElement> {
  drawChange();
  return screen.findByTestId("change-action-card");
}

/** A decision answers this request, with the decision comment appended. */
function answer(request: ChangeActionRequest, run?: GraduationRun, body = "Confirmed.") {
  return () => {
    backend.requests = [request];
    decided = [
      { id: "t1-c10", author: ME, body, quotes: [], attachments: [], createdAt: "2026-01-04T00:00:00Z" },
    ];
    if (run) backend.runs = [run];
    return { request, run: run ?? null, commentId: "t1-c10", originKind: "change_discussion" as const };
  };
}

describe("the request card (OCW-FR-QCPB)", () => {
  it("OCW-FR-QCPB: a comment with a change_action_request renders a card with the action and its state, from list change action requests", async () => {
    const shown = await card();
    await waitFor(() => expect(shown).toHaveTextContent(`Apply the change ${CHANGE}`));
    expect(shown).toHaveTextContent("Waiting for you");
    expect(calls("list_change_action_requests")[0][1]).toEqual({ streamId: STREAM_ID, change: CHANGE });
    expect(within(shown).getByRole("button", { name: "Confirm" })).toBeInTheDocument();
    expect(within(shown).getByRole("button", { name: "Decline" })).toBeInTheDocument();
    // The rationale stands once, as the agent's message above the card.
    expect(screen.getAllByText("The tasks are ready.")).toHaveLength(1);
  });

  it("OCW-FR-QCPB: the card of a rework renders its instruction as plain text", async () => {
    backend.discussions = [requestThread("rework")];
    backend.requests = [actionRequest({ action: "rework", instruction: "Split <i>task 2</i>." })];
    const shown = await card();
    const instruction = await within(shown).findByTestId("change-action-instruction");
    expect(instruction).toHaveTextContent("Split <i>task 2</i>.");
    expect(instruction.querySelector("i")).toBeNull();
    expect(shown).toHaveTextContent(`Rework the change ${CHANGE}`);
  });

  it("OCW-FR-QCPB: Confirm invokes confirm change action request with the request id", async () => {
    backend.decideAction = answer(actionRequest({ state: "started", runId: "g-1" }), runOf("g-1"));
    const shown = await card();
    await userEvent.click(await within(shown).findByRole("button", { name: "Confirm" }));
    await waitFor(() => expect(calls("confirm_change_action_request")).toHaveLength(1));
    expect(calls("confirm_change_action_request")[0][1]).toEqual({ requestId: "q1" });
  });

  it("OCW-FR-QCPB: Decline invokes decline change action request with the request id and the feedback", async () => {
    backend.decideAction = answer(actionRequest({ state: "declined" }));
    const shown = await card();
    await userEvent.type(await within(shown).findByRole("textbox"), "Not yet.");
    await userEvent.click(within(shown).getByRole("button", { name: "Decline" }));
    await waitFor(() => expect(calls("decline_change_action_request")).toHaveLength(1));
    expect(calls("decline_change_action_request")[0][1]).toEqual({ requestId: "q1", feedback: "Not yet." });
  });
});

describe("the states of the card (OCW-FR-CTWP)", () => {
  it("OCW-FR-CTWP: a started request's card shows the title and the state of its run, and Open run opens it", async () => {
    backend.requests = [actionRequest({ state: "started", runId: "g-1" })];
    backend.runs = [runOf("g-1")];
    const shown = await card();
    const run = await within(shown).findByTestId("change-action-run");
    await waitFor(() => expect(run).toHaveTextContent(`Apply ${CHANGE} · Working`));
    expect(within(shown).queryByRole("button", { name: "Confirm" })).toBeNull();
    await userEvent.click(within(run).getByRole("button", { name: "Open run" }));
    expect(openRun).toHaveBeenCalledWith("g-1");
  });

  it("OCW-FR-CTWP: a declined card states that the author declined it", async () => {
    backend.requests = [actionRequest({ state: "declined" })];
    const shown = await card();
    expect(await within(shown).findByTestId("change-action-declined")).toHaveTextContent("You declined this request.");
    expect(within(shown).queryByRole("button", { name: "Decline" })).toBeNull();
  });

  it("OCW-FR-CTWP: a failed card states its code in words", async () => {
    backend.requests = [actionRequest({ state: "failed", lastRefusal: "change_not_found" })];
    const shown = await card();
    expect(await within(shown).findByTestId("change-action-failed")).toHaveTextContent(
      "The change is no longer on its stream, so no run started.",
    );
  });

  it("OCW-FR-CTWP: a refused confirmation is stated on the card on the terms of OCW-FR-EKYF, and the card still offers Confirm", async () => {
    backend.decideAction = () => ({ reject: "change_not_ready: tasks" });
    const shown = await card();
    await userEvent.click(await within(shown).findByRole("button", { name: "Confirm" }));
    expect(await within(shown).findByTestId("change-action-refusal")).toHaveTextContent(
      "The change is not ready to apply. It is missing artifacts: tasks. No run started.",
    );
    expect(within(shown).getByRole("button", { name: "Confirm" })).toBeEnabled();
    expect(calls("dispatch_agent_turn")).toHaveLength(0);
  });

  it("OCW-FR-CTWP, OCW-FR-EKYF: proposal_pending on the card of an apply request states the proposal to decide or the edits to commit", async () => {
    backend.decideAction = () => ({ reject: "proposal_pending" });
    const shown = await card();
    await userEvent.click(await within(shown).findByRole("button", { name: "Confirm" }));
    expect(await within(shown).findByTestId("change-action-refusal")).toHaveTextContent(
      "The change holds a pending edit proposal, or accepted edits that are not committed. Decide the proposal or commit the edits first. No run started.",
    );
  });

  it("OCW-FR-CTWP, OCW-FR-SCIA: proposal_pending on the card of a rework request states the proposal to decide first", async () => {
    backend.discussions = [requestThread("rework")];
    backend.requests = [actionRequest({ action: "rework", instruction: "Split task 2." })];
    backend.decideAction = () => ({ reject: "proposal_pending" });
    const shown = await card();
    await userEvent.click(await within(shown).findByRole("button", { name: "Confirm" }));
    expect(await within(shown).findByTestId("change-action-refusal")).toHaveTextContent(
      "The change holds a pending edit proposal. Decide it first, then start the rework. No run started.",
    );
  });

  it("OCW-FR-CTWP: the last refused confirmation of a pending request is stated after a read", async () => {
    backend.requests = [actionRequest({ lastRefusal: "openspec_mode_inactive" })];
    const shown = await card();
    const refusal = await within(shown).findByTestId("change-action-refusal");
    expect(refusal).toHaveTextContent("OpenSpec mode is not active for this project");
    expect(within(refusal).getByRole("button", { name: "Open OpenSpec settings" })).toBeInTheDocument();
  });
});

describe("after a decision (OCW-FR-GDZT)", () => {
  it("OCW-FR-GDZT: a confirmation dispatches the agents of the conversation's active set with the decision comment", async () => {
    backend.decideAction = answer(actionRequest({ state: "started", runId: "g-1" }), runOf("g-1"));
    const shown = await card();
    await userEvent.click(await within(shown).findByRole("button", { name: "Confirm" }));
    await waitFor(() => expect(calls("dispatch_agent_turn")).toHaveLength(1));
    const sent = dispatched(0);
    expect(sent.nickname).toBe("arch");
    expect(sent.triggerCommentId).toBe("t1-c10");
    // The outcome's origin_kind, as the dispatch's origin names it.
    expect(originKind(sent.origin)).toBe("change_discussion");
    expect(sent.origin.discussionId).toBe("t1");
    await waitFor(() => expect(within(shown).getByTestId("change-action-run")).toBeInTheDocument());
  });

  it("OCW-FR-GDZT: a decline dispatches the agents of the conversation's active set with the decision comment", async () => {
    backend.decideAction = answer(actionRequest({ state: "declined" }), undefined, "Not yet.");
    const shown = await card();
    await userEvent.type(await within(shown).findByRole("textbox"), "Not yet.");
    await userEvent.click(within(shown).getByRole("button", { name: "Decline" }));
    await waitFor(() => expect(calls("dispatch_agent_turn")).toHaveLength(1));
    const sent = dispatched(0);
    expect(sent.nickname).toBe("arch");
    expect(sent.triggerCommentId).toBe("t1-c10");
    expect(originKind(sent.origin)).toBe("change_discussion");
    expect(sent.origin.discussionId).toBe("t1");
    expect(await within(shown).findByTestId("change-action-declined")).toBeInTheDocument();
  });

  it("OCW-FR-GDZT: the tab reads the requests again on change action requests changed for its stream and change alone", async () => {
    await card();
    await waitFor(() => expect(calls("list_change_action_requests").length).toBeGreaterThan(0));
    const before = calls("list_change_action_requests").length;
    act(() => emit("change-action-requests-changed", { streamId: "s-9", change: CHANGE, requestId: "q1", cause: "decided" }));
    act(() => emit("change-action-requests-changed", { streamId: STREAM_ID, change: "other", requestId: "q1", cause: "decided" }));
    expect(calls("list_change_action_requests")).toHaveLength(before);
    backend.requests = [actionRequest({ state: "declined" })];
    act(() => emit("change-action-requests-changed", { streamId: STREAM_ID, change: CHANGE, requestId: "q1", cause: "decided" }));
    expect(await screen.findByTestId("change-action-declined")).toBeInTheDocument();
    expect(calls("list_change_action_requests")).toHaveLength(before + 1);
  });
});
