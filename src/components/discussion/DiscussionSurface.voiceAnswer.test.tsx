/**
 * A question set heard and answered aloud (`VOM-voice-mode.md` VOM-FR-HBLE,
 * VOM-FR-PINR, `DQA-discussion-question-answering.md` DQA-FR-SUUU). The surface,
 * the block, and the controller are real, because the order of their mounts and
 * the page that the block holds are what counts.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";

const api = vi.hoisted(() => ({
  appendVoiceCapture: vi.fn(async () => undefined),
  cancelVoiceCapture: vi.fn(async () => undefined),
  cancelVoiceSpeech: vi.fn(async () => undefined),
  closeVoiceActivitySession: vi.fn(async () => ({ captureId: null })),
  feedVoiceActivity: vi.fn(async () => undefined),
  finishVoiceCapture: vi.fn(async (): Promise<string> => "own words"),
  openVoiceActivitySession: vi.fn(async () => ({ activityId: "va1" })),
  retryVoiceCapture: vi.fn(async (): Promise<string> => "said again"),
  setVoiceActivityGate: vi.fn(async () => undefined),
  speakVoiceSegment: vi.fn(async () => undefined),
  startVoiceCapture: vi.fn(async () => ({ sessionId: "c1", route: "realtime", live: true })),
}));
const submit = vi.hoisted(() => ({
  submitQuestionAnswers: vi.fn(async (): Promise<unknown> => undefined),
}));
const mic = vi.hoisted(() => ({ stop: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));
vi.mock("../../api", async (original) => ({
  ...(await original<typeof import("../../api")>()),
  ...api,
}));
vi.mock("../DiscussionQuestions/submit", async (original) => ({
  ...(await original<typeof import("../DiscussionQuestions/submit")>()),
  submitQuestionAnswers: submit.submitQuestionAnswers,
}));
vi.mock("../../voice/capture", async (original) => ({
  ...(await original<typeof import("../../voice/capture")>()),
  openMicrophone: vi.fn(async () => ({ rate: 48_000, stop: mic.stop })),
}));
vi.mock("../../voice/playback", () => ({
  isPlaybackIdle: vi.fn(() => false),
  pausePlayback: vi.fn(),
  playFrame: vi.fn(),
  prepareAudio: vi.fn(),
  resumePlayback: vi.fn(),
  setDrainedListener: vi.fn(),
  stopPlayback: vi.fn(),
}));

import { resumePlayback } from "../../voice/playback";
import { resetLogBufferForTest } from "../../logging";
import { resetDiscussionFocus } from "../../state/discussionFocus";
import {
  clearAllDiscussionSessions,
  getDiscussionSession,
} from "../../state/discussionSession";
import { recallAnswer, rememberAnswer } from "../../state/questionAnswerDrafts";
import { clearQuestionSets, publishSet } from "../../state/questionSets";
import { getVoiceState, resetVoiceStateForTest } from "../../state/voiceSession";
import type { Discussion, PendingQuestionSet, ProjectAgent } from "../../types";
import {
  SHORT_PRESS_MS,
  TAIL_MS,
  endVoiceForTeardown,
  noteActivity,
  noteDiscussionChanged,
  leaveVoiceMode,
  noteProvider,
  pressRecord,
  resetVoiceControllerForTest,
  retryRecording,
  setResponseMode,
  setVoiceBehaviour,
  voiceComposerMounted,
} from "../../voice/voiceController";
import { DiscussionSurface } from "./DiscussionSurface";

const arch = {
  scope: "project",
  agent: { id: "a1", nickname: "arch", title: "", modelId: "m", instructions: "", voice: "cedar" },
  availability: "ready",
  shadowed: false,
} as unknown as ProjectAgent;

function discussion(): Discussion {
  return {
    id: "d1",
    target: { kind: "artifact", artifactId: "a.md" },
    fragmentTarget: null,
    comments: [],
    resolved: false,
    createdAt: "2026-02-01T00:00:00Z",
    updatedAt: "2026-02-01T00:00:00Z",
  };
}

function question(position: number, text: string) {
  return {
    position,
    text,
    options: [
      { position: 1, value: "One spec", description: "It is easy to find." },
      { position: 2, value: "Two specs", description: "Each stays small." },
    ],
  };
}

const SET = {
  setId: "s1",
  discussionId: "d1",
  askedBy: { kind: "agent", agentId: "a1", handle: "arch", title: "Architect" },
  askedAt: "2026-02-01T00:00:00Z",
  questions: [question(1, "First?"), question(2, "Second?"), question(3, "Third?")],
} as unknown as PendingQuestionSet;

const onReply = vi.fn(async () => undefined);

function surface() {
  return (
    <DiscussionSurface
      discussion={discussion()}
      owner="rail"
      agents={[arch]}
      liveTurns={false}
      onReply={onReply}
      onSetResolved={vi.fn(async () => undefined)}
    />
  );
}

async function pass(ms = 0) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
}

/** In voice mode, with the set standing. */
async function withSet(mode: "voice" | "text" = "voice") {
  const view = render(surface());
  // The read of the mount answers first, or it replaces the set with nothing.
  await pass();
  fireEvent.click(screen.getByTestId("voice-button"));
  act(() => setResponseMode(mode));
  act(() => publishSet("d1", SET));
  await pass();
  return view;
}

