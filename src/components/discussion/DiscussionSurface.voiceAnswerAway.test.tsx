/**
 * Spoken answers while no surface of the discussion is mounted
 * (`VOM-voice-mode.md` VOM-FR-BCRD, VOM-FR-PINR,
 * `DQA-discussion-question-answering.md` DQA-FR-ITCZ, DQA-FR-WKTP,
 * DQA-FR-VICX). The surface, the block, and the controller are real.
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

/** One press and one more of the controller, as the hotkey does with no surface. */
async function sayAway(transcript: string) {
  api.finishVoiceCapture.mockResolvedValueOnce(transcript);
  act(() => pressRecord());
  await pass();
  act(() => pressRecord());
  await pass(TAIL_MS);
  await pass();
}

describe("spoken answers with no surface mounted", () => {
  it("VOM-FR-BCRD, DQA-FR-ITCZ: two answers stay with their own questions, and a remount shows the next one", async () => {
    const view = await withSet("text");
    view.unmount();
    await sayAway("one");
    await sayAway("two");
    expect(recallAnswer("s1", 1).typed).toBe("one");
    expect(recallAnswer("s1", 2).typed).toBe("two");
    expect(submit.submitQuestionAnswers).not.toHaveBeenCalled();
    render(surface());
    await pass();
    expect(heading()).toHaveTextContent("Third?");
    await say("three");
    expect(recallAnswer("s1", 1).typed).toBe("one");
    expect(submit.submitQuestionAnswers).toHaveBeenCalledTimes(1);
  });

  it("DQA-FR-GVSA, DQA-FR-ITCZ: a page turn by hand after a spoken move gives a remount the first question", async () => {
    const view = await withSet("text");
    await say("one");
    expect(heading()).toHaveTextContent("Second?");
    fireEvent.click(screen.getByTestId("discussion-questions-next"));
    expect(heading()).toHaveTextContent("Third?");
    view.unmount();
    render(surface());
    await pass();
    expect(heading()).toHaveTextContent("First?");
  });

  it("VOM-FR-PINR: a second answer that starts before the first transcript answers the next question", async () => {
    const view = await withSet("text");
    view.unmount();
    let first: (text: string) => void = () => {};
    api.finishVoiceCapture.mockReturnValueOnce(
      new Promise<string>((done) => {
        first = done;
      }),
    );
    api.startVoiceCapture.mockResolvedValueOnce({ sessionId: "c1", route: "realtime", live: true });
    act(() => pressRecord());
    await pass();
    act(() => pressRecord());
    await pass(TAIL_MS);
    api.startVoiceCapture.mockResolvedValueOnce({ sessionId: "c2", route: "realtime", live: true });
    api.finishVoiceCapture.mockResolvedValueOnce("two");
    act(() => pressRecord());
    await pass();
    act(() => pressRecord());
    await pass(TAIL_MS);
    first("one");
    await pass();
    await pass();
    expect(recallAnswer("s1", 1).typed).toBe("one");
    expect(recallAnswer("s1", 2).typed).toBe("two");
  });

  it("DQA-FR-WKTP, DQA-FR-VICX: a refused submission shows its error when the block mounts again, and keeps the answers", async () => {
    submit.submitQuestionAnswers.mockRejectedValueOnce("set_gone");
    const view = await withSet("text");
    view.unmount();
    await sayAway("one");
    await sayAway("two");
    await sayAway("three");
    expect(submit.submitQuestionAnswers).toHaveBeenCalledTimes(1);
    render(surface());
    await pass();
    expect(screen.getByRole("alert")).toHaveTextContent(/\S/);
    expect(recallAnswer("s1", 1).typed).toBe("one");
    expect(recallAnswer("s1", 2).typed).toBe("two");
    expect(recallAnswer("s1", 3).typed).toBe("three");
    fireEvent.click(screen.getByTestId("discussion-questions-submit"));
    await pass();
    expect(submit.submitQuestionAnswers).toHaveBeenCalledTimes(2);
    expect(screen.queryByRole("alert")).toBeNull();
  });
});
