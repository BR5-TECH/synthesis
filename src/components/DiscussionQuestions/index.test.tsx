import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { DiscussionQuestions } from "./index";
import { composeAnswers, progressLine } from "./answers";
import {
  forgetEveryDraft,
  recallAnswer,
} from "../../state/questionAnswerDrafts";
import type { PendingQuestionSet } from "../../types";

function setOf(setId: string, count: number): PendingQuestionSet {
  return {
    setId,
    discussionId: "t1",
    askedBy: {
      kind: "agent",
      agentId: "a1",
      handle: "arch",
      title: "Architect",
    },
    askedAt: "2026-09-10T12:00:00Z",
    questions: Array.from({ length: count }, (_, index) => ({
      position: index + 1,
      text: `Question ${index + 1}?`,
      options: [
        { position: 1, value: `one for ${index + 1}` },
        { position: 2, value: `two for ${index + 1}` },
      ],
    })),
  };
}

function renderBlock(
  set: PendingQuestionSet,
  onSubmit = vi.fn().mockResolvedValue(undefined),
) {
  const onSubmitted = vi.fn();
  const block = (held: PendingQuestionSet) => (
    <DiscussionQuestions
      set={held}
      onSubmit={onSubmit}
      onSubmitted={onSubmitted}
      describeError={(reason) => `refused: ${reason}`}
    />
  );
  const view = render(block(set));
  return {
    onSubmit,
    onSubmitted,
    /** A set that replaces another, without remounting the block. */
    rerender: (held: PendingQuestionSet) => view.rerender(block(held)),
  };
}

const submit = () => screen.getByTestId("discussion-questions-submit");
/** DQA-FR-FCZL: the own-answer row's radio, which is the last of the group. */
const own = () =>
  screen
    .getByTestId("discussion-questions-own")
    .querySelector("input[type=radio]") as HTMLInputElement;
const ownField = () => screen.getByTestId("discussion-questions-own-field");
const noteField = () => screen.getByTestId("discussion-questions-note");
const next = () => screen.getByTestId("discussion-questions-next");
const previous = () => screen.getByTestId("discussion-questions-previous");

