/**
 * The test of a Voice AI settings tab (`VAI-voice-ai-settings.md`
 * VAI-FR-ZCMB, VAI-FR-NWRL, VAI-FR-YBQE, VAI-FR-SKOF; `VOP-voice-ai-providers.md`
 * VOP-FR-TPXA).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  appendVoiceCapture: vi.fn(async () => undefined),
  cancelVoiceCapture: vi.fn(async () => undefined),
  cancelVoiceSpeech: vi.fn(async () => undefined),
  finishVoiceCapture: vi.fn(async (): Promise<string> => "hello there"),
  speakVoiceSegment: vi.fn(async (..._args: unknown[]): Promise<void> => undefined),
  startVoiceCapture: vi.fn(async () => ({ sessionId: "s1", route: "http", live: false })),
}));
const audio = vi.hoisted(() => ({
  drained: null as null | (() => void),
  idle: false,
  playFrame: vi.fn(),
  prepareAudio: vi.fn(),
  stopPlayback: vi.fn(),
  beginSegment: vi.fn(),
  endSegmentInput: vi.fn(),
  failed: null as null | ((code: string) => void),
}));
const mic = vi.hoisted(() => ({
  open: vi.fn(),
  stop: vi.fn(),
  onChunk: null as null | ((pcm: ArrayBuffer) => void),
  onLost: null as null | (() => void),
}));
const log = vi.hoisted(() => ({ info: vi.fn(), warn: vi.fn() }));

vi.mock("../api", () => api);
vi.mock("../logging", () => ({ logDebug: vi.fn(), logInfo: log.info, logWarn: log.warn }));
vi.mock("./playback", () => ({
  isPlaybackIdle: () => audio.idle,
  playFrame: audio.playFrame,
  prepareAudio: audio.prepareAudio,
  setDrainedListener: (listener: (() => void) | null) => {
    audio.drained = listener;
  },
  setPlaybackFailureListener: (listener: ((code: string) => void) | null) => {
    audio.failed = listener;
  },
  beginSegment: audio.beginSegment,
  endSegmentInput: audio.endSegmentInput,
  stopPlayback: audio.stopPlayback,
}));
vi.mock("./capture", async (original) => ({
  ...(await original<typeof import("./capture")>()),
  openMicrophone: mic.open,
}));

import { MicrophoneError } from "./capture";
import { TEST_LIMIT_MS, createVoiceTest, type VoiceTestState } from "./voiceTest";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

/** Each pending promise of the test settles. */
async function settle() {
  for (let i = 0; i < 50; i += 1) await Promise.resolve();
}

const VOICE = { name: "female-polish", options: { speedPercent: 130, style: "calm" } };

const made: { cancel(): void }[] = [];

function bench() {
  const states: VoiceTestState[] = [];
  const test = createVoiceTest(
    "custom",
    () => VOICE,
    (s) => states.push(s),
  );
  const last = () => states[states.length - 1];
  made.push(test);
  return { test, states, last };
}

beforeEach(() => {
  vi.clearAllMocks();
  audio.drained = null;
  audio.idle = false;
  mic.onChunk = null;
  mic.onLost = null;
  mic.open.mockImplementation(async (onChunk: (pcm: ArrayBuffer) => void, onLost: () => void) => {
    mic.onChunk = onChunk;
    mic.onLost = onLost;
    return { rate: 48_000, stop: mic.stop };
  });
  api.finishVoiceCapture.mockImplementation(async () => "hello there");
  api.speakVoiceSegment.mockImplementation(async () => undefined);
  api.startVoiceCapture.mockImplementation(async () => ({
    sessionId: "s1",
    route: "http",
    live: false,
  }));
});

afterEach(() => {
  for (const test of made.splice(0)) test.cancel();
  vi.useRealTimers();
});

