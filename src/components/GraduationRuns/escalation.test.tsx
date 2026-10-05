import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { invoke } from "@tauri-apps/api/core";

import { forgetEveryDraft } from "../../state/escalationDrafts";
import { makeEscalation, makeRun } from "../../test/graduationFixtures";
import type { GraduationEscalation } from "../../types";
import { RunEscalationForm } from "./escalation";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));

const invoked = vi.mocked(invoke);

beforeEach(() => {
  invoked.mockReset();
  invoked.mockResolvedValue(undefined);
  forgetEveryDraft();
});
afterEach(cleanup);

function draw(escalation: GraduationEscalation, id = "r1") {
  const run = makeRun(id, "awaiting_author", { escalation });
  const onAnswered = vi.fn();
  const onError = vi.fn();
  render(
    <RunEscalationForm
      run={run}
      busy={false}
      onAnswered={onAnswered}
      onError={onError}
    />,
  );
  return { run, onAnswered, onError };
}

const twoQuestions = (): GraduationEscalation =>
  makeEscalation({
    // Recorded at 5 before 2, so the order the questions were recorded in and
    // the order their positions sort in disagree.
    questions: [
      { position: 5, question: "Should search hold its place?", options: [] },
      {
        position: 2,
        question: "Which unit should paging move by?",
        options: [
          {
            answer: "record",
            summary: "By record",
            description: "A page is a fixed number of records.",
          },
          {
            answer: "segment",
            summary: "By segment",
            description: "A page is one phase and pass.",
          },
        ],
      },
    ],
  });

describe("what the escalation renders", () => {
  it("GEA-FR-SXXB: the reason and every recorded question, in the order they were recorded", () => {
    const escalation = twoQuestions();
    draw(escalation);
    expect(screen.getByText(escalation.reason)).toBeInTheDocument();
    const asked = screen
      .getAllByTestId("graduation-question")
      .map((question) => within(question).getByRole("group").textContent);
    // The order they were recorded in, which is not the order their positions
    // sort in.
    expect(asked[0]).toMatch(/Should search hold its place\?/);
    expect(asked[1]).toMatch(/Which unit should paging move by\?/);
  });

  it("GEA-FR-ZSDY: the proposed responses and the free text are rows of one group", () => {
    draw(twoQuestions());
    const first = screen.getAllByTestId("graduation-question")[1];
    const radios = within(first).getAllByRole("radio");
    // Two proposed responses and the author's own words.
    expect(radios).toHaveLength(3);
    expect(new Set(radios.map((radio) => radio.getAttribute("name"))).size).toBe(1);
  });

  it("GEA-FR-DNBI: a response reads as its summary with its description beneath", () => {
    draw(twoQuestions());
    expect(screen.getByText("By record")).toBeInTheDocument();
    expect(
      screen.getByText("A page is a fixed number of records."),
    ).toBeInTheDocument();
  });

  it("GEA-FR-YMLV: the count is over the set rather than over the recorded positions", () => {
    draw(twoQuestions());
    const heads = screen
      .getAllByTestId("graduation-question")
      .map((question) => within(question).getByText(/^Question /).textContent);
    // The questions are recorded at positions 5 and 2, which are stable ids
    // rather than places in a set of two.
    expect(heads).toEqual(["Question 1 of 2", "Question 2 of 2"]);
  });

  it("GEA-FR-ZGHY: a question recorded with no proposed responses renders the field alone", () => {
    draw(twoQuestions());
    const alone = screen.getAllByTestId("graduation-question")[0];
    expect(within(alone).queryAllByRole("radio")).toHaveLength(0);
    expect(
      within(alone).getByRole("textbox", { name: "Your answer to question 5" }),
    ).toBeInTheDocument();
  });

  it("GEA-FR-RZQX: nothing is preselected and no field is prefilled", () => {
    draw(twoQuestions());
    for (const radio of screen.getAllByRole("radio")) {
      expect(radio).not.toBeChecked();
    }
    for (const field of screen.getAllByRole("textbox")) {
      expect(field).toHaveValue("");
    }
  });

  it("GEA-FR-SKWP / GEA-FR-NPUV: a response reads as its summary and its description and as nothing else", () => {
    draw(
      makeEscalation({
        questions: [
          {
            position: 1,
            question: "Which side wins?",
            options: [
              {
                answer: "rebase_onto_theirs",
                summary: "Take theirs",
                description: "Their version of the file stands.",
              },
            ],
          },
        ],
      }),
    );
    const area = screen.getByTestId("graduation-escalation");
    // The value the operation is answered with is what goes back, never what
    // the author is asked to read.
    expect(area.textContent).not.toContain("rebase_onto_theirs");
    // A confidence is a value a later contract may ask for; this one does not.
    expect(area.textContent).not.toMatch(/confiden/i);
  });

  it("GEA-FR-BVMQ: it is an in-panel dialog rather than an overlay of the window", () => {
    draw(twoQuestions());
    const area = screen.getByTestId("graduation-escalation");
    expect(area).not.toHaveAttribute("role", "dialog");
    expect(area).not.toHaveAttribute("aria-modal");
  });

  it("GRU-FR-MRPE: agent text renders as escaped plain text", () => {
    draw(
      makeEscalation({
        reason: "# Head <b>bold</b>",
        questions: [
          {
            position: 1,
            question: "<i>Which</i>?",
            options: [
              {
                answer: "a",
                summary: "<b>A</b>",
                description: "<b>because</b>",
              },
            ],
          },
        ],
      }),
    );
    const area = screen.getByTestId("graduation-escalation");
    expect(area.querySelector("b")).toBeNull();
    expect(area.querySelector("i")).toBeNull();
    expect(area.textContent).toContain("<b>bold</b>");
  });
});