describe("the discussion question block", () => {
  beforeEach(() => {
    forgetEveryDraft();
  });
  afterEach(cleanup);

  it("DQA-FR-PDLN: names the agent that asked and its recorded title", () => {
    renderBlock(setOf("s1", 1));
    expect(screen.getByText("arch")).toBeInTheDocument();
    expect(screen.getByText("Architect")).toBeInTheDocument();
  });

  it("DQA-FR-BXHU, DQA-FR-GVSA: shows the first question, one at a time", () => {
    renderBlock(setOf("s1", 3));
    expect(screen.getByText("Question 1?")).toBeInTheDocument();
    expect(screen.queryByText("Question 2?")).not.toBeInTheDocument();
    // At the first question there is nowhere back to go.
    expect(previous()).toBeDisabled();
    expect(next()).toBeEnabled();
  });

  it("DQA-FR-XQOR, DQA-FR-QWTB: renders the recorded options, none preselected", () => {
    const set = setOf("s1", 1);
    renderBlock(set);
    // Every recorded option, and the own answer after them (DQA-FR-FCZL).
    const options = screen.getAllByRole("radio");
    expect(options).toHaveLength(set.questions[0].options.length + 1);
    for (const option of options) expect(option).not.toBeChecked();
    expect(screen.getByText("one for 1")).toBeInTheDocument();
    expect(screen.getByText("two for 1")).toBeInTheDocument();
    // And nothing the block composed of its own around them.
    expect(options[2]).toBe(own());
  });

  it("DQA-FR-BXHU, DQA-FR-TFCE: pages both ways and disables at the two ends", () => {
    renderBlock(setOf("s1", 2));
    fireEvent.click(next());
    expect(screen.getByText("Question 2?")).toBeInTheDocument();
    expect(next()).toBeDisabled();
    expect(previous()).toBeEnabled();

    fireEvent.click(previous());
    expect(screen.getByText("Question 1?")).toBeInTheDocument();
    expect(previous()).toBeDisabled();
  });

  it("DQA-FR-ZLKD: a question is never skipped because another is unanswered", () => {
    renderBlock(setOf("s1", 3));
    fireEvent.click(next());
    expect(screen.getByText("Question 2?")).toBeInTheDocument();
    fireEvent.click(next());
    expect(screen.getByText("Question 3?")).toBeInTheDocument();
  });

  it("DQA-FR-HLDS: choosing another option replaces the selection", () => {
    renderBlock(setOf("s1", 1));
    const [first, second] = screen.getAllByRole("radio");
    fireEvent.click(first);
    expect(first).toBeChecked();
    fireEvent.click(second);
    expect(second).toBeChecked();
    expect(first).not.toBeChecked();
  });

  it("DQA-FR-JJON: no note is offered until a recorded option is selected", () => {
    renderBlock(setOf("s1", 1));
    expect(screen.queryByTestId("discussion-questions-note")).not.toBeInTheDocument();

    fireEvent.click(screen.getAllByRole("radio")[0]);
    expect(noteField()).toBeInTheDocument();
  });

  it("DQA-FR-JJON: a question answered in the author's own words offers no note", () => {
    renderBlock(setOf("s1", 1));
    fireEvent.click(screen.getAllByRole("radio")[0]);
    expect(noteField()).toBeInTheDocument();

    fireEvent.click(own());
    expect(screen.queryByTestId("discussion-questions-note")).not.toBeInTheDocument();
  });

  it("DQA-FR-ROXG: the note is a field whose placeholder says what it is for", () => {
    renderBlock(setOf("s1", 1));
    fireEvent.click(screen.getAllByRole("radio")[0]);
    // Nothing to press and nothing to open: the field is there, and its
    // placeholder is the whole of what marks it optional.
    expect(noteField()).toHaveAttribute("placeholder", "Add a note…");
    expect(noteField()).toHaveValue("");
    expect(screen.queryByRole("button", { name: /note/i })).toBeNull();
  });

  it("DQA-FR-ROXG: the note belongs to its own question and survives paging", () => {
    renderBlock(setOf("s1", 2));
    fireEvent.click(screen.getAllByRole("radio")[0]);
    fireEvent.change(noteField(), { target: { value: "about the first" } });
    fireEvent.click(next());
    // The second question's note is its own: nothing is selected there, so
    // there is no note at all yet (DQA-FR-JJON).
    expect(screen.queryByLabelText(/^Note for question/)).not.toBeInTheDocument();
    fireEvent.click(screen.getAllByRole("radio")[0]);
    expect(noteField()).toHaveValue("");
    fireEvent.click(previous());
    // And the first question's note is showing without being asked for again,
    // because it carries text.
    expect(noteField()).toHaveValue("about the first");
  });

  it("DQA-FR-LSNW: a selection survives paging away and back", () => {
    renderBlock(setOf("s1", 2));
    fireEvent.click(screen.getAllByRole("radio")[1]);
    fireEvent.click(next());
    fireEvent.click(previous());
    expect(screen.getAllByRole("radio")[1]).toBeChecked();
  });

  it("DQA-FR-CBQK: two questions reading alike are answered independently", () => {
    // The motivation the requirement states: an answer keyed by anything but the
    // recorded position would attach to whichever question matched first. The
    // recorded positions here are also non-contiguous, so an implementation
    // keyed on the page index answers the wrong one.
    const set: PendingQuestionSet = {
      ...setOf("s1", 0),
      questions: [
        {
          position: 2,
          text: "Which layer?",
          options: [
            { position: 1, value: "ui" },
            { position: 2, value: "core" },
          ],
        },
        {
          position: 7,
          text: "Which layer?",
          options: [
            { position: 1, value: "ui" },
            { position: 2, value: "core" },
          ],
        },
      ],
    };
    const { onSubmit } = renderBlock(set);
    fireEvent.click(screen.getAllByRole("radio")[0]);
    fireEvent.click(next());
    // The second question of the same text is still unanswered.
    for (const option of screen.getAllByRole("radio")) {
      expect(option).not.toBeChecked();
    }
    fireEvent.click(screen.getAllByRole("radio")[1]);
    fireEvent.click(submit());
    expect(onSubmit).toHaveBeenCalledWith([
      { questionPosition: 2, optionPosition: 1, optionValue: "ui" },
      { questionPosition: 7, optionPosition: 2, optionValue: "core" },
    ]);
  });

  it("DQA-FR-TFCE: moving takes the keyboard with it", () => {
    renderBlock(setOf("s1", 2));
    fireEvent.click(next());
    expect(screen.getByTestId("discussion-questions-question")).toHaveFocus();
    fireEvent.click(previous());
    expect(screen.getByTestId("discussion-questions-question")).toHaveFocus();
  });

  it("DQA-FR-WKTP: a refusal arriving as an Error still reads as its typed message", () => {
    // The backend refuses with a bare slug; a transport failure arrives as an
    // Error. Both must reach the message table as the same slug.
    const onSubmit = vi.fn().mockRejectedValue(new Error("discussion_locked"));
    renderBlock(setOf("s1", 1), onSubmit);
    fireEvent.click(screen.getAllByRole("radio")[0]);
    fireEvent.click(submit());
    return waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent("refused: discussion_locked"),
    );
  });

  it("DQA-FR-WGQY: Submit is enabled only once every question is answered", () => {
    renderBlock(setOf("s1", 2));
    expect(submit()).toBeDisabled();
    fireEvent.click(screen.getAllByRole("radio")[0]);
    // One of two answered: still not enough.
    expect(submit()).toBeDisabled();
    fireEvent.click(next());
    fireEvent.click(screen.getAllByRole("radio")[0]);
    expect(submit()).toBeEnabled();
  });

  it("DQA-FR-MJPV: states which question is showing and how many still need one", () => {
    renderBlock(setOf("s1", 3));
    const progress = () => screen.getByTestId("discussion-questions-progress");
    expect(progress()).toHaveTextContent(
      "Question 1 of 3 · 0 answered · 3 still need an answer",
    );
    fireEvent.click(screen.getAllByRole("radio")[0]);
    expect(progress()).toHaveTextContent(
      "Question 1 of 3 · 1 answered · 2 still need an answer",
    );
  });

  it("DQA-FR-KDVU: submits the whole ordered set in one call", async () => {
    const set = setOf("s1", 2);
    const { onSubmit } = renderBlock(set);
    fireEvent.click(screen.getAllByRole("radio")[1]);
    fireEvent.change(noteField(), { target: { value: "keep the diagram here" } });
    fireEvent.click(next());
    fireEvent.click(screen.getAllByRole("radio")[0]);
    fireEvent.click(submit());

    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1));
    expect(onSubmit).toHaveBeenCalledWith([
      {
        questionPosition: 1,
        optionPosition: 2,
        optionValue: "two for 1",
        note: "keep the diagram here",
      },
      { questionPosition: 2, optionPosition: 1, optionValue: "one for 2" },
    ]);
  });

  it("DQA-FR-AYNP: Submit is single-flight, and recovers when the request settles", async () => {
    let release: (() => void) | undefined;
    const onSubmit = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          release = resolve;
        }),
    );
    renderBlock(setOf("s1", 1), onSubmit);
    fireEvent.click(screen.getAllByRole("radio")[0]);

    // Three activations in one tick, dispatched on the element directly so the
    // disabled attribute is not what suppresses them — the guard is.
    const button = submit();
    fireEvent.click(button);
    fireEvent.click(button);
    button.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(onSubmit).toHaveBeenCalledTimes(1);

    // And once the request settles the control is usable again, so a refusal is
    // retried without a reload.
    release?.();
    await waitFor(() => expect(submit()).toBeEnabled());
  });

  it("DQA-FR-WKTP: a refusal keeps every entry and renders the error inline", async () => {
    const onSubmit = vi.fn().mockRejectedValue("question_answers_incomplete");
    renderBlock(setOf("s1", 1), onSubmit);
    fireEvent.click(screen.getAllByRole("radio")[1]);
    fireEvent.change(noteField(), { target: { value: "still here" } });
    fireEvent.click(submit());

    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(
        "refused: question_answers_incomplete",
      ),
    );
    // Nothing the author entered was touched.
    expect(screen.getAllByRole("radio")[1]).toBeChecked();
    expect(noteField()).toHaveValue("still here");
    expect(recallAnswer("s1", 1)).toEqual({ selected: 2, ownWords: false, typed: "", note: "still here" });
  });

  it("DQA-FR-IPFD: an accepted submission tells the caller", async () => {
    const { onSubmitted } = renderBlock(setOf("s1", 1));
    fireEvent.click(screen.getAllByRole("radio")[0]);
    fireEvent.click(submit());
    await waitFor(() => expect(onSubmitted).toHaveBeenCalledTimes(1));
  });

  it("DQA-FR-OMZL: nothing shows the set as answered until the backend accepts", async () => {
    let release: ((value: undefined) => void) | undefined;
    const onSubmit = vi.fn(
      () => new Promise<undefined>((resolve) => (release = resolve)),
    );
    const { onSubmitted } = renderBlock(setOf("s1", 1), onSubmit);
    fireEvent.click(screen.getAllByRole("radio")[0]);
    fireEvent.click(submit());

    // In flight: the block still stands, its controls are held, and the caller
    // has not been told to take it down.
    expect(screen.getByTestId("discussion-questions")).toBeInTheDocument();
    expect(submit()).toBeDisabled();
    expect(screen.getAllByRole("radio")[0]).toBeDisabled();
    expect(onSubmitted).not.toHaveBeenCalled();

    release?.(undefined);
    await waitFor(() => expect(onSubmitted).toHaveBeenCalledTimes(1));
  });

  it("DQA-FR-RDPU, DQA-FR-VNKQ: the block and its question area carry their own treatment", () => {
    // The measure, the centring, and the question area's own background are the
    // stylesheet's (`../../styles/kit/discussion-questions.css`), which
    // `class-coverage.test.ts` proves declares every class named here. What this
    // asserts is the half the component owns: that the block is one region held
    // to a measure and that the question stands in an area of its own inside it,
    // rather than being drawn flat into the surface behind it.
    renderBlock(setOf("s1", 1));
    const block = screen.getByTestId("discussion-questions");
    expect(block).toHaveClass("discussion-questions");
    const area = screen.getByTestId("discussion-questions-question");
    expect(area).toHaveClass("discussion-questions__question");
    expect(block).toContainElement(area);
  });

  it("DQA-FR-EWLB: every control carries the name it would have spelled out", () => {
    renderBlock(setOf("s1", 2));
    fireEvent.click(screen.getAllByRole("radio")[0]);
    expect(screen.getByLabelText("Note for question 1 of 2")).toBeInTheDocument();
    expect(previous()).toHaveTextContent("Previous");
    expect(next()).toHaveTextContent("Next");
    expect(submit()).toHaveTextContent("Submit");

    // DQA-FR-FCZL: and the own answer's own two controls.
    fireEvent.click(own());
    expect(screen.getByText("Something else — I will say")).toBeInTheDocument();
    expect(
      screen.getByLabelText("Your own answer to question 1 of 2"),
    ).toBeInTheDocument();
  });

  it("DQA-FR-SEBN, DQA-FR-MJPV: the dots agree with the progress line and Submit", () => {
    renderBlock(setOf("s1", 2));
    const dots = () => [
      ...screen.getByTestId("discussion-questions-dots").children,
    ];
    const progress = () => screen.getByTestId("discussion-questions-progress");
    expect(dots()[0]).not.toHaveAttribute("data-answered");

    // An answer in the author's own words leaves `selected` null, so a dot that
    // read the selection alone would never light up for it.
    fireEvent.click(own());
    expect(dots()[0]).not.toHaveAttribute("data-answered");
    fireEvent.change(ownField(), { target: { value: "three per layer" } });
    expect(dots()[0]).toHaveAttribute("data-answered", "true");
    expect(progress()).toHaveTextContent("1 answered");

    // And a chosen option lights its own.
    fireEvent.click(next());
    fireEvent.click(screen.getAllByRole("radio")[0]);
    expect(dots()[1]).toHaveAttribute("data-answered", "true");
    expect(submit()).toBeEnabled();
  });

  it("DQA-FR-GVSA: a set that replaces another reads none of its answers", () => {
    const { rerender } = renderBlock(setOf("s1", 1));
    fireEvent.click(screen.getAllByRole("radio")[0]);
    fireEvent.change(noteField(), { target: { value: "for the first set" } });

    rerender(setOf("s2", 1));
    // A new set has a new id, so it reads none of the entries of the one
    // before it — there is no answer, and so no note is offered.
    for (const option of screen.getAllByRole("radio")) {
      expect(option).not.toBeChecked();
    }
    expect(screen.queryByTestId("discussion-questions-note")).not.toBeInTheDocument();
  });

  it("DQA-FR-FCZL: the own-answer field takes the keyboard when its row is chosen", () => {
    renderBlock(setOf("s1", 2));
    fireEvent.click(own());
    expect(ownField()).toHaveFocus();
  });

  it("DQA-FR-TFCE: a page turn takes the keyboard to the question, not to a field", () => {
    // The two focus rules run on the same component and could fight: this one
    // is what says the older rule still wins when a page turns.
    renderBlock(setOf("s1", 2));
    fireEvent.click(own());
    expect(ownField()).toHaveFocus();

    fireEvent.click(next());
    expect(screen.getByTestId("discussion-questions-question")).toHaveFocus();
  });

  it("DQA-FR-LSNW: own words survive paging away and back", () => {
    renderBlock(setOf("s1", 2));
    fireEvent.click(own());
    fireEvent.change(ownField(), { target: { value: "three per layer" } });

    fireEvent.click(next());
    // The second question is its own: the own row is there and unchosen.
    expect(own()).not.toBeChecked();
    expect(
      screen.queryByTestId("discussion-questions-own-field"),
    ).not.toBeInTheDocument();

    fireEvent.click(previous());
    expect(own()).toBeChecked();
    expect(ownField()).toHaveValue("three per layer");
  });

  it("DQA-FR-WKTP: a refusal keeps an answer given in the author's own words", async () => {
    const onSubmit = vi.fn().mockRejectedValue("question_answers_incomplete");
    renderBlock(setOf("s1", 1), onSubmit);
    fireEvent.click(own());
    fireEvent.change(ownField(), { target: { value: "three per layer" } });
    fireEvent.click(submit());

    await waitFor(() => expect(screen.getByRole("alert")).toBeInTheDocument());
    expect(own()).toBeChecked();
    expect(ownField()).toHaveValue("three per layer");
    expect(recallAnswer("s1", 1)).toEqual({
      selected: null,
      ownWords: true,
      typed: "three per layer",
      note: "",
    });
  });

  it("DQA-FR-ROXG, DQA-FR-FCZL: both fields start at one line", () => {
    // The height is set from the content by the block, and a textarea falls back
    // to its own default of two rows while that is being measured. Without this
    // the field stands two lines tall however little is written in it, and the
    // rule it is written on sits an empty line below the words.
    renderBlock(setOf("s1", 1));
    fireEvent.click(own());
    expect(ownField()).toHaveAttribute("rows", "1");

    fireEvent.click(screen.getAllByRole("radio")[0]);
    expect(noteField()).toHaveAttribute("rows", "1");
  });

  it("DQA-FR-DYFR: a re-read down to no questions renders nothing and does not throw", () => {
    // The block returns early when the re-read set records no question at its
    // page. Every hook has to be reached before that return, or React matches a
    // different number of them on the render after and tears the tree down.
    const { rerender } = renderBlock(setOf("s1", 2));
    fireEvent.click(screen.getAllByRole("radio")[0]);
    expect(noteField()).toBeInTheDocument();

    const emptied: PendingQuestionSet = { ...setOf("s1", 2), questions: [] };
    expect(() => rerender(emptied)).not.toThrow();
    expect(screen.queryByTestId("discussion-questions")).not.toBeInTheDocument();

    // And it comes back when the questions do.
    expect(() => rerender(setOf("s1", 2))).not.toThrow();
    expect(screen.getByTestId("discussion-questions")).toBeInTheDocument();
  });

  it("DQA-FR-FCZL: the own answer stands on every question of the set", () => {
    renderBlock(setOf("s1", 3));
    for (let question = 1; question <= 3; question += 1) {
      expect(own()).toBeInTheDocument();
      if (question < 3) fireEvent.click(next());
    }
  });

  it("DQA-FR-FCZL: choosing the own answer opens a field, blank until it is written in", () => {
    renderBlock(setOf("s1", 1));
    // The field is not there until the row is chosen.
    expect(
      screen.queryByTestId("discussion-questions-own-field"),
    ).not.toBeInTheDocument();

    fireEvent.click(own());
    expect(ownField()).toBeInTheDocument();
    // A row pressed is not yet a question answered.
    expect(submit()).toBeDisabled();
    fireEvent.change(ownField(), { target: { value: "   " } });
    expect(submit()).toBeDisabled();
    fireEvent.change(ownField(), { target: { value: "three, one per layer" } });
    expect(submit()).toBeEnabled();
  });

  it("DQA-FR-HLDS: the own answer and the options are one group", () => {
    renderBlock(setOf("s1", 1));
    const [first] = screen.getAllByRole("radio");
    fireEvent.click(first);
    expect(first).toBeChecked();

    fireEvent.click(own());
    expect(own()).toBeChecked();
    expect(first).not.toBeChecked();

    // And back the other way, which restores the option that was chosen.
    fireEvent.click(first);
    expect(first).toBeChecked();
    expect(own()).not.toBeChecked();
  });

  it("DQA-FR-KDVU, DQA-FR-JJON: an own answer submits its words and no option", async () => {
    const { onSubmit } = renderBlock(setOf("s1", 1));
    // A note written against an option first, to prove it is not sent.
    fireEvent.click(screen.getAllByRole("radio")[0]);
    fireEvent.change(noteField(), { target: { value: "not for this" } });
    fireEvent.click(own());
    fireEvent.change(ownField(), { target: { value: "three, one per layer" } });
    fireEvent.click(submit());

    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1));
    expect(onSubmit).toHaveBeenCalledWith([
      { questionPosition: 1, ownAnswer: "three, one per layer" },
    ]);
  });

  it("DQA-FR-JJON: the note the author wrote returns with the option they choose", () => {
    renderBlock(setOf("s1", 1));
    fireEvent.click(screen.getAllByRole("radio")[0]);
    fireEvent.change(noteField(), { target: { value: "kept" } });

    fireEvent.click(own());
    expect(screen.queryByLabelText(/^Note for question/)).not.toBeInTheDocument();

    fireEvent.click(screen.getAllByRole("radio")[0]);
    expect(noteField()).toHaveValue("kept");
  });
});

