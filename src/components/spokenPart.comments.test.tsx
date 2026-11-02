/**
 * The spoken part stays out of what a person reads (`CMT-comments.md`
 * CMT-FR-BMCI, CMT-FR-XWVW, CMT-FR-RZGT). The stored body keeps it.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));

import type { Comment, Discussion, DiscussionListItem } from "../types";
import { matchesFilter } from "./commentGrouping";
import { discussionSubject } from "./Comments";
import { DiscussionSurface } from "./discussion/DiscussionSurface";
import { resetLogBufferForTest } from "../logging";
import { resetDiscussionFocus } from "../state/discussionFocus";
import { clearQuestionSets } from "../state/questionSets";
import {
  clearAllDiscussionSessions,
  getDiscussionSession,
} from "../state/discussionSession";

function agentComment(id: string, body: string): Comment {
  return {
    id,
    author: { kind: "agent", agentId: "a1", handle: "arch" },
    body,
    quotes: [],
    attachments: [],
    createdAt: "2026-02-01T00:00:00Z",
  };
}

function discussion(comments: Comment[]): Discussion {
  return {
    id: "d1",
    target: { kind: "artifact", artifactId: "a.md" },
    fragmentTarget: null,
    comments,
    resolved: false,
    createdAt: "2026-02-01T00:00:00Z",
    updatedAt: "2026-02-01T00:00:00Z",
  };
}

beforeEach(() => {
  clearAllDiscussionSessions();
  clearQuestionSets();
  resetDiscussionFocus();
  resetLogBufferForTest();
});

afterEach(cleanup);

async function surfaceOf(body: string) {
  render(
    <DiscussionSurface
      discussion={discussion([agentComment("c1", body)])}
      owner="rail"
      agents={[]}
      liveTurns={false}
      onReply={vi.fn(async () => undefined)}
      onSetResolved={vi.fn(async () => undefined)}
    />,
  );
  await act(async () => {
    await Promise.resolve();
  });
}

describe("the spoken part in what a person reads", () => {
  it("CMT-FR-BMCI: a comment renders with the tag and its text hidden", async () => {
    await surfaceOf("<spoken>Hidden words.</spoken>\n\nThe written text.");
    expect(screen.getByText("The written text.")).toBeInTheDocument();
    expect(screen.queryByText(/Hidden words/)).toBeNull();
    expect(document.body.textContent).not.toContain("spoken");
  });

  it("CMT-FR-XWVW: a comment of the tag alone renders its text", async () => {
    await surfaceOf("<spoken>Yes, two specs.</spoken>");
    expect(screen.getByText("Yes, two specs.")).toBeInTheDocument();
  });

  it("CMT-FR-BMCI: a tag inside code renders as code", async () => {
    await surfaceOf("Write `<spoken>x</spoken>` here. <spoken>Short.</spoken>");
    expect(document.body.textContent).toContain("<spoken>x</spoken>");
    expect(document.body.textContent).not.toContain("Short.");
  });

  it("CMT-FR-RZGT: a Quote of a whole comment holds no tag", async () => {
    await surfaceOf("<spoken>Hidden words.</spoken>\n\nThe written text.");
    fireEvent.click(screen.getByRole("button", { name: /^Quote comment 1/ }));
    expect(getDiscussionSession("d1").quotes).toEqual([
      { commentId: "c1", excerpt: "The written text." },
    ]);
  });

  it("CMT-FR-RZGT: the subject line of a discussion holds no tag", () => {
    expect(
      discussionSubject(discussion([agentComment("c1", "<spoken>Short.</spoken> The written line.")])),
    ).toBe("The written line.");
  });

  it("CMT-FR-RZGT, CMT-FR-XWVW: a subject of the tag alone reads its text", () => {
    expect(discussionSubject(discussion([agentComment("c1", "<spoken>Short.</spoken>")]))).toBe(
      "Short.",
    );
  });

  it("CMT-FR-RZGT: the filter of the Comments panel reads the body as it renders", () => {
    const item = {
      discussion: discussion([agentComment("c1", "<spoken>hidden words</spoken> Written words.")]),
    } as unknown as DiscussionListItem;
    expect(matchesFilter(item, "written")).toBe(true);
    expect(matchesFilter(item, "hidden")).toBe(false);
    expect(matchesFilter(item, "spoken")).toBe(false);
  });
});
