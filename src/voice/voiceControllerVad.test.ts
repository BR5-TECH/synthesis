/**
 * VAD sessions in the controller (`VOM-voice-mode.md`): opening, closing,
 * utterances, the gate, and the surfaces.
 * `VadSession`, the batcher, and the pump stay real, so a failed feed runs the
 * true path.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const { api, audio, mic } = await vi.hoisted(() => import("../test/voiceVadMocks"));

vi.mock("../api", () => api);
vi.mock("../logging", () => ({ logDebug: vi.fn(), logInfo: vi.fn(), logWarn: vi.fn() }));
vi.mock("./playback", () => audio);
vi.mock("./capture", async (original) => ({
  ...(await original<typeof import("./capture")>()),
  openMicrophone: mic.open,
}));

import { getDiscussionSession } from "../state/discussionSession";
import { getLiveBubble } from "../state/liveBubble";
import { getVoiceState, patchVoiceState } from "../state/voiceSession";
import { MicrophoneError } from "./capture";
import {
  KEY,
  bench,
  change,
  confirmed,
  deferred,
  discussion,
  expectSettledWith,
  handle,
  lastGate,
  openSession,
  ready,
  replyPaused,
  replyPlays,
  settle,
  useVadBench,
} from "../test/voiceVadKit";
import {
  RECORDING_LIMIT_MS,
  TAIL_MS,
  endVoiceForTeardown,
  enterVoiceMode,
  leaveVoiceMode,
  noteActivity,
  noteComposerChanged,
  noteProvider,
  noteTranscript,
  openVadSession,
  pressRecord,
  registerVoiceComposer,
  releaseRecord,
  retryRecording,
  setInputMode,
  setVadMuted,
} from "./voiceController";
import { getMicLevel, resetMicLevelForTest } from "../state/voiceLevel";

useVadBench();

beforeEach(() => {
  resetMicLevelForTest();
});

describe("opening a session", () => {
  it("VOM-FR-PDZR, VOM-FR-MMAG, VOM-FR-SIEO, VOM-FR-LNUH: selecting VAD opens one session with no press and calls no provider", async () => {
    await openSession();
    expect(mic.open).toHaveBeenCalledTimes(1);
    expect(api.openVoiceActivitySession).toHaveBeenCalledWith(48_000, ["arch"]);
    expect(mic.open.mock.invocationCallOrder[0]).toBeLessThan(
      api.openVoiceActivitySession.mock.invocationCallOrder[0],
    );
    expect(getVoiceState().phase).toBe("idle");
    expect(api.startVoiceCapture).not.toHaveBeenCalled();
    expect(api.finishVoiceCapture).not.toHaveBeenCalled();
    expect(api.speakVoiceSegment).not.toHaveBeenCalled();
  });

  it("VOM-FR-PDZR, VOM-FR-WPRW: in VAD mode a press and the hotkey open and close nothing", async () => {
    await openSession();
    pressRecord();
    releaseRecord();
    await settle();
    expect(getVoiceState().vadOpen).toBe(true);
    expect(mic.open).toHaveBeenCalledTimes(1);
    expect(api.closeVoiceActivitySession).not.toHaveBeenCalled();
  });

  it("VOM-FR-PDZR: Push-to-Talk opens no session", async () => {
    enterVoiceMode(KEY, discussion([]));
    patchVoiceState({ providerReady: true });
    registerVoiceComposer(KEY, handle());
    await settle();
    expect(getVoiceState().inputMode).toBe("ptt");
    expect(mic.open).not.toHaveBeenCalled();
  });

  it("VOM-FR-VVTY, VOM-FR-VGCE: a switch to VAD stops playback and opens at once", async () => {
    enterVoiceMode(KEY, discussion([]));
    patchVoiceState({ providerReady: true, providerSpeaks: true });
    registerVoiceComposer(KEY, handle());
    await replyPlays();
    audio.stopPlayback.mockClear();
    setInputMode("vad");
    expect(audio.stopPlayback).toHaveBeenCalledTimes(1);
    expect(api.cancelVoiceSpeech).toHaveBeenCalledTimes(1);
    expect(getVoiceState().playing).toBe(false);
    await settle();
    expect(getVoiceState().vadOpen).toBe(true);
  });

  it("VOM-FR-FASJ, VOM-FR-CUCN: Open again drops a held recording", async () => {
    await confirmed();
    api.finishVoiceCapture.mockRejectedValueOnce("provider_unreachable");
    noteActivity(change("speech_end", { captureId: "c1" }));
    await settle();
    expect(getVoiceState()).toMatchObject({ vadOpen: true, canRetry: true });
    // VOM-FR-CSXI: a fatal failure closes the session, and the held recording stays.
    noteActivity(change("failed", { code: "voice_detector_failed", fatal: true }));
    await settle();
    expect(getVoiceState().vadOpen).toBe(false);
    expect(api.cancelVoiceCapture).not.toHaveBeenCalled();
    openVadSession("again");
    await settle();
    expect(api.cancelVoiceCapture).toHaveBeenCalledWith("c1");
    expect(getVoiceState()).toMatchObject({ vadOpen: true, canRetry: false, error: null });
  });

  it.each(["microphone_denied", "microphone_missing"])(
    "VOM-FR-CUCN, VOM-FR-ZFSA: %s opens no session",
    async (code) => {
      mic.open.mockRejectedValueOnce(new MicrophoneError(code));
      ready();
      await settle();
      expect(getVoiceState()).toMatchObject({
        error: code,
        canRetry: false,
        vadOpen: false,
        phase: "idle",
      });
      expect(api.openVoiceActivitySession).not.toHaveBeenCalled();
    },
  );

  it("VOM-FR-CUCN: a refused open releases the microphone, retries nothing, and Open again works", async () => {
    api.openVoiceActivitySession.mockRejectedValueOnce("voice_detector_unavailable");
    ready();
    await settle();
    expect(mic.stop).toHaveBeenCalledTimes(1);
    expect(getVoiceState()).toMatchObject({
      error: "voice_detector_unavailable",
      vadOpen: false,
      phase: "idle",
    });
    await vi.advanceTimersByTimeAsync(60_000);
    expect(api.openVoiceActivitySession).toHaveBeenCalledTimes(1);
    openVadSession("again");
    await settle();
    expect(getVoiceState().vadOpen).toBe(true);
    expect(getVoiceState().error).toBeNull();
  });

  it("VOM-FR-GTAQ: a blocked composer still opens, and the gate blocks utterances", async () => {
    await openSession(handle({ blocked: () => true }));
    expect(lastGate()).toMatchObject({ blocked: true });
  });

  it.each([
    ["Push-to-Talk", () => setInputMode("ptt")],
    ["a teardown", () => endVoiceForTeardown()],
    ["leaving", () => leaveVoiceMode()],
    ["a lost provider", () => noteProvider(false, false)],
  ])(
    "VOM-FR-PDZR, VOM-FR-BQBI, VOM-FR-KQAI: %s during the open leaves no session",
    async (_, act) => {
      const microphone = deferred<{ rate: number; stop: () => void }>();
      mic.open.mockReturnValueOnce(microphone.promise);
      ready();
      await settle();
      act();
      microphone.resolve({ rate: 48_000, stop: mic.stop });
      await settle();
      expect(mic.stop).toHaveBeenCalledTimes(1);
      expect(api.closeVoiceActivitySession).toHaveBeenCalledWith("va1", true);
      expect(getVoiceState().vadOpen).toBe(false);
      expect(getVoiceState().phase).toBe("idle");
      expect(api.setVoiceActivityGate).not.toHaveBeenCalled();
    },
  );

  it("VOM-FR-PDZR, VOM-FR-KEMU: a new entry during a cancelled open keeps that open, and one session stays", async () => {
    const microphone = deferred<{ rate: number; stop: () => void }>();
    mic.open.mockReturnValueOnce(microphone.promise);
    ready();
    await settle();
    leaveVoiceMode();
    ready();
    microphone.resolve({ rate: 48_000, stop: mic.stop });
    await settle();
    expect(getVoiceState()).toMatchObject({ vadOpen: true, phase: "idle", activeKey: KEY });
    expect(mic.open).toHaveBeenCalledTimes(1);
    expect(api.openVoiceActivitySession).toHaveBeenCalledTimes(1);
    expect(api.closeVoiceActivitySession).not.toHaveBeenCalled();
  });

  it("VOM-FR-PDZR, VOM-FR-VGCE: VAD, Push-to-Talk, and VAD again during the open leave one session", async () => {
    const microphone = deferred<{ rate: number; stop: () => void }>();
    mic.open.mockReturnValueOnce(microphone.promise);
    ready();
    await settle();
    setInputMode("ptt");
    setInputMode("vad");
    expect(getVoiceState().inputMode).toBe("vad");
    microphone.resolve({ rate: 48_000, stop: mic.stop });
    await settle();
    expect(getVoiceState()).toMatchObject({ vadOpen: true, phase: "idle" });
    expect(api.openVoiceActivitySession).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-RBWX, VOM-FR-CUCN: Open again after a fatal failure while muted opens unmuted", async () => {
    await openSession();
    setVadMuted(true);
    noteActivity(change("failed", { code: "voice_detector_failed", fatal: true }));
    await settle();
    expect(getVoiceState()).toMatchObject({ vadOpen: false, muted: false });
    openVadSession("again");
    await settle();
    expect(getVoiceState()).toMatchObject({ vadOpen: true, muted: false });
  });

  it("VOM-FR-CSXI: a track that ends during the open closes the session", async () => {
    const open = deferred<{ activityId: string }>();
    api.openVoiceActivitySession.mockReturnValueOnce(open.promise);
    ready();
    await settle();
    mic.onLost?.();
    open.resolve({ activityId: "va1" });
    await settle();
    expect(getVoiceState()).toMatchObject({ error: "microphone_unavailable", vadOpen: false });
    expect(mic.stop).toHaveBeenCalledTimes(1);
    expect(api.closeVoiceActivitySession).toHaveBeenCalledTimes(1);
  });
});

describe("Mute and the level", () => {
  /** 48 kHz PCM16 of more than one 30 ms frame, at a constant level. */
  function loud(value = 8000): ArrayBuffer {
    return new Int16Array(2048).fill(value).buffer;
  }

  /** Whether each fed block was silence, in feed order. */
  function fedSilence(): boolean[] {
    const calls = api.feedVoiceActivity.mock.calls as unknown as [string, Uint8Array][];
    return calls.map(([, bytes]) => new Uint8Array(bytes).every((b) => b === 0));
  }

  it("VOM-FR-MUZB: while muted silence goes to the session, the gate blocks, and the session stays open", async () => {
    await openSession();
    mic.onChunk?.(loud());
    await settle();
    setVadMuted(true);
    expect(lastGate()).toMatchObject({ blocked: true });
    // VOP-FR-KUGV: the feed goes on for longer than the 30 s limit of the backend.
    for (let second = 0; second < 40; second += 1) {
      mic.onChunk?.(loud());
      await vi.advanceTimersByTimeAsync(1_000);
    }
    const fed = fedSilence();
    expect(fed[0]).toBe(false);
    expect(fed.length).toBeGreaterThan(30);
    expect(fed.slice(1).every(Boolean)).toBe(true);
    expect(getVoiceState()).toMatchObject({ vadOpen: true, muted: true });
    expect(mic.stop).not.toHaveBeenCalled();
    expect(api.closeVoiceActivitySession).not.toHaveBeenCalled();
  });

  it("VOM-FR-MUZB: a Mute during an utterance takes effect when that utterance ends", async () => {
    await confirmed();
    setVadMuted(true);
    expect(lastGate()).toMatchObject({ blocked: true });
    mic.onChunk?.(loud());
    await settle();
    expect(fedSilence()).toEqual([false]);
    noteActivity(change("speech_end", { captureId: "c1" }));
    await settle();
    mic.onChunk?.(loud());
    await settle();
    expect(fedSilence()).toEqual([false, true]);
    expect(lastGate()).toMatchObject({ blocked: true });
    expect(bench.composer.post).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-MUZB: a Mute while speech is heard and not confirmed yet keeps its audio", async () => {
    await openSession();
    noteActivity(change("speech_start"));
    setVadMuted(true);
    mic.onChunk?.(loud());
    await settle();
    expect(fedSilence()).toEqual([false]);
    noteActivity(change("misfire"));
    mic.onChunk?.(loud());
    await settle();
    expect(fedSilence()).toEqual([false, true]);
  });

  it("VOM-FR-RBWX: Unmute feeds again, and a session opens unmuted", async () => {
    await openSession();
    setVadMuted(true);
    setVadMuted(false);
    expect(lastGate()).toMatchObject({ blocked: false });
    mic.onChunk?.(loud());
    await settle();
    expect(fedSilence()).toEqual([false]);
    setVadMuted(true);
    setInputMode("ptt");
    await settle();
    expect(getVoiceState().muted).toBe(false);
    setInputMode("vad");
    await settle();
    expect(getVoiceState()).toMatchObject({ vadOpen: true, muted: false });
  });

  it("VOM-FR-WFHN, VOM-FR-ZDLY: the level comes from the microphone, and is flat while muted or closed", async () => {
    await openSession();
    mic.onChunk?.(loud(8000));
    expect(getMicLevel()).toBeGreaterThan(0.3);
    mic.onChunk?.(loud(0));
    expect(getMicLevel()).toBe(0);
    mic.onChunk?.(loud(8000));
    setVadMuted(true);
    expect(getMicLevel()).toBe(0);
    mic.onChunk?.(loud(8000));
    expect(getMicLevel()).toBe(0);
    setVadMuted(false);
    mic.onChunk?.(loud(8000));
    setInputMode("ptt");
    expect(getMicLevel()).toBe(0);
    expect(api.startVoiceCapture).not.toHaveBeenCalled();
  });
});

