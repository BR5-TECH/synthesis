import { afterEach, describe, expect, it, vi } from "vitest";

import { ChunkPump, FrameBatcher, MicrophoneError, openMicrophone } from "./capture";

function buffer(...bytes: number[]): ArrayBuffer {
  return new Uint8Array(bytes).buffer;
}

describe("the chunk pump", () => {
  it("VOM non-functional: holds audio until the session opens", async () => {
    const sent: number[][] = [];
    const pump = new ChunkPump(async (_, bytes) => {
      sent.push([...bytes]);
    });
    pump.push(buffer(1));
    expect(sent).toEqual([]);
    pump.attach("s");
    await pump.flush();
    expect(sent).toEqual([[1]]);
  });

  it("VOM non-functional: one chunk is in flight, and a backlog merges in order", async () => {
    const sent: number[][] = [];
    const gates: (() => void)[] = [];
    const pump = new ChunkPump(
      (_, bytes) =>
        new Promise<void>((resolve) => {
          sent.push([...bytes]);
          gates.push(resolve);
        }),
    );
    pump.attach("s");
    pump.push(buffer(1));
    pump.push(buffer(2));
    pump.push(buffer(3));
    expect(sent).toEqual([[1]]);
    const flushed = pump.flush();
    gates[0]();
    await vi.waitFor(() => expect(sent).toEqual([[1], [2, 3]]));
    gates[1]();
    await flushed;
  });

  it("VOM non-functional: a failed chunk does not stop the next one", async () => {
    const sent: number[][] = [];
    let first = true;
    const pump = new ChunkPump(async (_, bytes) => {
      sent.push([...bytes]);
      if (first) {
        first = false;
        throw new Error("voice_session_lost");
      }
    });
    pump.attach("s");
    pump.push(buffer(1));
    await pump.flush();
    pump.push(buffer(2));
    await pump.flush();
    expect(sent).toEqual([[1], [2]]);
  });
});

describe("the frame batcher", () => {
  function batcher(rate?: number) {
    const sent: number[] = [];
    const batch = new FrameBatcher((pcm) => sent.push(pcm.byteLength));
    if (rate !== undefined) batch.setRate(rate);
    return { batch, sent };
  }

  it("VOM non-functional: with no rate, audio waits", () => {
    const { batch, sent } = batcher();
    batch.push(new ArrayBuffer(100_000));
    expect(sent).toEqual([]);
  });

  it.each([
    [48_000, 2880],
    [16_000, 960],
  ])("VOM non-functional: at %i Hz, a frame of 30 ms is %i bytes", (rate, bytes) => {
    const { batch, sent } = batcher(rate);
    batch.push(new ArrayBuffer(bytes - 2));
    expect(sent).toEqual([]);
    batch.push(new ArrayBuffer(2));
    expect(sent).toEqual([bytes]);
  });

  it("VOM non-functional: blocks join in order", () => {
    const parts: number[][] = [];
    const batch = new FrameBatcher((pcm) => parts.push([...new Uint8Array(pcm)]));
    batch.setRate(100);
    batch.push(buffer(1, 2));
    batch.push(buffer(3, 4, 5, 6));
    expect(parts).toEqual([[1, 2, 3, 4, 5, 6]]);
  });

  it("VOM non-functional: audio from before the rate goes with the next block", () => {
    const { batch, sent } = batcher();
    batch.push(new ArrayBuffer(4000));
    batch.setRate(48_000);
    batch.push(new ArrayBuffer(2));
    expect(sent).toEqual([4002]);
  });

  it("VOM-FR-XEIA: a flush sends a part of a frame, and an empty flush sends nothing", () => {
    const { batch, sent } = batcher(48_000);
    batch.push(new ArrayBuffer(10));
    batch.flush();
    batch.flush();
    expect(sent).toEqual([10]);
  });
});

