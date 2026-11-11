/**
 * The question set a plan update records in a change discussion
 * (`../../../specifications/ui/DQA-discussion-question-answering.md`
 * DQA-FR-XFEE, DQA-FR-ZNOQ, DQA-FR-QWTB; `../../../specifications/core/CMS-comments-storage.md`
 * CMS-FR-CSCD). It renders and submits on the terms of every set, its head
 * names the Synthesis participant, and a question with no option is answered
 * in the author's own words alone.
 */
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { DiscussionQuestions } from "./index";
import { composeAnswers } from "./answers";
import { forgetEveryDraft } from "../../state/questionAnswerDrafts";
import type { PendingQuestionSet } from "../../types";

/** OPU-FR-VXVO: the set of a plan update, as the Synthesis participant asked it. */
function planSet(): PendingQuestionSet {
  return {
    setId: "qs-plan",
    discussionId: "pt-1",
    askedBy: { kind: "agent", agentId: "synthesis", handle: "Synthesis", title: "" },
    askedAt: "2026-11-08T09:00:00Z",
    questions: [
      {
        position: 1,
        text: "Where does the logout entry stand?",
        // OPU-FR-VXVO: the option's value is the escalation option's summary.
        options: [
          { position: 1, value: "In the shared header", description: "One entry, one test." },
          { position: 2, value: "On every page", description: "More routes to test." },
        ],
      },
      { position: 2, text: "What does the notice say?", options: [] },
    ],
  };
}

const submit = () => screen.getByTestId("discussion-questions-submit");
const next = () => screen.getByTestId("discussion-questions-next");

beforeEach(() => {
  forgetEveryDraft();
});
afterEach(cleanup);

describe("the question set of a plan update", () => {
  it("DQA-FR-XFEE: the head names the Synthesis participant", () => {
    render(
      <DiscussionQuestions set={planSet()} onSubmit={vi.fn()} onSubmitted={vi.fn()} describeError={(r) => r} />,
    );
    expect(screen.getByLabelText("Questions from Synthesis")).toBeInTheDocument();
    expect(screen.getByText("Synthesis")).toBeInTheDocument();
  });

  it("DQA-FR-QWTB, DQA-FR-ZNOQ, DQA-FR-XFEE: the options read as their summaries, a question with no option is the field alone, and Submit sends the whole set", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const onSubmitted = vi.fn();
    render(
      <DiscussionQuestions set={planSet()} onSubmit={onSubmit} onSubmitted={onSubmitted} describeError={(r) => r} />,
    );
    const first = screen.getByTestId("discussion-questions-question");
    expect(within(first).getByText("In the shared header")).toBeInTheDocument();
    expect(within(first).getByText("On every page")).toBeInTheDocument();
    // No option is preselected.
    for (const radio of within(first).getAllByRole("radio")) expect(radio).not.toBeChecked();
    await userEvent.click(within(first).getAllByRole("radio")[0]);

    await userEvent.click(next());
    const second = screen.getByTestId("discussion-questions-question");
    expect(within(second).queryAllByRole("radio")).toHaveLength(0);
    expect(submit()).toBeDisabled();
    await userEvent.type(
      within(second).getByRole("textbox", { name: "Your own answer to question 2 of 2" }),
      "Your session ended.",
    );
    expect(submit()).toBeEnabled();
    await userEvent.click(submit());
    expect(onSubmit).toHaveBeenCalledTimes(1);
    expect(onSubmit).toHaveBeenCalledWith([
      { questionPosition: 1, optionPosition: 1, optionValue: "In the shared header" },
      { questionPosition: 2, ownAnswer: "Your session ended." },
    ]);
    await vi.waitFor(() => expect(onSubmitted).toHaveBeenCalled());
  });

  it("DQA-FR-ZNOQ, CMS-FR-CSCD: a question with no option composes no answer while its text is blank", () => {
    const set = planSet();
    expect(composeAnswers({ ...set, questions: [set.questions[1]] })).toBeNull();
  });
});
