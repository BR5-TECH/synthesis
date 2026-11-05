import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const log = vi.hoisted(() => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn() }));
const prefs = vi.hoisted(() => ({ value: null as null | Record<string, unknown> }));
vi.mock("../logging", () => ({ logDebug: log.debug, logInfo: log.info, logWarn: log.warn }));
vi.mock("../state/appPreferences", async (original) => ({
  ...(await original<typeof import("../state/appPreferences")>()),
  peekAppPreferences: () => prefs.value,
}));

import { resetAudioDevicesForTest, setDeviceFallbackListener } from "./audioDevices";
import {
  beginSegment,
  endSegmentInput,
  notePlaybackEntry,
  pausePlayback,
  playFrame,
  playTestTone,
  prepareAudio,
  resetPlaybackForTest,
  RUN_LIMIT_MS,
  setDrainedListener,
  setPlaybackFailureListener,
  stopPlayback,
} from "./playback";
import { resetPlaybackDiagnosticsForTest, setPlaybackEnvironmentProbe } from "./playbackDiagnostics";

class FakeSource {
  buffer: { duration: number } | null = null;
  playbackRate = { value: 1 };
  onended: (() => void) | null = null;
  startedAt: number | null = null;
  stopped = false;
  connect() {}
  start(at: number) {
    this.startedAt = at;
  }
  stop() {
    this.stopped = true;
  }
}

let sources: FakeSource[];
let contexts: FakeContext[];
/** The state that a resume gives a context. */
let resumeGives: string;
/** The state of a new context. */
let initialState: string;
/** The ids that `setSinkId` received. */
let sinkCalls: string[];

class FakeContext {
  currentTime = 1;
  destination = {};
  state: string;
  sampleRate = 44_100;
  baseLatency = 0.01;
  onstatechange: (() => void) | null = null;
  constructor() {
    this.state = initialState;
    contexts.push(this);
  }
  resume = vi.fn(async () => {
    this.state = resumeGives;
  });
  suspend = vi.fn(async () => {
    this.state = "suspended";
  });
  close = vi.fn(async () => {
    this.state = "closed";
  });
  createBuffer(_: number, length: number, rate: number) {
    return { duration: length / rate, getChannelData: () => new Float32Array(length) };
  }
  createBufferSource() {
    const source = new FakeSource();
    sources.push(source);
    return source;
  }
  decodeAudioData = vi.fn(async (_data: ArrayBuffer): Promise<{ duration: number }> => ({
    duration: 0.5,
  }));
}

class SinkContext extends FakeContext {
  setSinkId(id: string): Promise<void> {
    sinkCalls.push(id);
    return Promise.resolve();
  }
}

function pcmBytes(samples: number, rate = 24_000): number[] {
  const out = new Uint8Array(5 + samples * 2);
  new DataView(out.buffer).setUint32(1, rate, true);
  return Array.from(out);
}

function pcmFrame(samples: number, rate = 24_000): ArrayBuffer {
  return Uint8Array.from(pcmBytes(samples, rate)).buffer;
}

function wavFrame(): ArrayBuffer {
  return new Uint8Array([1, 0, 0, 0, 0, 0x52, 0x49, 0x46, 0x46, 9, 9]).buffer;
}

/** The segment records, at DEBUG or at WARN. */
function segmentRecords(): Record<string, unknown>[] {
  return [
    ...fieldsOf(log.debug, "a voice segment ended"),
    ...fieldsOf(log.warn, "a voice segment ended before its audio could play"),
  ];
}

function fieldsOf(spy: typeof log.debug, message: string): Record<string, unknown>[] {
  return spy.mock.calls.filter((call) => call[1] === message).map((call) => call[2]);
}

function useDevices(devices: { kind: string; deviceId: string; label: string }[]) {
  vi.stubGlobal("navigator", { mediaDevices: { enumerateDevices: vi.fn(async () => devices) } });
}

beforeEach(() => {
  sources = [];
  contexts = [];
  resumeGives = "running";
  initialState = "running";
  sinkCalls = [];
  prefs.value = null;
  log.debug.mockClear();
  log.info.mockClear();
  log.warn.mockClear();
  vi.stubGlobal("AudioContext", FakeContext);
  resetPlaybackForTest();
  resetPlaybackDiagnosticsForTest();
  resetAudioDevicesForTest();
});

