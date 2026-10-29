import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Strict: the log sink is answered, and any other command is refused. A
// refused submission logs, and its flush must not reach the real runtime.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === "append_log_records") return undefined;
    throw new Error(`unexpected command ${cmd}`);
  }),
}));

import { DiscussionQuestions } from "./index";
import {
  DESCRIPTION_PLACEHOLDER,
  setHasDescriptions,
  shownDescription,
} from "./answers";
import { forgetEveryDraft } from "../../state/questionAnswerDrafts";
import { resetLogBufferForTest } from "../../logging";
import type { PendingQuestionSet } from "../../types";

/** A set whose options carry descriptions, as the tool records every set. */
function describedSet(setId = "s1", count = 2): PendingQuestionSet {
  return {
    setId,
    discussionId: "t1",
    askedBy: { kind: "agent", agentId: "a1", handle: "arch", title: "Architect" },
    askedAt: "2026-09-10T12:00:00Z",
    questions: Array.from({ length: count }, (_, index) => ({
      position: index + 1,
      text: `Question ${index + 1}?`,
      options: [
        {
          position: 1,
          value: `One ${index + 1}`,
          description: `Easy to find ${index + 1}; it grows large.`,
        },
        {
          position: 2,
          value: `Two ${index + 1}`,
          description: `Each stays small ${index + 1}.`,
        },
      ],
    })),
  };
}

/** DQA-FR-EWUG: a set an older build recorded, with no description at all. */
function legacySet(): PendingQuestionSet {
  const set = describedSet("legacy", 1);
  return {
    ...set,
    questions: set.questions.map((question) => ({
      ...question,
      options: question.options.map(({ position, value }) => ({ position, value })),
    })),
  };
}

function renderBlock(
  set: PendingQuestionSet,
  onSubmit: (answers: unknown) => Promise<unknown> = vi.fn().mockResolvedValue(undefined),
) {
  const onSubmitted = vi.fn();
  render(
    <DiscussionQuestions
      set={set}
      onSubmit={onSubmit}
      onSubmitted={onSubmitted}
      describeError={(reason) => `refused: ${reason}`}
    />,
  );
  return { onSubmit, onSubmitted };
}

const detail = () => screen.getByTestId("discussion-questions-detail");
const optionRow = (label: string) =>
  screen.getByText(label, { selector: ".discussion-questions__option" }).closest("label")!;
const optionRadio = (label: string) => within(optionRow(label)).getByRole("radio");
const ownRow = () => screen.getByTestId("discussion-questions-own");
const ownRadio = () => within(ownRow()).getByRole("radio");

