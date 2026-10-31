/**
 * VAD sessions in the controller (`VOM-voice-mode.md`): paused replies, a lost
 * feed, Retry, and the first post of an opening composer.
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
import { getVoiceState } from "../state/voiceSession";
import {
  KEY,
  bench,
  change,
  comment,
  confirmed,
  deferred,
  discussion,
  expectSettledWith,
  handle,
  lastGate,
  openSession,
  replyPaused,
  replyPlays,
  settle,
  useVadBench,
} from "../test/voiceVadKit";
import {
  SURFACE_GRACE_MS,
  TAIL_MS,
  bindVoiceToDiscussion,
  endVoiceForTeardown,
  enterVoiceMode,
  leaveVoiceMode,
  noteActivity,
  noteDiscussionChanged,
  noteProvider,
  noteTranscript,
  pressRecord,
  registerVoiceComposer,
  releaseRecord,
  retryRecording,
  setResponseMode,
  stopSpeech,
  type PostOutcome,
} from "./voiceController";

useVadBench();

describe("a paused reply on each exit", () => {
  async function paused(with_?: ReturnType<typeof handle>) {
    await openSession(with_);
    await replyPaused();
    noteActivity(change("speech_confirmed", { captureId: "c1", live: true }));
  }

  async function end() {
    noteActivity(change("speech_end", { captureId: "c1" }));
    await settle();
  }

  it("VOM-FR-UNXR: a post drops it", async () => {
    await paused();
    await end();
    expectSettledWith(false);
    expect(api.cancelVoiceSpeech).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-UNXR, VOM-FR-BVNX: a comment that reached no agent posted, so it drops it", async () => {
    await paused(handle({ post: vi.fn(async (): Promise<PostOutcome> => "reached_nobody") }));
    await end();
    expectSettledWith(false);
    expect(getVoiceState().note).toBe("no_agent");
  });

  it.each(["busy", "failed"] as const)(
    "VOM-FR-UNXR, VOM-FR-ABPU: a %s post resumes it and keeps the text",
    async (outcome) => {
      await paused(handle({ post: vi.fn(async (): Promise<PostOutcome> => outcome) }));
      await end();
      expectSettledWith(true);
      expect(getDiscussionSession(KEY).body).toBe("hello");
    },
  );

  it("VOM-FR-UNXR, VOM-FR-RENV: an empty transcript resumes it", async () => {
    await paused();
    api.finishVoiceCapture.mockResolvedValueOnce("  ");
    await end();
    expectSettledWith(true);
    expect(getVoiceState().note).toBe("no_speech");
  });

  it("VOM-FR-UNXR, VOM-FR-ZUWW: a composer blocked at the end resumes it", async () => {
    let blocked = false;
    await paused(handle({ blocked: () => blocked }));
    blocked = true;
    await end();
    expectSettledWith(true);
    expect(getVoiceState().note).toBe("kept_in_body");
  });

  it("VOM-FR-UNXR, VOM-FR-WWZX: a retryable failure resumes it and offers Retry", async () => {
    await paused();
    api.finishVoiceCapture.mockRejectedValueOnce("provider_unreachable");
    await end();
    expectSettledWith(true);
    expect(getVoiceState()).toMatchObject({ error: "provider_unreachable", canRetry: true });
    expect(getVoiceState().vadOpen).toBe(true);
  });

  it("VOM-FR-UNXR, VOM-FR-WWZX: a failure with no held audio offers no Retry", async () => {
    await paused();
    api.finishVoiceCapture.mockRejectedValueOnce("voice_session_active");
    await end();
    expectSettledWith(true);
    expect(getVoiceState().canRetry).toBe(false);
  });

  it("VOM-FR-KVHG, VOM-FR-XWPZ: a misfire resumes it and keeps a held recording", async () => {
    await confirmed();
    api.finishVoiceCapture.mockRejectedValueOnce("provider_unreachable");
    noteActivity(change("speech_end", { captureId: "c1" }));
    await settle();
    await replyPaused();
    noteActivity(change("misfire"));
    expectSettledWith(true);
    expect(getVoiceState().canRetry).toBe(true);
    expect(api.cancelVoiceCapture).not.toHaveBeenCalled();
  });

  it("VOM-FR-WWZX: a failure with no utterance resumes it and the session stays", async () => {
    await openSession();
    await replyPaused();
    noteActivity(change("failed", { code: "no_active_provider", fatal: false }));
    expectSettledWith(true);
    expect(getVoiceState()).toMatchObject({ error: "no_active_provider", vadOpen: true });
  });

  it("VOM-FR-WWZX: a lost capture ends the utterance with no finish and no Retry", async () => {
    await paused();
    noteActivity(change("failed", { code: "voice_session_lost", fatal: false }));
    expectSettledWith(true);
    expect(getVoiceState()).toMatchObject({ canRetry: false, transcript: "", vadOpen: true });
    expect(api.finishVoiceCapture).not.toHaveBeenCalled();
  });

  it("VOM-FR-CSXI: a fatal failure closes the session and resumes it", async () => {
    await openSession();
    await replyPaused();
    noteActivity(change("failed", { code: "voice_detector_failed", fatal: true }));
    await settle();
    expectSettledWith(true);
    expect(getVoiceState()).toMatchObject({ error: "voice_detector_failed", vadOpen: false });
  });

  it("VOM-FR-CSXI: a fatal failure in an utterance has no capture to finish", async () => {
    await paused();
    noteActivity(change("failed", { code: "voice_no_audio", fatal: true }));
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    expectSettledWith(true);
    expect(api.finishVoiceCapture).not.toHaveBeenCalled();
  });

  it("VOM-FR-KVHG: a close during unconfirmed speech resumes it", async () => {
    await openSession();
    await replyPaused();
    pressRecord();
    await settle();
    expect(api.closeVoiceActivitySession).toHaveBeenCalledWith("va1", true);
    expectSettledWith(true);
  });

  it("VOM-FR-UNXR: a close in an utterance that posts drops it", async () => {
    await paused();
    api.closeVoiceActivitySession.mockResolvedValueOnce({ captureId: "c1" });
    pressRecord();
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    expectSettledWith(false);
  });

  it.each([
    ["leaving", () => leaveVoiceMode()],
    ["an unmount", () => bench.unregister()],
    ["a teardown", () => endVoiceForTeardown()],
  ])("VOM-FR-EGNX, VOM-FR-KQAI: %s drops it", async (_, act) => {
    await paused();
    audio.stopPlayback.mockClear();
    act();
    await vi.advanceTimersByTimeAsync(SURFACE_GRACE_MS + TAIL_MS);
    expect(getVoiceState().playbackPaused).toBe(false);
    expect(audio.resumePlayback).not.toHaveBeenCalled();
    expect(audio.stopPlayback).toHaveBeenCalled();
  });
});

describe("barge-in", () => {
  it("VOM-FR-FVRS: speech pauses a reply once, and nothing stops or cancels it", async () => {
    await openSession();
    await replyPlays();
    audio.stopPlayback.mockClear();
    noteActivity(change("speech_start"));
    noteActivity(change("speech_start"));
    expect(audio.pausePlayback).toHaveBeenCalledTimes(1);
    expect(getVoiceState()).toMatchObject({ playing: true, playbackPaused: true });
    expect(audio.stopPlayback).not.toHaveBeenCalled();
    expect(api.cancelVoiceSpeech).not.toHaveBeenCalled();
    expect(lastGate()).toMatchObject({ playing: false });
  });

  it("VOM-FR-FVRS: speech with no reply pauses nothing", async () => {
    await openSession();
    noteActivity(change("speech_start"));
    expect(audio.pausePlayback).not.toHaveBeenCalled();
  });

  it("VOM-FR-ZPOD, VOM-FR-RKOG: only the newest reply waits, and it replaces a paused one", async () => {
    await openSession();
    await replyPaused();
    noteDiscussionChanged(discussion([comment("r1"), comment("r2", "agent", "Second.")]));
    noteDiscussionChanged(
      discussion([comment("r1"), comment("r2", "agent", "Second."), comment("r3", "agent", "Third.")]),
    );
    await settle();
    expect(api.speakVoiceSegment).toHaveBeenCalledTimes(1);
    noteActivity(change("speech_confirmed", { captureId: "c1", live: true }));
    noteActivity(change("speech_end", { captureId: "c1" }));
    await settle();
    expect(api.speakVoiceSegment).toHaveBeenCalledTimes(2);
    expect(api.speakVoiceSegment.mock.calls[1]).toEqual(
      expect.arrayContaining(["Third."]),
    );
    expect(audio.resumePlayback).not.toHaveBeenCalled();
  });

  it("VOM-FR-ZPOD, VOM-FR-VSCQ: a comment that is not spoken waits for nothing", async () => {
    await openSession();
    await replyPaused();
    noteDiscussionChanged(discussion([comment("r1"), comment("h1", "human")]));
    noteActivity(change("misfire"));
    expectSettledWith(true);
    expect(api.speakVoiceSegment).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-ZPOD, VOM-FR-ILJW: in response mode Text, no reply waits", async () => {
    await openSession();
    noteActivity(change("speech_start"));
    noteDiscussionChanged(discussion([comment("r1")]));
    noteActivity(change("misfire"));
    setResponseMode("voice");
    await settle();
    expect(api.speakVoiceSegment).not.toHaveBeenCalled();
  });

  it("VOM-FR-ZPOD, VOM-FR-TRWN: a waiting reply with no speech still replaces the paused one", async () => {
    await openSession();
    await replyPaused();
    noteDiscussionChanged(discussion([comment("r1"), comment("r2", "agent", "")]));
    noteActivity(change("misfire"));
    await settle();
    expect(getVoiceState()).toMatchObject({ playing: false, playbackPaused: false });
    expect(api.speakVoiceSegment).toHaveBeenCalledTimes(1);
    expect(audio.resumePlayback).not.toHaveBeenCalled();
  });

  it.each([
    ["Stop", () => stopSpeech()],
    ["Text", () => setResponseMode("text")],
    ["a provider with no speech", () => noteProvider(true, false)],
  ])("VOM-FR-QDWW, VOM-FR-ZPOD: %s drops the paused and the waiting reply", async (_, act) => {
    await openSession();
    await replyPaused();
    noteDiscussionChanged(discussion([comment("r1"), comment("r2")]));
    act();
    expect(getVoiceState()).toMatchObject({ playing: false, playbackPaused: false });
    expect(api.cancelVoiceSpeech).toHaveBeenCalledTimes(1);
    noteActivity(change("misfire"));
    await settle();
    expect(api.speakVoiceSegment).toHaveBeenCalledTimes(1);
    expect(audio.resumePlayback).not.toHaveBeenCalled();
  });

  it("VOM-FR-QDWW: a reply stopped while paused needs nothing more at the settle", async () => {
    await openSession();
    await replyPaused();
    noteActivity(change("speech_confirmed", { captureId: "c1", live: true }));
    stopSpeech();
    noteActivity(change("speech_end", { captureId: "c1" }));
    await settle();
    expect(api.cancelVoiceSpeech).toHaveBeenCalledTimes(1);
    expect(audio.resumePlayback).not.toHaveBeenCalled();
  });

  it("VOM-FR-FVRS: a reply that ended during the pause is no longer paused", async () => {
    await openSession();
    audio.isPlaybackIdle.mockReturnValue(true);
    setResponseMode("voice");
    const segment = deferred<undefined>();
    api.speakVoiceSegment.mockReturnValueOnce(segment.promise);
    noteDiscussionChanged(discussion([comment("r1")]));
    await settle();
    noteActivity(change("speech_start"));
    expect(getVoiceState().playbackPaused).toBe(true);
    segment.resolve(undefined);
    await settle();
    expect(getVoiceState()).toMatchObject({ playing: false, playbackPaused: false });
    noteActivity(change("misfire"));
    expect(audio.resumePlayback).not.toHaveBeenCalled();
  });

  it("VOM-FR-QPFQ: a segment that fails during the pause leaves nothing paused", async () => {
    await openSession();
    setResponseMode("voice");
    const segment = deferred<undefined>();
    api.speakVoiceSegment.mockReturnValueOnce(segment.promise);
    noteDiscussionChanged(discussion([comment("r1")]));
    await settle();
    noteActivity(change("speech_start"));
    segment.reject("speech_failed");
    await settle();
    expect(getVoiceState()).toMatchObject({
      error: "speech_failed",
      playing: false,
      playbackPaused: false,
    });
    noteActivity(change("misfire"));
    expect(audio.resumePlayback).not.toHaveBeenCalled();
  });
});

describe("a lost feed", () => {
  it("VOM-FR-CSXI: a lost track ends the session, and the utterance still posts", async () => {
    await confirmed();
    api.closeVoiceActivitySession.mockResolvedValueOnce({ captureId: "c1" });
    mic.onLost?.();
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    expect(getVoiceState()).toMatchObject({ error: "microphone_unavailable", vadOpen: false });
    expect(bench.composer.post).toHaveBeenCalledTimes(1);
  });

  it.each([
    ["voice_session_lost", "voice_session_lost"],
    [new Error("with text"), "unknown"],
  ])("VOM-FR-CSXI: a refused feed closes the session (%s)", async (refusal, code) => {
    await openSession();
    api.feedVoiceActivity.mockRejectedValueOnce(refusal);
    mic.onChunk?.(new ArrayBuffer(4096));
    await settle();
    expect(getVoiceState()).toMatchObject({ error: code, canRetry: false, vadOpen: false });
    expect(api.closeVoiceActivitySession).toHaveBeenCalledTimes(1);
    pressRecord();
    await settle();
    expect(getVoiceState()).toMatchObject({ error: null, vadOpen: true });
  });

  it("VOM-FR-CSXI: a second refused feed in the tail closes nothing more", async () => {
    await confirmed();
    api.feedVoiceActivity.mockRejectedValueOnce("voice_session_lost");
    api.feedVoiceActivity.mockRejectedValueOnce("voice_session_lost");
    mic.onChunk?.(new ArrayBuffer(4096));
    await settle();
    mic.onChunk?.(new ArrayBuffer(4096));
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    expect(api.closeVoiceActivitySession).toHaveBeenCalledTimes(1);
  });
});

describe("Retry", () => {
  async function held() {
    await confirmed();
    api.finishVoiceCapture.mockRejectedValueOnce("provider_unreachable");
    noteActivity(change("speech_end", { captureId: "c1" }));
    await settle();
    expect(getVoiceState().canRetry).toBe(true);
  }

  it("VOM-FR-WICV, VOM-FR-XWPZ: Retry waits for a misfire, and the misfire keeps the recording", async () => {
    await held();
    noteActivity(change("speech_start"));
    await retryRecording();
    expect(api.retryVoiceCapture).not.toHaveBeenCalled();
    noteActivity(change("misfire"));
    await retryRecording();
    expect(api.retryVoiceCapture).toHaveBeenCalledWith("c1");
  });

  it("VOM-FR-FASJ, VOM-FR-WRVR: the next utterance discards the held recording", async () => {
    await held();
    noteActivity(change("speech_start"));
    noteActivity(change("speech_confirmed", { captureId: "c2", live: true }));
    expect(api.cancelVoiceCapture).toHaveBeenCalledWith("c1");
    expect(getVoiceState()).toMatchObject({ error: null, canRetry: false });
    await retryRecording();
    expect(api.retryVoiceCapture).not.toHaveBeenCalled();
  });

  it("VOM-FR-XWPZ: a misfire does not take the phase of a Retry", async () => {
    await held();
    const retry = deferred<string>();
    api.retryVoiceCapture.mockReturnValueOnce(retry.promise);
    void retryRecording();
    noteActivity(change("speech_start"));
    noteActivity(change("misfire"));
    expect(getVoiceState().phase).toBe("transcribing");
    retry.resolve("again");
    await settle();
  });

  it("VOM-FR-WICV, VOM-FR-HUXY: a Retry that loses its audio to an utterance does not end it", async () => {
    await held();
    const retry = deferred<string>();
    api.retryVoiceCapture.mockReturnValueOnce(retry.promise);
    void retryRecording();
    noteActivity(change("speech_start"));
    noteActivity(change("speech_confirmed", { captureId: "c2", live: true }));
    retry.reject("voice_session_lost");
    await settle();
    expect(getVoiceState()).toMatchObject({ phase: "recording", canRetry: false, error: null });
    noteTranscript("c2", "words");
    expect(getVoiceState().transcript).toBe("words");
    noteActivity(change("speech_end", { captureId: "c2" }));
    await settle();
    expect(bench.composer.post).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-WICV: a failure after a close still offers Retry", async () => {
    await confirmed();
    api.closeVoiceActivitySession.mockResolvedValueOnce({ captureId: "c1" });
    api.finishVoiceCapture.mockRejectedValueOnce("provider_unreachable");
    pressRecord();
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    expect(getVoiceState()).toMatchObject({ vadOpen: false, canRetry: true });
    await retryRecording();
    expect(bench.composer.post).toHaveBeenCalledWith("again");
  });
});

describe("the first post of an opening composer", () => {
  function moved() {
    const next = handle();
    bindVoiceToDiscussion(KEY, discussion([comment("h1", "human")], "d2"));
    registerVoiceComposer("d2", next);
    return next;
  }

  it("VOM-FR-ILJW: an utterance follows the composer to the new discussion", async () => {
    await confirmed();
    const next = moved();
    noteActivity(change("speech_end", { captureId: "c1" }));
    await settle();
    expect(next.post).toHaveBeenCalledTimes(1);
    expect(bench.composer.post).not.toHaveBeenCalled();
  });

  it("VOM-FR-ILJW: a transcript on its way follows too", async () => {
    await confirmed();
    const finish = deferred<string>();
    api.finishVoiceCapture.mockReturnValueOnce(finish.promise);
    noteActivity(change("speech_end", { captureId: "c1" }));
    const next = moved();
    finish.resolve("hello");
    await settle();
    expect(next.post).toHaveBeenCalledTimes(1);
    expect(bench.composer.post).not.toHaveBeenCalled();
  });

  it("VOM-FR-ILJW: a Push-to-Talk recording follows too", async () => {
    enterVoiceMode(KEY, discussion([]));
    bench.composer = handle();
    registerVoiceComposer(KEY, bench.composer);
    pressRecord();
    await settle();
    await vi.advanceTimersByTimeAsync(400);
    const next = moved();
    releaseRecord();
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    expect(next.post).toHaveBeenCalledTimes(1);
    expect(bench.composer.post).not.toHaveBeenCalled();
  });

  it("VOM-FR-ILJW: a held recording follows too", async () => {
    await confirmed();
    api.finishVoiceCapture.mockRejectedValueOnce("provider_unreachable");
    noteActivity(change("speech_end", { captureId: "c1" }));
    await settle();
    const next = moved();
    await retryRecording();
    expect(next.post).toHaveBeenCalledWith("again");
    expect(bench.composer.post).not.toHaveBeenCalled();
  });

  it("VOM-FR-GZPA: a session closes when the new composer never mounts", async () => {
    await openSession();
    bindVoiceToDiscussion(KEY, discussion([], "d2"));
    await vi.advanceTimersByTimeAsync(SURFACE_GRACE_MS);
    expect(getVoiceState().vadOpen).toBe(false);
  });

  it("VOM-FR-GZPA: a session stays when the new composer mounts in time", async () => {
    await openSession();
    bindVoiceToDiscussion(KEY, discussion([], "d2"));
    await vi.advanceTimersByTimeAsync(1000);
    registerVoiceComposer("d2", handle());
    await vi.advanceTimersByTimeAsync(SURFACE_GRACE_MS);
    expect(getVoiceState().vadOpen).toBe(true);
  });
});
