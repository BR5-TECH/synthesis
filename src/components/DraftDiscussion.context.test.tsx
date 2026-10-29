/**
 * The context indicator and the size warning in the discussion column's header
 * (`../../specifications/ui/DDS-draft-discussion.md` DDS-FR-CTXI … DDS-FR-HJMF).
 *
 * The figures come from two places: the estimate the column reads from the
 * backend, and the figures a turn event carries. Both are driven here through
 * the mocked `invoke` and through the store the shell's turn listener feeds.
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

import { resetDraftDiscussions } from "../state/draftDiscussion";
import { resetLayoutPreferencesCache } from "../state/layoutPreferences";
import {
  clearAllDiscussionSessions,
  followAgentTurn,
  patchDiscussionSession,
} from "../state/discussionSession";
import { resetTurnContextsForTest } from "../state/turnContexts";
import { flushLogs, resetLogBufferForTest } from "../logging";
import { otherCommand } from "../test/invokeMocks";
import {
  DRAFT,
  columnElement,
  discussionSaying,
  type ColumnProps,
} from "../test/draftDiscussionFixtures";
import { projectAgent } from "../test/editorAgentsFixtures";
import type { AgentTurn, DiscussionContext, TurnContext } from "../types";

const SPLIT = {
  instructions: 2100,
  document: 60000,
  history: 9400,
  currentComment: 120,
  toolResults: 12500,
};

function estimate(over: Partial<DiscussionContext> = {}): DiscussionContext {
  return {
    window: 200_000,
    estimatedTokens: 50_000,
    split: SPLIT,
    document: { lines: 900, characters: 240_000, estimatedTokens: 60_000 },
    limitTokens: 60_000,
    aboveLimit: false,
    ...over,
  };
}

function reported(over: Partial<TurnContext> = {}): TurnContext {
  return {
    window: 200_000,
    inputTokens: 84_210,
    split: SPLIT,
    document: { lines: 900, characters: 240_000, estimatedTokens: 60_000 },
    limitTokens: 60_000,
    aboveLimit: false,
    ...over,
  };
}

function turnWith(context: TurnContext | null, over: Partial<AgentTurn> = {}): AgentTurn {
  return {
    id: "turn-1",
    agentId: "agent-b",
    nickname: "olaf",
    origin: { discussionId: "disc-1", target: { kind: "draft", draftId: DRAFT } },
    triggerCommentId: "disc-1-c0",
    state: "delivered",
    failure: null,
    retryPermitted: false,
    startedAt: "2026-01-01T00:00:00Z",
    endedAt: "2026-01-01T00:00:05Z",
    activeToolCalls: [],
    imagesOmitted: false,
    context,
    ...over,
  };
}

/** The estimate the mock answers, changeable per test. */
let answer: DiscussionContext = estimate();

function routed(cmd: string): Promise<unknown> {
  if (cmd === "estimate_discussion_context") return Promise.resolve(answer);
  return otherCommand(cmd);
}

const estimateCalls = () =>
  invokeMock.mock.calls.filter((c) => c[0] === "estimate_discussion_context");

const AGENTS = [projectAgent("arch", "agent-a"), projectAgent("olaf", "agent-b")];

/** The messages the column logged, as they reached the backend. */
const loggedMessages = () =>
  invokeMock.mock.calls
    .filter((c) => c[0] === "append_log_records")
    .flatMap((c) => (c[1] as { records: { message: string }[] }).records.map((r) => r.message));

/** Wait until a failed estimate was handled, which logs a warning. */
async function estimateFailureHandled(times = 1) {
  await waitFor(() => {
    flushLogs();
    expect(
      loggedMessages().filter((m) => m === "the discussion context estimate failed"),
    ).toHaveLength(times);
  });
}

function renderColumn(over: Partial<ColumnProps> = {}) {
  return render(
    columnElement({ agents: AGENTS, selectedThreadId: "disc-1", ...over }),
  );
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => routed(cmd));
  answer = estimate();
  resetDraftDiscussions();
  resetLayoutPreferencesCache();
  resetLogBufferForTest();
  resetTurnContextsForTest();
  clearAllDiscussionSessions();
});

afterEach(cleanup);