describe("closing a session", () => {
  it("VOM-FR-XEIA, VOM-FR-FTFU: the tail, the microphone, the last audio, then the close", async () => {
    await confirmed();
    api.closeVoiceActivitySession.mockResolvedValueOnce({ captureId: "c1" });
    setInputMode("ptt");
    expect(getVoiceState().vadOpen).toBe(false);
    await vi.advanceTimersByTimeAsync(TAIL_MS - 1);
    expect(mic.stop).not.toHaveBeenCalled();
    expect(api.closeVoiceActivitySession).not.toHaveBeenCalled();
    mic.onChunk?.(new ArrayBuffer(64));
    await vi.advanceTimersByTimeAsync(1);
    expect(mic.stop).toHaveBeenCalledTimes(1);
    expect(api.feedVoiceActivity).toHaveBeenCalledTimes(1);
    expect(api.closeVoiceActivitySession).toHaveBeenCalledWith("va1", false);
    expect(mic.stop.mock.invocationCallOrder[0]).toBeLessThan(
      api.feedVoiceActivity.mock.invocationCallOrder[0],
    );
    expect(api.feedVoiceActivity.mock.invocationCallOrder[0]).toBeLessThan(
      api.closeVoiceActivitySession.mock.invocationCallOrder[0],
    );
    expect(api.finishVoiceCapture).toHaveBeenCalledTimes(1);
    expect(bench.composer.post).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-XEIA: `speech_end` during the tail finishes the capture once", async () => {
    await confirmed();
    setInputMode("ptt");
    noteActivity(change("speech_end", { captureId: "c1" }));
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    expect(api.finishVoiceCapture).toHaveBeenCalledTimes(1);
    expect(bench.composer.post).toHaveBeenCalledTimes(1);
  });

  it("VOP-FR-FAAB: the close reply and a late `speech_end` finish the capture once", async () => {
    await confirmed();
    const finish = deferred<string>();
    api.finishVoiceCapture.mockReturnValueOnce(finish.promise);
    api.closeVoiceActivitySession.mockResolvedValueOnce({ captureId: "c1" });
    setInputMode("ptt");
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    noteActivity(change("speech_end", { captureId: "c1" }));
    finish.resolve("hello");
    await settle();
    expect(api.finishVoiceCapture).toHaveBeenCalledTimes(1);
    expect(bench.composer.post).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-XEIA, VOM-FR-WICV: a close during Transcribing keeps the capture for Retry", async () => {
    await confirmed();
    const finish = deferred<string>();
    api.finishVoiceCapture.mockReturnValueOnce(finish.promise);
    api.closeVoiceActivitySession.mockResolvedValueOnce({ captureId: "c1" });
    noteActivity(change("speech_end", { captureId: "c1" }));
    setInputMode("ptt");
    await settle();
    expect(mic.stop).toHaveBeenCalledTimes(1);
    expect(api.closeVoiceActivitySession).toHaveBeenCalledWith("va1", false);
    finish.reject("provider_unreachable");
    await settle();
    expect(getVoiceState().canRetry).toBe(true);
    await retryRecording();
    expect(api.retryVoiceCapture).toHaveBeenCalledWith("c1");
  });

  it("VOM-FR-BQBI: selecting Push-to-Talk closes an open session", async () => {
    await openSession();
    setInputMode("ptt");
    await settle();
    expect(getVoiceState()).toMatchObject({ inputMode: "ptt", vadOpen: false });
    expect(mic.stop).toHaveBeenCalledTimes(1);
    expect(api.closeVoiceActivitySession).toHaveBeenCalledWith("va1", true);
  });

  it("VOM-FR-XEIA, VOM-FR-ABPU: leaving voice mode keeps the speech as unsent text", async () => {
    await confirmed();
    api.closeVoiceActivitySession.mockResolvedValueOnce({ captureId: "c1" });
    leaveVoiceMode();
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    expect(bench.composer.post).not.toHaveBeenCalled();
    expect(getDiscussionSession(KEY).body).toBe("hello");
  });

  it("VOM-FR-BQBI: Push-to-Talk and leaving in the tail close once", async () => {
    await confirmed();
    api.closeVoiceActivitySession.mockResolvedValueOnce({ captureId: "c1" });
    setInputMode("ptt");
    leaveVoiceMode();
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    expect(api.closeVoiceActivitySession).toHaveBeenCalledTimes(1);
    expect(api.finishVoiceCapture).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-KQAI: a teardown discards at once", async () => {
    await confirmed();
    endVoiceForTeardown();
    expect(mic.stop).toHaveBeenCalledTimes(1);
    await settle();
    expect(api.closeVoiceActivitySession).toHaveBeenCalledWith("va1", true);
    noteActivity(change("speech_end", { captureId: "c1" }));
    await settle();
    expect(bench.composer.post).not.toHaveBeenCalled();
    expect(api.finishVoiceCapture).not.toHaveBeenCalled();
    expect(getDiscussionSession(KEY).body).toBe("");
    expect(getVoiceState().error).toBeNull();
  });

  it("VOM-FR-KQAI: a teardown during the tail calls no provider", async () => {
    await confirmed();
    setInputMode("ptt");
    endVoiceForTeardown();
    expect(mic.stop).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    expect(api.closeVoiceActivitySession).toHaveBeenCalledWith("va1", true);
    expect(api.finishVoiceCapture).not.toHaveBeenCalled();
    expect(bench.composer.post).not.toHaveBeenCalled();
  });

  it("VOM-FR-XEIA: a refused close keeps the speech and leaves nothing behind", async () => {
    await confirmed();
    api.closeVoiceActivitySession.mockRejectedValueOnce("broken");
    setInputMode("ptt");
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    expect(api.finishVoiceCapture).toHaveBeenCalledWith("c1");
    expect(bench.composer.post).toHaveBeenCalledTimes(1);
    expect(getVoiceState()).toMatchObject({ phase: "idle", transcript: "", vadOpen: false });
    setInputMode("vad");
    await settle();
    expect(getVoiceState().vadOpen).toBe(true);
  });

  it("VOM-FR-PDZR: a switch back to VAD during a slow close opens after it, and keeps its phase", async () => {
    await openSession();
    const close = deferred<{ captureId: string | null }>();
    api.closeVoiceActivitySession.mockReturnValueOnce(close.promise);
    setInputMode("ptt");
    await settle();
    setInputMode("vad");
    await settle();
    expect(mic.open).toHaveBeenCalledTimes(1);
    expect(getVoiceState().phase).toBe("opening");
    close.resolve({ captureId: null });
    await settle();
    expect(mic.open).toHaveBeenCalledTimes(2);
    expect(getVoiceState()).toMatchObject({ vadOpen: true, phase: "idle" });
  });

  it("VOM-FR-BQBI, VOM-FR-HHWL: a slow close does not take the phase of a recording", async () => {
    await openSession();
    const close = deferred<{ captureId: string | null }>();
    api.closeVoiceActivitySession.mockReturnValueOnce(close.promise);
    setInputMode("ptt");
    await settle();
    pressRecord();
    await settle();
    const { recordingSince } = getVoiceState();
    expect(getVoiceState().phase).toBe("recording");
    close.resolve({ captureId: null });
    await settle();
    expect(getVoiceState().phase).toBe("recording");
    expect(getVoiceState().recordingSince).toBe(recordingSince);
    releaseRecord();
  });

  it.each(["speech_start", "speech_confirmed", "misfire", "failed"] as const)(
    "VOM-FR-NZXN: %s after the close changes nothing",
    async (state) => {
      await openSession();
      const close = deferred<{ captureId: string | null }>();
      api.closeVoiceActivitySession.mockReturnValueOnce(close.promise);
      setInputMode("ptt");
      await settle();
      const during = getVoiceState();
      noteActivity(change(state, { captureId: "c9", code: "provider_rejected" }));
      expect(getVoiceState()).toEqual(during);
      close.resolve({ captureId: null });
      await settle();
      const after = getVoiceState();
      noteActivity(change(state, { captureId: "c9", code: "provider_rejected" }));
      expect(getVoiceState()).toEqual(after);
    },
  );
});

