/** One open VAD session (`VOM-voice-mode.md` VOM-FR-MMAG, `VOP-voice-ai-providers.md`). */
import { beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  closeVoiceActivitySession: vi.fn(async (): Promise<{ captureId: string | null }> => ({
    captureId: null,
  })),
  feedVoiceActivity: vi.fn(async (): Promise<void> => undefined),
  openVoiceActivitySession: vi.fn(async () => ({ activityId: "va1" })),
  setVoiceActivityGate: vi.fn(async (): Promise<void> => undefined),
}));
const mic = vi.hoisted(() => ({
  stop: vi.fn(),
  open: vi.fn(),
  onChunk: null as null | ((pcm: ArrayBuffer) => void),
  onLost: null as null | (() => void),
}));
const log = vi.hoisted(() => ({ logWarn: vi.fn() }));

vi.mock("../api", () => api);
vi.mock("../logging", () => log);
vi.mock("./capture", async (original) => ({
  ...(await original<typeof import("./capture")>()),
  openMicrophone: mic.open,
}));

import { MicrophoneError } from "./capture";
import { VadSession, type VadGate } from "./vadSession";

let gate: VadGate;
const lost = vi.fn();
const source = { gate: () => gate, lost };

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

function frame(bytes = 2880): ArrayBuffer {
  return new ArrayBuffer(bytes);
}

async function open(): Promise<VadSession> {
  return VadSession.open(["arch"], source);
}

beforeEach(() => {
  vi.clearAllMocks();
  mic.onChunk = null;
  mic.onLost = null;
  gate = { blocked: false, playing: false, keyterms: ["arch"] };
  mic.open.mockImplementation(
    async (onChunk: (pcm: ArrayBuffer) => void, onLost: () => void) => {
      mic.onChunk = onChunk;
      mic.onLost = onLost;
      return { rate: 48_000, stop: mic.stop };
    },
  );
});

describe("opening", () => {
  it("VOM-FR-MMAG: audio from before the reply goes after it, in order", async () => {
    const reply = deferred<{ activityId: string }>();
    api.openVoiceActivitySession.mockReturnValueOnce(reply.promise);
    const opening = open();
    await vi.waitFor(() => expect(mic.onChunk).not.toBeNull());
    await vi.waitFor(() => expect(api.openVoiceActivitySession).toHaveBeenCalled());
    mic.onChunk?.(new Uint8Array([1, 2]).buffer);
    mic.onChunk?.(new Uint8Array(new Array(2880).fill(3)).buffer);
    expect(api.feedVoiceActivity).not.toHaveBeenCalled();
    reply.resolve({ activityId: "va1" });
    await opening;
    expect(api.feedVoiceActivity).toHaveBeenCalledTimes(1);
    const [activityId, bytes] = api.feedVoiceActivity.mock.calls[0] as unknown as [
      string,
      Uint8Array,
    ];
    expect(activityId).toBe("va1");
    expect(bytes.byteLength).toBe(2882);
    expect([...bytes.slice(0, 3)]).toEqual([1, 2, 3]);
  });

  it("VOM-FR-CUCN: a refused open releases the microphone and passes the code on", async () => {
    api.openVoiceActivitySession.mockRejectedValueOnce("voice_detector_unavailable");
    await expect(open()).rejects.toBe("voice_detector_unavailable");
    expect(mic.stop).toHaveBeenCalledTimes(1);
    expect(api.feedVoiceActivity).not.toHaveBeenCalled();
  });

  it("VOM-FR-CUCN: a refused microphone calls no command", async () => {
    mic.open.mockRejectedValueOnce(new MicrophoneError("microphone_denied"));
    await expect(open()).rejects.toBeInstanceOf(MicrophoneError);
    expect(api.openVoiceActivitySession).not.toHaveBeenCalled();
  });

  it("VOM-FR-CSXI: a loss during the open waits for the caller to hold the session", async () => {
    const reply = deferred<{ activityId: string }>();
    api.openVoiceActivitySession.mockReturnValueOnce(reply.promise);
    const opening = open();
    await vi.waitFor(() => expect(mic.onLost).not.toBeNull());
    await vi.waitFor(() => expect(api.openVoiceActivitySession).toHaveBeenCalled());
    mic.onLost?.();
    reply.resolve({ activityId: "va1" });
    const session = await opening;
    expect(lost).not.toHaveBeenCalled();
    expect(session.takeLoss()).toBe("microphone_unavailable");
    expect(session.takeLoss()).toBeNull();
  });
});

