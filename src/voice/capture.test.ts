import { afterEach, describe, expect, it, vi } from "vitest";

import { ChunkPump, MicrophoneError, openMicrophone } from "./capture";

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
});