describe("the playback of a test", () => {
  it("VAI-FR-NLYF, VOM-FR-EPBL: the speech of a test logs as a test segment, and its end closes the input", async () => {
    const { test } = bench();
    test.press();
    await settle();
    test.release();
    await settle();
    expect(audio.beginSegment).toHaveBeenCalledWith("test");
    expect(audio.endSegmentInput).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-VGQF, VAI-FR-YBQE: a frame that cannot play ends the test with playback_failed", async () => {
    const segment = deferred<void>();
    api.speakVoiceSegment.mockImplementationOnce(() => segment.promise);
    const { test, last } = bench();
    test.press();
    await settle();
    test.release();
    await settle();
    expect(last().phase).toBe("speaking");
    // Playback calls the failure listener, then the drained listener.
    audio.failed?.("playback_failed");
    audio.drained?.();
    expect(last()).toMatchObject({ phase: "idle", error: "playback_failed" });
    expect(api.cancelVoiceSpeech).toHaveBeenCalled();
    segment.resolve();
    await settle();
    expect(last()).toMatchObject({ phase: "idle", error: "playback_failed" });
  });
});

describe("a test of the tab", () => {
  it("VAI-FR-ZCMB, VOP-FR-TPXA: a hold records with the provider of the tab, and the transcript is spoken back in its voice", async () => {
    const { test, last, states } = bench();
    test.press();
    expect(last().phase).toBe("opening");
    expect(audio.prepareAudio).toHaveBeenCalled();
    await settle();
    expect(api.startVoiceCapture).toHaveBeenCalledWith(48_000, [], false, "custom");
    expect(last().phase).toBe("listening");
    mic.onChunk!(new Int16Array([1, 2]).buffer);
    await settle();
    expect(api.appendVoiceCapture).toHaveBeenCalledWith("s1", expect.any(Uint8Array));

    test.release();
    await settle();
    expect(mic.stop).toHaveBeenCalled();
    expect(api.finishVoiceCapture).toHaveBeenCalledWith("s1");
    expect(states.some((s) => s.phase === "transcribing")).toBe(true);
    expect(last()).toMatchObject({ phase: "speaking", transcript: "hello there" });
    expect(api.speakVoiceSegment).toHaveBeenCalledWith(
      expect.stringMatching(/^voice-test-/),
      "hello there",
      "female-polish",
      expect.any(Function),
      VOICE.options,
      "custom",
    );
    audio.drained!();
    expect(last()).toMatchObject({ phase: "idle", transcript: "hello there", error: null });
  });

  it("VAI-FR-ZCMB: playback that drains before the whole segment was sent does not end the speech", async () => {
    const segment = deferred<void>();
    api.speakVoiceSegment.mockImplementation(() => segment.promise);
    const { test, last } = bench();
    test.press();
    await settle();
    test.release();
    await settle();
    // A rate frame, or a gap between two frames, drains playback.
    audio.drained!();
    expect(last().phase).toBe("speaking");
    segment.resolve();
    await settle();
    expect(last().phase).toBe("speaking");
    audio.drained!();
    expect(last().phase).toBe("idle");
  });

  it("VAI-FR-YBQE, VAI-FR-NWRL: a frame that arrives after a cancel does not play", async () => {
    let onFrame: (frame: ArrayBuffer) => void = () => undefined;
    api.speakVoiceSegment.mockImplementation(
      (...args: unknown[]) => {
        onFrame = args[3] as (frame: ArrayBuffer) => void;
        return new Promise<void>(() => undefined);
      },
    );
    const { test } = bench();
    test.press();
    await settle();
    test.release();
    await settle();
    onFrame(new ArrayBuffer(4));
    expect(audio.playFrame).toHaveBeenCalledTimes(1);
    test.cancel();
    onFrame(new ArrayBuffer(4));
    expect(audio.playFrame).toHaveBeenCalledTimes(1);
  });

  it("VAI-FR-ZCMB: a speech that already played ends the test when its segment resolves", async () => {
    audio.idle = true;
    const { test, last } = bench();
    test.press();
    await settle();
    test.release();
    await settle();
    expect(last().phase).toBe("idle");
  });

  it("VAI-FR-ZCMB: an empty transcript states that nothing was heard, and nothing is spoken", async () => {
    api.finishVoiceCapture.mockImplementation(async () => "   ");
    const { test, last } = bench();
    test.press();
    await settle();
    test.release();
    await settle();
    expect(last()).toMatchObject({ phase: "idle", nothingHeard: true, transcript: "" });
    expect(api.speakVoiceSegment).not.toHaveBeenCalled();
  });

  it("VAI-FR-SKOF: the records of a test hold the provider and counts, and no text", async () => {
    const { test } = bench();
    test.press();
    await settle();
    test.release();
    await settle();
    const records = JSON.stringify([...log.info.mock.calls, ...log.warn.mock.calls]);
    expect(records).toContain("custom");
    expect(records).not.toContain("hello there");
  });
});

