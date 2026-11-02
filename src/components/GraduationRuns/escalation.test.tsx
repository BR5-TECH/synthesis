import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { invoke } from "@tauri-apps/api/core";

import { resetLogBufferForTest } from "../../logging";
import {
  forgetEveryDraft,
  recallAnswer,
  recallRefusal,
  rememberAnswer,
  streamOwner,
} from "../../state/escalationDrafts";
import { graduationErrorMessage } from "../../state/graduation";
import { makeEscalation, makeRun } from "../../test/graduationFixtures";
import type { GraduationEscalation, GraduationRun } from "../../types";
import { noteQuestionShown } from "../../voice/voiceController";
import { noteSpokenPage, registerQuestionBlock } from "../../voice/voiceQuestions";
import { askingTurn, EscalationForm, RunEscalationForm } from "./escalation";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
// DQA-FR-CYVA: voice mode is a discussion's alone, so an escalation reaches it
// by no route.
vi.mock("../../voice/voiceQuestions", async (original) => ({
  ...(await original<typeof import("../../voice/voiceQuestions")>()),
  registerQuestionBlock: vi.fn(() => () => {}),
  noteSpokenPage: vi.fn(),
}));
vi.mock("../../voice/voiceController", async (original) => ({
  ...(await original<typeof import("../../voice/voiceController")>()),
  noteQuestionShown: vi.fn(),
}));

const invoked = vi.mocked(invoke);

/** What `answer_graduation_escalation` does in this test; accepts by default. */
let answer: () => Promise<unknown> = () => Promise.resolve(undefined);
/** A deferred answer still pending when a test ends is settled here. */
let release: (() => void) | null = null;

beforeEach(() => {
  resetLogBufferForTest();
  forgetEveryDraft();
  answer = () => Promise.resolve(undefined);
  release = null;
  vi.mocked(registerQuestionBlock).mockClear();
  vi.mocked(noteQuestionShown).mockClear();
  vi.mocked(noteSpokenPage).mockClear();
  invoked.mockReset();
  invoked.mockImplementation((command: string) => {
    if (command === "append_log_records") return Promise.resolve(undefined);
    if (command === "answer_graduation_escalation") return answer();
    return Promise.reject(new Error(`unexpected command ${command}`));
  });
});
afterEach(() => {
  release?.();
  cleanup();
});

function runOf(escalation: GraduationEscalation, id = "r1"): GraduationRun {
  return makeRun(id, "awaiting_author", { escalation });
}

function draw(escalation: GraduationEscalation, id = "r1", busy = false) {
  const onAnswered = vi.fn();
  const view = render(
    <RunEscalationForm run={runOf(escalation, id)} busy={busy} onAnswered={onAnswered} />,
  );
  const redraw = (next: GraduationEscalation, nextBusy = busy) =>
    view.rerender(
      <RunEscalationForm run={runOf(next, id)} busy={nextBusy} onAnswered={onAnswered} />,
    );
  return { onAnswered, redraw, unmount: view.unmount };
}

const twoQuestions = (over: Partial<GraduationEscalation> = {}): GraduationEscalation =>
  makeEscalation({
    // Recorded at 5 before 2, so the recorded order and the order their
    // positions sort in disagree.
    questions: [
      { position: 5, question: "Should search hold its place?", options: [] },
      {
        position: 2,
        question: "Which unit should paging move by?",
        options: [
          { answer: "record", summary: "By record", description: "A page is a fixed number of records." },
          { answer: "segment", summary: "By segment", description: "A page is one phase and pass." },
        ],
      },
    ],
    ...over,
  });

const block = () => screen.getByTestId("discussion-questions");
const progress = () => screen.getByTestId("discussion-questions-progress");
const submit = () => screen.getByTestId("discussion-questions-submit");
const next = () => screen.getByTestId("discussion-questions-next");
const previous = () => screen.getByTestId("discussion-questions-previous");
const answerCalls = () =>
  invoked.mock.calls.filter(([command]) => command === "answer_graduation_escalation");

/** Answer both questions of `twoQuestions`: own text on 5, "By segment" on 2. */
async function answerBoth(note?: string) {
  await userEvent.type(
    screen.getByRole("textbox", { name: "Your own answer to question 1 of 2" }),
    "Yes",
  );
  await userEvent.click(next());
  await userEvent.click(screen.getByRole("radio", { name: "By segment" }));
  if (note !== undefined) {
    await userEvent.type(screen.getByTestId("discussion-questions-note"), note);
  }
}

