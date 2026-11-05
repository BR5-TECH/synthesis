/**
 * The test of a Voice AI settings tab (`VAI-voice-ai-settings.md`
 * VAI-FR-HTVQ, VAI-FR-ZCMB, VAI-FR-NWRL, VAI-FR-YBQE, VAI-FR-SKOF).
 *
 * Hold, speak, and release: the provider of the tab transcribes the audio and
 * speaks the transcript back. No discussion, no agent, and no translation take
 * part, and the transcript goes with the tab.
 */
import {
  appendVoiceCapture,
  cancelVoiceCapture,
  cancelVoiceSpeech,
  finishVoiceCapture,
  speakVoiceSegment,
  startVoiceCapture,
} from "../api";
import { logInfo, logWarn } from "../logging";
import type { VoiceOptions, VoiceProviderId } from "../types";
import { ChunkPump, MicrophoneError, openMicrophone, type Microphone } from "./capture";
import {
  beginSegment,
  endSegmentInput,
  isPlaybackIdle,
  playFrame,
  prepareAudio,
  setDrainedListener,
  setPlaybackFailureListener,
  stopPlayback,
} from "./playback";
import { errorCode } from "./voiceMessages";

/** VAI-FR-NWRL: the longest hold of a test. */
export const TEST_LIMIT_MS = 15_000;

/** VOP-FR-MOSY: the longest text of one segment. */
const SEGMENT_CHARS = 4096;

export type VoiceTestPhase = "idle" | "opening" | "listening" | "transcribing" | "speaking";

export interface VoiceTestState {
  phase: VoiceTestPhase;
  /** The text the provider heard, or null before a transcript. */
  transcript: string | null;
  /** VAI-FR-ZCMB: the transcript was empty. */
  nothingHeard: boolean;
  /** VAI-FR-YBQE: a typed code. */
  error: string | null;
}

export const IDLE_TEST: VoiceTestState = {
  phase: "idle",
  transcript: null,
  nothingHeard: false,
  error: null,
};

export interface VoiceTestVoice {
  name: string | null;
  options: VoiceOptions;
}

export interface VoiceTest {
  press(): void;
  release(): void;
  /** VAI-FR-NWRL: the tab changes or the window closes. */
  cancel(): void;
}

let counter = 0;

