/**
 * MPV-FR-UWQL: the plan proposal comments that arrive in the shown thread.
 */
import { describe, expect, it, vi } from "vitest";
import { renderHook } from "@testing-library/react";

const requestPendingFocus = vi.fn();
vi.mock("../../state/discussionFocus", async (original) => ({
  ...(await original<typeof import("../../state/discussionFocus")>()),
  requestPendingFocus: (...args: unknown[]) => requestPendingFocus(...args),
}));
vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
  flushLogs: vi.fn(),
}));

import type { MapPlanReviewStore } from "../../state/mapPlan/reviewStore";
import { mapComment, mapThread } from "../../test/mapPlanFixtures";
import type { Discussion, Participant } from "../../types";
import { showDeliveryKeepingFocus, useProposalArrivals } from "./useProposalArrivals";

const AGENT: Participant = { kind: "agent", agentId: "a1", handle: "arch" } as Participant;

function withProposal(thread: Discussion, id: string, proposalId: string): Discussion {
  const comment = mapComment(id, AGENT, "Here.", { attachments: [{ kind: "mapPlanProposal", proposalId }] });
  return { ...thread, comments: [...thread.comments, comment] };
}

function setup(initial: Discussion | undefined) {
  const arrived = vi.fn();
  const review = { arrived } as unknown as MapPlanReviewStore;
  const onOpen = vi.fn();
  const hook = renderHook(({ thread }) => useProposalArrivals(thread, review, onOpen), {
    initialProps: { thread: initial },
  });
  return { arrived, onOpen, hook };
}

describe("useProposalArrivals (MPV-FR-UWQL)", () => {
  it("MPV-FR-UWQL: a proposal comment that the thread held when it was shown opens nothing", () => {
    const { arrived } = setup(withProposal(mapThread("t1", ["Plan."]), "c9", "p1"));
    expect(arrived).not.toHaveBeenCalled();
  });

  it("MPV-FR-UWQL: a proposal comment that arrives asks the store once, with the open callback", () => {
    const thread = mapThread("t1", ["Plan."]);
    const { arrived, onOpen, hook } = setup(thread);
    const next = withProposal(thread, "c9", "p1");
    hook.rerender({ thread: next });
    hook.rerender({ thread: { ...next } });
    expect(arrived).toHaveBeenCalledTimes(1);
    expect(arrived.mock.calls[0].slice(0, 2)).toEqual(["t1", "p1"]);
    (arrived.mock.calls[0][2] as () => void)();
    expect(onOpen).toHaveBeenCalledTimes(1);
  });

  it("MPV-FR-UWQL: a thread shown again takes what it holds as seen", () => {
    const first = mapThread("t1", ["Plan."]);
    const other = mapThread("t2", ["Other."]);
    const { arrived, hook } = setup(first);
    hook.rerender({ thread: other });
    hook.rerender({ thread: withProposal(first, "c9", "p1") });
    expect(arrived).not.toHaveBeenCalled();
  });

  it("MPV-FR-UWQL: a comment with no proposal asks nothing", () => {
    const thread = mapThread("t1", ["Plan."]);
    const { arrived, hook } = setup(thread);
    hook.rerender({ thread: { ...thread, comments: [...thread.comments, mapComment("c2", AGENT, "Prose.")] } });
    expect(arrived).not.toHaveBeenCalled();
  });
});

describe("showDeliveryKeepingFocus (MPV-FR-KHDP)", () => {
  const COLUMN = '[data-testid="column"]';

  function focusedComposer(): HTMLElement {
    document.body.innerHTML = '<div data-testid="column"><textarea></textarea></div><input id="outside" />';
    const composer = document.querySelector("textarea")!;
    composer.focus();
    return composer;
  }

  it("MPV-FR-KHDP: a lens switch while the focus is in the column asks the new column's composer to take it", () => {
    requestPendingFocus.mockClear();
    focusedComposer();
    const show = vi.fn();
    showDeliveryKeepingFocus(show, true, "t1", COLUMN);
    expect(show).toHaveBeenCalledTimes(1);
    expect(requestPendingFocus).toHaveBeenCalledWith("t1", "composer");
  });

  it("MPV-FR-KHDP: no switch, or a focus outside the column, asks nothing", () => {
    requestPendingFocus.mockClear();
    focusedComposer();
    showDeliveryKeepingFocus(vi.fn(), false, "t1", COLUMN);
    (document.getElementById("outside") as HTMLInputElement).focus();
    showDeliveryKeepingFocus(vi.fn(), true, "t1", COLUMN);
    expect(requestPendingFocus).not.toHaveBeenCalled();
  });
});