describe("the block an escalation renders", () => {
  it("GEA-FR-QBWN, GEA-FR-KMVR: the shared question block, headed by the turn that asked", () => {
    draw(twoQuestions({ origin: "review" }));
    expect(screen.getByRole("region", { name: "Questions from the review turn" })).toBe(block());
    expect(block().querySelector("header")).toHaveTextContent("Review turn");
    expect(screen.queryByText("Send answers")).toBeNull();
    expect(screen.queryByTestId("graduation-question")).toBeNull();
  });

  it.each([
    ["work", "Work turn"],
    ["review", "Review turn"],
    ["semantic_merge", "Merge turn"],
  ] as const)("GEA-FR-KMVR: the origin %s is named as the %s", (origin, name) => {
    expect(askingTurn(origin)).toBe(name);
  });

  it("GEA-FR-SXXB, ESU-FR-12, GRU-FR-MRPE: the reason is verbatim, escaped, and closed by default", () => {
    const reason = "Line one <b>bold</b>\n\nLine  two";
    draw(twoQuestions({ reason }));
    const disclosure = screen.getByTestId("graduation-reason");
    expect(disclosure.tagName).toBe("DETAILS");
    expect(disclosure).not.toHaveAttribute("open");
    expect(within(disclosure).getByText("Why the run stopped").tagName).toBe("SUMMARY");
    const text = disclosure.querySelector("p")!;
    expect(text.textContent).toBe(reason);
    expect(text).not.toBeVisible();
    expect(disclosure.querySelector("b")).toBeNull();
    // The questions show in full while it is closed.
    expect(screen.getByText("Should search hold its place?")).toBeVisible();
    (disclosure as HTMLDetailsElement).open = true;
    expect(text).toBeVisible();
  });

  it("GEA-FR-LWQD: an update's disclosure says the update stopped", () => {
    render(
      <EscalationForm
        ownerId={streamOwner("w1")}
        escalation={twoQuestions({ origin: "semantic_merge" })}
        busy={false}
        onSend={() => Promise.resolve()}
        onAnswered={() => {}}
        subject="update"
      />,
    );
    expect(screen.getByText("Why the update stopped")).toBeInTheDocument();
    expect(block().querySelector("header")).toHaveTextContent("Merge turn");
  });

  it("DQA-FR-BXHU, GEA-FR-QBWN: one question at a time, paged both ways, in recorded order", async () => {
    draw(twoQuestions());
    expect(progress()).toHaveTextContent("Question 1 of 2");
    expect(screen.getByText("Should search hold its place?")).toBeInTheDocument();
    expect(screen.queryByText("Which unit should paging move by?")).toBeNull();
    expect(previous()).toBeDisabled();
    const dots = screen.getByTestId("discussion-questions-dots").children;
    expect(dots).toHaveLength(2);
    expect(dots[0]).toHaveAttribute("data-current", "true");

    await userEvent.click(next());
    expect(progress()).toHaveTextContent("Question 2 of 2");
    expect(screen.getByText("Which unit should paging move by?")).toBeInTheDocument();
    expect(next()).toBeDisabled();
    // DQA-FR-TFCE: focus lands in the question that arrived.
    expect(screen.getByTestId("discussion-questions-question")).toHaveFocus();
  });

  it("GEA-FR-DNBI, GEA-FR-SKWP, GEA-FR-RZQX, GEA-FR-NPUV: a response reads as its summary, described by its description, and nothing is preselected", async () => {
    draw(twoQuestions());
    await userEvent.click(next());
    const record = screen.getByRole("radio", { name: "By record" });
    expect(record).toHaveAccessibleDescription("A page is a fixed number of records.");
    for (const radio of screen.getAllByRole("radio")) expect(radio).not.toBeChecked();
    expect(screen.queryByTestId("discussion-questions-note")).toBeNull();
    expect(block().textContent).not.toMatch(/confiden/i);
  });

  it("GEA-FR-SKWP: the value a response is answered with is never what the author reads", () => {
    draw(
      makeEscalation({
        questions: [
          {
            position: 1,
            question: "Which side wins?",
            options: [
              { answer: "rebase_onto_theirs", summary: "Take theirs", description: "Theirs stands." },
            ],
          },
        ],
      }),
    );
    expect(block().textContent).not.toContain("rebase_onto_theirs");
    expect(screen.getByRole("radio", { name: "Take theirs" })).toBeInTheDocument();
  });

  it("GEA-FR-ZGHY, DQA-FR-ZNOQ: a question with no responses is the field alone, and its text answers it", async () => {
    draw(makeEscalation({ questions: [{ position: 3, question: "Anything else?", options: [] }] }));
    const question = screen.getByTestId("discussion-questions-question");
    expect(within(question).queryAllByRole("radio")).toHaveLength(0);
    expect(screen.queryByTestId("discussion-questions-rails")).toBeNull();
    const field = within(question).getByRole("textbox", { name: "Your own answer to question 1 of 1" });
    expect(field).toHaveValue("");
    expect(submit()).toBeDisabled();
    await userEvent.type(field, "No");
    expect(submit()).toBeEnabled();
    expect(screen.getByTestId("discussion-questions-dots").children[0]).toHaveAttribute(
      "data-answered",
      "true",
    );
    expect(screen.queryByTestId("discussion-questions-note")).toBeNull();
  });

  it("DQA-FR-CYVA: an escalation registers nothing with voice mode and speaks nothing", async () => {
    draw(twoQuestions());
    await userEvent.click(next());
    expect(vi.mocked(registerQuestionBlock)).not.toHaveBeenCalled();
    expect(vi.mocked(noteQuestionShown)).not.toHaveBeenCalled();
    expect(vi.mocked(noteSpokenPage)).not.toHaveBeenCalled();
  });

  it("GEA-FR-BVMQ: it is an in-panel block rather than an overlay of the window", () => {
    draw(twoQuestions());
    const area = screen.getByTestId("graduation-escalation");
    expect(area).not.toHaveAttribute("role", "dialog");
    expect(area.querySelector("[aria-modal]")).toBeNull();
  });
});

