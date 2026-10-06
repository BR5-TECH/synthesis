/**
 * The fixed local participant on the shared discussion surface
 * (`CVP-conversation-presentation.md` CVP-FR-TIBK; `CMT-comments.md`
 * CMT-FR-ZCAE; `DQA-discussion-question-answering.md` DQA-FR-GOZY).
 *
 * The participant is stamped once and never rewritten (CMS-FR-KTHN). These tests
 * pin what a surface *shows* for it: the current project identity, or **Me**.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const handlers = new Map<string, Set<() => void>>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, cb: () => void) => {
    const set = handlers.get(event) ?? new Set();
    set.add(cb);
    handlers.set(event, set);
    return () => set.delete(cb);
  }),
  emit: vi.fn(async () => {}),
}));

import { DiscussionSurface } from "./DiscussionSurface";
import { resetDiscussionFocus } from "../../state/discussionFocus";
import { clearAllDiscussionSessions } from "../../state/discussionSession";
import { clearQuestionSets, publishSet } from "../../state/questionSets";
import { resetLogBufferForTest } from "../../logging";
import { resetProjectIdentity } from "../../state/projectIdentity";
import { forgetEveryDraft } from "../../state/questionAnswerDrafts";
import type { Comment, Discussion, Participant, PendingQuestionSet } from "../../types";

const LOCAL: Participant = { kind: "human", login: "", displayName: "Me" };
const GITHUB: Participant = { kind: "human", login: "raver119" };

let identity: () => Promise<unknown>;

function comment(id: string, body: string, author: Participant): Comment {
  return { id, author, body, quotes: [], attachments: [], createdAt: "2026-02-01T00:00:00Z" };
}

function discussion(comments: Comment[]): Discussion {
  return {
    id: "d1",
    target: { kind: "artifact", artifactId: "a.md" },
    fragmentTarget: null,
    comments,
    locked: false,
    resolved: false,
    createdAt: "2026-02-01T00:00:00Z",
    updatedAt: "2026-02-01T00:00:00Z",
  };
}

const saved = () => [
  comment("c1", "written without a token", LOCAL),
  comment("c2", "written with a token", GITHUB),
];

function view(d: Discussion, over: Partial<Parameters<typeof DiscussionSurface>[0]> = {}) {
  return (
    <DiscussionSurface
      discussion={d}
      owner="tab"
      variant="embedded"
      agents={[]}
      liveTurns={false}
      identity={LOCAL}
      onReply={vi.fn(async () => undefined)}
      onSetLock={vi.fn(async () => undefined)}
      onSetResolved={vi.fn(async () => undefined)}
      {...over}
    />
  );
}

const authors = () =>
  Array.from(document.querySelectorAll(".comment__author")).map((el) => el.textContent);

const tokensChanged = () =>
  act(() => {
    for (const cb of handlers.get("github-tokens-changed") ?? []) cb();
  });

beforeEach(() => {
  invokeMock.mockReset();
  handlers.clear();
  identity = () => Promise.resolve(LOCAL);
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "resolve_comment_author_identity") return identity();
    return undefined;
  });
  clearAllDiscussionSessions();
  clearQuestionSets();
  resetDiscussionFocus();
  forgetEveryDraft();
  resetProjectIdentity();
  resetLogBufferForTest();
});
afterEach(cleanup);

describe("CVP-FR-TIBK, CMT-FR-ZCAE: the label of the local participant's comments", () => {
  it("renders Me while the project stores no token, and leaves a saved GitHub author as saved", async () => {
    render(view(discussion(saved())));
    await waitFor(() => expect(authors()).toEqual(["Me", "raver119"]));
  });

  it("renders the current project login once a token resolves, for past comments, without changing what is stored", async () => {
    const d = discussion(saved());
    render(view(d));
    await waitFor(() => expect(authors()[0]).toBe("Me"));

    identity = () => Promise.resolve({ kind: "human", login: "octocat" });
    tokensChanged();
    await waitFor(() => expect(authors()).toEqual(["octocat", "raver119"]));
    expect(d.comments[0].author).toEqual(LOCAL);
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "read_discussion"),
    ).toHaveLength(0);
  });

  it("renders Me again when the identity stops resolving, a required binding included", async () => {
    identity = () => Promise.resolve({ kind: "human", login: "octocat" });
    render(view(discussion(saved())));
    await waitFor(() => expect(authors()[0]).toBe("octocat"));

    identity = () => Promise.reject("github_token_selection_required");
    tokensChanged();
    await waitFor(() => expect(authors()[0]).toBe("Me"));
  });

  it("CVP-FR-TIBK: the Quote action names the same label the author line shows", async () => {
    identity = () => Promise.resolve({ kind: "human", login: "octocat" });
    render(view(discussion(saved())));
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Quote comment 1 by octocat" }),
      ).toBeInTheDocument(),
    );
  });

  it("CVP-FR-TIBK, CMT-FR-24: the composer is enabled for Me and shows no token reason", async () => {
    render(view(discussion(saved())));
    await waitFor(() => expect(authors()[0]).toBe("Me"));
    expect(screen.getByRole("textbox", { name: "Reply to thread d1" })).toBeEnabled();
    expect(screen.queryByText(/GitHub account|Global settings|Choose a token/i)).toBeNull();
  });

  it("CVP-FR-32: a comment of the local participant stays the author's own after a token resolves", async () => {
    identity = () => Promise.resolve({ kind: "human", login: "octocat" });
    render(view(discussion(saved()), { identity: { kind: "human", login: "octocat" } }));
    await waitFor(() => expect(authors()[0]).toBe("octocat"));
    const own = Array.from(document.querySelectorAll(".comment")).map((el) =>
      el.getAttribute("data-own"),
    );
    expect(own[0]).toBe("true");
    expect(own[1]).toBe("false");
  });
});

describe("DQA-FR-GOZY: answering under the local participant", () => {
  const set: PendingQuestionSet = {
    setId: "s1",
    discussionId: "d1",
    askedBy: { kind: "agent", agentId: "a1", handle: "arch", title: "Architect" },
    askedAt: "2026-02-01T00:00:00Z",
    questions: [
      {
        position: 1,
        text: "Which session?",
        options: [{ position: 1, value: "kickoff" }],
      },
    ],
  };

  it("submits the answers once, supplying no author, with no picker and no token error", async () => {
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "resolve_comment_author_identity") return LOCAL;
      if (cmd === "read_discussion_question_set") return set;
      if (cmd === "submit_discussion_question_answers") {
        void args;
        return {
          discussion: discussion([
            comment("q1", "Which session?", { kind: "agent", agentId: "a1", handle: "arch" }),
            comment("a1", "kickoff", LOCAL),
          ]),
          finalAnswerCommentId: "a1",
        };
      }
      return [];
    });
    render(view(discussion(saved())));
    act(() => publishSet("d1", set));
    await waitFor(() =>
      expect(screen.getByTestId("discussion-questions-submit")).toBeInTheDocument(),
    );
    fireEvent.click(screen.getAllByRole("radio")[0]);
    fireEvent.click(screen.getByTestId("discussion-questions-submit"));

    await waitFor(() =>
      expect(
        invokeMock.mock.calls.filter((c) => c[0] === "submit_discussion_question_answers"),
      ).toHaveLength(1),
    );
    const call = invokeMock.mock.calls.find(
      (c) => c[0] === "submit_discussion_question_answers",
    )!;
    expect(Object.keys(call[1] as object).sort()).toEqual(["answers", "discussionId", "setId"]);
    expect(screen.queryByText(/GitHub account|Global settings|Choose a token/i)).toBeNull();
  });
});
