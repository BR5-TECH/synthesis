/**
 * The saved activity row that follows a delivered answer
 * (`CTA-comment-agent-turns.md` CTA-FR-RFQB, CTA-FR-KDTM, CTA-FR-TGLS,
 * CTA-FR-GCKO, CTA-FR-MWPE; `CVP-conversation-presentation.md` CVP-FR-NMVH).
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
  mergeDiscussionTurn,
  upsertDiscussionTurn,
} from "../../state/discussionSession";
import { clearQuestionSets } from "../../state/questionSets";
import type { AgentTurn, Comment, Discussion, SavedToolCall } from "../../types";
import { artifactDiscussionOrigin } from "../../test/origins";

beforeEach(() => {
  clearAllDiscussionSessions();
  clearQuestionSets();
  resetDiscussionFocus();
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const saved: SavedToolCall[] = [
  { tool: "read_file", keyInput: "docs/spec.md", state: "done" },
  { tool: "openrouter:web_search", keyInput: "tauri 2 events", state: "failed", advisor: "sec" },
];

function agentComment(id: string, handle: string, toolCalls?: SavedToolCall[]): Comment {
  return {
    id,
    author: { kind: "agent", agentId: `agent-${handle}`, handle },
    body: `answer ${id}`,
    quotes: [],
    attachments: [],
    ...(toolCalls ? { toolCalls } : {}),
    createdAt: "2026-02-01T00:01:00Z",
  };
}

function discussion(...agents: Comment[]): Discussion {
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
      ...agents,
    ],
    resolved: false,
    createdAt: "2026-02-01T00:00:00Z",
    updatedAt: "2026-02-01T00:00:00Z",
  };
}

function turn(id: string, nickname: string, over: Partial<AgentTurn> = {}): AgentTurn {
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

function surface(thread: Discussion, over: Partial<DiscussionSurfaceProps> = {}): React.ReactElement {
  return (
    <DiscussionSurface
      discussion={thread}
      owner="rail"
      agents={[]}
      liveTurns={false}
      onReply={vi.fn(async () => undefined)}
      onSetResolved={vi.fn(async () => undefined)}
      {...over}
    />
  );
}

describe("CTA-FR-RFQB, CTA-FR-KDTM: the saved row after its comment", () => {
  it("follows the comment, is collapsed, and expands to the saved calls with the advisor name", () => {
    render(surface(discussion(agentComment("c2", "arch", saved))));
    const row = screen.getByTestId("saved-activity-row");
    expect(row).toHaveTextContent("@arch called 2 tools");
    expect(row.previousElementSibling).toHaveAttribute("data-comment-id", "c2");
    const toggle = within(row).getByTestId("saved-activity-row-toggle");
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(within(row).queryByTestId("tool-call-list")).toBeNull();
    fireEvent.click(toggle);
    const entries = within(row).getAllByTestId("tool-call-entry");
    expect(entries).toHaveLength(2);
    expect(entries[0]).toHaveTextContent("read_file");
    expect(entries[0]).toHaveTextContent("docs/spec.md");
    expect(entries[0]).toHaveAttribute("data-state", "done");
    expect(within(entries[0]).queryByTestId("tool-call-advisor")).toBeNull();
    expect(within(entries[1]).getByTestId("tool-call-advisor")).toHaveTextContent("@sec");
    expect(entries[1]).toHaveAttribute("data-state", "failed");
  });

  it("CTA-FR-KDTM: each earlier answer keeps its own row, and a comment with no list has none", () => {
    render(
      surface(
        discussion(
          agentComment("c2", "arch", saved),
          agentComment("c3", "sec", [saved[0]]),
          agentComment("c4", "ops"),
          agentComment("c5", "ops", []),
        ),
      ),
    );
    const rows = screen.getAllByTestId("saved-activity-row");
    expect(rows.map((r) => r.textContent)).toEqual([
      "@arch called 2 tools",
      "@sec called 1 tool",
    ]);
    expect(rows.map((r) => r.getAttribute("data-comment-id"))).toEqual(["c2", "c3"]);
  });

  it("CTA-FR-KDTM: a relaunch shows the row collapsed again", () => {
    const thread = discussion(agentComment("c2", "arch", saved));
    const first = render(surface(thread));
    fireEvent.click(screen.getByTestId("saved-activity-row-toggle"));
    expect(screen.getByTestId("tool-call-list")).toBeInTheDocument();
    first.unmount();
    render(surface(thread));
    expect(screen.getByTestId("saved-activity-row-toggle")).toHaveAttribute(
      "aria-expanded",
      "false",
    );
  });

  it("CTA-FR-TGLS, CMT-FR-YWQD: the row has no author line, timestamp, Quote, cancel, or Retry", () => {
    render(surface(discussion(agentComment("c2", "arch", saved))));
    const row = screen.getByTestId("saved-activity-row");
    expect(row.querySelectorAll("button")).toHaveLength(1);
    expect(row.querySelector(".comment__head, .comment__time, .comment__author")).toBeNull();
    expect(within(row).queryByRole("button", { name: /quote|retry|cancel/i })).toBeNull();
    // The comment before it offers Quote; the row does not.
    expect(row.querySelector(".comment__quote-button")).toBeNull();
  });

  it("CTA-FR-MWPE: the draft column's stream variant renders the same row after the comment", () => {
    render(surface(discussion(agentComment("c2", "arch", saved)), { variant: "stream" }));
    const row = screen.getByTestId("saved-activity-row");
    expect(row).toHaveTextContent("@arch called 2 tools");
    expect(row.previousElementSibling?.closest("[data-comment-id]")?.getAttribute("data-comment-id")).toBe("c2");
    fireEvent.click(within(row).getByTestId("saved-activity-row-toggle"));
    expect(within(row).getAllByTestId("tool-call-entry")).toHaveLength(2);
  });
});

describe("CTA-FR-ZBIB, CTA-FR-GCKO, CTA-FR-KDTM: the saved row and the foot row", () => {
  it("a delivered turn leaves no foot row, so the same list never shows twice", () => {
    const calls = [{ id: "call-1", tool: "read_file", activationSeq: 1, state: "done" as const, keyInput: "x" }];
    const running = turn("t1", "arch", { toolCalls: calls });
    act(() => mergeDiscussionTurn("d1", running));
    render(surface(discussion()));
    act(() => mergeDiscussionTurn("d1", { ...running, state: "delivered", endedAt: "2026-02-01T00:01:00Z" }));
    expect(screen.queryByTestId("activity-row")).toBeNull();
  });

  it("a turn that starts removes the foot rows and never a saved row", () => {
    const calls = [{ id: "call-1", tool: "read_file", activationSeq: 1, state: "done" as const, keyInput: "x" }];
    const first = turn("t1", "sec", { toolCalls: calls });
    act(() => mergeDiscussionTurn("d1", first));
    render(surface(discussion(agentComment("c2", "arch", saved))));
    act(() => mergeDiscussionTurn("d1", { ...first, state: "failed", endedAt: "2026-02-01T00:01:00Z" }));
    expect(screen.getAllByTestId("activity-row")).toHaveLength(1);
    expect(screen.getAllByTestId("saved-activity-row")).toHaveLength(1);
    act(() => upsertDiscussionTurn("d1", turn("t5", "ops")));
    expect(screen.queryByTestId("activity-row")).toBeNull();
    expect(screen.getAllByTestId("saved-activity-row")).toHaveLength(1);
  });
});