afterEach(() => {
  resetPlaybackForTest();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("frames in both forms", () => {
  it("VOM-FR-DYTO: a frame as a byte array plays as an ArrayBuffer does", async () => {
    playFrame(pcmFrame(2400));
    playFrame(pcmBytes(2400));
    playFrame(new Uint8Array(pcmBytes(2400)));
    await vi.waitFor(() => expect(sources).toHaveLength(3));
    expect(sources.map((s) => s.buffer?.duration)).toEqual([0.1, 0.1, 0.1]);
    expect(sources[1].startedAt).toBeCloseTo(1.15, 6);
  });

  it("VOM-FR-DYTO, VOM-FR-OXLD: a value that is no frame is dropped with a warning that names its shape", async () => {
    playFrame("text" as unknown as ArrayBuffer);
    playFrame([1, 2]);
    playFrame(pcmFrame(10));
    await vi.waitFor(() => expect(sources).toHaveLength(1));
    expect(fieldsOf(log.warn, "a voice frame was dropped")).toEqual([
      { form: "string", bytes: null },
      { form: "array", bytes: 2 },
    ]);
  });
});

describe("a frame that cannot play", () => {
  it("VOM-FR-VGQF, VOM-FR-OXLD: a decode error fails the segment, logs a warning, and later frames still play", async () => {
    const failed = vi.fn();
    setPlaybackFailureListener(failed);
    prepareAudio();
    contexts[0].decodeAudioData.mockRejectedValueOnce(new DOMException("bad", "EncodingError"));
    playFrame(wavFrame());
    playFrame(pcmFrame(10));
    await vi.waitFor(() => expect(sources).toHaveLength(1));
    expect(failed).toHaveBeenCalledWith("playback_failed");
    expect(fieldsOf(log.warn, "voice playback failed")).toEqual([
      { error: "EncodingError", frameKind: "file" },
    ]);
  });

  it("VOM-FR-VGQF: a segment that failed is no played segment in the log", async () => {
    setPlaybackFailureListener(() => stopPlayback());
    prepareAudio();
    contexts[0].decodeAudioData.mockRejectedValueOnce(new Error("bad"));
    beginSegment("reply");
    playFrame(wavFrame());
    endSegmentInput();
    await vi.waitFor(() => expect(fieldsOf(log.debug, "a voice segment ended")).toHaveLength(1));
    expect(fieldsOf(log.debug, "a voice segment ended")[0]).toMatchObject({ failed: true });
    expect(fieldsOf(log.warn, "a voice segment ended before its audio could play")).toEqual([]);
  });

  it("VOM-FR-OXLD: the error log holds no audio and no text", async () => {
    prepareAudio();
    contexts[0].decodeAudioData.mockRejectedValueOnce(new Error("secret body text"));
    playFrame(wavFrame());
    await vi.waitFor(() => expect(fieldsOf(log.warn, "voice playback failed")).toHaveLength(1));
    expect(JSON.stringify(log.warn.mock.calls)).not.toContain("secret body text");
  });
});

describe("a context that does not run", () => {
  it("VOM-FR-WFTM: a closed context is replaced before the segment plays", async () => {
    prepareAudio();
    contexts[0].state = "closed";
    playFrame(pcmFrame(10));
    await vi.waitFor(() => expect(sources).toHaveLength(1));
    expect(contexts).toHaveLength(2);
    expect(contexts[0].close).not.toHaveBeenCalled();
  });

  it("VOM-FR-WFTM, VOM-FR-ULPY: a context that closes after the frame came is replaced, and the log says so", async () => {
    prepareAudio();
    playFrame(pcmFrame(10));
    contexts[0].state = "closed";
    await vi.waitFor(() => expect(sources).toHaveLength(1));
    expect(contexts).toHaveLength(2);
    expect(fieldsOf(log.warn, "the audio context of playback did not run, so it was replaced")).toEqual([
      { state: "closed" },
    ]);
    expect(fieldsOf(log.debug, "the audio context of playback changed").map((f) => f.event)).toEqual([
      "created",
      "created",
      "replaced",
    ]);
  });

  it("VOM-FR-WFTM, VOM-FR-KKNN: a replaced context sends its audio to the saved speaker again", async () => {
    vi.stubGlobal("AudioContext", SinkContext);
    prefs.value = { voiceOutputDevice: { id: "b", label: "Headset" } };
    useDevices([{ kind: "audiooutput", deviceId: "b", label: "Headset" }]);
    prepareAudio();
    playFrame(pcmFrame(10));
    await vi.waitFor(() => expect(sources).toHaveLength(1));
    contexts[0].state = "closed";
    playFrame(pcmFrame(10));
    await vi.waitFor(() => expect(sources).toHaveLength(2));
    expect(contexts).toHaveLength(2);
    expect(sinkCalls).toEqual(["b", "b"]);
  });

  it("VOM-FR-WFTM: a context that a resume leaves suspended is replaced one time", async () => {
    prepareAudio();
    contexts[0].state = "suspended";
    resumeGives = "suspended";
    contexts[0].resume.mockImplementation(async () => undefined);
    resumeGives = "running";
    playFrame(pcmFrame(10));
    await vi.waitFor(() => expect(sources).toHaveLength(1));
    expect(contexts).toHaveLength(2);
    expect(fieldsOf(log.warn, "the audio context of playback did not run, so it was replaced")).toEqual([
      { state: "suspended" },
    ]);
  });

  it("VOM-FR-WFTM: a resume that does not settle in time replaces the context", async () => {
    vi.useFakeTimers();
    prepareAudio();
    contexts[0].state = "suspended";
    contexts[0].resume.mockImplementation(() => new Promise<void>(() => undefined));
    playFrame(pcmFrame(10));
    // The limit starts once the chain reaches the resume.
    await vi.waitFor(() => expect(vi.getTimerCount()).toBeGreaterThan(0));
    await vi.advanceTimersByTimeAsync(RUN_LIMIT_MS);
    await vi.waitFor(() => expect(sources).toHaveLength(1));
    expect(contexts).toHaveLength(2);
  });

  it("VOM-FR-WFTM: when the new context does not run either, the segment fails", async () => {
    const failed = vi.fn();
    setPlaybackFailureListener(failed);
    initialState = "suspended";
    resumeGives = "suspended";
    playFrame(pcmFrame(10));
    await vi.waitFor(() => expect(failed).toHaveBeenCalledWith("playback_failed"));
    expect(contexts).toHaveLength(2);
    expect(sources).toHaveLength(0);
  });

  it("VOM-FR-WFTM, VOM-FR-FVRS: a paused reply keeps its context, and nothing is replaced", async () => {
    prepareAudio();
    pausePlayback();
    await vi.waitFor(() => expect(contexts[0].state).toBe("suspended"));
    playFrame(pcmFrame(10));
    await vi.waitFor(() => expect(sources).toHaveLength(1));
    expect(contexts).toHaveLength(1);
  });
});

describe("the log of playback", () => {
  it("VOM-FR-ULPY: the creation and each state change of the context are logged", () => {
    prepareAudio();
    contexts[0].state = "interrupted";
    contexts[0].onstatechange?.();
    expect(fieldsOf(log.debug, "the audio context of playback changed")).toEqual([
      expect.objectContaining({
        event: "created",
        state: "running",
        sampleRate: 44_100,
        baseLatency: 0.01,
        outputLatency: null,
        sinkIdSupported: false,
      }),
      expect.objectContaining({ event: "state", state: "interrupted" }),
    ]);
  });

  it("VOM-FR-EPBL: a segment logs its frames, forms, bytes, signature, and the microphone", async () => {
    setPlaybackEnvironmentProbe(() => ({ microphoneOpen: true, vadOpen: true }));
    beginSegment("reply");
    playFrame(wavFrame());
    playFrame(pcmBytes(2400));
    endSegmentInput();
    await vi.waitFor(() => expect(sources).toHaveLength(2));
    sources[0].onended?.();
    expect(fieldsOf(log.debug, "a voice segment ended")).toEqual([]);
    vi.spyOn(performance, "now").mockReturnValue(performance.now() + 10_000);
    sources[1].onended?.();
    const records = [
      ...fieldsOf(log.debug, "a voice segment ended"),
      ...fieldsOf(log.warn, "a voice segment ended before its audio could play"),
    ];
    expect(records).toEqual([
      expect.objectContaining({
        source: "reply",
        pcmFrames: 1,
        fileFrames: 1,
        binaryFrames: 1,
        arrayFrames: 1,
        bytes: 11 + 4805,
        signature: "RIFF",
        audioSeconds: 0.6,
        stopped: false,
        failed: false,
        microphoneOpen: true,
        vadOpen: true,
      }),
    ]);
  });

  it("VOM-FR-NFQT: a segment that ends in less than half of its audio is a warning", async () => {
    let now = 1000;
    vi.spyOn(performance, "now").mockImplementation(() => now);
    beginSegment("reply");
    playFrame(pcmFrame(24_000));
    endSegmentInput();
    await vi.waitFor(() => expect(sources).toHaveLength(1));
    now += 100;
    sources[0].onended?.();
    expect(fieldsOf(log.warn, "a voice segment ended before its audio could play")).toEqual([
      expect.objectContaining({ audioSeconds: 1, elapsedSeconds: 0.1 }),
    ]);
  });

  it("VOM-FR-NFQT: a segment that plays its length, and a stopped one, are DEBUG", async () => {
    let now = 1000;
    vi.spyOn(performance, "now").mockImplementation(() => now);
    beginSegment("reply");
    playFrame(pcmFrame(24_000));
    endSegmentInput();
    await vi.waitFor(() => expect(sources).toHaveLength(1));
    now += 600;
    sources[0].onended?.();
    beginSegment("filler");
    playFrame(pcmFrame(24_000));
    await vi.waitFor(() => expect(sources).toHaveLength(2));
    stopPlayback();
    expect(fieldsOf(log.debug, "a voice segment ended")).toEqual([
      expect.objectContaining({ source: "reply", elapsedSeconds: 0.6 }),
      expect.objectContaining({ source: "filler", stopped: true }),
    ]);
    expect(log.warn).not.toHaveBeenCalled();
  });

  it("VOM-FR-NFQT: the rate of the segment counts in its audio length", async () => {
    let now = 1000;
    vi.spyOn(performance, "now").mockImplementation(() => now);
    beginSegment("reply");
    playFrame(new Uint8Array([3, 0xd0, 0x07, 0, 0]).buffer);
    playFrame(pcmFrame(24_000));
    endSegmentInput();
    await vi.waitFor(() => expect(sources).toHaveLength(1));
    now += 300;
    sources[0].onended?.();
    expect(fieldsOf(log.debug, "a voice segment ended")).toEqual([
      expect.objectContaining({ audioSeconds: 0.5, rateFrames: 1 }),
    ]);
  });
});

describe("the speaker", () => {
  it("VOM-FR-KKNN, VOM-FR-IQNQ: the saved speaker plays where setSinkId exists, and the first segment logs it", async () => {
    vi.stubGlobal("AudioContext", SinkContext);
    prefs.value = { voiceOutputDevice: { id: "old-id", label: "Desk speakers" } };
    useDevices([{ kind: "audiooutput", deviceId: "new-id", label: "Desk speakers" }]);
    beginSegment("reply");
    playFrame(pcmFrame(10));
    await vi.waitFor(() => expect(sources).toHaveLength(1));
    expect(sinkCalls).toEqual(["new-id"]);
    expect(fieldsOf(log.info, "the speaker of voice playback")).toEqual([
      { device: "Desk speakers", saved: true, found: true, sinkIdSupported: true },
    ]);
    playFrame(pcmFrame(10));
    await vi.waitFor(() => expect(sources).toHaveLength(2));
    expect(fieldsOf(log.info, "the speaker of voice playback")).toHaveLength(1);
    notePlaybackEntry();
    playFrame(pcmFrame(10));
    await vi.waitFor(() => expect(sources).toHaveLength(3));
    expect(fieldsOf(log.info, "the speaker of voice playback")).toHaveLength(2);
  });

  it("VOM-FR-GIES: a saved speaker that is not present falls back to the default and reports it", async () => {
    vi.stubGlobal("AudioContext", SinkContext);
    const fallback = vi.fn();
    setDeviceFallbackListener(fallback);
    prefs.value = { voiceOutputDevice: { id: "gone", label: "Gone speakers" } };
    useDevices([{ kind: "audiooutput", deviceId: "other", label: "Other" }]);
    playFrame(pcmFrame(10));
    await vi.waitFor(() => expect(sources).toHaveLength(1));
    expect(sinkCalls).toEqual([""]);
    expect(fallback).toHaveBeenCalledWith("speaker");
    expect(fieldsOf(log.info, "the speaker of voice playback")).toEqual([
      { device: "system default", saved: true, found: false, sinkIdSupported: true },
    ]);
  });

  it("VOM-FR-KKNN: with no setSinkId, the saved speaker has no effect and no note shows", async () => {
    const fallback = vi.fn();
    setDeviceFallbackListener(fallback);
    prefs.value = { voiceOutputDevice: { id: "a", label: "Desk" } };
    playFrame(pcmFrame(10));
    await vi.waitFor(() => expect(sources).toHaveLength(1));
    expect(fallback).not.toHaveBeenCalled();
    expect(fieldsOf(log.info, "the speaker of voice playback")).toEqual([
      { device: "system default", saved: true, found: null, sinkIdSupported: false },
    ]);
  });

  it("VOM-FR-KKNN: a speaker cleared later goes back to the system default", async () => {
    vi.stubGlobal("AudioContext", SinkContext);
    prefs.value = { voiceOutputDevice: { id: "b", label: "Headset" } };
    useDevices([{ kind: "audiooutput", deviceId: "b", label: "Headset" }]);
    playFrame(pcmFrame(10));
    await vi.waitFor(() => expect(sources).toHaveLength(1));
    prefs.value = { voiceOutputDevice: null };
    playFrame(pcmFrame(10));
    await vi.waitFor(() => expect(sources).toHaveLength(2));
    expect(sinkCalls).toEqual(["b", ""]);
  });

  it("VOM-FR-KKNN: a speaker chosen later applies from the next frame", async () => {
    vi.stubGlobal("AudioContext", SinkContext);
    useDevices([{ kind: "audiooutput", deviceId: "b", label: "Headset" }]);
    playFrame(pcmFrame(10));
    await vi.waitFor(() => expect(sources).toHaveLength(1));
    expect(sinkCalls).toEqual([]);
    prefs.value = { voiceOutputDevice: { id: "b", label: "Headset" } };
    playFrame(pcmFrame(10));
    await vi.waitFor(() => expect(sources).toHaveLength(2));
    expect(sinkCalls).toEqual(["b"]);
  });
});

describe("the test sound", () => {
  it("VAI-FR-BPUR: the tone plays through playback and resolves when it ended", async () => {
    let ended = false;
    const tone = playTestTone().then(() => (ended = true));
    await vi.waitFor(() => expect(sources).toHaveLength(1));
    expect(sources[0].buffer?.duration).toBe(1);
    expect(ended).toBe(false);
    sources[0].onended?.();
    await tone;
    expect(ended).toBe(true);
    expect(segmentRecords()[0]).toMatchObject({ source: "tone" });
  });

  it("VAI-FR-BPUR: a tone that cannot play rejects with playback_failed", async () => {
    initialState = "suspended";
    resumeGives = "suspended";
    await expect(playTestTone()).rejects.toBe("playback_failed");
  });

  it("VAI-FR-BPUR: a stop of playback settles the tone", async () => {
    const tone = playTestTone();
    await vi.waitFor(() => expect(sources).toHaveLength(1));
    stopPlayback();
    await expect(tone).resolves.toBeUndefined();
  });

  it("VAI-FR-BPUR: the tone keeps the listeners of another speech", async () => {
    const failed = vi.fn();
    setPlaybackFailureListener(failed);
    const tone = playTestTone();
    await vi.waitFor(() => expect(sources).toHaveLength(1));
    sources[0].onended?.();
    await tone;
    contexts[0].decodeAudioData.mockRejectedValueOnce(new Error("bad"));
    playFrame(wavFrame());
    await vi.waitFor(() => expect(failed).toHaveBeenCalledWith("playback_failed"));
    const drained = vi.fn();
    setDrainedListener(drained);
    playFrame(pcmFrame(10));
    await vi.waitFor(() => expect(sources).toHaveLength(2));
    sources[1].onended?.();
    expect(drained).toHaveBeenCalledTimes(1);
  });

});
