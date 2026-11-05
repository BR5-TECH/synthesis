/**
 * A spoken reply and the playback under it (`VOM-voice-mode.md` VOM-FR-VGQF,
 * VOM-FR-EPBL, VOM-FR-QPFQ, VOM-FR-UCQN).
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  cancelVoiceSpeech: vi.fn(async () => undefined),
  speakVoiceSegment: vi.fn(),
}));
const log = vi.hoisted(() => ({ warn: vi.fn() }));
const audio = vi.hoisted(() => ({
  isPlaybackIdle: vi.fn(() => false),
  playFrame: vi.fn(),
  setDrainedListener: vi.fn(),
  setPlaybackFailureListener: vi.fn(),
  beginSegment: vi.fn(),
  endSegmentInput: vi.fn(),
  stopPlayback: vi.fn(),
}));
vi.mock("../api", () => api);
vi.mock("../logging", () => ({ logDebug: vi.fn(), logInfo: vi.fn(), logWarn: log.warn }));
vi.mock("./playback", () => audio);

import { getVoiceState, resetVoiceStateForTest } from "../state/voiceSession";
import { resetVoiceSpeechForTest, speak } from "./voiceSpeech";

/** The failure listener of the latest speech. */
function failure(): (code: string) => void {
  const calls = audio.setPlaybackFailureListener.mock.calls as unknown as [(code: string) => void][];
  return calls[calls.length - 1][0];
}

beforeEach(() => {
  vi.clearAllMocks();
  resetVoiceStateForTest();
  resetVoiceSpeechForTest();
  api.speakVoiceSegment.mockReset();
  api.speakVoiceSegment.mockImplementation(async () => undefined);
  audio.beginSegment.mockReset();
  audio.endSegmentInput.mockReset();
});

describe("a reply that playback cannot play", () => {
  it("VOM-FR-VGQF, VOM-FR-QPFQ: a playback failure stops the reply and shows playback_failed", async () => {
    await speak({ text: "Hello there.", agentId: "a1" }, null);
    expect(getVoiceState().playing).toBe(true);
    failure()("playback_failed");
    const state = getVoiceState();
    expect(state.playing).toBe(false);
    expect(state.error).toBe("playback_failed");
    expect(state.canRetry).toBe(false);
    expect(api.cancelVoiceSpeech).toHaveBeenCalledWith("speech-1");
    expect(log.warn).toHaveBeenCalledWith(["frontend", "ai"], "a voice reply failed", {
      code: "playback_failed",
    });
  });

  it("VOM-FR-UCQN: a filler that cannot play stops and shows no error", async () => {
    await speak({ text: "One moment.", agentId: "a1", filler: true }, null);
    failure()("playback_failed");
    expect(getVoiceState().playing).toBe(false);
    expect(getVoiceState().error).toBeNull();
    expect(log.warn).toHaveBeenCalledWith(["frontend", "ai"], "a voice filler failed", {
      code: "playback_failed",
    });
  });

  it("VOM-FR-VGQF: a failure that comes after a newer reply changes nothing of it", async () => {
    await speak({ text: "First.", agentId: "a1" }, null);
    const old = failure();
    await speak({ text: "Second.", agentId: "a1" }, null);
    old("playback_failed");
    expect(getVoiceState().playing).toBe(true);
    expect(getVoiceState().error).toBeNull();
  });

  it("VOM-FR-VGQF: a segment that rejects with playback_failed shows it", async () => {
    api.speakVoiceSegment.mockRejectedValueOnce("playback_failed");
    await speak({ text: "Hello.", agentId: "a1" }, null);
    expect(getVoiceState().error).toBe("playback_failed");
  });
});

describe("the segments of the playback log", () => {
  it("VOM-FR-EPBL: each segment of a reply opens a log segment, and its end closes the input", async () => {
    const order: string[] = [];
    audio.beginSegment.mockImplementation((source: string) => order.push(`begin:${source}`));
    audio.endSegmentInput.mockImplementation(() => order.push("end"));
    api.speakVoiceSegment.mockImplementation(async () => {
      order.push("segment");
    });
    await speak({ text: `${"A short one. "}${"b".repeat(5000)}.`, agentId: "a1" }, null);
    const segments = api.speakVoiceSegment.mock.calls.length;
    expect(segments).toBeGreaterThanOrEqual(2);
    expect(order).toEqual(
      Array.from({ length: segments }, () => ["begin:reply", "segment", "end"]).flat(),
    );
  });

  it("VOM-FR-EPBL: a filler logs as a filler", async () => {
    await speak({ text: "One moment.", agentId: "a1", filler: true }, null);
    expect(audio.beginSegment).toHaveBeenCalledWith("filler");
  });
});