/** One press and one more, with `transcript` as what comes back. */
async function say(transcript: string) {
  api.finishVoiceCapture.mockResolvedValueOnce(transcript);
  fireEvent.pointerDown(screen.getByTestId("voice-record"));
  await pass();
  fireEvent.pointerDown(screen.getByTestId("voice-record"));
  await pass(TAIL_MS);
  await pass();
}

function spokenTexts(): string[] {
  const calls = api.speakVoiceSegment.mock.calls as unknown as [string, string][];
  return calls.map((call) => call[1]);
}

const heading = () => screen.getByTestId("discussion-questions-question").querySelector("legend");

function activity(state: string, over: Record<string, unknown> = {}) {
  act(() => noteActivity({ activityId: "va1", state, ...over } as never));
}

async function openVadWithSet() {
  render(surface());
  fireEvent.click(screen.getByTestId("voice-button"));
  fireEvent.click(screen.getByRole("button", { name: "VAD" }));
  fireEvent.pointerDown(screen.getByTestId("voice-record"));
  await pass();
  act(() => setResponseMode("voice"));
  act(() => publishSet("d1", SET));
  await pass();
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  // `clearAllMocks` leaves a queue of once-only answers, and a queue must not
  // reach the next test.
  api.finishVoiceCapture.mockReset();
  api.finishVoiceCapture.mockImplementation(async () => "own words");
  api.speakVoiceSegment.mockReset();
  api.speakVoiceSegment.mockImplementation(async () => undefined);
  submit.submitQuestionAnswers.mockReset();
  submit.submitQuestionAnswers.mockImplementation(async () => undefined);
  resetVoiceStateForTest();
  resetVoiceControllerForTest();
  clearAllDiscussionSessions();
  clearQuestionSets();
  resetDiscussionFocus();
  resetLogBufferForTest();
  noteProvider(true, true);
  setVoiceBehaviour("toggle");
});

afterEach(async () => {
  cleanup();
  endVoiceForTeardown();
  await vi.advanceTimersByTimeAsync(TAIL_MS * 10);
  vi.useRealTimers();
});

