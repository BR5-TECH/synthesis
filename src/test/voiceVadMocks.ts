/**
 * The mocks of the VAD controller tests. This file imports no application code,
 * because `vi.hoisted` loads it before `vi.mock` applies.
 */
import { vi } from "vitest";

export const api = {
  appendVoiceCapture: vi.fn(async () => undefined),
  cancelVoiceCapture: vi.fn(async () => undefined),
  cancelVoiceSpeech: vi.fn(async () => undefined),
  closeVoiceActivitySession: vi.fn(async (): Promise<{ captureId: string | null }> => ({
    captureId: null,
  })),
  feedVoiceActivity: vi.fn(async (): Promise<void> => undefined),
  finishVoiceCapture: vi.fn(async (): Promise<string> => "hello"),
  openVoiceActivitySession: vi.fn(async () => ({ activityId: "va1" })),
  retryVoiceCapture: vi.fn(async (): Promise<string> => "again"),
  setVoiceActivityGate: vi.fn(async () => undefined),
  speakVoiceSegment: vi.fn(async () => undefined),
  startVoiceCapture: vi.fn(async () => ({ sessionId: "s1", route: "realtime", live: true })),
  dispatchSpeechTurn: vi.fn(async (args: { nickname: string; speechId: string }) => ({
    id: `t-${args.nickname}`,
    agentId: args.nickname,
    nickname: args.nickname,
    origin: { kind: "artifact", artifactId: "a.md", discussionId: "d1" },
    triggerCommentId: null,
    speechId: args.speechId,
    replyHeld: false,
    state: "running",
    failure: null,
    retryPermitted: false,
    startedAt: "2026-02-01T00:00:00Z",
    endedAt: null,
    activeToolCalls: [],
  })),
  steerSpeechTurn: vi.fn(async () => []),
  endSpeechTurn: vi.fn(async () => []),
};

export const mic = {
  stop: vi.fn(),
  open: vi.fn(),
  onChunk: null as null | ((pcm: ArrayBuffer) => void),
  onLost: null as null | (() => void),
};

export const audio = {
  isPlaybackIdle: vi.fn(() => false),
  pausePlayback: vi.fn(),
  playFrame: vi.fn(),
  prepareAudio: vi.fn(),
  resumePlayback: vi.fn(),
  setDrainedListener: vi.fn(),
  stopPlayback: vi.fn(),
};
