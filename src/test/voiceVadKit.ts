/** What the VAD controller tests share (`src/voice/voiceControllerVad*.test.ts`). */
import { afterEach, beforeEach, expect, vi } from "vitest";

import { clearAllDiscussionSessions } from "../state/discussionSession";
import { getVoiceState, patchVoiceState, resetVoiceStateForTest } from "../state/voiceSession";
import type { Comment, Discussion, VoiceActivityChange } from "../types";
import {
  SURFACE_GRACE_MS,
  TAIL_MS,
  enterVoiceMode,
  noteActivity,
  noteDiscussionChanged,
  pressRecord,
  registerVoiceComposer,
  resetVoiceControllerForTest,
  setInputMode,
  setResponseMode,
  type PostOutcome,
  type VoiceComposerHandle,
} from "../voice/voiceController";
import { api, audio, mic } from "./voiceVadMocks";

export const KEY = "d1";

export function comment(id: string, kind: "agent" | "human" = "agent", body = "A reply."): Comment {
  return {
    id,
    author:
      kind === "agent"
        ? { kind: "agent", agentId: "a1", handle: "arch" }
        : { kind: "human", login: "me" },
    body,
    quotes: [],
    attachments: [],
    createdAt: "2026-02-01T00:00:00Z",
  };
}

export function discussion(comments: Comment[], id = KEY): Discussion {
  return {
    id,
    target: { kind: "artifact", artifactId: "a.md" },
    fragmentTarget: null,
    comments,
    resolved: false,
    createdAt: "2026-02-01T00:00:00Z",
    updatedAt: "2026-02-01T00:00:00Z",
  };
}

export function handle(over: Partial<VoiceComposerHandle> = {}) {
  return {
    post: vi.fn(async (): Promise<PostOutcome> => "posted"),
    tagged: (text: string) => text,
    blocked: () => false,
    nicknames: () => ["arch"],
    voiceOf: () => "cedar",
    ...over,
  } satisfies VoiceComposerHandle;
}

export function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

export function change(
  state: VoiceActivityChange["state"],
  over: Partial<VoiceActivityChange> = {},
): VoiceActivityChange {
  return { activityId: "va1", state, captureId: null, live: null, code: null, fatal: null, ...over };
}

export async function settle() {
  await vi.advanceTimersByTimeAsync(0);
}

export function lastGate() {
  const calls = api.setVoiceActivityGate.mock.calls as unknown as [string, Record<string, unknown>][];
  return calls[calls.length - 1]?.[1];
}

/** The composer of the test. A test can replace it. */
export const bench = {
  composer: handle(),
  unregister: (() => undefined) as () => void,
};

export function ready(with_: ReturnType<typeof handle> = handle()) {
  bench.composer = with_;
  enterVoiceMode(KEY, discussion([]));
  patchVoiceState({ providerReady: true, providerSpeaks: true });
  bench.unregister = registerVoiceComposer(KEY, bench.composer);
  setInputMode("vad");
}

export async function openSession(with_?: ReturnType<typeof handle>) {
  ready(with_);
  pressRecord();
  await settle();
  expect(getVoiceState().vadOpen).toBe(true);
}

/** Up to a confirmed utterance of capture `c1`. */
export async function confirmed(with_?: ReturnType<typeof handle>) {
  await openSession(with_);
  noteActivity(change("speech_start"));
  noteActivity(change("speech_confirmed", { captureId: "c1", live: true }));
}

/** What ends the open segment, so that no promise goes to the next test. */
let endSegment: (() => void) | null = null;

/** A reply that sounds: the segment stays open, so playback does not drain. */
export async function replyPlays() {
  setResponseMode("voice");
  const segment = deferred<undefined>();
  endSegment = () => segment.resolve(undefined);
  api.speakVoiceSegment.mockReturnValueOnce(segment.promise);
  noteDiscussionChanged(discussion([comment("r1")]));
  await settle();
  expect(getVoiceState().playing).toBe(true);
}

export async function replyPaused() {
  await replyPlays();
  noteActivity(change("speech_start"));
  expect(getVoiceState().playbackPaused).toBe(true);
}

export function expectSettledWith(resumed: boolean) {
  const state = getVoiceState();
  expect(state.phase).toBe("idle");
  expect(state.hearing).toBe(false);
  expect(state.playbackPaused).toBe(false);
  expect(audio.resumePlayback).toHaveBeenCalledTimes(resumed ? 1 : 0);
}

export function useVadBench(): void {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
    vi.setSystemTime(new Date("2026-02-01T00:00:00Z"));
    resetVoiceStateForTest();
    resetVoiceControllerForTest();
    clearAllDiscussionSessions();
    mic.onChunk = null;
    mic.onLost = null;
    mic.open.mockImplementation(
      async (onChunk: (pcm: ArrayBuffer) => void, onLost: () => void) => {
        mic.onChunk = onChunk;
        mic.onLost = onLost;
        return { rate: 48_000, stop: mic.stop };
      },
    );
    audio.isPlaybackIdle.mockReturnValue(false);
  });

  afterEach(async () => {
    endSegment?.();
    endSegment = null;
    await vi.advanceTimersByTimeAsync(SURFACE_GRACE_MS + TAIL_MS);
    vi.useRealTimers();
  });
}