describe("question speech", () => {
  it("VOM-FR-HBLE, VOM-FR-UALP: a set speaks its first question, then its labels, then waits", async () => {
    await withSet();
    expect(spokenTexts()).toEqual(["First? One spec Two specs"]);
    expect(api.speakVoiceSegment).toHaveBeenCalledWith(
      expect.any(String),
      expect.any(String),
      "cedar",
      expect.any(Function),
    );
    expect(heading()).toHaveTextContent("First?");
  });

  it("VOM-FR-NWGL: no description and no word for the own-answer row", async () => {
    await withSet();
    expect(spokenTexts()[0]).not.toMatch(/easy to find|Something else/);
  });

  it("VOM-FR-EWOK: with response mode Text nothing speaks", async () => {
    await withSet("text");
    expect(api.speakVoiceSegment).not.toHaveBeenCalled();
  });

  it("VOM-FR-UALK, DQA-FR-ZLKD: a page turn speaks the question that arrives", async () => {
    await withSet();
    fireEvent.click(screen.getByTestId("discussion-questions-next"));
    await pass();
    expect(spokenTexts()).toEqual(["First? One spec Two specs", "Second? One spec Two specs"]);
    expect(api.cancelVoiceSpeech).toHaveBeenCalled();
  });

  it("VOM-FR-UALK: a re-read of the set speaks nothing", async () => {
    await withSet();
    act(() => publishSet("d1", structuredClone(SET)));
    await pass();
    expect(spokenTexts()).toHaveLength(1);
  });

  it("VOM-FR-UALK: a remount of the surface speaks nothing, on any page", async () => {
    const view = await withSet();
    fireEvent.click(screen.getByTestId("discussion-questions-next"));
    await pass();
    expect(spokenTexts()).toHaveLength(2);
    view.rerender(<div />);
    await pass();
    view.rerender(surface());
    await pass();
    expect(spokenTexts()).toHaveLength(2);
  });

  it("VOM-FR-UALK: each move speaks one time, back and forth", async () => {
    await withSet();
    fireEvent.click(screen.getByTestId("discussion-questions-next"));
    await pass();
    fireEvent.click(screen.getByTestId("discussion-questions-previous"));
    await pass();
    expect(spokenTexts()).toEqual([
      "First? One spec Two specs",
      "Second? One spec Two specs",
      "First? One spec Two specs",
    ]);
  });

  it("VOM-FR-UALP: a set with no agent behind it speaks in the default voice", async () => {
    await withSet();
    act(() =>
      publishSet("d1", { ...SET, setId: "s2", askedBy: { kind: "human", login: "me" } } as never),
    );
    await pass();
    const calls = api.speakVoiceSegment.mock.calls as unknown as unknown[][];
    expect(calls[calls.length - 1][2]).toBeNull();
  });

  it("VOM-FR-NWGL: a citation marker in a question is not spoken", async () => {
    const span = "\uE200cite\uE202turn0search1\uE201";
    await withSet();
    act(() =>
      publishSet("d1", { ...SET, setId: "s2", questions: [question(1, `Cited ${span}?`)] } as never),
    );
    await pass();
    expect(spokenTexts()[1]).toBe("Cited? One spec Two specs");
  });

  it("VOM-FR-LKKN, VOM-FR-QPFQ: a question that fails to speak shows an error and stays", async () => {
    api.speakVoiceSegment.mockRejectedValueOnce("speech_failed");
    await withSet();
    expect(getVoiceState()).toMatchObject({ error: "speech_failed", playing: false });
    expect(heading()).toHaveTextContent("First?");
  });

  it("VOM-FR-UALK: a set that replaces another speaks", async () => {
    await withSet();
    act(() => publishSet("d1", { ...SET, setId: "s2" }));
    await pass();
    expect(spokenTexts()).toHaveLength(2);
  });

  it("VOM-FR-RKOG, VOM-FR-LKKN: a new comment replaces the speech at once", async () => {
    await withSet();
    act(() =>
      noteDiscussionChanged({
        ...discussion(),
        comments: [
          {
            id: "r1",
            author: { kind: "agent", agentId: "a1", handle: "arch" },
            body: "<spoken>Another agent.</spoken>",
            quotes: [],
            attachments: [],
            createdAt: "2026-02-01T00:00:00Z",
          },
        ],
      } as never),
    );
    await pass();
    expect(api.cancelVoiceSpeech).toHaveBeenCalled();
    expect(spokenTexts()[1]).toBe("Another agent.");
  });

  it("VOM-FR-OSAQ, VOM-FR-LKKN: Stop leaves the set, and only a move speaks again", async () => {
    rememberAnswer("s1", 1, { selected: 1, ownWords: false, typed: "", note: "kept" });
    const before = recallAnswer("s1", 1);
    await withSet();
    fireEvent.click(screen.getByTestId("voice-stop"));
    await pass();
    expect(getVoiceState().playing).toBe(false);
    expect(heading()).toHaveTextContent("First?");
    expect(recallAnswer("s1", 1)).toEqual(before);
    act(() => publishSet("d1", structuredClone(SET)));
    await pass();
    expect(spokenTexts()).toHaveLength(1);
    fireEvent.click(screen.getByTestId("discussion-questions-next"));
    await pass();
    expect(spokenTexts()).toHaveLength(2);
  });
});

