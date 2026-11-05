/**
 * The speed and the style of a spoken reply (`VOM-voice-mode.md` VOM-FR-UALP,
 * VOM-FR-QUSH).
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  cancelVoiceSpeech: vi.fn(async () => undefined),
  speakVoiceSegment: vi.fn(),
}));
const audio = vi.hoisted(() => ({
  isPlaybackIdle: vi.fn(() => true),
  playFrame: vi.fn(),
  setDrainedListener: vi.fn(),
  stopPlayback: vi.fn(),
}));
vi.mock("../api", () => api);
vi.mock("../logging", () => ({ logDebug: vi.fn(), logInfo: vi.fn(), logWarn: vi.fn() }));
vi.mock("./playback", () => audio);

import { resetVoiceStateForTest } from "../state/voiceSession";
import { agentVoiceOptions } from "./voiceOptions";
import { resetVoiceSpeechForTest, speak } from "./voiceSpeech";

type OnFrame = (frame: ArrayBuffer) => void;

function frame(kind: number, rate: number): ArrayBuffer {
  const out = new Uint8Array(5);
  out[0] = kind;
  new DataView(out.buffer).setUint32(1, rate, true);
  return out.buffer;
}

function kinds(): [number, number][] {
  return (audio.playFrame.mock.calls as unknown as [ArrayBuffer][]).map(([f]) => [
    new Uint8Array(f)[0],
    new DataView(f).getUint32(1, true),
  ]);
}

beforeEach(() => {
  vi.clearAllMocks();
  resetVoiceStateForTest();
  resetVoiceSpeechForTest();
  // Once-values of a failed test do not go to the next one.
  api.speakVoiceSegment.mockReset();
  api.speakVoiceSegment.mockImplementation(async () => undefined);
});

describe("the options of a reply", () => {
  it("VOM-FR-UALP: the options of the agent reach the segment", async () => {
    await speak({ text: "Hello.", agentId: "a1" }, "cedar", { speedPercent: 150, style: "calm" });
    expect(api.speakVoiceSegment).toHaveBeenCalledWith(
      "speech-1",
      "Hello.",
      "cedar",
      expect.any(Function),
      { speedPercent: 150, style: "calm" },
    );
  });

  it("VOM-FR-UALP: with no options the segment takes the values of the provider", async () => {
    await speak({ text: "Hello.", agentId: "a1" }, "cedar");
    expect(api.speakVoiceSegment).toHaveBeenCalledWith("speech-1", "Hello.", "cedar", expect.any(Function));
  });

  it("AGR-FR-EPYX: an agent that sets nothing has no options", () => {
    expect(agentVoiceOptions(undefined)).toBeNull();
    expect(agentVoiceOptions({ voiceSpeedPercent: null, voiceStyle: "  " })).toBeNull();
    expect(agentVoiceOptions({ voiceSpeedPercent: 70 })).toEqual({ speedPercent: 70, style: "" });
    expect(agentVoiceOptions({ voiceStyle: " warm " })).toEqual({ speedPercent: null, style: "warm" });
  });
});

describe("the rate of a segment", () => {
  it("VOM-FR-QUSH: in one reply, a segment with no rate frame after one with a rate plays at rate 1", async () => {
    api.speakVoiceSegment
      .mockImplementationOnce(async (_id, _text, _voice, onFrame: OnFrame) => {
        onFrame(frame(3, 1500));
        onFrame(frame(0, 24_000));
      })
      .mockImplementationOnce(async (_id, _text, _voice, onFrame: OnFrame) => {
        onFrame(frame(0, 24_000));
      });
    const long = `${"A long first sentence ".repeat(12)}ends here. Then a second one.`;
    await speak({ text: long, agentId: "a1" }, null);
    expect(api.speakVoiceSegment).toHaveBeenCalledTimes(2);
    expect(kinds()).toEqual([
      [3, 1500],
      [0, 24_000],
      [3, 1000],
      [0, 24_000],
    ]);
  });

  it("VOM-FR-QUSH: a new reply starts at rate 1 with no frame, because Stop reset the rate", async () => {
    api.speakVoiceSegment.mockImplementationOnce(async (_id, _text, _voice, onFrame: OnFrame) => {
      onFrame(frame(3, 1500));
      onFrame(frame(0, 24_000));
    });
    await speak({ text: "One.", agentId: "a1" }, null);
    api.speakVoiceSegment.mockImplementationOnce(async (_id, _text, _voice, onFrame: OnFrame) => {
      onFrame(frame(0, 24_000));
    });
    await speak({ text: "Two.", agentId: "a1" }, null);
    expect(audio.stopPlayback).toHaveBeenCalled();
    expect(kinds()).toEqual([
      [3, 1500],
      [0, 24_000],
      [0, 24_000],
    ]);
  });

  it("VOM-FR-QUSH: segments with no rate frame add none", async () => {
    api.speakVoiceSegment.mockImplementation(async (_id, _text, _voice, onFrame: OnFrame) => {
      onFrame(frame(0, 24_000));
    });
    await speak({ text: "One.", agentId: "a1" }, null);
    await speak({ text: "Two.", agentId: "a1" }, null);
    expect(kinds()).toEqual([
      [0, 24_000],
      [0, 24_000],
    ]);
  });
});
