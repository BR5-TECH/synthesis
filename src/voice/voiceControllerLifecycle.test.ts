/**
 * The microphone of a Push-to-Talk recording at its edges (`VOM-voice-mode.md`):
 * a lost track, a leave while the microphone or the capture opens, and a press
 * while a VAD session still holds the microphone.
 */
import { describe, expect, it, vi } from "vitest";

const { api, audio, mic } = await vi.hoisted(() => import("../test/voiceVadMocks"));

vi.mock("../api", () => api);
vi.mock("../logging", () => ({ logDebug: vi.fn(), logInfo: vi.fn(), logWarn: vi.fn() }));
vi.mock("./playback", () => audio);
vi.mock("./capture", async (original) => ({
  ...(await original<typeof import("./capture")>()),
  openMicrophone: mic.open,
}));

import { getVoiceState, patchVoiceState } from "../state/voiceSession";

import {
  KEY,
  change,
  confirmed,
  deferred,
  discussion,
  handle,
  settle,
  useVadBench,
} from "../test/voiceVadKit";
import {
  TAIL_MS,
  enterVoiceMode,
  leaveVoiceMode,
  noteActivity,
  noteDiscussionChanged,
  pressRecord,
  registerVoiceComposer,
  releaseRecord,
  setInputMode,
} from "./voiceController";
import { MicrophoneError } from "./capture";
import { resetToasts, visibleToasts } from "../state/toasts";

useVadBench();

function readyPtt() {
  enterVoiceMode(KEY, discussion([]));
  patchVoiceState({ providerReady: true, providerSpeaks: true });
  registerVoiceComposer(KEY, handle());
}

describe("a lost microphone", () => {
  it("VOM-FR-NXEB, VOM-FR-DGOG: a track that ends right after the press posts nothing", async () => {
    readyPtt();
    pressRecord();
    await settle();
    mic.onLost?.();
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    await settle();
    expect(api.finishVoiceCapture).not.toHaveBeenCalled();
    expect(api.cancelVoiceCapture).toHaveBeenCalledWith("s1");
    expect(getVoiceState()).toMatchObject({ phase: "idle", error: "microphone_unavailable" });
  });

  it("VOM-FR-NXEB: a loss during the tail of a release sets no error", async () => {
    readyPtt();
    pressRecord();
    await settle();
    await vi.advanceTimersByTimeAsync(1000);
    releaseRecord();
    mic.onLost?.();
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    await settle();
    expect(api.finishVoiceCapture).toHaveBeenCalledTimes(1);
    expect(getVoiceState().error).toBeNull();
  });

  it("VOM-FR-NXEB: a track that ends during a recording ends it as a release does", async () => {
    readyPtt();
    pressRecord();
    await settle();
    expect(getVoiceState().phase).toBe("recording");
    await vi.advanceTimersByTimeAsync(1000);
    mic.onLost?.();
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    await settle();
    expect(mic.stop).toHaveBeenCalled();
    expect(api.finishVoiceCapture).toHaveBeenCalledWith("s1");
    expect(getVoiceState()).toMatchObject({ phase: "idle", error: "microphone_unavailable" });
  });
});

describe("a leave while the recording opens", () => {
  it("VOM-FR-ONLA: a leave before the microphone opens starts no capture", async () => {
    const microphone = deferred<{ rate: number; stop: () => void }>();
    mic.open.mockReturnValueOnce(microphone.promise);
    readyPtt();
    pressRecord();
    leaveVoiceMode();
    microphone.resolve({ rate: 48_000, stop: mic.stop });
    await settle();
    expect(api.startVoiceCapture).not.toHaveBeenCalled();
    expect(mic.stop).toHaveBeenCalled();
    expect(getVoiceState()).toMatchObject({ activeKey: null, phase: "idle" });
  });

  it("VOM-FR-ONLA: a leave while the capture opens cancels it and changes no phase", async () => {
    const start = deferred<{ sessionId: string; route: string; live: boolean }>();
    api.startVoiceCapture.mockReturnValueOnce(start.promise);
    readyPtt();
    pressRecord();
    await settle();
    expect(api.startVoiceCapture).toHaveBeenCalledTimes(1);
    leaveVoiceMode();
    start.resolve({ sessionId: "s1", route: "realtime", live: true });
    await settle();
    expect(api.cancelVoiceCapture).toHaveBeenCalledWith("s1");
    expect(api.finishVoiceCapture).not.toHaveBeenCalled();
    expect(mic.stop).toHaveBeenCalled();
    expect(getVoiceState()).toMatchObject({ activeKey: null, phase: "idle", recordingSince: null });
  });

  it("VOM-FR-ONLA: a move to another discussion while the capture opens keeps the new state idle", async () => {
    const start = deferred<{ sessionId: string; route: string; live: boolean }>();
    api.startVoiceCapture.mockReturnValueOnce(start.promise);
    readyPtt();
    pressRecord();
    await settle();
    enterVoiceMode("d2", discussion([], "d2"));
    start.resolve({ sessionId: "s1", route: "realtime", live: true });
    await settle();
    expect(api.cancelVoiceCapture).toHaveBeenCalledWith("s1");
    expect(getVoiceState()).toMatchObject({ activeKey: "d2", phase: "idle" });
  });
});