describe("the two rails of a described set", () => {
  beforeEach(() => {
    forgetEveryDraft();
    resetLogBufferForTest();
  });
  afterEach(() => cleanup());

  it("DQA-FR-MIBQ, DQA-FR-QWTB: the answers stand in the left rail and one description in the right", () => {
    renderBlock(describedSet());
    const rails = screen.getByTestId("discussion-questions-rails");
    // The left rail holds the rows by their labels; the right rail no control.
    const group = rails.querySelector(".discussion-questions__group")!;
    expect(within(group as HTMLElement).getAllByRole("radio")).toHaveLength(3);
    expect(optionRow("One 1")).toBeInTheDocument();
    expect(within(detail()).queryAllByRole("radio")).toHaveLength(0);
    expect(within(detail()).queryAllByRole("button")).toHaveLength(0);
    expect(detail().parentElement).toBe(rails);
  });

  it("DQA-FR-TKUM: with nothing hovered and nothing selected, the rail shows the placeholder", () => {
    renderBlock(describedSet());
    expect(detail()).toHaveTextContent(DESCRIPTION_PLACEHOLDER);
    expect(detail()).toHaveAttribute("data-placeholder", "true");
  });

  it("DQA-FR-TKUM: the hovered option's description shows, and leaving falls back", () => {
    renderBlock(describedSet());
    fireEvent.pointerEnter(optionRow("Two 1"));
    expect(detail()).toHaveTextContent("Each stays small 1.");
    expect(detail()).not.toHaveAttribute("data-placeholder");

    fireEvent.pointerLeave(optionRow("Two 1"));
    expect(detail()).toHaveTextContent(DESCRIPTION_PLACEHOLDER);
  });

  it("DQA-FR-TKUM: with no row hovered, the selected option's description shows; a hover wins over it", () => {
    renderBlock(describedSet());
    fireEvent.click(optionRadio("One 1"));
    expect(detail()).toHaveTextContent("Easy to find 1; it grows large.");

    fireEvent.pointerEnter(optionRow("Two 1"));
    expect(detail()).toHaveTextContent("Each stays small 1.");
    fireEvent.pointerLeave(optionRow("Two 1"));
    expect(detail()).toHaveTextContent("Easy to find 1; it grows large.");
  });

  it("DQA-FR-USTA: hovering the own-answer row shows the placeholder, even over a selection", () => {
    renderBlock(describedSet());
    fireEvent.click(optionRadio("One 1"));
    fireEvent.pointerEnter(ownRow());
    expect(detail()).toHaveTextContent(DESCRIPTION_PLACEHOLDER);
    fireEvent.pointerLeave(ownRow());
    expect(detail()).toHaveTextContent("Easy to find 1; it grows large.");
  });

  it("DQA-FR-USTA: choosing the own answer shows the placeholder", () => {
    renderBlock(describedSet());
    fireEvent.click(optionRadio("One 1"));
    fireEvent.click(ownRadio());
    expect(detail()).toHaveTextContent(DESCRIPTION_PLACEHOLDER);
  });

  it("DQA-FR-KBJA: keyboard focus on an option row counts as hovering it", () => {
    renderBlock(describedSet());
    fireEvent.focus(optionRadio("Two 1"));
    expect(detail()).toHaveTextContent("Each stays small 1.");
    fireEvent.blur(optionRadio("Two 1"));
    expect(detail()).toHaveTextContent(DESCRIPTION_PLACEHOLDER);

    // Focus on the own-answer row shows the placeholder over a selection, and
    // the selection returns when the focus leaves.
    fireEvent.click(optionRadio("One 1"));
    expect(detail()).toHaveTextContent("Easy to find 1; it grows large.");
    fireEvent.focus(ownRadio());
    expect(detail()).toHaveTextContent(DESCRIPTION_PLACEHOLDER);
    fireEvent.blur(ownRadio());
    expect(detail()).toHaveTextContent("Easy to find 1; it grows large.");
  });

  it("DQA-FR-TKUM: leaving one row after entering the next keeps the next one's description", () => {
    renderBlock(describedSet());
    fireEvent.pointerEnter(optionRow("One 1"));
    fireEvent.pointerEnter(optionRow("Two 1"));
    fireEvent.pointerLeave(optionRow("One 1"));
    expect(detail()).toHaveTextContent("Each stays small 1.");
  });

  it("DQA-FR-TKUM: a move to another question clears the hovered row", () => {
    renderBlock(describedSet());
    fireEvent.pointerEnter(optionRow("Two 1"));
    fireEvent.click(screen.getByTestId("discussion-questions-next"));
    expect(detail()).toHaveTextContent(DESCRIPTION_PLACEHOLDER);

    // The page that arrived shows its own selection, not the first page's.
    fireEvent.click(optionRadio("One 2"));
    expect(detail()).toHaveTextContent("Easy to find 2; it grows large.");
  });

  it("DQA-FR-CLHJ: each option row is described by its own description, and the rail is hidden from assistive technology", () => {
    renderBlock(describedSet());
    expect(optionRadio("One 1")).toHaveAccessibleDescription(
      "Easy to find 1; it grows large.",
    );
    expect(optionRadio("Two 1")).toHaveAccessibleDescription("Each stays small 1.");
    expect(ownRadio()).not.toHaveAttribute("aria-describedby");
    expect(detail()).toHaveAttribute("aria-hidden", "true");
    // The label stays the row's name: the description is not part of it.
    expect(optionRadio("One 1")).toHaveAccessibleName("One 1");
  });

  it("DQA-FR-JADB: the description shows with provider citation markers hidden", () => {
    const set = describedSet("s1", 1);
    set.questions[0].options[0].description = "Easy to find cite here.";
    renderBlock(set);
    fireEvent.pointerEnter(optionRow("One 1"));
    expect(detail().textContent).toBe("Easy to find here.");
    expect(optionRadio("One 1")).toHaveAccessibleDescription("Easy to find here.");
  });

  it("DQA-FR-QWTB, DQA-FR-KDVU: a described set still submits the chosen label as the recorded value", async () => {
    const { onSubmit, onSubmitted } = renderBlock(describedSet("s1", 1));
    fireEvent.click(optionRadio("Two 1"));
    fireEvent.click(screen.getByTestId("discussion-questions-submit"));
    expect(onSubmit).toHaveBeenCalledWith([
      { questionPosition: 1, optionPosition: 2, optionValue: "Two 1" },
    ]);
    await waitFor(() => expect(onSubmitted).toHaveBeenCalled());
  });
});