describe("the context indicator in the column header", () => {
  it("DDS-FR-CTXI: stands after the follow state and directly before the discussion menu", async () => {
    renderColumn();
    const indicator = await screen.findByTestId("context-indicator");
    const head = indicator.closest("header")!;
    const group = screen.getByTestId("context-indicator-group");
    const order = Array.from(head.children);
    expect(order.indexOf(group)).toBe(
      order.indexOf(screen.getByTestId("discussion-header-actions")) - 1,
    );
    expect(order.indexOf(screen.getByTestId("discussion-follow-state"))).toBeLessThan(
      order.indexOf(group),
    );
  });

  it("DDS-FR-CTXI: displaces neither the resolved state nor the discussion menu", async () => {
    renderColumn({ discussions: [{ ...discussionSaying("disc-1", ["x"]), resolved: true }] });
    const group = await screen.findByTestId("context-indicator-group");
    const order = Array.from(group.closest("header")!.children);
    const resolved = screen.getByTestId("discussion-resolved-state");
    expect(resolved).toHaveTextContent("resolved");
    expect(order.indexOf(resolved)).toBe(order.indexOf(group) - 1);
    expect(order.indexOf(group)).toBe(
      order.indexOf(screen.getByTestId("discussion-header-actions")) - 1,
    );
  });

  it("DDS-FR-CTXI: stands in the Editor tab's column too, for a file", async () => {
    renderColumn({
      target: { kind: "artifact", artifactId: "specs/a.md" },
      testId: "editor-discussion-column",
      discussions: [
        { ...discussionSaying("disc-1", ["x"]), target: { kind: "artifact", artifactId: "specs/a.md" } },
      ],
    });
    const column = screen.getByTestId("editor-discussion-column");
    expect(await within(column).findByTestId("context-indicator")).toHaveTextContent("~25%");
    expect(estimateCalls()[0][1]).toMatchObject({
      target: { kind: "artifact", artifactId: "specs/a.md" },
    });
  });

  it("DDS-FR-KXWQ, DDS-FR-TSPL: before any turn shows the estimate, marked, with the first agent's window", async () => {
    renderColumn();
    const indicator = await screen.findByTestId("context-indicator");
    // 50,000 of 200,000 is 25%; `~` marks an estimate.
    expect(indicator).toHaveTextContent("~25%");
    expect(indicator).toHaveAttribute("data-estimate", "true");
    expect(estimateCalls()[0][1]).toEqual({
      target: { kind: "draft", draftId: DRAFT },
      discussionId: "disc-1",
      agentId: "agent-a",
    });
  });

  it("DDS-FR-KXWQ: a column with no discussion on screen estimates with no discussion", async () => {
    renderColumn({ discussions: [] });
    await screen.findByTestId("context-indicator");
    expect(estimateCalls()[0][1]).toMatchObject({ discussionId: null });
  });

  it("DDS-FR-KXWQ: shows the provider-reported input of the latest turn, not an estimate", async () => {
    renderColumn();
    await screen.findByText("~25%");
    patchDiscussionSession("disc-1", { atTail: true });
    act(() => followAgentTurn(turnWith(reported())));
    const indicator = screen.getByTestId("context-indicator");
    // 84,210 of 200,000 is 42%.
    expect(indicator).toHaveTextContent("42%");
    expect(indicator).not.toHaveTextContent("~");
    expect(indicator).not.toHaveAttribute("data-estimate");
  });

  it("DDS-FR-KXWQ: keeps a delivered turn's figures after the turn ends", async () => {
    // The turn's events arrive before the column mounts, as for a column
    // opened after the answer came.
    act(() => followAgentTurn(turnWith(reported({ inputTokens: 100_000 }))));
    renderColumn();
    expect(await screen.findByTestId("context-indicator")).toHaveTextContent("50%");
  });

  it("DDS-FR-KXWQ: an older turn's late event does not replace the latest turn's figures", async () => {
    renderColumn();
    act(() =>
      followAgentTurn(
        turnWith(reported({ inputTokens: 100_000 }), {
          id: "turn-2",
          startedAt: "2026-01-02T00:00:00Z",
        }),
      ),
    );
    act(() =>
      followAgentTurn(
        turnWith(reported({ inputTokens: 20_000 }), {
          id: "turn-1",
          startedAt: "2026-01-01T00:00:00Z",
        }),
      ),
    );
    expect(await screen.findByTestId("context-indicator")).toHaveTextContent("50%");
  });

  it("DDS-FR-TSPL: uses the window the latest turn reported, which is its agent's", async () => {
    renderColumn();
    act(() => followAgentTurn(turnWith(reported({ window: 100_000, inputTokens: 25_000 }))));
    expect(await screen.findByTestId("context-indicator")).toHaveTextContent("25%");
  });

  it("DDS-FR-TSPL: with no known window shows the token count, no percent, and no warning", async () => {
    answer = estimate({ window: null, limitTokens: null, aboveLimit: true });
    renderColumn();
    const indicator = await screen.findByTestId("context-indicator");
    expect(indicator).toHaveTextContent("~50k tokens");
    expect(indicator).not.toHaveTextContent("%");
    expect(indicator).toHaveAttribute("role", "img");
    expect(screen.queryByTestId("context-warning")).toBeNull();
  });

  it("DDS-FR-WRNB: shows the split on keyboard focus, each part an estimate, with the window", async () => {
    renderColumn();
    const indicator = await screen.findByTestId("context-indicator");
    act(() => indicator.focus());
    const tip = screen.getByTestId("context-indicator-tip");
    for (const label of [
      "Instructions",
      "Document",
      "Discussion history",
      "Current comment",
      "Tool results",
    ]) {
      expect(tip).toHaveTextContent(label);
    }
    expect(tip).toHaveTextContent("~60,000 est.");
    expect(tip).toHaveTextContent("~12,500 est.");
    expect(tip).toHaveTextContent("Window 200,000 tokens.");
    expect(indicator).toHaveAttribute("aria-describedby");
  });

  it("DDS-FR-WRNB: shows the split on hover", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      renderColumn();
      const indicator = await screen.findByTestId("context-indicator");
      fireEvent.pointerEnter(indicator.parentElement!);
      act(() => {
        vi.advanceTimersByTime(500);
      });
      expect(screen.getByTestId("context-indicator-tip")).toBeInTheDocument();
      fireEvent.pointerLeave(indicator.parentElement!);
      act(() => {
        vi.advanceTimersByTime(500);
      });
      expect(screen.queryByTestId("context-indicator-tip")).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  it("DDS-FR-WRNB, DDS-FR-ZQVE: neither tooltip opens over the open discussion menu", async () => {
    answer = estimate({ aboveLimit: true });
    renderColumn();
    const indicator = await screen.findByTestId("context-indicator");
    const warning = screen.getByTestId("context-warning");
    const trigger = within(screen.getByTestId("discussion-header-actions")).getByRole("button", {
      name: "Thread actions for disc-1",
    });
    fireEvent.click(trigger);
    expect(trigger).toHaveAttribute("aria-expanded", "true");
    fireEvent.focus(indicator);
    expect(screen.queryByTestId("context-indicator-tip")).toBeNull();
    fireEvent.focus(warning);
    expect(screen.queryByTestId("context-warning-tip")).toBeNull();
    // With the menu closed, the tooltip opens again.
    fireEvent.blur(warning);
    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() => expect(trigger).toHaveAttribute("aria-expanded", "false"));
    fireEvent.focus(indicator);
    expect(screen.getByTestId("context-indicator-tip")).toBeInTheDocument();
  });

  it("DDS-FR-WRNB: carries an accessible name that states its value", async () => {
    renderColumn();
    const meter = await screen.findByRole("meter");
    expect(meter).toHaveAccessibleName(
      "Context use: 25% of the model's context window, estimated",
    );
    expect(meter).toHaveAttribute("aria-valuenow", "25");
  });
});