describe("sending the set", () => {
  it("GEA-FR-VWQH, GEA-FR-TSQO, GEA-FR-PLTO: Submit sends the whole ordered set once, a chosen response with its trimmed note", async () => {
    const { onAnswered } = draw(twoQuestions());
    expect(submit()).toBeDisabled();
    await answerBoth("  keep it short ");
    expect(submit()).toBeEnabled();
    await userEvent.click(submit());
    await waitFor(() => expect(onAnswered).toHaveBeenCalledTimes(1));
    expect(answerCalls()).toHaveLength(1);
    expect(answerCalls()[0][1]).toStrictEqual({
      runId: "r1",
      answers: [
        { position: 5, answer: "Yes", summary: "Yes" },
        { position: 2, answer: "segment", summary: "By segment", note: "keep it short" },
      ],
    });
  });

  it("GEA-FR-PLTO, DQA-FR-JJON, GEA-FR-QKDA: the own answer offers no note and sends none, and the note returns with the option", async () => {
    const { onAnswered } = draw(twoQuestions());
    await answerBoth("for the option");
    await userEvent.click(screen.getByRole("radio", { name: "Something else — I will say" }));
    expect(screen.queryByTestId("discussion-questions-note")).toBeNull();
    await userEvent.click(screen.getByRole("radio", { name: "By segment" }));
    expect(screen.getByTestId("discussion-questions-note")).toHaveValue("for the option");
    await userEvent.click(screen.getByRole("radio", { name: "Something else — I will say" }));
    await userEvent.type(
      screen.getByRole("textbox", { name: "Your own answer to question 2 of 2" }),
      "neither",
    );
    await userEvent.click(submit());
    await waitFor(() => expect(onAnswered).toHaveBeenCalled());
    const [, args] = answerCalls()[0] as [string, { answers: Record<string, unknown>[] }];
    expect(args.answers[1]).toStrictEqual({ position: 2, answer: "neither", summary: "neither" });
  });

  it("GEA-FR-VIPR, DQA-FR-AYNP: one submission at a time, and nothing reads as answered until the backend accepts", async () => {
    answer = () =>
      new Promise((resolve) => {
        release = () => resolve(undefined);
      });
    const { onAnswered } = draw(twoQuestions());
    await answerBoth("n");
    await userEvent.dblClick(submit());
    await waitFor(() => expect(submit()).toBeDisabled());
    expect(answerCalls()).toHaveLength(1);
    for (const radio of screen.getAllByRole("radio")) expect(radio).toBeDisabled();
    for (const field of screen.getAllByRole("textbox")) expect(field).toBeDisabled();
    expect(previous()).toBeDisabled();
    expect(onAnswered).not.toHaveBeenCalled();
    expect(block()).toBeInTheDocument();
    release!();
    await waitFor(() => expect(onAnswered).toHaveBeenCalledTimes(1));
    // GEA-FR-KTME: the accepted set takes its draft with it.
    expect(recallAnswer("r1", 2).selected).toBeNull();
    expect(recallRefusal("r1")).toBeNull();
  });

  it("GEA-FR-TEMG: a refusal renders inline at the foot of the block, keeps every answer, and a retry succeeds", async () => {
    answer = () => Promise.reject("run_state_not_permitted");
    const { onAnswered } = draw(twoQuestions());
    await answerBoth("my note");
    await userEvent.click(submit());
    const alert = await within(block()).findByRole("alert");
    expect(alert).toHaveTextContent(graduationErrorMessage("run_state_not_permitted"));
    expect(screen.getByRole("radio", { name: "By segment" })).toBeChecked();
    expect(screen.getByTestId("discussion-questions-note")).toHaveValue("my note");
    expect(submit()).toBeEnabled();
    expect(previous()).toBeEnabled();
    expect(onAnswered).not.toHaveBeenCalled();

    answer = () => Promise.resolve(undefined);
    await userEvent.click(submit());
    await waitFor(() => expect(onAnswered).toHaveBeenCalledTimes(1));
    expect(answerCalls()).toHaveLength(2);
  });

  it("DQA-FR-EWLB, GEA-FR-TEMG: a refused submission gives the keyboard back to Submit", async () => {
    answer = () => Promise.reject("run_state_not_permitted");
    draw(twoQuestions());
    await answerBoth();
    submit().focus();
    await userEvent.keyboard("{Enter}");
    await within(block()).findByRole("alert");
    await waitFor(() => expect(submit()).toHaveFocus());
  });

  it("GEA-FR-TEMG: a refusal that arrives as an Error reads as its message, not as the error's name", async () => {
    answer = () => Promise.reject(new Error("run_state_not_permitted"));
    draw(twoQuestions());
    await answerBoth();
    await userEvent.click(submit());
    const alert = await within(block()).findByRole("alert");
    expect(alert.textContent).not.toMatch(/^Error:/);
    expect(alert).toHaveTextContent(graduationErrorMessage("run_state_not_permitted"));
  });

  it("busy: while the run is busy no control of the block can be used", () => {
    rememberAnswer("r1", 5, { selected: null, ownWords: true, typed: "Yes", note: "" });
    const { redraw } = draw(twoQuestions(), "r1", true);
    expect(screen.getByRole("textbox")).toBeDisabled();
    expect(next()).toBeDisabled();
    expect(submit()).toBeDisabled();
    redraw(twoQuestions(), false);
    expect(screen.getByRole("textbox")).toBeEnabled();
    expect(next()).toBeEnabled();
  });
});