describe("the rail under the pointer and the keyboard together", () => {
  beforeEach(() => {
    forgetEveryDraft();
    resetLogBufferForTest();
  });
  afterEach(() => cleanup());

  it("DQA-FR-KBJA, DQA-FR-TKUM: the pointer wins while it is on a row, and the focused row returns when it leaves", () => {
    renderBlock(describedSet());
    fireEvent.focus(optionRadio("One 1"));
    fireEvent.pointerEnter(optionRow("Two 1"));
    expect(detail()).toHaveTextContent("Each stays small 1.");
    fireEvent.pointerLeave(optionRow("Two 1"));
    expect(detail()).toHaveTextContent("Easy to find 1; it grows large.");
  });

  it("DQA-FR-TKUM, DQA-FR-USTA: with the own answer chosen, hovering an option shows it, and leaving shows the placeholder", () => {
    renderBlock(describedSet());
    fireEvent.click(ownRadio());
    fireEvent.pointerEnter(optionRow("Two 1"));
    expect(detail()).toHaveTextContent("Each stays small 1.");
    fireEvent.pointerLeave(optionRow("Two 1"));
    expect(detail()).toHaveTextContent(DESCRIPTION_PLACEHOLDER);
  });

  it("DQA-FR-TKUM: a three-option question describes each of its three rows", () => {
    const set = describedSet("s3", 1);
    set.questions[0].options.push({
      position: 3,
      value: "Three 1",
      description: "One per layer.",
    });
    renderBlock(set);
    expect(optionRadio("Three 1")).toHaveAccessibleDescription("One per layer.");
    fireEvent.pointerEnter(optionRow("Three 1"));
    expect(detail()).toHaveTextContent("One per layer.");
  });

  it("DQA-FR-TKUM: paging back shows the earlier question's selection, with no hovered row left over", () => {
    renderBlock(describedSet());
    fireEvent.click(optionRadio("Two 1"));
    fireEvent.pointerEnter(optionRow("Two 1"));
    fireEvent.click(screen.getByTestId("discussion-questions-next"));
    fireEvent.click(screen.getByTestId("discussion-questions-previous"));
    expect(detail()).toHaveTextContent("Each stays small 1.");
    // The row hovered before the page turn is no longer held.
    fireEvent.click(optionRadio("One 1"));
    expect(detail()).toHaveTextContent("Easy to find 1; it grows large.");
  });

  it("DQA-FR-TKUM: a set that replaces another starts from no hovered row", () => {
    const view = render(
      <DiscussionQuestions
        set={describedSet("s1", 1)}
        onSubmit={vi.fn().mockResolvedValue(undefined)}
        onSubmitted={vi.fn()}
        describeError={(reason) => reason}
      />,
    );
    fireEvent.pointerEnter(optionRow("One 1"));
    view.rerender(
      <DiscussionQuestions
        set={describedSet("s2", 1)}
        onSubmit={vi.fn().mockResolvedValue(undefined)}
        onSubmitted={vi.fn()}
        describeError={(reason) => reason}
      />,
    );
    expect(detail()).toHaveTextContent(DESCRIPTION_PLACEHOLDER);
  });

  it("DQA-FR-MIBQ, DQA-FR-OGVU: the description follows the group, so a stacked layout puts it below the note", () => {
    renderBlock(describedSet());
    fireEvent.click(optionRadio("One 1"));
    const group = screen.getByTestId("discussion-questions-rails").firstElementChild!;
    expect(group).toHaveClass("discussion-questions__group");
    expect(group.nextElementSibling).toBe(detail());
    expect(group).toContainElement(screen.getByTestId("discussion-questions-note"));
  });

  it("DQA-FR-CLHJ: two renderings of one set describe their rows by ids of their own", () => {
    const set = describedSet("s1", 1);
    renderBlock(set);
    renderBlock(set);
    const ids = screen
      .getAllByRole("radio")
      .map((radio) => radio.getAttribute("aria-describedby"))
      .filter((id): id is string => id !== null);
    expect(ids).toHaveLength(4);
    expect(new Set(ids).size).toBe(4);
  });

  it("DQA-FR-XQOR: a description is plain text, not Markdown or markup", () => {
    const set = describedSet("s1", 1);
    set.questions[0].options[0].description = "**bold** and <b>tag</b>";
    renderBlock(set);
    fireEvent.pointerEnter(optionRow("One 1"));
    expect(detail().textContent).toBe("**bold** and <b>tag</b>");
    expect(detail().querySelector("b, strong")).toBeNull();
  });
});