describe("what is selected is the answer", () => {
  it("GEA-FR-UEFS: typing selects the free-text row", async () => {
    draw(twoQuestions());
    const first = screen.getAllByTestId("graduation-question")[1];
    await userEvent.click(within(first).getByRole("radio", { name: /By record/ }));
    await userEvent.type(
      within(first).getByRole("textbox", { name: "Your answer to question 2" }),
      "neither",
    );
    expect(within(first).getByRole("radio", { name: /In my own words/ })).toBeChecked();
    expect(within(first).getByRole("radio", { name: /By record/ })).not.toBeChecked();
  });

  it("GEA-FR-GCTA: neither act clears the other's content", async () => {
    draw(twoQuestions());
    const first = screen.getAllByTestId("graduation-question")[1];
    const field = within(first).getByRole("textbox", {
      name: "Your answer to question 2",
    });
    await userEvent.click(within(first).getByRole("radio", { name: /By record/ }));
    await userEvent.type(field, "my words");
    await userEvent.click(within(first).getByRole("radio", { name: /By record/ }));
    // The words are still there to change back to.
    expect(field).toHaveValue("my words");
    await userEvent.click(
      within(first).getByRole("radio", { name: /In my own words/ }),
    );
    expect(field).toHaveValue("my words");
  });

  it("GEA-FR-YLWC / GEA-FR-RVWL: the free-text row selected and empty answers nothing", async () => {
    draw(twoQuestions());
    const first = screen.getAllByTestId("graduation-question")[1];
    await userEvent.click(
      within(first).getByRole("radio", { name: /In my own words/ }),
    );
    expect(screen.getByTestId("graduation-answered-count")).toHaveTextContent(
      "0 of 2 answered",
    );
    await userEvent.type(
      within(first).getByRole("textbox", { name: "Your answer to question 2" }),
      "   ",
    );
    expect(screen.getByTestId("graduation-answered-count")).toHaveTextContent(
      "0 of 2 answered",
    );
  });

  it("GEA-FR-FZMW: the surface says how many questions still need an answer", async () => {
    draw(twoQuestions());
    expect(screen.getByTestId("graduation-answered-count")).toHaveTextContent(
      "2 questions need an answer",
    );
    await userEvent.click(screen.getByRole("radio", { name: /By record/ }));
    expect(screen.getByTestId("graduation-answered-count")).toHaveTextContent(
      "1 of 2 answered · 1 question needs an answer",
    );
  });
});