describe("a spoken answer", () => {
  it("VOM-FR-PINR, DQA-FR-SUUU, DQA-FR-ITCZ: it fills the own answer, posts nothing, and moves on", async () => {
    await withSet();
    await say("keep them apart");
    expect(recallAnswer("s1", 1)).toMatchObject({ ownWords: true, typed: "keep them apart" });
    expect(onReply).not.toHaveBeenCalled();
    expect(heading()).toHaveTextContent("Second?");
    expect(spokenTexts()).toContain("Second? One spec Two specs");
  });

  it("VOM-FR-PINR, VOM-FR-EWOK: it works with response mode Text, and nothing speaks", async () => {
    await withSet("text");
    await say("keep them apart");
    expect(recallAnswer("s1", 1).typed).toBe("keep them apart");
    expect(heading()).toHaveTextContent("Second?");
    expect(api.speakVoiceSegment).not.toHaveBeenCalled();
  });

  it("DQA-FR-DYSP: words that match a label stay the own answer", async () => {
    await withSet();
    await say("One spec");
    expect(recallAnswer("s1", 1)).toMatchObject({
      ownWords: true,
      typed: "One spec",
      selected: null,
    });
  });

  it("DQA-FR-SUUU: it replaces an option and own words, and the note stays", async () => {
    rememberAnswer("s1", 1, { selected: 2, ownWords: false, typed: "older", note: "a note" });
    await withSet();
    await say("newer");
    expect(recallAnswer("s1", 1)).toMatchObject({
      ownWords: true,
      typed: "newer",
      note: "a note",
    });
    // What goes out is the own answer alone, whatever the draft keeps.
    fireEvent.click(screen.getByTestId("discussion-questions-previous"));
    await pass();
    expect(screen.getByLabelText(/Your own answer to question 1/)).toHaveValue("newer");
    fireEvent.click(screen.getByTestId("discussion-questions-next"));
    await pass();
    await say("two");
    await say("three");
    const calls = submit.submitQuestionAnswers.mock.calls as unknown as unknown[][];
    const sent = calls[0][2] as Record<string, unknown>[];
    expect(sent[0]).toMatchObject({ ownAnswer: "newer" });
    expect(sent[0].optionValue ?? null).toBeNull();
  });

  it("VOM-FR-FUFE: the words stay as spoken, and the session body stays", async () => {
    await withSet();
    await say("at arch two specs");
    expect(recallAnswer("s1", 1).typed).toBe("at arch two specs");
    expect(getDiscussionSession("d1").body).toBe("");
  });

  it("DQA-FR-ITCZ: the next question comes after the answered one, not from the start", async () => {
    await withSet();
    fireEvent.click(screen.getByTestId("discussion-questions-next"));
    await pass();
    await say("second answer");
    expect(heading()).toHaveTextContent("Third?");
  });

  it("DQA-FR-ITCZ: an answered question is passed over", async () => {
    rememberAnswer("s1", 2, { selected: 1, ownWords: false, typed: "", note: "" });
    await withSet();
    await say("first answer");
    expect(heading()).toHaveTextContent("Third?");
    await say("third answer");
    expect(submit.submitQuestionAnswers).toHaveBeenCalledTimes(1);
  });

  it("DQA-FR-ITCZ: from the last question it goes back to the one left open", async () => {
    await withSet();
    fireEvent.click(screen.getByTestId("discussion-questions-next"));
    fireEvent.click(screen.getByTestId("discussion-questions-next"));
    await pass();
    await say("third answer");
    expect(heading()).toHaveTextContent("First?");
  });

  it("DQA-FR-YMGD, DQA-FR-KDVU: the last answer submits once, with no confirmation", async () => {
    await withSet();
    await say("one");
    await say("two");
    await say("three");
    expect(submit.submitQuestionAnswers).toHaveBeenCalledTimes(1);
    const calls = submit.submitQuestionAnswers.mock.calls as unknown as unknown[][];
    expect(calls[0].slice(0, 2)).toEqual(["d1", "s1"]);
    expect(calls[0][2]).toMatchObject([
      { ownAnswer: "one" },
      { ownAnswer: "two" },
      { ownAnswer: "three" },
    ]);
    expect(onReply).not.toHaveBeenCalled();
  });

  it("DQA-FR-YMGD: a set of one question submits on its first answer", async () => {
    render(surface());
    await pass();
    fireEvent.click(screen.getByTestId("voice-button"));
    act(() => publishSet("d1", { ...SET, questions: [question(1, "Only?")] } as never));
    await pass();
    await say("the only answer");
    expect(submit.submitQuestionAnswers).toHaveBeenCalledTimes(1);
  });

  it("DQA-FR-AYNP, DQA-FR-YMGD: an answer during a submission does not send it twice", async () => {
    let done: (value?: unknown) => void = () => {};
    submit.submitQuestionAnswers.mockReturnValueOnce(
      new Promise((settle) => {
        done = settle;
      }),
    );
    await withSet();
    await say("one");
    await say("two");
    await say("three");
    await say("three again");
    expect(submit.submitQuestionAnswers).toHaveBeenCalledTimes(1);
    done();
    await pass();
  });

  it("DQA-FR-YMGD: a click on the last open question submits nothing", async () => {
    await withSet();
    await say("one");
    await say("two");
    expect(heading()).toHaveTextContent("Third?");
    fireEvent.click(screen.getAllByRole("radio")[0]);
    await pass();
    expect(submit.submitQuestionAnswers).not.toHaveBeenCalled();
    expect(heading()).toHaveTextContent("Third?");
  });

  it("DQA-FR-ITCZ, DQA-FR-TFCE: an answer that needs no move leaves the next click its focus", async () => {
    rememberAnswer("s1", 3, { selected: 1, ownWords: false, typed: "", note: "" });
    await withSet();
    fireEvent.pointerDown(screen.getByTestId("voice-record"));
    await pass();
    // The recording answers the first question, and the page is on the second,
    // which is the next one open.
    fireEvent.click(screen.getByTestId("discussion-questions-next"));
    await pass();
    fireEvent.pointerDown(screen.getByTestId("voice-record"));
    await pass(TAIL_MS);
    await pass();
    expect(heading()).toHaveTextContent("Second?");
    fireEvent.click(screen.getByTestId("discussion-questions-previous"));
    await pass();
    expect(screen.getByTestId("discussion-questions-question")).toHaveFocus();
  });

  it("DQA-FR-YMGD: an answer by click submits nothing and moves nothing", async () => {
    await withSet();
    fireEvent.click(screen.getAllByRole("radio")[0]);
    await pass();
    expect(heading()).toHaveTextContent("First?");
    expect(submit.submitQuestionAnswers).not.toHaveBeenCalled();
  });

  it("DQA-FR-GGKI, DQA-FR-TFCE: a spoken move leaves focus, and a click moves it", async () => {
    await withSet();
    const record = screen.getByTestId("voice-record");
    record.focus();
    await say("one");
    expect(heading()).toHaveTextContent("Second?");
    expect(record).toHaveFocus();
    fireEvent.click(screen.getByTestId("discussion-questions-next"));
    await pass();
    expect(screen.getByTestId("discussion-questions-question")).toHaveFocus();
    // The block has paged by hand now, and a spoken move still leaves focus.
    record.focus();
    await say("three");
    expect(heading()).toHaveTextContent("Second?");
    expect(record).toHaveFocus();
  });

  it("DQA-FR-GGKI, DQA-FR-TFCE: a spoken move that disables Next takes focus to the question", async () => {
    await withSet();
    fireEvent.click(screen.getByTestId("discussion-questions-next"));
    await pass();
    const next = screen.getByTestId("discussion-questions-next");
    next.focus();
    await say("second answer");
    expect(heading()).toHaveTextContent("Third?");
    expect(next).toBeDisabled();
    expect(screen.getByTestId("discussion-questions-question")).toHaveFocus();
  });

  it("DQA-FR-VICX, DQA-FR-WKTP: a refusal keeps the answers and is not repeated", async () => {
    submit.submitQuestionAnswers.mockRejectedValueOnce("set_gone");
    await withSet();
    await say("one");
    await say("two");
    await say("three");
    expect(submit.submitQuestionAnswers).toHaveBeenCalledTimes(1);
    expect(recallAnswer("s1", 3).typed).toBe("three");
    expect(screen.getByRole("alert")).toBeInTheDocument();
    await pass(TAIL_MS * 10);
    expect(submit.submitQuestionAnswers).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByTestId("discussion-questions-submit"));
    await pass();
    expect(submit.submitQuestionAnswers).toHaveBeenCalledTimes(2);
  });

  it("DQA-FR-VICX: a further spoken answer can submit the set", async () => {
    submit.submitQuestionAnswers.mockRejectedValueOnce("set_gone");
    await withSet();
    await say("one");
    await say("two");
    await say("three");
    await say("three again");
    expect(submit.submitQuestionAnswers).toHaveBeenCalledTimes(2);
  });

  it("VOM-FR-MAVW, VOM-FR-RENV: an empty transcript changes nothing and speaks nothing", async () => {
    await withSet();
    await say("  ");
    expect(recallAnswer("s1", 1).ownWords).toBe(false);
    expect(heading()).toHaveTextContent("First?");
    expect(getVoiceState().note).toBe("no_speech");
    expect(spokenTexts()).toHaveLength(1);
  });

  it("VOM-FR-MAVW, VOM-FR-WICV: a failure holds the question, and Retry answers it", async () => {
    await withSet();
    api.finishVoiceCapture.mockRejectedValueOnce("transcription_failed");
    fireEvent.pointerDown(screen.getByTestId("voice-record"));
    await pass();
    fireEvent.pointerDown(screen.getByTestId("voice-record"));
    await pass(TAIL_MS);
    await pass();
    expect(recallAnswer("s1", 1).ownWords).toBe(false);
    expect(heading()).toHaveTextContent("First?");
    expect(getVoiceState().canRetry).toBe(true);
    expect(spokenTexts()).toHaveLength(1);
    await act(async () => {
      await retryRecording();
    });
    expect(recallAnswer("s1", 1).typed).toBe("said again");
    expect(heading()).toHaveTextContent("Second?");
  });

  it("VOM-FR-PINR: it answers the question that showed when it started", async () => {
    await withSet();
    fireEvent.pointerDown(screen.getByTestId("voice-record"));
    await pass();
    fireEvent.click(screen.getByTestId("discussion-questions-next"));
    fireEvent.pointerDown(screen.getByTestId("voice-record"));
    await pass(TAIL_MS);
    await pass();
    expect(recallAnswer("s1", 1).typed).toBe("own words");
    expect(recallAnswer("s1", 2).ownWords).toBe(false);
  });

  it("VOM-FR-UXYH: a transcript whose set went joins the session body", async () => {
    await withSet();
    let finish: (text: string) => void = () => {};
    api.finishVoiceCapture.mockReturnValueOnce(
      new Promise<string>((done) => {
        finish = done;
      }),
    );
    fireEvent.pointerDown(screen.getByTestId("voice-record"));
    await pass();
    fireEvent.pointerDown(screen.getByTestId("voice-record"));
    await pass(TAIL_MS);
    act(() => publishSet("d1", null));
    await pass();
    finish("too late");
    await pass();
    expect(getDiscussionSession("d1").body).toBe("too late");
    expect(getVoiceState().note).toBe("kept_in_body");
  });

  it("VOM-FR-WICV, VOM-FR-UXYH: Retry after the set went joins the session body", async () => {
    await withSet();
    api.finishVoiceCapture.mockRejectedValueOnce("transcription_failed");
    fireEvent.pointerDown(screen.getByTestId("voice-record"));
    await pass();
    fireEvent.pointerDown(screen.getByTestId("voice-record"));
    await pass(TAIL_MS);
    await pass();
    act(() => publishSet("d1", null));
    await pass();
    await act(async () => {
      await retryRecording();
    });
    expect(getDiscussionSession("d1").body).toBe("said again");
  });

  it("VOM-FR-UXYH: a set that another replaces takes none of the words", async () => {
    await withSet();
    let finish: (text: string) => void = () => {};
    api.finishVoiceCapture.mockReturnValueOnce(
      new Promise<string>((done) => {
        finish = done;
      }),
    );
    fireEvent.pointerDown(screen.getByTestId("voice-record"));
    await pass();
    fireEvent.pointerDown(screen.getByTestId("voice-record"));
    await pass(TAIL_MS);
    act(() => publishSet("d1", { ...SET, setId: "s2" }));
    await pass();
    finish("too late");
    await pass();
    expect(recallAnswer("s2", 1).ownWords).toBe(false);
    expect(getDiscussionSession("d1").body).toBe("too late");
  });

  it("VOM-FR-ABPU: voice mode that ends during a spoken answer keeps the words", async () => {
    await withSet();
    let finish: (text: string) => void = () => {};
    api.finishVoiceCapture.mockReturnValueOnce(
      new Promise<string>((done) => {
        finish = done;
      }),
    );
    fireEvent.pointerDown(screen.getByTestId("voice-record"));
    await pass();
    fireEvent.pointerDown(screen.getByTestId("voice-record"));
    await pass(TAIL_MS);
    act(() => leaveVoiceMode());
    await pass();
    finish("kept");
    await pass();
    expect(recallAnswer("s1", 1).ownWords).toBe(false);
    expect(getDiscussionSession("d1").body).toBe("kept");
  });

  it("VOM-FR-ABPU: a recording from before the set joins the session body", async () => {
    render(surface());
    await pass();
    fireEvent.click(screen.getByTestId("voice-button"));
    fireEvent.pointerDown(screen.getByTestId("voice-record"));
    await pass();
    act(() => publishSet("d1", SET));
    // VOM-FR-BCRD: the recording goes on until the author releases it.
    await pass(SHORT_PRESS_MS);
    fireEvent.pointerDown(screen.getByTestId("voice-record"));
    await pass(TAIL_MS);
    await pass();
    expect(recallAnswer("s1", 1).ownWords).toBe(false);
    expect(getDiscussionSession("d1").body).toBe("own words");
    expect(onReply).not.toHaveBeenCalled();
  });

  it("VOM-FR-FHYY, VOM-FR-ZUWW: the hotkey records a spoken answer at the session control", async () => {
    await withSet();
    expect(voiceComposerMounted()).toBe(true);
    expect(screen.queryByTestId("discussion-composer")).toBeNull();
    api.finishVoiceCapture.mockResolvedValueOnce("by hotkey");
    act(() => pressRecord());
    await pass();
    act(() => pressRecord());
    await pass(TAIL_MS);
    await pass();
    expect(recallAnswer("s1", 1).typed).toBe("by hotkey");
  });
});

