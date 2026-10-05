/**
 * The pending contributions a discussion surface draws
 * (`CTA-comment-agent-turns.md` CTA-FR-ZOLW, CTA-FR-XMCQ;
 * `DDS-draft-discussion.md` DDS-FR-TGWY).
 *
 * An agent asked again before it answers runs a second turn, and the surface
 * still draws one row for that agent.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));

import { invoke } from "@tauri-apps/api/core";
import { DiscussionSurface, type DiscussionSurfaceProps } from "./DiscussionSurface";
import { resetDiscussionFocus } from "../../state/discussionFocus";
import {
  clearAllDiscussionSessions,
  getDiscussionSession,
  mergeDiscussionTurn,
} from "../../state/discussionSession";
import { clearQuestionSets } from "../../state/questionSets";
import type { AgentTurn, Discussion } from "../../types";
import { artifactDiscussionOrigin } from "../../test/origins";

const invokeMock = vi.mocked(invoke);

beforeEach(() => {
  clearAllDiscussionSessions();
  clearQuestionSets();
  resetDiscussionFocus();
  invokeMock.mockClear();
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function discussion(answers: string[] = []): Discussion {
  return {
    id: "d1",
    target: { kind: "artifact", artifactId: "a.md" },
    fragmentTarget: null,
    comments: [
      {
        id: "c1",
        author: { kind: "human", login: "raver119" },
        body: "first",
        quotes: [],
        attachments: [],
        createdAt: "2026-02-01T00:00:00Z",
      },
      ...answers.map((body, i) => ({
        id: `answer-${i}`,
        author: { kind: "agent" as const, agentId: "agent-arch", handle: "arch" },
        body,
        quotes: [],
        attachments: [],
        createdAt: "2026-02-01T00:02:00Z",
      })),
    ],
    locked: false,
    resolved: false,
    createdAt: "2026-02-01T00:00:00Z",
    updatedAt: "2026-02-01T00:00:00Z",
  };
}

function running(
  id: string,
  nickname: string,
  over: Partial<AgentTurn> = {},
): AgentTurn {
  return {
    id,
    agentId: `agent-${nickname}`,
    nickname,
    origin: artifactDiscussionOrigin("d1", "a.md"),
    triggerCommentId: "c1",
    state: "running",
    failure: null,
    retryPermitted: false,
    // `tN` starts N seconds in, so a higher number is a newer turn.
    startedAt: new Date(Date.UTC(2026, 1, 1, 0, 0, Number(id.slice(1)))).toISOString(),
    endedAt: null,
    activeToolCalls: [],
    ...over,
  } as unknown as AgentTurn;
}

function ended(t: AgentTurn): AgentTurn {
  return { ...t, state: "delivered", endedAt: "2026-02-01T00:01:00Z" };
}

const reading = { id: "k1", tool: "read_file", activationSeq: 1 };

function surface(over: Partial<DiscussionSurfaceProps> = {}): React.ReactElement {
  return (
    <DiscussionSurface
      discussion={discussion()}
      owner="rail"
      agents={[]}
      liveTurns={false}
      onReply={vi.fn(async () => undefined)}
      onSetLock={vi.fn(async () => undefined)}
      onSetResolved={vi.fn(async () => undefined)}
      {...over}
    />
  );
}

function hold(...turns: AgentTurn[]): void {
  act(() => {
    for (const t of turns) mergeDiscussionTurn("d1", t);
  });
}

const rows = () => screen.queryAllByTestId("comment-pending");
const cancelCalls = () =>
  invokeMock.mock.calls
    .filter(([cmd]) => cmd === "cancel_agent_turn")
    .map(([, args]) => (args as { turnId: string }).turnId);

describe("CTA-FR-ZOLW: one pending contribution for each agent", () => {
  it("an agent with two running turns draws one row, and two agents draw two", () => {
    const errors = vi.spyOn(console, "error").mockImplementation(() => {});
    hold(running("t1", "arch"), running("t2", "sec"), running("t3", "arch"));
    render(surface());
    expect(rows().map((r) => r.textContent)).toEqual([
      expect.stringContaining("arch"),
      expect.stringContaining("sec"),
    ]);
    expect(errors).not.toHaveBeenCalled();
  });

  it("CTA-FR-XMCQ: the row reads the newest turn of that agent", () => {
    hold(
      running("t1", "arch"),
      running("t2", "arch", { activeToolCalls: [reading] } as Partial<AgentTurn>),
    );
    render(surface());
    expect(rows()).toHaveLength(1);
    expect(rows()[0]).toHaveAttribute("data-turn-id", "t2");
    expect(rows()[0]).toHaveTextContent("Reading a project file…");
  });

  it("the row stays while one turn still runs, and leaves when none does", () => {
    const t1 = running("t1", "arch");
    const t2 = running("t2", "arch", { activeToolCalls: [reading] } as Partial<AgentTurn>);
    hold(t1, t2);
    render(surface());
    const row = rows()[0];
    hold(ended(t2));
    expect(rows()).toHaveLength(1);
    // Keyed by the agent, so the same row now reads the turn that remains.
    expect(rows()[0]).toBe(row);
    expect(row).toHaveAttribute("data-turn-id", "t1");
    expect(row).toHaveTextContent("Thinking…");
    hold(ended(t1));
    expect(rows()).toHaveLength(0);
  });

  it("each row stands at its agent's oldest running turn", () => {
    const t1 = running("t1", "arch");
    hold(t1, running("t2", "sec"), running("t3", "arch"));
    render(surface());
    const names = () => rows().map((r) => (r.textContent ?? "").includes("arch") ? "arch" : "sec");
    expect(names()).toEqual(["arch", "sec"]);
    hold(ended(t1));
    expect(names()).toEqual(["sec", "arch"]);
  });
});

describe("CTA-FR-XMCQ: cancel ends every running turn of that agent", () => {
  it("CVP-FR-HWTN: cancels both of arch's turns, leaves sec's row, and a late event changes nothing", () => {
    hold(running("t1", "arch"), running("t2", "sec"), running("t3", "arch"));
    render(surface());
    fireEvent.click(screen.getByRole("button", { name: "Cancel arch's reply" }));
    expect(cancelCalls().sort()).toEqual(["t1", "t3"]);
    expect(getDiscussionSession("d1").turns.map((t) => t.id)).toEqual(["t2"]);
    expect(rows()).toHaveLength(1);
    expect(rows()[0]).toHaveTextContent("sec");
    // CVP-FR-HWTN: a running event that was already on its way changes nothing.
    hold(running("t1", "arch"));
    expect(rows()).toHaveLength(1);
  });

  it("an owner that supplies its own cancel gets one call for each turn", () => {
    const onCancelTurn = vi.fn();
    render(
      surface({
        pendingTurns: [running("t1", "arch"), running("t3", "arch")],
        onCancelTurn,
      }),
    );
    expect(rows()).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: "Cancel arch's reply" }));
    expect(onCancelTurn.mock.calls.map(([id]) => id).sort()).toEqual(["t1", "t3"]);
  });
});

describe("DDS-FR-TGWY: the draft column draws one row for each agent", () => {
  it("two turns of one agent are one transient row", () => {
    hold(running("t1", "arch"), running("t2", "sec"), running("t3", "arch"));
    render(surface({ variant: "stream" }));
    const transient = screen.getAllByTestId("dds-transient");
    expect(transient).toHaveLength(2);
    expect(transient[0]).toHaveAttribute("data-turn-id", "t3");
    expect(transient[0]).toHaveTextContent("thinking");
    expect(transient[0]).toHaveTextContent("arch");
    expect(transient[1]).toHaveAttribute("data-turn-id", "t2");
    expect(transient[1]).toHaveTextContent("sec");
  });

  it("the row stays in place while one turn runs, and the answer takes its place", () => {
    const t1 = running("t1", "arch");
    const t2 = running("t2", "arch");
    hold(t1, t2);
    const view = render(surface({ variant: "stream" }));
    const row = screen.getByTestId("dds-transient");
    view.rerender(surface({ variant: "stream", discussion: discussion(["one answer"]) }));
    hold(ended(t2));
    expect(screen.getAllByTestId("dds-transient")).toHaveLength(1);
    expect(screen.getByTestId("dds-transient")).toBe(row);
    expect(row).toHaveAttribute("data-turn-id", "t1");
    // The answer of the turn that ended stands above the row that remains.
    const answer = screen.getByText("one answer");
    expect(answer.compareDocumentPosition(row) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

    view.rerender(
      surface({ variant: "stream", discussion: discussion(["one answer", "last answer"]) }),
    );
    hold(ended(t1));
    expect(screen.queryByTestId("dds-transient")).not.toBeInTheDocument();
    expect(screen.getByText("last answer")).toBeInTheDocument();
  });
});