describe("a press while a VAD session closes", () => {
  async function closingInItsTail() {
    await confirmed();
    setInputMode("ptt");
    noteActivity(change("speech_end"));
    await settle();
    // The VAD session waits in its tail, and the microphone is still open.
    expect(mic.stop).not.toHaveBeenCalled();
  }

  it("VOM-FR-JMMF: a press during the tail of a VAD utterance opens no microphone", async () => {
    await closingInItsTail();
    expect(getVoiceState().phase).toBe("recording");
    pressRecord();
    await settle();
    expect(mic.open).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    await settle();
    expect(mic.stop).toHaveBeenCalledTimes(1);
    expect(mic.open).toHaveBeenCalledTimes(1);
  });

  // The phase guards the press in the app. These two tests clear the phase,
  // so that they reach the wait of the press itself.
  it("VOM-FR-JMMF: the press opens its microphone after the VAD microphone is released", async () => {
    await closingInItsTail();
    patchVoiceState({ phase: "idle" });
    pressRecord();
    await settle();
    expect(mic.open).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    await settle();
    expect(mic.stop).toHaveBeenCalledTimes(1);
    expect(mic.open).toHaveBeenCalledTimes(2);
    expect(mic.stop.mock.invocationCallOrder[0]).toBeLessThan(
      mic.open.mock.invocationCallOrder[1],
    );
    releaseRecord();
  });

  it("VOM-FR-JMMF, VOM-FR-DGOG: a short press during the wait opens no microphone", async () => {
    await closingInItsTail();
    patchVoiceState({ phase: "idle" });
    pressRecord();
    releaseRecord();
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    await settle();
    expect(mic.open).toHaveBeenCalledTimes(1);
    expect(api.startVoiceCapture).not.toHaveBeenCalled();
  });
});

describe("a recording that fails after a leave", () => {
  it("VOM-FR-ONLA: a microphone that fails after a move shows no error in the new discussion", async () => {
    const microphone = deferred<{ rate: number; stop: () => void }>();
    mic.open.mockReturnValueOnce(microphone.promise);
    readyPtt();
    pressRecord();
    enterVoiceMode("d2", discussion([], "d2"));
    microphone.reject(new MicrophoneError("microphone_unavailable"));
    await settle();
    expect(getVoiceState()).toMatchObject({ activeKey: "d2", phase: "idle", error: null });
  });

  it("VOM-FR-ONLA: a capture that fails after a move shows no error in the new discussion", async () => {
    const start = deferred<{ sessionId: string; route: string; live: boolean }>();
    api.startVoiceCapture.mockReturnValueOnce(start.promise);
    readyPtt();
    pressRecord();
    await settle();
    enterVoiceMode("d2", discussion([], "d2"));
    start.reject(new Error("no_active_provider"));
    await settle();
    expect(mic.stop).toHaveBeenCalled();
    expect(getVoiceState()).toMatchObject({ activeKey: "d2", phase: "idle", error: null });
  });
});

describe("a resolved discussion", () => {
  function resolved(id = KEY) {
    return { ...discussion([], id), resolved: true };
  }

  it("VOM-FR-HRFL: a resolution with voice mode off, or of another discussion, changes nothing", () => {
    resetToasts();
    noteDiscussionChanged(resolved());
    expect(visibleToasts()).toEqual([]);
    enterVoiceMode("opening-key", null, { kind: "artifact", artifactId: "a.md" });
    noteDiscussionChanged(resolved());
    expect(getVoiceState().activeKey).toBe("opening-key");
    expect(visibleToasts()).toEqual([]);
  });

  it("VOM-FR-HRFL: a PTT recording in progress is discarded, and two events give one toast", async () => {
    resetToasts();
    readyPtt();
    pressRecord();
    await settle();
    await vi.advanceTimersByTimeAsync(1000);
    noteDiscussionChanged(resolved());
    noteDiscussionChanged(resolved());
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    await settle();
    expect(api.finishVoiceCapture).not.toHaveBeenCalled();
    expect(api.cancelVoiceCapture).toHaveBeenCalledWith("s1");
    expect(mic.stop).toHaveBeenCalled();
    expect(getVoiceState()).toMatchObject({ activeKey: null, phase: "idle" });
    expect(visibleToasts()).toHaveLength(1);
  });
});