describe("the answer helpers", () => {
  beforeEach(() => {
    forgetEveryDraft();
  });
  afterEach(cleanup);

  it("DQA-FR-WGQY: an incomplete set composes nothing to send", () => {
    expect(composeAnswers(setOf("s1", 2))).toBeNull();
  });

  it("DQA-FR-MJPV: the progress line drops the outstanding clause when none is", () => {
    const set = setOf("s1", 1);
    expect(progressLine(set, 0)).toBe("Question 1 of 1 · 0 answered · 1 still needs an answer");
  });

  it("DQA-FR-JADB: hides citation markers in the question and its options, and submits the recorded value", () => {
    const span = "\uE200cite\uE202turn0search1\uE201";
    const set = setOf("s1", 1);
    set.questions[0].text = `Which store? ${span}`;
    set.questions[0].options[0].value = `Postgres ${span}`;
    const { onSubmit } = renderBlock(set);

    expect(screen.getByTestId("discussion-questions-question")).not.toHaveTextContent(/turn0search1/);
    expect(screen.getByText("Which store?")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("radio", { name: "Postgres" }));
    fireEvent.click(submit());
    expect(onSubmit).toHaveBeenCalledWith([
      { questionPosition: 1, optionPosition: 1, optionValue: `Postgres ${span}` },
    ]);
  });
});