describe("a VAD session while a set stands", () => {
  it("VOM-FR-GTAQ, VOM-FR-PINR: an utterance answers the question and posts nothing", async () => {
    await openVadWithSet();
    activity("speech_start");
    activity("speech_confirmed", { captureId: "c7", live: true });
    api.finishVoiceCapture.mockResolvedValueOnce("hands free");
    activity("speech_end", { captureId: "c7" });
    await pass();
    expect(recallAnswer("s1", 1).typed).toBe("hands free");
    expect(onReply).not.toHaveBeenCalled();
    expect(getVoiceState().vadOpen).toBe(true);
  });

  it("VOM-FR-LKKN, VOM-FR-UNXR: the answer drops the paused question and the next one speaks", async () => {
    await openVadWithSet();
    activity("speech_start");
    expect(getVoiceState().playbackPaused).toBe(true);
    activity("speech_confirmed", { captureId: "c7", live: true });
    api.finishVoiceCapture.mockResolvedValueOnce("hands free");
    activity("speech_end", { captureId: "c7" });
    await pass();
    expect(getVoiceState()).toMatchObject({ playbackPaused: false, phase: "idle" });
    expect(spokenTexts().pop()).toBe("Second? One spec Two specs");
    expect(resumePlayback).not.toHaveBeenCalled();
  });

  it("VOM-FR-UNXR, DQA-FR-YMGD: the last answer drops the paused question and resumes nothing", async () => {
    await openVadWithSet();
    rememberAnswer("s1", 2, { selected: 1, ownWords: false, typed: "", note: "" });
    rememberAnswer("s1", 3, { selected: 1, ownWords: false, typed: "", note: "" });
    activity("speech_start");
    activity("speech_confirmed", { captureId: "c7", live: true });
    api.finishVoiceCapture.mockResolvedValueOnce("hands free");
    activity("speech_end", { captureId: "c7" });
    await pass();
    expect(submit.submitQuestionAnswers).toHaveBeenCalledTimes(1);
    expect(resumePlayback).not.toHaveBeenCalled();
    expect(getVoiceState().playbackPaused).toBe(false);
  });

  it("VOM-FR-MAVW, VOM-FR-UNXR: an empty utterance resumes the question and does not restart it", async () => {
    await openVadWithSet();
    activity("speech_start");
    activity("speech_confirmed", { captureId: "c7", live: true });
    api.finishVoiceCapture.mockResolvedValueOnce("  ");
    activity("speech_end", { captureId: "c7" });
    await pass();
    expect(resumePlayback).toHaveBeenCalledTimes(1);
    expect(spokenTexts()).toHaveLength(1);
    expect(recallAnswer("s1", 1).ownWords).toBe(false);
    expect(getVoiceState().note).toBe("no_speech");
  });

  it("VOM-FR-LKKN, VOM-FR-KVHG: a misfire resumes the question", async () => {
    await openVadWithSet();
    activity("speech_start");
    activity("misfire");
    await pass();
    expect(getVoiceState().playbackPaused).toBe(false);
    expect(heading()).toHaveTextContent("First?");
    expect(spokenTexts()).toHaveLength(1);
  });

  it("VOM-FR-LKKN, VOM-FR-ZPOD: a set that arrives during an utterance waits for it", async () => {
    render(surface());
    fireEvent.click(screen.getByTestId("voice-button"));
    fireEvent.click(screen.getByRole("button", { name: "VAD" }));
    fireEvent.pointerDown(screen.getByTestId("voice-record"));
    await pass();
    act(() => setResponseMode("voice"));
    activity("speech_start");
    activity("speech_confirmed", { captureId: "c7", live: true });
    act(() => publishSet("d1", SET));
    await pass();
    expect(api.speakVoiceSegment).not.toHaveBeenCalled();
    api.finishVoiceCapture.mockResolvedValueOnce("from before");
    activity("speech_end", { captureId: "c7" });
    await pass();
    expect(spokenTexts()).toEqual(["First? One spec Two specs"]);
    expect(recallAnswer("s1", 1).ownWords).toBe(false);
    expect(getDiscussionSession("d1").body).toBe("from before");
  });

  it("VOM-FR-LKKN, VOM-FR-ZPOD: a waiting question whose set went says nothing", async () => {
    render(surface());
    await pass();
    fireEvent.click(screen.getByTestId("voice-button"));
    fireEvent.click(screen.getByRole("button", { name: "VAD" }));
    fireEvent.pointerDown(screen.getByTestId("voice-record"));
    await pass();
    act(() => setResponseMode("voice"));
    activity("speech_start");
    activity("speech_confirmed", { captureId: "c7", live: true });
    act(() => publishSet("d1", SET));
    await pass();
    act(() => publishSet("d1", null));
    await pass();
    activity("speech_end", { captureId: "c7" });
    await pass();
    expect(api.speakVoiceSegment).not.toHaveBeenCalled();
  });
});
