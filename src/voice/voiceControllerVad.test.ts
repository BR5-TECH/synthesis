/**
 * VAD sessions in the controller (`VOM-voice-mode.md`): opening, closing,
 * utterances, the gate, and the surfaces.
 * `VadSession`, the batcher, and the pump stay real, so a failed feed runs the
 * true path.
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

import { getDiscussionSession } from "../state/discussionSession";
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
  SURFACE_GRACE_MS,
  TAIL_MS,
  endVoiceForTeardown,
  enterVoiceMode,
  leaveVoiceMode,
  noteActivity,
  noteComposerChanged,
  noteProvider,
  noteTranscript,
  pressRecord,
  registerVoiceComposer,
  releaseRecord,
  retryRecording,
  setInputMode,
} from "./voiceController";

useVadBench();

describe("opening a session", () => {
  it("VOM-FR-PDZR, VOM-FR-MMAG, VOM-FR-SIEO, VOM-FR-LNUH: a press opens one session and calls no provider", async () => {
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

  it("VOM-FR-PDZR: selecting VAD opens nothing", async () => {
    ready();
    await settle();
    expect(mic.open).not.toHaveBeenCalled();
    expect(getVoiceState().inputMode).toBe("vad");
  });

  it("VOM-FR-VVTY: opening stops playback", async () => {
    ready();
    await replyPlays();
    audio.stopPlayback.mockClear();
    pressRecord();
    expect(audio.stopPlayback).toHaveBeenCalledTimes(1);
    expect(api.cancelVoiceSpeech).toHaveBeenCalledTimes(1);
    expect(getVoiceState().playing).toBe(false);
    await settle();
  });

  it("VOM-FR-FASJ: opening drops a held recording", async () => {
    await confirmed();
    api.finishVoiceCapture.mockRejectedValueOnce("provider_unreachable");
    noteActivity(change("speech_end", { captureId: "c1" }));
    await settle();
    pressRecord();
    await settle();
    expect(getVoiceState()).toMatchObject({ vadOpen: false, canRetry: true });
    expect(api.cancelVoiceCapture).not.toHaveBeenCalled();
    pressRecord();
    await settle();
    expect(api.cancelVoiceCapture).toHaveBeenCalledWith("c1");
    expect(getVoiceState()).toMatchObject({ vadOpen: true, canRetry: false, error: null });
  });

  it.each(["microphone_denied", "microphone_missing"])(
    "VOM-FR-CUCN, VOM-FR-ZFSA: %s opens no session",
    async (code) => {
      ready();
      mic.open.mockRejectedValueOnce(new MicrophoneError(code));
      pressRecord();
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

  it("VOM-FR-CUCN: a refused open releases the microphone, and the next press works", async () => {
    ready();
    api.openVoiceActivitySession.mockRejectedValueOnce("voice_detector_unavailable");
    pressRecord();
    await settle();
    expect(mic.stop).toHaveBeenCalledTimes(1);
    expect(getVoiceState()).toMatchObject({
      error: "voice_detector_unavailable",
      vadOpen: false,
      phase: "idle",
    });
    pressRecord();
    await settle();
    expect(getVoiceState().vadOpen).toBe(true);
    expect(getVoiceState().error).toBeNull();
  });

  it("VOM-FR-ZUWW: a blocked composer opens nothing", async () => {
    ready(handle({ blocked: () => true }));
    pressRecord();
    await settle();
    expect(mic.open).not.toHaveBeenCalled();
  });

  it.each([
    ["a press", () => pressRecord()],
    ["Push-to-Talk", () => setInputMode("ptt")],
    ["a teardown", () => endVoiceForTeardown()],
    ["leaving", () => leaveVoiceMode()],
    ["a lost provider", () => noteProvider(false, false)],
  ])(
    "VOM-FR-PDZR, VOM-FR-BQBI, VOM-FR-KQAI: %s during the open leaves no session",
    async (_, act) => {
      ready();
      const microphone = deferred<{ rate: number; stop: () => void }>();
      mic.open.mockReturnValueOnce(microphone.promise);
      pressRecord();
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

  it("VOM-FR-CSXI: a track that ends during the open closes the session", async () => {
    ready();
    const open = deferred<{ activityId: string }>();
    api.openVoiceActivitySession.mockReturnValueOnce(open.promise);
    pressRecord();
    await settle();
    mic.onLost?.();
    open.resolve({ activityId: "va1" });
    await settle();
    expect(getVoiceState()).toMatchObject({ error: "microphone_unavailable", vadOpen: false });
    expect(mic.stop).toHaveBeenCalledTimes(1);
    expect(api.closeVoiceActivitySession).toHaveBeenCalledTimes(1);
  });
});

describe("closing a session", () => {
  it("VOM-FR-XEIA, VOM-FR-FTFU: the tail, the microphone, the last audio, then the close", async () => {
    await confirmed();
    api.closeVoiceActivitySession.mockResolvedValueOnce({ captureId: "c1" });
    pressRecord();
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
    pressRecord();
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
    pressRecord();
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
    pressRecord();
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
    pressRecord();
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
    pressRecord();
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
    pressRecord();
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    expect(api.finishVoiceCapture).toHaveBeenCalledWith("c1");
    expect(bench.composer.post).toHaveBeenCalledTimes(1);
    expect(getVoiceState()).toMatchObject({ phase: "idle", transcript: "", vadOpen: false });
    pressRecord();
    await settle();
    expect(getVoiceState().vadOpen).toBe(true);
  });

  it("VOM-FR-PDZR: a press during a slow close opens after it, and keeps its phase", async () => {
    await openSession();
    const close = deferred<{ captureId: string | null }>();
    api.closeVoiceActivitySession.mockReturnValueOnce(close.promise);
    pressRecord();
    await settle();
    pressRecord();
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
      pressRecord();
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
      noteTranscript("c1", "words");
      expect(getVoiceState().transcript).toBe("words");
    },
  );

  it("VOM-FR-OTRF, VOM-FR-KHGC: the detector waits until the utterance settled", async () => {
    await confirmed();
    const finish = deferred<string>();
    api.finishVoiceCapture.mockReturnValueOnce(finish.promise);
    noteActivity(change("speech_end", { captureId: "c1" }));
    expect(getVoiceState()).toMatchObject({ phase: "transcribing", hearing: false, vadOpen: true });
    expect(lastGate()).toMatchObject({ blocked: true, settled: null });
    finish.resolve("hello");
    await settle();
    expect(lastGate()).toMatchObject({ blocked: false, settled: "c1" });
    expect(bench.composer.post).toHaveBeenCalledWith("hello");
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
    expect(lastGate()).toMatchObject({ settled: "c2" });
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

  it("VOM-FR-OTRF: a capture that nobody here knows does not hold the detector", async () => {
    await openSession();
    noteActivity(change("speech_end", { captureId: "c9" }));
    expect(api.cancelVoiceCapture).toHaveBeenCalledWith("c9");
    expect(lastGate()).toMatchObject({ settled: "c9" });
  });

  it("VOM-FR-KVHG: a capture that nobody here knows still resumes a paused reply", async () => {
    await openSession();
    await replyPaused();
    noteActivity(change("speech_end", { captureId: "c9" }));
    expectSettledWith(true);
  });

  it("VOM-FR-ABPU, VOM-FR-KHGC: a throw in the post keeps the text and frees the detector", async () => {
    await confirmed(handle({ post: vi.fn(async () => Promise.reject(new Error("broken"))) }));
    noteActivity(change("speech_end", { captureId: "c1" }));
    await settle();
    expect(getDiscussionSession(KEY).body).toBe("hello");
    expect(getVoiceState()).toMatchObject({ phase: "idle", note: "kept_in_body", transcript: "" });
    expect(lastGate()).toMatchObject({ settled: "c1", blocked: false });
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

  it("VOM-FR-GZPA, VOM-FR-EGNX: with no surface, speech stops and the session closes at 2 s", async () => {
    expect(SURFACE_GRACE_MS).toBe(2000);
    await openSession();
    await replyPlays();
    bench.unregister();
    expect(getVoiceState().playing).toBe(false);
    expect(lastGate()).toMatchObject({ blocked: true });
    await vi.advanceTimersByTimeAsync(SURFACE_GRACE_MS - 1);
    expect(getVoiceState().vadOpen).toBe(true);
    await vi.advanceTimersByTimeAsync(1);
    expect(getVoiceState().vadOpen).toBe(false);
  });

  it("VOM-FR-GZPA: a composer that comes back keeps the session", async () => {
    await openSession();
    bench.unregister();
    await vi.advanceTimersByTimeAsync(1000);
    const back = registerVoiceComposer(KEY, handle());
    expect(lastGate()).toMatchObject({ blocked: false });
    await vi.advanceTimersByTimeAsync(SURFACE_GRACE_MS * 2);
    expect(getVoiceState().vadOpen).toBe(true);
    back();
    await vi.advanceTimersByTimeAsync(SURFACE_GRACE_MS);
    expect(getVoiceState().vadOpen).toBe(false);
  });

  it("VOM-FR-GZPA, VOM-FR-VGLT, VOM-FR-GTAQ: a standby handle keeps the session, blocked", async () => {
    await openSession();
    bench.unregister();
    const standby = registerVoiceComposer(KEY, handle({ blocked: () => true, nicknames: () => [] }));
    await vi.advanceTimersByTimeAsync(SURFACE_GRACE_MS * 2);
    expect(getVoiceState().vadOpen).toBe(true);
    expect(lastGate()).toMatchObject({ blocked: true, keyterms: [] });
    standby();
    const back = registerVoiceComposer(KEY, handle());
    await vi.advanceTimersByTimeAsync(SURFACE_GRACE_MS * 2);
    expect(getVoiceState().vadOpen).toBe(true);
    expect(lastGate()).toMatchObject({ blocked: false, keyterms: ["arch"] });
    back();
  });

  it("VOM-FR-GZPA, VOM-FR-ABPU: an utterance with no surface keeps its text in the body", async () => {
    await confirmed();
    api.closeVoiceActivitySession.mockResolvedValueOnce({ captureId: "c1" });
    bench.unregister();
    await vi.advanceTimersByTimeAsync(SURFACE_GRACE_MS + TAIL_MS);
    expect(bench.composer.post).not.toHaveBeenCalled();
    expect(getDiscussionSession(KEY).body).toBe("hello");
    expect(getVoiceState().note).toBe("kept_in_body");
  });

  it.each([
    ["leaving", () => leaveVoiceMode()],
    ["a teardown", () => endVoiceForTeardown()],
  ])("VOM-FR-GZPA: %s during the grace closes once", async (_, act) => {
    await openSession();
    bench.unregister();
    act();
    await vi.advanceTimersByTimeAsync(SURFACE_GRACE_MS * 2);
    expect(api.closeVoiceActivitySession).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-MQHD: voice mode starts in Push-to-Talk each time", async () => {
    await openSession();
    leaveVoiceMode();
    await settle();
    enterVoiceMode(KEY, discussion([]));
    expect(getVoiceState()).toMatchObject({ inputMode: "ptt", vadOpen: false });
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