describe("the unsent answer draft", () => {
  it("GEA-FR-QULY: an answer and its note survive paging", async () => {
    draw(twoQuestions());
    await answerBoth("noted");
    await userEvent.click(previous());
    expect(screen.getByRole("textbox", { name: "Your own answer to question 1 of 2" })).toHaveValue("Yes");
    await userEvent.click(next());
    expect(screen.getByRole("radio", { name: "By segment" })).toBeChecked();
    expect(screen.getByTestId("discussion-questions-note")).toHaveValue("noted");
  });

  it("GEA-FR-QULY, GEA-FR-TEMG: the draft and the refusal survive the block being drawn again", async () => {
    answer = () => Promise.reject("run_state_not_permitted");
    const escalation = twoQuestions();
    const { unmount } = draw(escalation);
    await answerBoth();
    await userEvent.click(submit());
    await within(block()).findByRole("alert");
    unmount();
    draw(escalation);
    expect(within(block()).getByRole("alert")).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Your own answer to question 1 of 2" })).toHaveValue("Yes");
  });

  it("GEA-FR-XTUE, GEA-FR-THNY: the draft is keyed by its run, so no run reads another's", async () => {
    const escalation = twoQuestions();
    const { unmount } = draw(escalation, "r1");
    await userEvent.type(screen.getByRole("textbox"), "Yes");
    unmount();
    draw(escalation, "r2");
    expect(screen.getByRole("textbox")).toHaveValue("");
  });

  it("GEA-FR-VBUG: a re-read keeps every entry it still records, and drops the rest", async () => {
    const { redraw } = draw(twoQuestions());
    await answerBoth();
    redraw(
      makeEscalation({ questions: [{ position: 5, question: "Should search hold its place?", options: [] }] }),
    );
    // DQA-FR-DYFR: the page it stood on is gone, so the block shows what is left.
    expect(progress()).toHaveTextContent("Question 1 of 1");
    expect(screen.getByRole("textbox")).toHaveValue("Yes");
    redraw(twoQuestions());
    await userEvent.click(next());
    expect(screen.getByRole("radio", { name: "By segment" })).not.toBeChecked();
  });

  it("DQA-FR-GVSA, GEA-FR-KTME: a re-read keeps the page; a later escalation opens on its first question with nothing of the earlier one", async () => {
    answer = () => Promise.reject("run_state_not_permitted");
    const { redraw } = draw(twoQuestions());
    await answerBoth();
    await userEvent.click(submit());
    await within(block()).findByRole("alert");

    redraw(twoQuestions({ reason: "A newer account." }));
    expect(progress()).toHaveTextContent("Question 2 of 2");
    expect(screen.getByTestId("graduation-reason")).toHaveTextContent("A newer account.");

    redraw(twoQuestions({ raisedAt: "2026-09-07T10:00:00Z" }));
    expect(progress()).toHaveTextContent("Question 1 of 2");
    expect(previous()).toBeDisabled();
    expect(within(block()).queryByRole("alert")).toBeNull();
    expect(screen.getByRole("textbox")).toHaveValue("");
  });

  it("GEA-FR-KTME, DQA-FR-AYNP: a send that settles after a later escalation arrived changes nothing of the later one", async () => {
    answer = () =>
      new Promise((resolve) => {
        release = () => resolve(undefined);
      });
    const { onAnswered, redraw } = draw(twoQuestions());
    await answerBoth();
    await userEvent.click(submit());
    await waitFor(() => expect(submit()).toBeDisabled());

    const later = twoQuestions({ raisedAt: "2026-09-07T10:00:00Z" });
    redraw(later);
    // The new set's controls are not held closed by the old send.
    await userEvent.type(screen.getByRole("textbox"), "later");
    expect(screen.getByRole("textbox")).toBeEnabled();
    // Settle the old send, then let every handler chained on it run: a
    // macrotask runs only after every pending microtask has.
    await act(async () => {
      release!();
      release = null;
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    expect(answerCalls()).toHaveLength(1);
    expect(onAnswered).not.toHaveBeenCalled();
    expect(recallAnswer("r1", 5).typed).toBe("later");
  });

  it("GEA-FR-KTME, GEA-FR-TEMG: a refusal that arrives after a later escalation is not that escalation's, and the later one can be sent", async () => {
    let refuse: (() => void) | null = null;
    answer = () =>
      new Promise((_, reject) => {
        refuse = () => reject("run_state_not_permitted");
      });
    const { onAnswered, redraw } = draw(twoQuestions());
    await answerBoth();
    await userEvent.click(submit());
    await waitFor(() => expect(submit()).toBeDisabled());

    redraw(makeEscalation({ raisedAt: "2026-09-07T10:00:00Z", questions: [{ position: 1, question: "Later?", options: [] }] }));
    await act(async () => {
      refuse!();
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    expect(within(block()).queryByRole("alert")).toBeNull();
    expect(recallRefusal("r1")).toBeNull();

    answer = () => Promise.resolve(undefined);
    await userEvent.type(screen.getByRole("textbox"), "yes");
    await userEvent.click(submit());
    await waitFor(() => expect(onAnswered).toHaveBeenCalledTimes(1));
    expect(answerCalls()).toHaveLength(2);
  });

  it("DQA-FR-AYNP: two activations before the block repaints are one submission", async () => {
    answer = () =>
      new Promise((resolve) => {
        release = () => resolve(undefined);
      });
    draw(twoQuestions());
    await answerBoth();
    const button = submit();
    act(() => {
      fireEvent.click(button);
      fireEvent.click(button);
    });
    expect(answerCalls()).toHaveLength(1);
  });

  it("GEA-FR-THNY: what the author enters is written to no backend operation", async () => {
    draw(twoQuestions());
    await answerBoth("a note");
    const commands = new Set(invoked.mock.calls.map(([command]) => command));
    commands.delete("append_log_records");
    expect([...commands]).toEqual([]);
  });
});