describe("utterances", () => {
  it("VOM-FR-NZXN: an event of another session changes nothing", async () => {
    await openSession();
    const before = getVoiceState();
    noteActivity(change("speech_start", { activityId: "va0" }));
    noteActivity(change("speech_end", { activityId: "va0", captureId: "c1" }));
    expect(getVoiceState()).toEqual(before);
    expect(api.cancelVoiceCapture).not.toHaveBeenCalled();
  });

  it.each([[true], [false], [null]])(
    "VOM-FR-WRVR, VOM-FR-SRGN, VOM-FR-HUXY: confirmed speech is a recording (live %s)",
    async (live) => {
      await openSession();
      noteActivity(change("speech_start"));
      expect(getVoiceState()).toMatchObject({ hearing: true, phase: "idle" });
      noteActivity(change("speech_confirmed", { captureId: "c1", live }));
      expect(getVoiceState()).toMatchObject({
        phase: "recording",
        live: live ?? false,
        recordingSince: Date.now(),
      });
      // VOM-FR-HUXY: a chunked utterance shows its live text in the live bubble.
      noteTranscript("c1", "words");
      expect(getLiveBubble().parts[0]).toMatchObject({ captureId: "c1", liveText: "words" });
    },
  );

  it("VOM-FR-OTRF, VOM-FR-GBCN, VOM-FR-VOUG: the detector does not wait for the transcript", async () => {
    await confirmed();
    const finish = deferred<string>();
    api.finishVoiceCapture.mockReturnValueOnce(finish.promise);
    noteActivity(change("speech_end", { captureId: "c1" }));
    await settle();
    expect(getVoiceState()).toMatchObject({
      phase: "idle",
      hearing: false,
      vadOpen: true,
      transcribing: 1,
    });
    expect(lastGate()).toMatchObject({ blocked: false, chunked: true });
    expect(lastGate()).not.toHaveProperty("settled");
    finish.resolve("hello");
    await settle();
    expect(getVoiceState().transcribing).toBe(0);
    expect(bench.composer.post).toHaveBeenCalledWith("hello");
  });

  it("VOM-FR-QCZQ, VOM-FR-GBCN: a second utterance records while the first transcribes, and posts after it", async () => {
    await confirmed();
    const first = deferred<string>();
    api.finishVoiceCapture.mockReturnValueOnce(first.promise);
    noteActivity(change("speech_end", { captureId: "c1" }));
    noteActivity(change("speech_start"));
    noteActivity(change("speech_confirmed", { captureId: "c2", live: true }));
    expect(getVoiceState().phase).toBe("recording");
    api.finishVoiceCapture.mockResolvedValueOnce("second");
    noteActivity(change("speech_end", { captureId: "c2" }));
    await settle();
    expect(bench.composer.post).not.toHaveBeenCalled();
    first.resolve("first");
    await settle();
    expect(vi.mocked(bench.composer.post).mock.calls.map((c) => c[0])).toEqual(["first", "second"]);
  });

  it("VOM-FR-OTRF: one session takes many utterances", async () => {
    await confirmed();
    noteActivity(change("speech_end", { captureId: "c1" }));
    await settle();
    noteActivity(change("speech_start"));
    noteActivity(change("speech_confirmed", { captureId: "c2", live: true }));
    noteActivity(change("speech_end", { captureId: "c2" }));
    await settle();
    expect(bench.composer.post).toHaveBeenCalledTimes(2);
    expect(lastGate()).toMatchObject({ blocked: false });
    expect(api.openVoiceActivitySession).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-WSXW: the backend ends a long utterance, and the session stays", async () => {
    await confirmed();
    await vi.advanceTimersByTimeAsync(RECORDING_LIMIT_MS);
    expect(api.finishVoiceCapture).not.toHaveBeenCalled();
    noteActivity(change("speech_end", { captureId: "c1" }));
    await settle();
    expect(getVoiceState().vadOpen).toBe(true);
  });

  it("VOM-FR-XQUY: a release, as from window blur, has no effect on a session", async () => {
    await confirmed();
    releaseRecord();
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    expect(getVoiceState()).toMatchObject({ vadOpen: true, phase: "recording" });
    expect(api.finishVoiceCapture).not.toHaveBeenCalled();
  });

  it("VOM-FR-XWPZ: a misfire shows no note and touches no reply", async () => {
    await openSession();
    noteActivity(change("speech_start"));
    noteActivity(change("misfire"));
    expect(getVoiceState().note).toBeNull();
    expect(audio.resumePlayback).not.toHaveBeenCalled();
    expect(api.cancelVoiceSpeech).not.toHaveBeenCalled();
  });

  it("VOM-FR-OTRF: a capture that nobody here knows is cancelled", async () => {
    await openSession();
    noteActivity(change("speech_end", { captureId: "c9" }));
    expect(api.cancelVoiceCapture).toHaveBeenCalledWith("c9");
    expect(bench.composer.post).not.toHaveBeenCalled();
  });

  it("VOM-FR-KVHG: a capture that nobody here knows still resumes a paused reply", async () => {
    await openSession();
    await replyPaused();
    noteActivity(change("speech_end", { captureId: "c9" }));
    expectSettledWith(true);
  });

  it("VOM-FR-ABPU: a throw in the post keeps the text, and the detector listens on", async () => {
    await confirmed(handle({ post: vi.fn(async () => Promise.reject(new Error("broken"))) }));
    noteActivity(change("speech_end", { captureId: "c1" }));
    await settle();
    expect(getDiscussionSession(KEY).body).toBe("hello");
    expect(getVoiceState()).toMatchObject({ phase: "idle", note: "kept_in_body", transcript: "" });
    expect(lastGate()).toMatchObject({ blocked: false });
  });
});