describe("the size warning", () => {
  it("DDS-FR-ZQVE: stands only while the document is above the limit", async () => {
    renderColumn();
    await screen.findByTestId("context-indicator");
    expect(screen.queryByTestId("context-warning")).toBeNull();
  });

  it("DDS-FR-ZQVE: states the document size and the limit, and says to split or shorten", async () => {
    answer = estimate({
      aboveLimit: true,
      document: { lines: 4000, characters: 608_000, estimatedTokens: 152_000 },
    });
    renderColumn();
    const warning = await screen.findByTestId("context-warning");
    expect(warning.querySelector('[data-icon="alert-triangle"]')).not.toBeNull();
    expect(warning).toHaveAccessibleName("The document is above the size limit");
    act(() => warning.focus());
    const tip = screen.getByTestId("context-warning-tip");
    expect(tip).toHaveTextContent("~152,000 tokens est.");
    expect(tip).toHaveTextContent("60,000 (30% of window)");
    expect(tip).toHaveTextContent("Split or shorten the document.");
  });

  it("DDS-FR-ZQVE: follows the latest turn's report of the limit", async () => {
    renderColumn();
    await screen.findByTestId("context-indicator");
    act(() => followAgentTurn(turnWith(reported({ aboveLimit: true }))));
    expect(screen.getByTestId("context-warning")).toBeInTheDocument();
  });

  it("DDS-FR-HJMF: blocks neither the composer nor posting", async () => {
    answer = estimate({ aboveLimit: true });
    renderColumn();
    await screen.findByTestId("context-warning");
    const fields = screen.getAllByRole("textbox");
    const field = fields[fields.length - 1];
    expect(field).not.toBeDisabled();
  });
});