describe("the end of a hold", () => {
  it("VAI-FR-NWRL: a release while the microphone opens starts no capture and closes the microphone", async () => {
    const opening = deferred<{ rate: number; stop: () => void }>();
    mic.open.mockImplementation(() => opening.promise);
    const { test, last } = bench();
    test.press();
    test.release();
    opening.resolve({ rate: 48_000, stop: mic.stop });
    await settle();
    expect(mic.stop).toHaveBeenCalled();
    expect(api.startVoiceCapture).not.toHaveBeenCalled();
    expect(last().phase).toBe("idle");
  });

  it("VAI-FR-NWRL: a release while the capture opens cancels the capture", async () => {
    const starting = deferred<{ sessionId: string; route: string; live: boolean }>();
    api.startVoiceCapture.mockImplementation(() => starting.promise);
    const { test, last } = bench();
    test.press();
    await settle();
    test.release();
    starting.resolve({ sessionId: "s9", route: "http", live: false });
    await settle();
    expect(api.cancelVoiceCapture).toHaveBeenCalledWith("s9");
    expect(mic.stop).toHaveBeenCalled();
    expect(api.finishVoiceCapture).not.toHaveBeenCalled();
    expect(last().phase).toBe("idle");
  });

  it("VAI-FR-NWRL: a hold ends after 15 seconds", async () => {
    vi.useFakeTimers();
    const { test } = bench();
    test.press();
    await settle();
    expect(api.finishVoiceCapture).not.toHaveBeenCalled();
    expect(TEST_LIMIT_MS).toBe(15_000);
    await vi.advanceTimersByTimeAsync(TEST_LIMIT_MS - 1);
    expect(api.finishVoiceCapture).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(api.finishVoiceCapture).toHaveBeenCalledWith("s1");
  });

  it("VAI-FR-NWRL: a cancel while listening closes the microphone and cancels the capture", async () => {
    const { test } = bench();
    test.press();
    await settle();
    test.cancel();
    expect(mic.stop).toHaveBeenCalled();
    expect(api.cancelVoiceCapture).toHaveBeenCalledWith("s1");
    test.release();
    await settle();
    expect(api.finishVoiceCapture).not.toHaveBeenCalled();
  });

  it("VAI-FR-NWRL: a cancel while speaking stops the speech, and a late drain changes nothing", async () => {
    const { test, states, last } = bench();
    test.press();
    await settle();
    test.release();
    await settle();
    expect(last().phase).toBe("speaking");
    // The listener of the speech, kept from before the cancel.
    const drained = audio.drained!;
    test.cancel();
    expect(api.cancelVoiceSpeech).toHaveBeenCalledWith(expect.stringMatching(/^voice-test-/));
    expect(audio.stopPlayback).toHaveBeenCalled();
    expect(audio.drained).toBeNull();
    const count = states.length;
    drained();
    expect(states).toHaveLength(count);
  });

  it("VAI-FR-NWRL: a cancel while transcribing keeps the late transcript out", async () => {
    const finishing = deferred<string>();
    api.finishVoiceCapture.mockImplementation(() => finishing.promise);
    const { test, states } = bench();
    test.press();
    await settle();
    test.release();
    await settle();
    test.cancel();
    expect(api.cancelVoiceCapture).toHaveBeenCalledWith("s1");
    const count = states.length;
    finishing.resolve("late words");
    await settle();
    expect(states).toHaveLength(count);
    expect(api.speakVoiceSegment).not.toHaveBeenCalled();
  });
});