export function createVoiceTest(
  provider: VoiceProviderId,
  voiceOf: () => VoiceTestVoice,
  onState: (state: VoiceTestState) => void,
): VoiceTest {
  let state = IDLE_TEST;
  /** One test at a time. A new press or a cancel makes the older one stale. */
  let run = 0;
  let mic: Microphone | null = null;
  let session: string | null = null;
  let speech: string | null = null;
  let holding = false;
  let limit: ReturnType<typeof setTimeout> | null = null;
  let pump: ChunkPump | null = null;

  const set = (next: Partial<VoiceTestState>) => {
    state = { ...state, ...next };
    onState(state);
  };

  const closeMicrophone = () => {
    if (limit !== null) clearTimeout(limit);
    limit = null;
    mic?.stop();
    mic = null;
  };

  const stopSpeech = () => {
    if (speech !== null) {
      const id = speech;
      speech = null;
      void cancelVoiceSpeech(id).catch(() => undefined);
    }
    setDrainedListener(null);
    setPlaybackFailureListener(null);
    stopPlayback();
  };

  const abandon = () => {
    closeMicrophone();
    if (session !== null) {
      const id = session;
      session = null;
      void cancelVoiceCapture(id).catch((error: unknown) => {
        logWarn(["frontend", "ai"], "a voice test capture did not cancel", {
          provider,
          code: errorCode(error),
        });
      });
    }
    stopSpeech();
  };

  const fail = (current: number, error: unknown) => {
    if (current !== run) return;
    const code = error instanceof MicrophoneError ? error.message : errorCode(error);
    logWarn(["frontend", "ai"], "a voice test failed", { provider, code });
    abandon();
    set({ ...IDLE_TEST, error: code });
  };

  const speak = async (current: number, text: string) => {
    const id = `voice-test-${++counter}`;
    speech = id;
    set({ phase: "speaking" });
    const { name, options } = voiceOf();
    const live = () => current === run && speech === id;
    const done = () => {
      speech = null;
      setDrainedListener(null);
      setPlaybackFailureListener(null);
      set({ phase: "idle" });
    };
    // VAI-FR-ZCMB: playback drains also on a rate frame and between two
    // frames. The speech ends only when the whole segment was sent.
    let sent = false;
    stopPlayback();
    setDrainedListener(() => {
      if (sent && live()) done();
    });
    // VOM-FR-VGQF: a frame that cannot play fails the test.
    setPlaybackFailureListener((code) => {
      if (live()) fail(current, code);
    });
    // VAI-FR-NLYF: the test logs its playback as voice mode does.
    beginSegment("test");
    // VAI-FR-YBQE, VAI-FR-NWRL: a frame that is still in the channel after a
    // new press or a cancel does not play.
    const onFrame = (frame: ArrayBuffer | number[]) => {
      if (live()) playFrame(frame);
    };
    await speakVoiceSegment(id, text.slice(0, SEGMENT_CHARS), name, onFrame, options, provider);
    if (live()) endSegmentInput();
    sent = true;
    // A segment with no audio, or audio that already played, gets no later event.
    if (live() && isPlaybackIdle()) done();
  };

  const finish = async (current: number) => {
    const id = session;
    closeMicrophone();
    if (id === null) return;
    set({ phase: "transcribing" });
    try {
      await pump?.flush();
      const text = (await finishVoiceCapture(id)).trim();
      if (session === id) session = null;
      if (current !== run) return;
      logInfo(["frontend", "ai"], "a voice test was transcribed", {
        provider,
        chars: text.length,
      });
      if (text === "") {
        set({ phase: "idle", transcript: "", nothingHeard: true });
        return;
      }
      set({ transcript: text });
      await speak(current, text);
    } catch (error) {
      if (session === id) session = null;
      fail(current, error);
    }
  };

  const start = async (current: number) => {
    try {
      const chunks = new ChunkPump(appendVoiceCapture);
      pump = chunks;
      const opened = await openMicrophone((pcm) => chunks.push(pcm), () => {
        logWarn(["frontend", "ai"], "the microphone of a voice test was lost", { provider });
        fail(current, "microphone_unavailable");
      });
      if (current !== run || !holding) {
        // VAI-FR-NWRL: released before the capture opened.
        opened.stop();
        if (current === run) set({ ...IDLE_TEST });
        return;
      }
      mic = opened;
      const capture = await startVoiceCapture(opened.rate, [], false, provider);
      if (current !== run || !holding) {
        opened.stop();
        if (mic === opened) mic = null;
        void cancelVoiceCapture(capture.sessionId).catch(() => undefined);
        if (current === run) set({ ...IDLE_TEST });
        return;
      }
      session = capture.sessionId;
      chunks.attach(capture.sessionId);
      set({ phase: "listening" });
      limit = setTimeout(() => release(), TEST_LIMIT_MS);
    } catch (error) {
      fail(current, error);
    }
  };

  const press = () => {
    if (holding) return;
    // VAI-FR-YBQE: a new press stops a test that still speaks.
    abandon();
    run += 1;
    holding = true;
    logInfo(["frontend", "ai"], "a voice test started", { provider });
    // The press is the user gesture that lets audio play later.
    prepareAudio();
    set({ ...IDLE_TEST, phase: "opening" });
    void start(run);
  };

  function release() {
    if (!holding) return;
    holding = false;
    if (state.phase === "listening") void finish(run);
  }

  const cancel = () => {
    holding = false;
    run += 1;
    abandon();
  };

  return { press, release, cancel };
}