describe("when the figures refresh", () => {
  it("DDS-FR-HJMF: reads the estimate again when the shown discussion changes", async () => {
    const two = [discussionSaying("disc-1", ["a"]), discussionSaying("disc-2", ["b"])];
    const view = renderColumn({ discussions: two });
    await screen.findByTestId("context-indicator");
    view.rerender(
      columnElement({ agents: AGENTS, discussions: two, selectedThreadId: "disc-2" }),
    );
    await waitFor(() => expect(estimateCalls()).toHaveLength(2));
    expect(estimateCalls()[1][1]).toMatchObject({ discussionId: "disc-2" });
  });

  it("DDS-FR-KXWQ, DDS-FR-HJMF: never shows one discussion's estimate for another, also when the new read fails", async () => {
    const two = [discussionSaying("disc-1", ["a"]), discussionSaying("disc-2", ["b"])];
    const view = renderColumn({ discussions: two });
    expect(await screen.findByTestId("context-indicator")).toHaveTextContent("~25%");
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "estimate_discussion_context"
        ? Promise.reject(new Error("estimate failed"))
        : otherCommand(cmd),
    );
    view.rerender(
      columnElement({ agents: AGENTS, discussions: two, selectedThreadId: "disc-2" }),
    );
    expect(screen.queryByTestId("context-indicator")).toBeNull();
    await waitFor(() => expect(estimateCalls()).toHaveLength(2));
    await estimateFailureHandled();
    expect(screen.queryByTestId("context-indicator")).toBeNull();
  });

  it("DDS-FR-HJMF: reads the estimate again when the document is saved, and shows the new size", async () => {
    const view = renderColumn({ documentDirty: false });
    await screen.findByText("~25%");
    view.rerender(
      columnElement({ agents: AGENTS, selectedThreadId: "disc-1", documentDirty: true }),
    );
    // Typing alone reads nothing.
    expect(estimateCalls()).toHaveLength(1);
    answer = estimate({ estimatedTokens: 100_000, aboveLimit: true });
    view.rerender(
      columnElement({ agents: AGENTS, selectedThreadId: "disc-1", documentDirty: false }),
    );
    expect(await screen.findByText("~50%")).toBeInTheDocument();
    expect(estimateCalls()).toHaveLength(2);
    expect(screen.getByTestId("context-warning")).toBeInTheDocument();
  });

  it("DDS-FR-HJMF: a save after a turn shows the saved document's warning beside the turn's value", async () => {
    const view = renderColumn({ documentDirty: false });
    await screen.findByText("~25%");
    act(() => followAgentTurn(turnWith(reported())));
    expect(screen.queryByTestId("context-warning")).toBeNull();
    view.rerender(
      columnElement({ agents: AGENTS, selectedThreadId: "disc-1", documentDirty: true }),
    );
    answer = estimate({ aboveLimit: true });
    view.rerender(
      columnElement({ agents: AGENTS, selectedThreadId: "disc-1", documentDirty: false }),
    );
    expect(await screen.findByTestId("context-warning")).toBeInTheDocument();
    // The value is still the turn's reported input.
    expect(screen.getByTestId("context-indicator")).toHaveTextContent("42%");
  });

  it("DDS-FR-TSPL, DDS-FR-ZQVE: after a turn, a save estimates with the answering agent's window, not the first agent's", async () => {
    // agent-a (the first agent) has a large window, agent-b a small one.
    invokeMock.mockImplementation((cmd: string, args?: { agentId?: string }) => {
      if (cmd !== "estimate_discussion_context") return otherCommand(cmd);
      return Promise.resolve(
        args?.agentId === "agent-b"
          ? estimate({ window: 100_000, limitTokens: 30_000, aboveLimit: true })
          : estimate({ window: 1_000_000, limitTokens: 300_000, aboveLimit: false }),
      );
    });
    const view = renderColumn({ documentDirty: false });
    await screen.findByTestId("context-indicator");
    act(() =>
      followAgentTurn(
        turnWith(reported({ window: 100_000, limitTokens: 30_000, aboveLimit: true })),
      ),
    );
    view.rerender(
      columnElement({ agents: AGENTS, selectedThreadId: "disc-1", documentDirty: true }),
    );
    view.rerender(
      columnElement({ agents: AGENTS, selectedThreadId: "disc-1", documentDirty: false }),
    );
    await waitFor(() =>
      expect(estimateCalls().filter((c) => c[1].agentId === "agent-b").length).toBeGreaterThan(1),
    );
    const calls = estimateCalls();
    expect(calls[calls.length - 1][1]).toMatchObject({ agentId: "agent-b" });
    expect(await screen.findByTestId("context-warning")).toBeInTheDocument();
  });

  it("DDS-FR-KXWQ: a failed estimate shows no indicator and raises nothing", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "estimate_discussion_context"
        ? Promise.reject(new Error("no_project_open"))
        : otherCommand(cmd),
    );
    renderColumn();
    await waitFor(() => expect(estimateCalls()).toHaveLength(1));
    await estimateFailureHandled();
    expect(screen.queryByTestId("context-indicator")).toBeNull();
  });
});