describe("a new press and a failure", () => {
  it("VAI-FR-YBQE: a new press stops a test that still speaks", async () => {
    const { test, last } = bench();
    test.press();
    await settle();
    test.release();
    await settle();
    expect(last().phase).toBe("speaking");
    test.press();
    expect(api.cancelVoiceSpeech).toHaveBeenCalled();
    expect(last()).toMatchObject({ phase: "opening", transcript: null });
    await settle();
    expect(api.startVoiceCapture).toHaveBeenCalledTimes(2);
  });

  it("VAI-FR-YBQE: a refused capture shows its code and closes the microphone", async () => {
    api.startVoiceCapture.mockImplementation(() => Promise.reject("not_verified"));
    const { test, last } = bench();
    test.press();
    await settle();
    expect(last()).toMatchObject({ phase: "idle", error: "not_verified" });
    expect(mic.stop).toHaveBeenCalled();
  });

  it("VAI-FR-YBQE: a refused microphone shows its code", async () => {
    mic.open.mockImplementation(() => Promise.reject(new MicrophoneError("microphone_denied")));
    const { test, last } = bench();
    test.press();
    await settle();
    expect(last()).toMatchObject({ phase: "idle", error: "microphone_denied" });
    expect(api.startVoiceCapture).not.toHaveBeenCalled();
  });

  it("VAI-FR-YBQE: a lost microphone ends the test with an error and cancels the capture", async () => {
    const { test, last } = bench();
    test.press();
    await settle();
    mic.onLost!();
    expect(last()).toMatchObject({ phase: "idle", error: "microphone_unavailable" });
    expect(api.cancelVoiceCapture).toHaveBeenCalledWith("s1");
  });

  it("VAI-FR-YBQE: a failed transcription shows its code", async () => {
    api.finishVoiceCapture.mockImplementation(() => Promise.reject("unreachable"));
    const { test, last } = bench();
    test.press();
    await settle();
    test.release();
    await settle();
    expect(last()).toMatchObject({ phase: "idle", error: "unreachable" });
    expect(api.speakVoiceSegment).not.toHaveBeenCalled();
  });

  it("VAI-FR-YBQE: a new press while the old test transcribes drops the old transcript and keeps the new session", async () => {
    const finishing = deferred<string>();
    api.finishVoiceCapture.mockImplementationOnce(() => finishing.promise);
    api.startVoiceCapture
      .mockImplementationOnce(async () => ({ sessionId: "s1", route: "http", live: false }))
      .mockImplementationOnce(async () => ({ sessionId: "s2", route: "http", live: false }));
    const { test, last } = bench();
    test.press();
    await settle();
    test.release();
    await settle();
    expect(last().phase).toBe("transcribing");
    test.press();
    expect(api.cancelVoiceCapture).toHaveBeenCalledWith("s1");
    await settle();
    expect(last().phase).toBe("listening");
    finishing.resolve("old words");
    await settle();
    expect(last()).toMatchObject({ phase: "listening", transcript: null });
    expect(api.speakVoiceSegment).not.toHaveBeenCalled();
    // The new session is still the one that a release finishes.
    test.release();
    await settle();
    expect(api.finishVoiceCapture).toHaveBeenLastCalledWith("s2");
  });

  it("VAI-FR-YBQE: a speech that fails shows its code, and a new press needs no cancel of it", async () => {
    api.speakVoiceSegment.mockImplementation(() => Promise.reject("speech_failed"));
    const { test, last } = bench();
    test.press();
    await settle();
    test.release();
    await settle();
    expect(last()).toMatchObject({ phase: "idle", error: "speech_failed" });
  });

  it("VAI-FR-NWRL: two presses make one test, and two releases one finish", async () => {
    const { test } = bench();
    test.press();
    test.press();
    await settle();
    expect(mic.open).toHaveBeenCalledTimes(1);
    test.release();
    test.release();
    await settle();
    expect(api.finishVoiceCapture).toHaveBeenCalledTimes(1);
  });
});