describe("a lost feed", () => {
  it.each([
    ["voice_session_lost", "voice_session_lost"],
    [new Error("with text"), "unknown"],
  ])("VOM-FR-CSXI: a refused feed reports a code (%s)", async (refusal, code) => {
    await open();
    api.feedVoiceActivity.mockRejectedValueOnce(refusal);
    mic.onChunk?.(frame());
    await vi.waitFor(() => expect(lost).toHaveBeenCalledWith(code));
  });

  it("VOM-FR-CSXI: a lost track reports its code", async () => {
    await open();
    mic.onLost?.();
    expect(lost).toHaveBeenCalledWith("microphone_unavailable");
  });

  it("VOM-FR-CSXI: after the close, a failure reports nothing", async () => {
    const session = await open();
    await session.close(false);
    mic.onLost?.();
    expect(lost).not.toHaveBeenCalled();
  });
});

describe("the gate", () => {
  it("VOP-FR-QZCE: each push is a whole record, and the same record does not go twice", async () => {
    const session = await open();
    session.pushGate();
    session.pushGate();
    expect(api.setVoiceActivityGate).toHaveBeenCalledTimes(1);
    expect(api.setVoiceActivityGate).toHaveBeenLastCalledWith("va1", {
      seq: 1,
      blocked: false,
      playing: false,
      settled: null,
      keyterms: ["arch"],
    });
    gate = { ...gate, blocked: true };
    session.pushGate();
    expect(api.setVoiceActivityGate).toHaveBeenLastCalledWith(
      "va1",
      expect.objectContaining({ seq: 2, blocked: true }),
    );
  });

  it("VOP-FR-FMRV: a settle goes once, and later records keep it", async () => {
    const session = await open();
    session.settle("c1");
    session.settle("c1");
    expect(api.setVoiceActivityGate).toHaveBeenCalledTimes(1);
    gate = { ...gate, playing: true };
    session.pushGate();
    expect(api.setVoiceActivityGate).toHaveBeenLastCalledWith(
      "va1",
      expect.objectContaining({ settled: "c1", playing: true }),
    );
  });

  it("VOP-FR-QZCE: a refused gate logs a code, and a later change pushes", async () => {
    const session = await open();
    api.setVoiceActivityGate.mockRejectedValueOnce("broken");
    session.pushGate();
    await vi.waitFor(() => expect(log.logWarn).toHaveBeenCalled());
    gate = { ...gate, blocked: true };
    session.pushGate();
    expect(api.setVoiceActivityGate).toHaveBeenCalledTimes(2);
  });

  it("VOP-FR-QZCE: a closed session pushes nothing", async () => {
    const session = await open();
    await session.close(true);
    session.pushGate();
    expect(api.setVoiceActivityGate).not.toHaveBeenCalled();
  });
});

describe("closing", () => {
  it("VOM-FR-XEIA: the rest of the audio goes before the close", async () => {
    const session = await open();
    mic.onChunk?.(frame(100));
    session.stopMicrophone();
    session.stopMicrophone();
    expect(mic.stop).toHaveBeenCalledTimes(1);
    expect(api.feedVoiceActivity).toHaveBeenCalledTimes(1);
    api.closeVoiceActivitySession.mockResolvedValueOnce({ captureId: "c1" });
    await expect(session.close(false)).resolves.toBe("c1");
    expect(api.closeVoiceActivitySession).toHaveBeenCalledWith("va1", false);
    expect(api.feedVoiceActivity.mock.invocationCallOrder[0]).toBeLessThan(
      api.closeVoiceActivitySession.mock.invocationCallOrder[0],
    );
  });

  it("VOM-FR-XEIA: the close waits for the chunk in flight", async () => {
    const session = await open();
    const feed = deferred<void>();
    api.feedVoiceActivity.mockReturnValueOnce(feed.promise);
    mic.onChunk?.(frame());
    const closing = session.close(false);
    await new Promise((done) => setTimeout(done, 0));
    expect(api.closeVoiceActivitySession).not.toHaveBeenCalled();
    feed.resolve();
    await closing;
    expect(api.closeVoiceActivitySession).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-XEIA: a refused close passes on, and the session stays closed", async () => {
    const session = await open();
    api.closeVoiceActivitySession.mockRejectedValueOnce("broken");
    await expect(session.close(false)).rejects.toBe("broken");
    session.pushGate();
    expect(api.setVoiceActivityGate).not.toHaveBeenCalled();
  });
});