describe("sending the set", () => {
  const answerBoth = async () => {
    await userEvent.click(screen.getByRole("radio", { name: /By segment/ }));
    await userEvent.type(
      screen.getByRole("textbox", { name: "Your answer to question 5" }),
      "Yes",
    );
  };

  it("GEA-FR-FFKD: Send is enabled only once every recorded question is answered", async () => {
    draw(twoQuestions());
    expect(screen.getByTestId("graduation-send-answers")).toBeDisabled();
    await answerBoth();
    expect(screen.getByTestId("graduation-send-answers")).toBeEnabled();
  });

  it("GEA-FR-VWQH / GEA-FR-BHKU: the whole ordered set goes back once, each entry at its recorded position", async () => {
    const { onAnswered } = draw(twoQuestions());
    await answerBoth();
    await userEvent.click(screen.getByTestId("graduation-send-answers"));
    await waitFor(() => expect(onAnswered).toHaveBeenCalled());
    const calls = invoked.mock.calls.filter(
      ([command]) => command === "answer_graduation_escalation",
    );
    expect(calls).toHaveLength(1);
    const [, args] = calls[0] as [string, { answers: unknown[] }];
    // Every entry names the question's recorded position, in the order the
    // questions were recorded, and a proposed response submits its own answer
    // and its own summary — the description it was described by is submitted
    // nowhere.
    expect(args.answers).toEqual([
      { position: 5, answer: "Yes", summary: "Yes" },
      { position: 2, answer: "segment", summary: "By segment" },
    ]);
  });

  it("GEA-FR-VIPR: nothing reads as answered until the backend has accepted the set", async () => {
    let accept: (() => void) | null = null;
    invoked.mockImplementation(async (command: string) => {
      if (command !== "answer_graduation_escalation") return undefined;
      await new Promise<void>((resolve) => {
        accept = resolve;
      });
      return undefined;
    });
    const { onAnswered } = draw(twoQuestions());
    await answerBoth();
    await userEvent.click(screen.getByTestId("graduation-send-answers"));
    // The call is in flight: the questions are still the surface, and nothing
    // it holds can be changed or sent again.
    await waitFor(() =>
      expect(screen.getByTestId("graduation-send-answers")).toBeDisabled(),
    );
    expect(screen.getByTestId("graduation-escalation")).toBeInTheDocument();
    for (const radio of screen.getAllByRole("radio")) expect(radio).toBeDisabled();
    for (const field of screen.getAllByRole("textbox")) expect(field).toBeDisabled();
    expect(onAnswered).not.toHaveBeenCalled();
    accept!();
    await waitFor(() => expect(onAnswered).toHaveBeenCalled());
  });

  it("GEA-FR-KTME: the draft is discarded once the backend has accepted the set", async () => {
    const escalation = twoQuestions();
    const { onAnswered } = draw(escalation);
    await answerBoth();
    await userEvent.click(screen.getByTestId("graduation-send-answers"));
    await waitFor(() => expect(onAnswered).toHaveBeenCalled());
    cleanup();
    draw(escalation);
    expect(
      screen.getByRole("textbox", { name: "Your answer to question 5" }),
    ).toHaveValue("");
    expect(screen.getByRole("radio", { name: /By segment/ })).not.toBeChecked();
  });

  it("GEA-FR-TEMG: a refusal leaves every entered answer intact", async () => {
    invoked.mockImplementation(async (command: string) => {
      if (command === "answer_graduation_escalation") throw "run_not_waiting";
      return undefined;
    });
    const { onError } = draw(twoQuestions());
    await answerBoth();
    await userEvent.click(screen.getByTestId("graduation-send-answers"));
    await waitFor(() => expect(onError).toHaveBeenCalled());
    expect(screen.getByRole("radio", { name: /By segment/ })).toBeChecked();
    expect(
      screen.getByRole("textbox", { name: "Your answer to question 5" }),
    ).toHaveValue("Yes");
    expect(screen.getByTestId("graduation-send-answers")).toBeEnabled();
  });
});

describe("the unsent answer draft", () => {
  it("GEA-FR-QULY: an answer entered is still there when the form is drawn again", async () => {
    const escalation = twoQuestions();
    draw(escalation);
    await userEvent.type(
      screen.getByRole("textbox", { name: "Your answer to question 5" }),
      "Yes",
    );
    cleanup();
    draw(escalation);
    expect(
      screen.getByRole("textbox", { name: "Your answer to question 5" }),
    ).toHaveValue("Yes");
  });

  it("GEA-FR-XTUE / GEA-FR-THNY: the draft is keyed by its run, so no run reads another's", async () => {
    const escalation = twoQuestions();
    draw(escalation, "r1");
    await userEvent.type(
      screen.getByRole("textbox", { name: "Your answer to question 5" }),
      "Yes",
    );
    cleanup();
    draw(escalation, "r2");
    expect(
      screen.getByRole("textbox", { name: "Your answer to question 5" }),
    ).toHaveValue("");
  });

  it("GEA-FR-VBUG: a re-read keeps every entry it still records a question for", async () => {
    draw(twoQuestions());
    await userEvent.type(
      screen.getByRole("textbox", { name: "Your answer to question 5" }),
      "Yes",
    );
    cleanup();
    // The second escalation records question 5 and nothing else.
    draw(
      makeEscalation({
        questions: [
          { position: 5, question: "Should search hold its place?", options: [] },
        ],
      }),
    );
    expect(
      screen.getByRole("textbox", { name: "Your answer to question 5" }),
    ).toHaveValue("Yes");
    expect(screen.getAllByTestId("graduation-question")).toHaveLength(1);
  });
});