describe("the gate and the surfaces", () => {
  it("VOM-FR-GTAQ, VOM-FR-ZUWW: a block starts nothing, and the end of it needs no press", async () => {
    let blocked = false;
    await openSession(handle({ blocked: () => blocked }));
    blocked = true;
    noteComposerChanged();
    expect(lastGate()).toMatchObject({ blocked: true });
    expect(getVoiceState().vadOpen).toBe(true);
    const calls = api.setVoiceActivityGate.mock.calls.length;
    noteComposerChanged();
    expect(api.setVoiceActivityGate.mock.calls.length).toBe(calls);
    blocked = false;
    noteComposerChanged();
    expect(lastGate()).toMatchObject({ blocked: false });
    expect(mic.open).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-LNUH: a new roster reaches the gate", async () => {
    let names = ["arch"];
    await openSession(handle({ nicknames: () => names }));
    names = ["helga"];
    noteComposerChanged();
    expect(lastGate()).toMatchObject({ keyterms: ["helga"] });
  });

  it("VOM-FR-BCRD: with no surface, speech goes on and the session stays open", async () => {
    await openSession();
    await replyPlays();
    bench.unregister();
    expect(getVoiceState().playing).toBe(true);
    expect(lastGate()).toMatchObject({ blocked: false });
    await vi.advanceTimersByTimeAsync(60_000);
    expect(getVoiceState().vadOpen).toBe(true);
  });

  it("VOM-FR-BCRD: a remount and an unmount do not close the session", async () => {
    await openSession();
    bench.unregister();
    const back = registerVoiceComposer(KEY, handle());
    back();
    await vi.advanceTimersByTimeAsync(60_000);
    expect(getVoiceState().vadOpen).toBe(true);
    expect(api.closeVoiceActivitySession).not.toHaveBeenCalled();
  });

  it("VOM-FR-VGLT, VOM-FR-GTAQ: a standby handle blocks the session, and the composer frees it", async () => {
    await openSession();
    bench.unregister();
    const standby = registerVoiceComposer(KEY, handle({ blocked: () => true, nicknames: () => [] }));
    expect(getVoiceState().vadOpen).toBe(true);
    expect(lastGate()).toMatchObject({ blocked: true, keyterms: [] });
    standby();
    const back = registerVoiceComposer(KEY, handle());
    expect(getVoiceState().vadOpen).toBe(true);
    expect(lastGate()).toMatchObject({ blocked: false, keyterms: ["arch"] });
    back();
  });

  it("VOM-FR-BCRD, VOM-FR-WYIE: an utterance with no surface posts through the retained handle", async () => {
    await confirmed();
    bench.unregister();
    noteActivity(change("speech_end", { captureId: "c1" }));
    await settle();
    expect(bench.composer.post).toHaveBeenCalledWith("hello");
    expect(getDiscussionSession(KEY).body).toBe("");
  });

  it.each([
    ["leaving", () => leaveVoiceMode()],
    ["a teardown", () => endVoiceForTeardown()],
  ])("VOM-FR-LSBQ: %s with no surface closes once", async (_, act) => {
    await openSession();
    bench.unregister();
    act();
    await vi.advanceTimersByTimeAsync(TAIL_MS * 4);
    expect(api.closeVoiceActivitySession).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-PDZR: a new entry in Push-to-Talk opens no session", async () => {
    await openSession();
    setInputMode("ptt");
    leaveVoiceMode();
    await settle();
    enterVoiceMode(KEY, discussion([]));
    await settle();
    expect(getVoiceState().vadOpen).toBe(false);
    expect(api.openVoiceActivitySession).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-VGCE: VAD waits for an idle phase, and Push-to-Talk does not", () => {
    enterVoiceMode(KEY, discussion([]));
    patchVoiceState({ phase: "recording" });
    setInputMode("vad");
    expect(getVoiceState().inputMode).toBe("ptt");
    patchVoiceState({ phase: "idle" });
    setInputMode("vad");
    expect(getVoiceState().inputMode).toBe("vad");
    patchVoiceState({ phase: "transcribing" });
    setInputMode("ptt");
    expect(getVoiceState().inputMode).toBe("ptt");
  });

  it("VOM-FR-WMAP, VOM-FR-BQBI: a provider that goes closes the session", async () => {
    await confirmed();
    api.closeVoiceActivitySession.mockResolvedValueOnce({ captureId: "c1" });
    noteProvider(false, false);
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    expect(getVoiceState().vadOpen).toBe(false);
    expect(api.finishVoiceCapture).toHaveBeenCalledTimes(1);
  });
});
