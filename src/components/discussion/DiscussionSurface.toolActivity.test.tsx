/**
 * The tool list of a pending contribution, the activity rows at the foot of a
 * discussion, and the timeout bar under the reply field
 * (`CTA-comment-agent-turns.md` CTA-FR-GGYJ, CTA-FR-BJWN, CTA-FR-ZBIB,
 * CTA-FR-GCKO, CTA-FR-ODDY, CTA-FR-MWPE; `CVP-conversation-presentation.md`
 * CVP-FR-JSHF; `DDS-draft-discussion.md` DDS-FR-ZUXP).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));

import { DiscussionSurface, type DiscussionSurfaceProps } from "./DiscussionSurface";
import { resetDiscussionFocus } from "../../state/discussionFocus";
import {
  clearAllDiscussionSessions,
  getDiscussionSession,
  mergeDiscussionTurn,
  upsertDiscussionTurn,
} from "../../state/discussionSession";
import { clearQuestionSets } from "../../state/questionSets";
import type { AgentTurn, Discussion, TurnToolCall } from "../../types";
import { artifactDiscussionOrigin } from "../../test/origins";

beforeEach(() => {
  clearAllDiscussionSessions();
  clearQuestionSets();
  resetDiscussionFocus();
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

function discussion(): Discussion {
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
    ],
    resolved: false,
    createdAt: "2026-02-01T00:00:00Z",
    updatedAt: "2026-02-01T00:00:00Z",
  };
}

const call = (
  seq: number,
  tool: string,
  state: TurnToolCall["state"],
  keyInput: string | null,
): TurnToolCall => ({ id: `call-${seq}`, tool, activationSeq: seq, state, keyInput });

function running(id: string, nickname: string, over: Partial<AgentTurn> = {}): AgentTurn {
  return {
    id,
    agentId: `agent-${nickname}`,
    nickname,
    origin: artifactDiscussionOrigin("d1", "a.md"),
    triggerCommentId: "c1",
    state: "running",
    failure: null,
    retryPermitted: false,
    startedAt: new Date(Date.UTC(2026, 1, 1, 0, 0, Number(id.slice(1)))).toISOString(),
    endedAt: null,
    activeToolCalls: [],
    imagesOmitted: false,
    toolCalls: [],
    deadlineStartedAt: null,
    deadlineAt: null,
    ...over,
  };
}

function ended(t: AgentTurn, state: AgentTurn["state"] = "delivered"): AgentTurn {
  return { ...t, state, endedAt: "2026-02-01T00:01:00Z", activeToolCalls: [] };
}

function surface(over: Partial<DiscussionSurfaceProps> = {}): React.ReactElement {
  return (
    <DiscussionSurface
      discussion={discussion()}
      owner="rail"
      agents={[]}
      liveTurns={false}
      onReply={vi.fn(async () => undefined)}
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

const searching = running("t1", "arch", {
  activeToolCalls: [{ id: "call-2", tool: "openrouter:web_search", activationSeq: 2 }],
  toolCalls: [
    call(1, "read_file", "done", "docs/spec.md"),
    call(2, "openrouter:web_search", "running", "tauri 2 events"),
  ],
});

describe("CTA-FR-GGYJ / CTA-FR-BJWN: the pending contribution opens its tool list", () => {
  it("a turn with tool calls renders its status as a collapsed disclosure that opens the list", () => {
    hold(searching);
    render(surface());
    const toggle = screen.getByTestId("comment-pending-toggle");
    expect(toggle).toHaveTextContent("Searching the web…");
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByTestId("tool-call-list")).toBeNull();

    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    const entries = screen.getAllByTestId("tool-call-entry");
    expect(entries.map((e) => e.textContent)).toEqual([
      "read_filedocs/spec.mddone",
      "openrouter:web_searchtauri 2 eventsrunning",
    ]);
  });

  it("a turn with no tool call renders its status with no disclosure control", () => {
    hold(running("t1", "arch"));
    render(surface());
    expect(screen.getByTestId("comment-pending-status")).toHaveTextContent("Thinking…");
    expect(screen.queryByTestId("comment-pending-toggle")).toBeNull();
    expect(
      screen.getByTestId("comment-pending-status").querySelector("[aria-expanded]"),
    ).toBeNull();
  });

  it("the list holds the calls of every running turn of the agent, the oldest turn first", () => {
    hold(
      running("t1", "arch", { toolCalls: [call(1, "search_notes", "done", "first")] }),
      running("t3", "arch", { toolCalls: [call(1, "load_skill", "running", "second")] }),
    );
    render(surface());
    fireEvent.click(screen.getByTestId("comment-pending-toggle"));
    expect(screen.getAllByTestId("tool-call-input").map((e) => e.textContent)).toEqual([
      "first",
      "second",
    ]);
  });
});

describe("CTA-FR-ZBIB / CTA-FR-GCKO: the activity row at the foot", () => {
  it("an ended turn leaves its list at the foot until the next turn starts", () => {
    hold(searching);
    render(surface());
    hold(ended({ ...searching, toolCalls: [call(1, "read_file", "done", "docs/spec.md")] }));
    expect(screen.queryByTestId("comment-pending")).toBeNull();
    const row = screen.getByTestId("activity-row");
    expect(row).toHaveTextContent("@arch called 1 tool");
    fireEvent.click(within(row).getByRole("button"));
    expect(within(row).getByTestId("tool-call-input")).toHaveTextContent("docs/spec.md");

    // CTA-FR-GCKO: a dispatch that started a turn removes the rows.
    act(() => upsertDiscussionTurn("d1", running("t5", "sec")));
    expect(screen.queryByTestId("activity-row")).toBeNull();
  });

  it("CTA-FR-ODDY: a dispatch that steered a running turn adds no row and removes none", () => {
    const sec = running("t2", "sec", { toolCalls: [call(1, "search_notes", "done", "x")] });
    hold(searching, sec);
    render(surface());
    hold(ended(sec));
    expect(screen.getAllByTestId("activity-row")).toHaveLength(1);
    // The dispatch answered with arch's turn, which had already started.
    act(() => upsertDiscussionTurn("d1", searching));
    expect(screen.getAllByTestId("activity-row")).toHaveLength(1);
    expect(screen.getAllByTestId("comment-pending")).toHaveLength(1);
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("a failed and a cancelled turn keep their lists, and a turn with no call leaves none", () => {
    const a = running("t1", "arch", { toolCalls: [call(1, "read_file", "failed", "a.md")] });
    const b = running("t2", "sec", { toolCalls: [call(1, "search_drafts", "failed", "b")] });
    const c = running("t3", "ops");
    hold(a, b, c);
    render(surface());
    hold(ended(a, "failed"), ended(b, "cancelled"), ended(c));
    expect(screen.getAllByTestId("activity-row").map((r) => r.textContent)).toEqual([
      "@arch called 1 tool",
      "@sec called 1 tool",
    ]);
    // CTA-FR-MNPS: the rows are session state, keyed by the discussion id.
    expect(getDiscussionSession("d1").activityRows).toHaveLength(2);
  });

  it("CTA-FR-MWPE: the draft column's stream variant renders the same row", () => {
    hold(searching);
    render(surface({ variant: "stream" }));
    expect(screen.getByTestId("dds-transient-toggle")).toHaveAttribute("aria-expanded", "false");
    hold(ended(searching));
    expect(screen.getByTestId("activity-row")).toHaveTextContent("@arch called 2 tools");
  });
});

describe("CVP-FR-JSHF / DDS-FR-ZUXP: the timeout bar under the reply field", () => {
  const now = Date.UTC(2026, 1, 1, 0, 1, 0);
  const deadline = (startedMs: number, endsMs: number): Partial<AgentTurn> => ({
    deadlineStartedAt: new Date(startedMs).toISOString(),
    deadlineAt: new Date(endsMs).toISOString(),
  });

  it("shows while a turn runs with a deadline, inside the field, and goes when none runs", () => {
    vi.useFakeTimers({ now });
    const t1 = running("t1", "arch", deadline(now - 100_000, now + 200_000));
    hold(t1);
    render(surface());
    const bar = screen.getByRole("progressbar", { name: "Time left for the agent turn" });
    expect(bar.closest(".comment-composer")).not.toBeNull();
    expect(bar).toHaveAttribute("aria-valuenow", "67");
    hold(ended(t1));
    expect(screen.queryByRole("progressbar")).toBeNull();
  });

  it("a turn that waits for its slot shows no bar", () => {
    hold(running("t1", "arch"));
    render(surface());
    expect(screen.queryByTestId("timeout-bar")).toBeNull();
  });

  it("CVP-FR-ZRWW: with several running turns the bar shows the one with the most time left", () => {
    vi.useFakeTimers({ now });
    hold(
      running("t1", "arch", deadline(now - 250_000, now + 50_000)),
      running("t2", "sec", deadline(now - 30_000, now + 270_000)),
    );
    render(surface());
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "90");
  });

  it("CVP-FR-XZTZ: a steered turn's new deadline fills the bar at once", () => {
    vi.useFakeTimers({ now });
    const t1 = running("t1", "arch", deadline(now - 290_000, now + 10_000));
    hold(t1);
    render(surface());
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "3");
    hold({ ...t1, ...deadline(now, now + 300_000) });
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "100");
  });
});
