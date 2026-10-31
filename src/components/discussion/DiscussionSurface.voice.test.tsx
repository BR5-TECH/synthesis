/**
 * A VAD session while a question set stands (`VOM-voice-mode.md` VOM-FR-VGLT,
 * `DQA-discussion-question-answering.md` DQA-FR-PXNC). The surface, the composer,
 * and the controller are real, because the order of their mounts is what counts.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";

const api = vi.hoisted(() => ({
  cancelVoiceSpeech: vi.fn(async () => undefined),
  closeVoiceActivitySession: vi.fn(async () => ({ captureId: null })),
  feedVoiceActivity: vi.fn(async () => undefined),
  openVoiceActivitySession: vi.fn(async () => ({ activityId: "va1" })),
  setVoiceActivityGate: vi.fn(async () => undefined),
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
vi.mock("../../voice/capture", async (original) => ({
  ...(await original<typeof import("../../voice/capture")>()),
  openMicrophone: vi.fn(async () => ({ rate: 48_000, stop: mic.stop })),
}));
vi.mock("../../voice/playback", () => ({
  isPlaybackIdle: vi.fn(() => true),
  pausePlayback: vi.fn(),
  playFrame: vi.fn(),
  prepareAudio: vi.fn(),
  resumePlayback: vi.fn(),
  setDrainedListener: vi.fn(),
  stopPlayback: vi.fn(),
}));

import { resetLogBufferForTest } from "../../logging";
import { resetDiscussionFocus } from "../../state/discussionFocus";
import { clearAllDiscussionSessions } from "../../state/discussionSession";
import { clearQuestionSets, publishSet } from "../../state/questionSets";
import { getVoiceState, resetVoiceStateForTest } from "../../state/voiceSession";
import type { Discussion, PendingQuestionSet, ProjectAgent } from "../../types";
import {
  SURFACE_GRACE_MS,
  endVoiceForTeardown,
  noteProvider,
  resetVoiceControllerForTest,
} from "../../voice/voiceController";
import { DiscussionSurface } from "./DiscussionSurface";

const arch = {
  scope: "project",
  agent: { id: "a1", nickname: "arch", title: "", modelId: "m", instructions: "" },
  availability: "ready",
  shadowed: false,
} as unknown as ProjectAgent;

function discussion(over: Partial<Discussion> = {}): Discussion {
  return {
    id: "d1",
    target: { kind: "artifact", artifactId: "a.md" },
    fragmentTarget: null,
    comments: [],
    resolved: false,
    createdAt: "2026-02-01T00:00:00Z",
    updatedAt: "2026-02-01T00:00:00Z",
    ...over,
  };
}

const QUESTION_SET = {
  setId: "s1",
  discussionId: "d1",
  askedBy: { kind: "agent", nickname: "arch", title: "Architect" },
  askedAt: "2026-02-01T00:00:00Z",
  questions: [{ position: 1, text: "Which session?", options: [{ position: 1, value: "kickoff" }] }],
} as unknown as PendingQuestionSet;

function surface(over: Partial<Discussion> = {}) {
  return (
    <DiscussionSurface
      discussion={discussion(over)}
      owner="rail"
      agents={[arch]}
      liveTurns={false}
      onReply={vi.fn(async () => undefined)}
      onSetResolved={vi.fn(async () => undefined)}
    />
  );
}

async function pass(ms = 0) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
}

async function openSession() {
  fireEvent.click(screen.getByTestId("voice-button"));
  fireEvent.click(screen.getByRole("button", { name: "VAD" }));
  fireEvent.pointerDown(screen.getByTestId("voice-record"));
  await pass();
  expect(getVoiceState().vadOpen).toBe(true);
}

function lastGate() {
  const calls = api.setVoiceActivityGate.mock.calls as unknown as [string, Record<string, unknown>][];
  return calls[calls.length - 1]?.[1];
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  resetVoiceStateForTest();
  resetVoiceControllerForTest();
  clearAllDiscussionSessions();
  clearQuestionSets();
  resetDiscussionFocus();
  resetLogBufferForTest();
  noteProvider(true, true);
});

afterEach(async () => {
  cleanup();
  endVoiceForTeardown();
  await vi.advanceTimersByTimeAsync(SURFACE_GRACE_MS);
  vi.useRealTimers();
});

describe("a question set with a session open", () => {
  it("VOM-FR-VGLT, DQA-FR-PXNC: the bar shows near the line, and the session stays", async () => {
    render(surface());
    await openSession();
    act(() => publishSet("d1", QUESTION_SET));
    expect(screen.queryByTestId("discussion-composer")).toBeNull();
    const note = screen.getByTestId("discussion-questions-composer-note");
    const bar = screen.getByTestId("voice-session-bar");
    expect(note.compareDocumentPosition(bar) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    await pass(SURFACE_GRACE_MS * 2);
    expect(getVoiceState().vadOpen).toBe(true);
    expect(api.closeVoiceActivitySession).not.toHaveBeenCalled();
    expect(lastGate()).toMatchObject({ blocked: true });
    expect(bar).toHaveTextContent(/not recorded/);
  });

  it("VOM-FR-VGLT: with no session, only the line shows", async () => {
    render(surface());
    fireEvent.click(screen.getByTestId("voice-button"));
    act(() => publishSet("d1", QUESTION_SET));
    expect(screen.getByTestId("discussion-questions-composer-note")).toBeInTheDocument();
    expect(screen.queryByTestId("voice-session-bar")).toBeNull();
  });

  it("VOM-FR-VGLT: a session of another discussion shows no bar here", async () => {
    render(
      <>
        {surface()}
        {surface({ id: "d2" })}
      </>,
    );
    const second = screen.getAllByTestId("discussion-composer")[1];
    fireEvent.click(within(second).getByTestId("voice-button"));
    fireEvent.click(screen.getByRole("button", { name: "VAD" }));
    fireEvent.pointerDown(screen.getByTestId("voice-record"));
    await pass();
    expect(getVoiceState()).toMatchObject({ vadOpen: true, activeKey: "d2" });
    act(() => publishSet("d1", QUESTION_SET));
    expect(screen.getByTestId("discussion-questions-composer-note")).toBeInTheDocument();
    expect(screen.queryByTestId("voice-session-bar")).toBeNull();
  });

  it("VOM-FR-GTAQ: when the set goes, listening goes on with no press", async () => {
    render(surface());
    await openSession();
    act(() => publishSet("d1", QUESTION_SET));
    await pass(SURFACE_GRACE_MS * 2);
    act(() => clearQuestionSets());
    expect(screen.getByTestId("discussion-composer")).toBeInTheDocument();
    expect(screen.queryByTestId("voice-session-bar")).toBeNull();
    await pass(SURFACE_GRACE_MS * 2);
    expect(getVoiceState().vadOpen).toBe(true);
    expect(lastGate()).toMatchObject({ blocked: false, keyterms: ["arch"] });
    expect(api.openVoiceActivitySession).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-GPNK, DQA-FR-PXNC: the bar closes the session, and the line stays", async () => {
    render(surface());
    await openSession();
    act(() => publishSet("d1", QUESTION_SET));
    fireEvent.pointerDown(screen.getByTestId("voice-record"));
    await pass();
    expect(getVoiceState().vadOpen).toBe(false);
    expect(screen.queryByTestId("voice-session-bar")).toBeNull();
    expect(screen.getByTestId("discussion-questions-composer-note")).toBeInTheDocument();
    act(() => clearQuestionSets());
    expect(screen.getByTestId("voice-record")).toHaveAttribute("data-vad", "idle");
  });

  it("VOM-FR-GZPA: a discussion that resolves shows no bar, and the session closes", async () => {
    const view = render(surface());
    await openSession();
    act(() => publishSet("d1", QUESTION_SET));
    view.rerender(surface({ resolved: true }));
    expect(screen.queryByTestId("voice-session-bar")).toBeNull();
    await pass(SURFACE_GRACE_MS);
    expect(getVoiceState().vadOpen).toBe(false);
  });
});
