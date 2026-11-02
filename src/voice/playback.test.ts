import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  pausePlayback,
  playFrame,
  prepareAudio,
  resetPlaybackForTest,
  resumePlayback,
  setDrainedListener,
  stopPlayback,
} from "./playback";

class FakeSource {
  buffer: { duration: number } | null = null;
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
let decoded: ArrayBuffer[];
let contexts: number;
let context: FakeContext;

class FakeContext {
  currentTime = 1;
  destination = {};
  constructor() {
    contexts += 1;
    // eslint-disable-next-line @typescript-eslint/no-this-alias
    context = this;
  }
  resume = vi.fn(async () => undefined);
  suspend = vi.fn(async () => undefined);
  createBuffer(_: number, length: number, rate: number) {
    return { duration: length / rate, getChannelData: () => new Float32Array(length) };
  }
  createBufferSource() {
    const source = new FakeSource();
    sources.push(source);
    return source;
  }
  decodeAudioData = vi.fn(async (data: ArrayBuffer) => {
    decoded.push(data);
    return { duration: 0.5 };
  });
}

function pcmFrame(samples: number, rate = 24_000): ArrayBuffer {
  const out = new Uint8Array(5 + samples * 2);
  new DataView(out.buffer).setUint32(1, rate, true);
  return out.buffer;
}

function fileFrame(...bytes: number[]): ArrayBuffer {
  return new Uint8Array([1, 0, 0, 0, 0, ...bytes]).buffer;
}

beforeEach(() => {
  sources = [];
  decoded = [];
  contexts = 0;
  vi.stubGlobal("AudioContext", FakeContext);
  resetPlaybackForTest();
});

afterEach(() => {
  resetPlaybackForTest();
  vi.unstubAllGlobals();
});

describe("playback", () => {
  it("VOM-FR-SJYH: frames start exactly where the frame before ends", async () => {
    prepareAudio();
    playFrame(pcmFrame(2400));
    playFrame(pcmFrame(2400));
    playFrame(pcmFrame(2400));
    await vi.waitFor(() => expect(sources).toHaveLength(3));
    expect(sources[0].startedAt).toBeCloseTo(1.05, 6);
    expect(sources[1].startedAt).toBeCloseTo(1.15, 6);
    expect(sources[2].startedAt).toBeCloseTo(1.25, 6);
  });

  it("VOM-FR-SJYH: a PCM frame does not pass a file that is decoding", async () => {
    playFrame(fileFrame(9));
    playFrame(pcmFrame(2400));
    await vi.waitFor(() => expect(sources).toHaveLength(2));
    expect(decoded).toHaveLength(1);
    expect(sources[0].buffer?.duration).toBe(0.5);
    expect(sources[1].startedAt).toBeCloseTo(1.55, 6);
  });

  it("VOM non-functional: one AudioContext serves playback, and it resumes", async () => {
    prepareAudio();
    prepareAudio();
    playFrame(pcmFrame(10));
    await vi.waitFor(() => expect(sources).toHaveLength(1));
    expect(contexts).toBe(1);
  });

  it("VOM-FR-QDWW: stops at once, and a frame in flight is dropped", async () => {
    playFrame(pcmFrame(2400));
    await vi.waitFor(() => expect(sources).toHaveLength(1));
    playFrame(fileFrame(1));
    stopPlayback();
    expect(sources[0].stopped).toBe(true);
    await vi.waitFor(() => expect(decoded).toHaveLength(1));
    expect(sources).toHaveLength(1);
  });

  it("VOM-FR-QDWW: tells when all audio has played", async () => {
    const drained = vi.fn();
    setDrainedListener(drained);
    playFrame(pcmFrame(10));
    await vi.waitFor(() => expect(sources).toHaveLength(1));
    expect(drained).not.toHaveBeenCalled();
    sources[0].onended?.();
    expect(drained).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-FVRS: a pause suspends the context once", () => {
    prepareAudio();
    pausePlayback();
    pausePlayback();
    expect(context.suspend).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-FVRS: with no context, a pause does nothing, and neither does a resume", () => {
    pausePlayback();
    resumePlayback();
    expect(contexts).toBe(0);
  });

  it("VOM-FR-KVHG: a resume goes once, and only after a pause", () => {
    prepareAudio();
    resumePlayback();
    expect(context.resume).not.toHaveBeenCalled();
    pausePlayback();
    resumePlayback();
    resumePlayback();
    expect(context.resume).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-FVRS, VOM-FR-SJYH: a frame in a pause waits in the queue, with no gap", async () => {
    prepareAudio();
    playFrame(pcmFrame(2400));
    await vi.waitFor(() => expect(sources).toHaveLength(1));
    pausePlayback();
    context.resume.mockClear();
    playFrame(pcmFrame(2400));
    await vi.waitFor(() => expect(sources).toHaveLength(2));
    expect(context.resume).not.toHaveBeenCalled();
    expect(sources[1].startedAt).toBeCloseTo(1.15, 6);
  });

  it("VOM-FR-QDWW: a stop during a pause leaves the next reply able to sound", async () => {
    prepareAudio();
    playFrame(pcmFrame(2400));
    await vi.waitFor(() => expect(sources).toHaveLength(1));
    pausePlayback();
    context.resume.mockClear();
    stopPlayback();
    expect(context.resume).toHaveBeenCalledTimes(1);
    expect(sources[0].stopped).toBe(true);
    playFrame(pcmFrame(10));
    expect(context.resume).toHaveBeenCalledTimes(2);
  });

  it("VOM-FR-FVRS: a context that refuses gives no unhandled rejection", async () => {
    prepareAudio();
    context.suspend.mockRejectedValueOnce(new Error("no"));
    context.resume.mockRejectedValueOnce(new Error("no"));
    pausePlayback();
    resumePlayback();
    await new Promise((done) => setTimeout(done, 0));
    expect(context.suspend).toHaveBeenCalledTimes(1);
    expect(context.resume).toHaveBeenCalledTimes(1);
  });
});