describe("the microphone", () => {
  afterEach(() => vi.unstubAllGlobals());

  const failing = async (error: unknown) => {
    vi.stubGlobal("navigator", { mediaDevices: { getUserMedia: vi.fn().mockRejectedValue(error) } });
    await expect(openMicrophone(() => undefined)).rejects.toBeInstanceOf(MicrophoneError);
    return openMicrophone(() => undefined).catch((e: MicrophoneError) => e.message);
  };

  it("VOM-FR-YIJS: asks for the WebView's voice processing constraints", async () => {
    const getUserMedia = vi.fn().mockRejectedValue(new DOMException("no", "NotAllowedError"));
    vi.stubGlobal("navigator", { mediaDevices: { getUserMedia } });
    await openMicrophone(() => undefined).catch(() => undefined);
    expect(getUserMedia).toHaveBeenCalledWith({
      audio: expect.objectContaining({
        echoCancellation: true,
        noiseSuppression: true,
        autoGainControl: true,
      }),
    });
  });

  it("VOM-FR-ZFSA: names a denied, a missing, and another failure", async () => {
    expect(await failing(new DOMException("no", "NotAllowedError"))).toBe("microphone_denied");
    expect(await failing(new DOMException("no", "NotFoundError"))).toBe("microphone_missing");
    expect(await failing(new DOMException("no", "AbortError"))).toBe("microphone_unavailable");
  });

  it("VOM-FR-ZFSA: a WebView with no capture API is a failure", async () => {
    vi.stubGlobal("navigator", {});
    await expect(openMicrophone(() => undefined)).rejects.toThrow("microphone_unavailable");
  });

  function stubbed(context: unknown) {
    const track = { stop: vi.fn() };
    vi.stubGlobal("navigator", {
      mediaDevices: { getUserMedia: vi.fn(async () => ({ getTracks: () => [track] })) },
    });
    vi.stubGlobal("AudioContext", context);
    return track;
  }

  it("VOM-FR-ZFSA, VOM-FR-YIJS: a worklet that does not load releases the microphone", async () => {
    const close = vi.fn(async () => undefined);
    const track = stubbed(
      class {
        audioWorklet = { addModule: vi.fn(async () => Promise.reject(new Error("no"))) };
        close = close;
      },
    );
    await expect(openMicrophone(() => undefined)).rejects.toThrow("microphone_unavailable");
    expect(track.stop).toHaveBeenCalled();
    expect(close).toHaveBeenCalled();
  });

  it("VOM-FR-ZFSA, VOM-FR-YIJS: an AudioContext that cannot start releases the microphone", async () => {
    const track = stubbed(
      class {
        constructor() {
          throw new Error("no");
        }
      },
    );
    await expect(openMicrophone(() => undefined)).rejects.toThrow("microphone_unavailable");
    expect(track.stop).toHaveBeenCalled();
  });

  it("VOM-FR-YIJS: stop releases the microphone and closes the capture", async () => {
    const close = vi.fn(async () => undefined);
    const node = { connect: vi.fn(), port: { onmessage: null as unknown } };
    class Context {
      sampleRate = 48_000;
      destination = {};
      audioWorklet = { addModule: vi.fn(async () => undefined) };
      resume = vi.fn(async () => undefined);
      close = close;
      createMediaStreamSource = () => ({ connect: vi.fn() });
      createGain = () => ({ gain: { value: 1 }, connect: vi.fn() });
    }
    const track = stubbed(Context);
    vi.stubGlobal(
      "AudioWorkletNode",
      class {
        constructor() {
          return node;
        }
      },
    );
    const chunks: ArrayBuffer[] = [];
    const mic = await openMicrophone((pcm) => chunks.push(pcm));
    expect(mic.rate).toBe(48_000);
    (node.port.onmessage as (e: { data: ArrayBuffer }) => void)({ data: new ArrayBuffer(2) });
    expect(chunks).toHaveLength(1);
    mic.stop();
    expect(track.stop).toHaveBeenCalled();
    expect(close).toHaveBeenCalled();
    expect(node.port.onmessage).toBeNull();
  });

  describe("a track that ends", () => {
    class Context {
      sampleRate = 48_000;
      destination = {};
      audioWorklet = { addModule: vi.fn(async () => undefined) };
      resume = vi.fn(async () => undefined);
      close = vi.fn(async () => undefined);
      createMediaStreamSource = () => ({ connect: vi.fn() });
      createGain = () => ({ gain: { value: 1 }, connect: vi.fn() });
    }

    function tracked() {
      const track = stubbed(Context) as { stop: () => void; onended?: (() => void) | null };
      vi.stubGlobal(
        "AudioWorkletNode",
        class {
          connect = vi.fn();
          port = { onmessage: null };
        },
      );
      return track;
    }

    it("VOM-FR-CSXI: reports a loss", async () => {
      const track = tracked();
      const lost = vi.fn();
      await openMicrophone(() => undefined, lost);
      track.onended?.();
      expect(lost).toHaveBeenCalledTimes(1);
    });

    it("VOM-FR-CSXI: `stop` is no loss", async () => {
      const track = tracked();
      const lost = vi.fn();
      const mic = await openMicrophone(() => undefined, lost);
      mic.stop();
      expect(track.onended).toBeNull();
      expect(lost).not.toHaveBeenCalled();
    });

    it("VOM-FR-CSXI: with no listener, no handler is set", async () => {
      const track = tracked();
      await openMicrophone(() => undefined);
      expect(track.onended).toBeUndefined();
    });
  });
});
