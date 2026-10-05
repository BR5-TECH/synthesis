/**
 * A discussion holding a question set, as the rail renders it
 * (`../../specifications/ui/DQA-discussion-question-answering.md`).
 *
 * The card is the one place the block, the disabled composer, the withheld Quote
 * and the paired history all follow from, and every presentation that renders a
 * discussion renders this card (DQA-FR-NRZB). Driven through the rail rather
 * than through the block alone, because what is under test is the integration
 * the block's own tests deliberately mock away.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));

import { CommentRail } from "./CommentRail";
import { publishSet, clearQuestionSets } from "../state/questionSets";
import { forgetEveryDraft } from "../state/questionAnswerDrafts";
import type { AnchoredThread } from "../state/commentAnchors";
import type {
  Comment,
  Discussion,
  Participant,
  PendingQuestionSet,
} from "../types";

const author: Participant = { kind: "human", login: "raver119" };
const arch: Participant = {
  kind: "agent",
  agentId: "a1",
  handle: "arch",
  title: "Architect",
};

const THREAD_ID = "disc-q";

function comment(id: string, by: Participant, body: string): Comment {
  return {
    id,
    author: by,
    body,
    quotes: [],
    attachments: [],
    createdAt: "2026-02-01T00:00:00Z",
  };
}

function discussion(comments: Comment[]): AnchoredThread {
  const thread: Discussion = {
    id: THREAD_ID,
    target: { kind: "draft", draftId: "d1" },
    fragmentTarget: null,
    comments,
    locked: false,
    resolved: false,
    createdAt: "2026-02-01T00:00:00Z",
    updatedAt: "2026-02-01T00:00:00Z",
  };
  return { thread, anchor: null };
}

function set(): PendingQuestionSet {
  return {
    setId: "s1",
    discussionId: THREAD_ID,
    askedBy: arch,
    askedAt: "2026-02-01T00:00:00Z",
    questions: [
      {
        position: 1,
        text: "One spec or two?",
        options: [
          { position: 1, value: "one" },
          { position: 2, value: "two" },
        ],
      },
    ],
  };
}

const noop = async () => {};
const refuse = async (): Promise<never> => {
  throw new Error("this test opens no discussion");
};

function draw(comments: Comment[]) {
  render(
    <CommentRail
      threads={[]}
      discussions={[discussion(comments)]}
      anchorTops={{}}
      scrollTop={0}
      arrangement="beside"
      identity={author}
      identityBlock={null}
      blocked={false}
      focusedThreadId={null}
      onFocusThread={() => {}}
      draft={null}
      onOpenDraft={refuse}
      onCancelDraft={() => {}}
      onReply={noop}
      onSetLock={noop}
      onSetResolved={noop}
      errors={{}}
      showResolved={false}
      onToggleResolved={() => {}}
      agents={[]}
      pendingTurns={[]}
      turnFailures={{}}
      onCancelTurn={() => {}}
    />,
  );
}

const opening = () => comment("c0", author, "@arch what would you change?");

beforeEach(() => {
  clearQuestionSets();
  forgetEveryDraft();
});
afterEach(cleanup);

describe("a discussion holding a question set", () => {
  it("DQA-FR-NRZB: renders the block on the card", () => {
    publishSet(THREAD_ID, set());
    draw([opening()]);

    expect(screen.getByTestId("discussion-questions")).toBeInTheDocument();
    expect(screen.getByText("One spec or two?")).toBeInTheDocument();
  });

  it("DQA-FR-TVMH: the block is outside the discussion's history", () => {
    publishSet(THREAD_ID, set());
    draw([opening()]);

    // The message list holds the conversation's one comment and nothing of the
    // block: it contributes no comment and occupies no position in the list.
    const messages = document.querySelector(".comment-card__messages");
    expect(messages).not.toBeNull();
    expect(
      within(messages as HTMLElement).queryByTestId("discussion-questions"),
    ).toBeNull();
    expect(within(messages as HTMLElement).getAllByText(/what would you change/)).toHaveLength(1);
  });

  it("DQA-FR-PXNC: the composer is disabled and says why", () => {
    publishSet(THREAD_ID, set());
    draw([opening()]);

    expect(screen.getByTestId("discussion-questions-composer-note")).toHaveTextContent(
      "Answer the questions above to continue the discussion.",
    );
    expect(document.querySelector(".comment-composer")).toBeNull();
    // And it says nothing about the agent, the turn, or how long an answer has
    // been owed.
    const note = screen.getByTestId("discussion-questions-composer-note").textContent ?? "";
    expect(note).not.toMatch(/arch|turn|waiting|ago/i);
  });

  it("DQA-FR-VJHT: no other human-authored contribution is offered", () => {
    publishSet(THREAD_ID, set());
    draw([opening()]);

    // Quote seeds the composer, so it is withheld with it.
    expect(screen.queryByLabelText(/^Quote comment/)).toBeNull();
  });

  it("DQA-FR-VJHT: reading, locking and resolving are untouched", () => {
    publishSet(THREAD_ID, set());
    draw([opening()]);

    // The conversation is still readable and the card's own controls stand.
    expect(screen.getByText(/what would you change/)).toBeInTheDocument();
    expect(document.querySelector(".comment-card__messages")).not.toBeNull();
  });

  it("a discussion holding no set renders the ordinary composer and no block", () => {
    draw([opening()]);

    expect(screen.queryByTestId("discussion-questions")).toBeNull();
    expect(screen.queryByTestId("discussion-questions-composer-note")).toBeNull();
    expect(document.querySelector(".comment-composer")).not.toBeNull();
  });

  it("DQA-FR-FBWO: a submitted pair renders as one joined entry", () => {
    draw([
      opening(),
      comment("s1:01-1q", arch, "One spec or two?\n\n1. one\n2. two"),
      comment("s1:01-2a", author, "**Selected option:** two"),
    ]);

    const question = document.querySelector('[data-pair="question"]');
    const answer = document.querySelector('[data-pair="answer"]');
    expect(question).not.toBeNull();
    expect(answer).not.toBeNull();
    // The answer is the very next entry, which is what the single rule joins.
    expect(question?.nextElementSibling).toBe(answer);
    // And the opening comment is no part of it.
    expect(document.querySelectorAll("[data-pair]")).toHaveLength(2);
  });

  it("DQA-FR-FBWO, ADQ-FR-RECR: an own answer draws as one entry too", () => {
    // The rail is where the entry is actually drawn, so the second answer body
    // has to be recognised here and not only in the pairing helper.
    draw([
      opening(),
      comment("s1:01-1q", arch, "One spec or two?\n\n1. one\n2. two"),
      comment("s1:01-2a", author, "**Own answer:** three, one per layer"),
    ]);

    const question = document.querySelector('[data-pair="question"]');
    const answer = document.querySelector('[data-pair="answer"]');
    expect(question).not.toBeNull();
    expect(answer).not.toBeNull();
    expect(question?.nextElementSibling).toBe(answer);
    expect(document.querySelectorAll("[data-pair]")).toHaveLength(2);
  });

  it("DQA-FR-GRUV: a comment between the halves breaks the pairing", () => {
    draw([
      opening(),
      comment("s1:01-1q", arch, "One spec or two?"),
      comment("interjection", author, "hold on"),
      comment("s1:01-2a", author, "**Selected option:** two"),
    ]);

    expect(document.querySelectorAll("[data-pair]")).toHaveLength(0);
  });

  it("DQA-FR-NKAX: the appended comments stay quotable ordinary comments", () => {
    draw([
      opening(),
      comment("s1:01-1q", arch, "One spec or two?"),
      comment("s1:01-2a", author, "**Selected option:** two"),
    ]);

    // No set stands any more, so every comment offers Quote — the pair
    // included.
    expect(screen.getAllByLabelText(/^Quote comment/)).toHaveLength(3);
  });
});