describe("the block's own behaviour, on a described set", () => {
  beforeEach(() => {
    forgetEveryDraft();
    resetLogBufferForTest();
  });
  afterEach(() => cleanup());

  it("DQA-FR-ROXG, DQA-FR-JJON, DQA-FR-MIBQ: the note is offered beside a chosen option, in the left rail", () => {
    renderBlock(describedSet());
    expect(screen.queryByTestId("discussion-questions-note")).toBeNull();
    fireEvent.click(optionRadio("One 1"));
    const note = screen.getByTestId("discussion-questions-note");
    expect(note.closest(".discussion-questions__group")).not.toBeNull();
    expect(detail()).not.toContainElement(note);
  });

  it("DQA-FR-FCZL, DQA-FR-MIBQ: the own answer opens its field in the left rail and takes the keyboard", () => {
    renderBlock(describedSet());
    fireEvent.click(ownRadio());
    const field = screen.getByTestId("discussion-questions-own-field");
    expect(field).toHaveFocus();
    expect(field.closest(".discussion-questions__group")).not.toBeNull();
  });

  it("DQA-FR-TFCE: a page turn takes the keyboard to the question that arrived", () => {
    renderBlock(describedSet());
    fireEvent.click(screen.getByTestId("discussion-questions-next"));
    expect(screen.getByTestId("discussion-questions-question")).toHaveFocus();
    expect(screen.getByText("Question 2?")).toBeInTheDocument();
  });

  it("DQA-FR-WKTP, DQA-FR-TKUM: a refusal keeps the selection and the rail as the author left them", async () => {
    const onSubmit = vi.fn().mockRejectedValue("question_answers_incomplete");
    renderBlock(describedSet("s1", 1), onSubmit);
    fireEvent.click(optionRadio("Two 1"));
    fireEvent.click(screen.getByTestId("discussion-questions-submit"));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "refused: question_answers_incomplete",
    );
    expect(optionRadio("Two 1")).toBeChecked();
    expect(detail()).toHaveTextContent("Each stays small 1.");
  });
});

describe("a set recorded without descriptions", () => {
  beforeEach(() => {
    forgetEveryDraft();
    resetLogBufferForTest();
  });
  afterEach(() => cleanup());

  it("DQA-FR-EWUG: renders in one column with no right rail and no placeholder", () => {
    renderBlock(legacySet());
    expect(screen.queryByTestId("discussion-questions-rails")).toBeNull();
    expect(screen.queryByTestId("discussion-questions-detail")).toBeNull();
    expect(screen.queryByText(DESCRIPTION_PLACEHOLDER)).toBeNull();
    fireEvent.pointerEnter(optionRow("One 1"));
    expect(screen.queryByText(DESCRIPTION_PLACEHOLDER)).toBeNull();
    expect(optionRadio("One 1")).not.toHaveAttribute("aria-describedby");
    fireEvent.focus(optionRadio("One 1"));
    expect(screen.queryByText(DESCRIPTION_PLACEHOLDER)).toBeNull();
  });

  it("DQA-FR-EWUG: answers and submits on the same terms as any other set", async () => {
    const { onSubmit, onSubmitted } = renderBlock(legacySet());
    fireEvent.click(optionRadio("One 1"));
    fireEvent.click(screen.getByTestId("discussion-questions-submit"));
    expect(onSubmit).toHaveBeenCalledWith([
      { questionPosition: 1, optionPosition: 1, optionValue: "One 1" },
    ]);
    await waitFor(() => expect(onSubmitted).toHaveBeenCalled());
  });
});

describe("the rail helpers", () => {
  beforeEach(() => {
    forgetEveryDraft();
    resetLogBufferForTest();
  });

  it("DQA-FR-MIBQ, DQA-FR-EWUG: a set renders rails when any option carries a non-blank description", () => {
    expect(setHasDescriptions(describedSet())).toBe(true);
    expect(setHasDescriptions(legacySet())).toBe(false);
    const blank = legacySet();
    blank.questions[0].options[0].description = "   ";
    expect(setHasDescriptions(blank)).toBe(false);
  });

  it("DQA-FR-TKUM, DQA-FR-USTA: the shown description follows hover, then selection, then none", () => {
    const question = describedSet().questions[0];
    const draft = { selected: 1, ownWords: false, typed: "", note: "" };
    expect(shownDescription(question, 2, draft)).toBe("Each stays small 1.");
    expect(shownDescription(question, null, draft)).toBe("Easy to find 1; it grows large.");
    expect(shownDescription(question, "own", draft)).toBeNull();
    expect(shownDescription(question, null, { ...draft, ownWords: true })).toBeNull();
    expect(shownDescription(question, null, { ...draft, selected: null })).toBeNull();
    // A position the question does not record, and a blank description.
    expect(shownDescription(question, 7, draft)).toBeNull();
    const blank = { ...question, options: [{ ...question.options[0], description: "  " }] };
    expect(shownDescription(blank, null, draft)).toBeNull();
  });
});
