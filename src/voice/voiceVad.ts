/**
 * The VAD session of voice mode (`VOM-voice-mode.md` VOM-FR-PDZR to VOM-FR-PJOC,
 * VOM-FR-NZXN to VOM-FR-GTAQ).
 *
 * A session opens by itself in VAD mode, and no record control exists there. The
 * backend finds each utterance. The session stays with the voice discussion
 * while no composer of it is mounted (VOM-FR-BCRD), and it listens for the next
 * utterance while earlier ones transcribe (VOM-FR-GBCN). Mute keeps the session
 * open and stops its audio (VOM-FR-MUZB).
 */
import { cancelVoiceCapture, finishVoiceCapture } from "../api";
import { logInfo, logWarn } from "../logging";
import { levelOfPcm, noteMicLevel } from "../state/voiceLevel";
import { getVoiceState, patchVoiceState, subscribeVoiceState } from "../state/voiceSession";
import type { VoiceActivityChange } from "../types";
import { MicrophoneError } from "./capture";
import { handleOf } from "./voiceHandles";
import { errorCode } from "./voiceMessages";
import { answerTargetOf } from "./voiceQuestions";
import { armFiller, cancelFiller } from "./voiceFiller";
import { afterSettle, bargeIn, stopSpeech } from "./voiceReplies";
import {
  answerTaken,
  beginUtterance,
  chunkedFor,
  dropHeldRecording,
  finishUtterance,
  hasUnsettledUtterance,
  settleUtterance,
  type Utterance,
} from "./voiceUtterances";
import { VadSession, type VadGate } from "./vadSession";

/** VOM-FR-FTFU. */
export const TAIL_MS = 200;

/** VOM-FR-PDZR: the open session. */
let vad: VadSession | null = null;
/** A session that closes. Its events still count until the close comes back. */
let closingVad: VadSession | null = null;
let vadClosed: Promise<void> | null = null;
/** VOM-FR-JMMF: the closing session still holds the microphone. */
let micReleased: Promise<void> | null = null;
/** A session that opens. The author can close it before it is open. */
let vadOpening: { cancelled: boolean } | null = null;
let stopGateWatch: (() => void) | null = null;
/** VOM-FR-WRVR: the utterance that the author speaks now. */
let speaking: Utterance | null = null;

function clearFeedback(): void {
  patchVoiceState({ error: null, canRetry: false, note: null });
}

/** VOM-FR-GTAQ, VOM-FR-MUZB, VOP-FR-QZCE: what the detector may do now. */
function vadGate(): VadGate {
  const state = getVoiceState();
  const key = state.activeKey;
  const handle = handleOf(key);
  const answers = key === null ? null : answerTargetOf(key);
  return {
    blocked: handle === null || handle.blocked() || !state.providerReady || state.muted,
    playing: state.playing && !state.playbackPaused,
    chunked: key !== null && chunkedFor(key, answers),
    keyterms: handle?.nicknames() ?? [],
  };
}

/** VOP-FR-QZCE: the composer changed what a session may do. */
export function pushVadGate(): void {
  vad?.pushGate();
}

export function vadIsBusy(): boolean {
  return vad !== null || vadOpening !== null;
}

/**
 * VOM-FR-JMMF: while a closing session still holds the microphone, a promise
 * that resolves when it releases it. Null when no session holds it.
 */
export function vadMicClosed(): Promise<void> | null {
  return micReleased;
}

/** VOM-FR-CSXI. */
function vadLost(code: string): void {
  logWarn(["frontend", "ai"], "a voice session lost its audio", { code });
  patchVoiceState({ error: code, canRetry: false });
  void endVadSession("release");
}

/**
 * VOM-FR-MUZB: muted audio does not go to the session, and silence goes in its
 * place. An utterance in progress keeps its audio to its end, so the detector
 * finds that end where the author stopped.
 */
function vadFeeds(): boolean {
  const state = getVoiceState();
  return !state.muted || state.hearing || speaking !== null;
}

/** VOM-FR-WFHN, VOM-FR-ZDLY: the level of the microphone, flat while muted. */
function vadHeard(pcm: ArrayBuffer): void {
  if (vad === null) return;
  noteMicLevel(getVoiceState().muted ? 0 : levelOfPcm(pcm));
}

async function openVad(nicknames: string[]): Promise<void> {
  const opening = { cancelled: false };
  vadOpening = opening;
  patchVoiceState({ phase: "opening" });
  await vadClosed;
  if (opening.cancelled) {
    vadOpening = null;
    patchVoiceState({ phase: "idle" });
    return;
  }
  try {
    const session = await VadSession.open(nicknames, {
      gate: vadGate,
      feeds: vadFeeds,
      heard: vadHeard,
      lost: vadLost,
    });
    vadOpening = null;
    if (opening.cancelled) {
      session.stopMicrophone();
      void session.close(true).catch(() => undefined);
      patchVoiceState({ phase: "idle" });
      return;
    }
    vad = session;
    stopGateWatch = subscribeVoiceState(() => vad?.pushGate());
    // VOM-FR-RBWX: a session opens unmuted.
    patchVoiceState({ phase: "idle", vadOpen: true, muted: false });
    session.pushGate();
    logInfo(["frontend", "ai"], "a voice session opened", {});
    const loss = session.takeLoss();
    if (loss !== null) vadLost(loss);
  } catch (error) {
    vadOpening = null;
    const code = error instanceof MicrophoneError ? error.message : errorCode(error);
    logWarn(["frontend", "ai"], "a voice session did not open", { code });
    patchVoiceState({ phase: "idle", error: code, canRetry: false });
  }
}

/** VOM-FR-OTRF: the end of the utterance. The session listens on at once. */
function finishSpoken(u: Utterance): void {
  if (speaking === u) speaking = null;
  patchVoiceState({ phase: "idle", hearing: false, recordingSince: null });
  // VOM-FR-YGAR: the end of the utterance arms its filler.
  armFiller(u.captureId);
  void finishUtterance(u, finishVoiceCapture(u.captureId));
}

/** VOM-FR-XEIA: the tail, the last audio, then the close. */
async function closeVad(
  session: VadSession,
  kind: "release" | "discard",
  released: () => void,
): Promise<void> {
  const job = speaking;
  if (kind === "release" && job !== null) {
    await new Promise((done) => setTimeout(done, TAIL_MS));
  }
  session.stopMicrophone();
  released();
  // A teardown during the tail discards too (VOM-FR-KQAI).
  // A capture that transcribes stays, because a failure leaves it for Retry.
  const discard =
    kind === "discard" || (job === null ? !hasUnsettledUtterance() : job.discard);
  let open: string | null = null;
  try {
    open = await session.close(discard);
  } catch (error) {
    logWarn(["frontend", "ai"], "a voice session did not close", { code: errorCode(error) });
    // The capture is still open, and it holds the speech.
    if (!discard) open = job?.captureId ?? null;
  }
  if (job === null) {
    // VOM-FR-KVHG: speech that was not confirmed resumes a paused reply.
    patchVoiceState({ hearing: false });
    afterSettle();
    return;
  }
  // The utterance can end by itself during the tail.
  if (speaking !== job) return;
  if (open === job.captureId && !job.discard) {
    finishSpoken(job);
    return;
  }
  speaking = null;
  patchVoiceState({ phase: "idle", hearing: false, recordingSince: null, transcript: "" });
  settleUtterance(job, null);
}

export async function endVadSession(kind: "release" | "discard"): Promise<void> {
  // Each exit passes here, and a session that still opens has none of its own.
  if (vadOpening) vadOpening.cancelled = true;
  if (kind === "discard") closingVad?.stopMicrophone();
  const session = vad;
  if (!session) return;
  vad = null;
  closingVad = session;
  stopGateWatch?.();
  stopGateWatch = null;
  // VOM-FR-ZDLY: no session, so the waveform is flat.
  patchVoiceState({ vadOpen: false, muted: false });
  noteMicLevel(0);
  let release: () => void = () => undefined;
  const free = new Promise<void>((done) => (release = done));
  micReleased = free;
  const released = () => {
    if (micReleased === free) micReleased = null;
    release();
  };
  const closed = closeVad(session, kind, released).finally(() => {
    released();
    closingVad = null;
    vadClosed = null;
    logInfo(["frontend", "ai"], "a voice session closed", { kind });
  });
  vadClosed = closed;
  await closed;
}

/** VOM-FR-WRVR, VOM-FR-FASJ. */
function utteranceConfirmed(change: VoiceActivityChange): void {
  const key = getVoiceState().activeKey;
  if (change.captureId === null || key === null) return;
  dropHeldRecording();
  clearFeedback();
  // VOM-FR-POBZ: a new utterance cancels a filler that did not speak.
  cancelFiller("new utterance");
  const answers = answerTargetOf(key, answerTaken);
  speaking = beginUtterance(key, change.captureId, chunkedFor(key, answers), answers);
  patchVoiceState({
    phase: "recording",
    live: change.live ?? false,
    transcript: "",
    recordingSince: Date.now(),
  });
}

/** VOM-FR-OTRF: a capture that nobody here knows is cancelled. */
function utteranceEnded(change: VoiceActivityChange): void {
  if (speaking !== null && speaking.captureId === change.captureId) {
    finishSpoken(speaking);
  } else if (change.captureId !== null) {
    void cancelVoiceCapture(change.captureId).catch(() => undefined);
    patchVoiceState({ hearing: false });
    afterSettle();
  }
}

/** VOM-FR-WWZX, VOM-FR-CSXI. */
function activityFailed(change: VoiceActivityChange): void {
  const code = change.code ?? "unknown";
  logWarn(["frontend", "ai"], "a voice utterance failed", { code });
  if (change.fatal) {
    vadLost(code);
    return;
  }
  const job = speaking;
  speaking = null;
  patchVoiceState({
    phase: "idle",
    hearing: false,
    recordingSince: null,
    transcript: job ? "" : getVoiceState().transcript,
    error: code,
    canRetry: false,
  });
  if (job) settleUtterance(job, null);
  else afterSettle();
}

/** VOM-FR-NZXN: each step of an utterance, as the backend finds it. */
export function noteActivity(change: VoiceActivityChange): void {
  // A close travels apart from the events, so an event can come after it.
  if ((vad ?? closingVad)?.activityId !== change.activityId) return;
  if (vad === null) {
    if (change.state === "speech_end" && speaking?.captureId === change.captureId) {
      finishSpoken(speaking);
    }
    return;
  }
  switch (change.state) {
    case "speech_start":
      patchVoiceState({ hearing: true });
      // VOM-FR-FVRS: barge-in.
      bargeIn();
      break;
    case "speech_confirmed":
      utteranceConfirmed(change);
      break;
    case "speech_end":
      utteranceEnded(change);
      break;
    case "misfire":
      // VOM-FR-XWPZ, VOM-FR-KVHG.
      patchVoiceState({ hearing: false });
      afterSettle();
      break;
    case "failed":
      activityFailed(change);
      break;
  }
}

/**
 * VOM-FR-PDZR, VOM-FR-CUCN: open a session with no press, at entry with saved
 * VAD, at a switch to VAD, or from the Open again control. A disabled composer
 * does not stop it, because the gate blocks utterances then (VOM-FR-GTAQ).
 */
export function openVadSession(reason: "entry" | "switch" | "again"): void {
  const state = getVoiceState();
  if (state.activeKey === null || state.inputMode !== "vad") return;
  // An exit cancelled an open that is still on its way. That open now serves
  // this entry, and the gate brings its roster up to date when it opens.
  if (vadOpening?.cancelled) {
    vadOpening.cancelled = false;
    logInfo(["frontend", "ai"], "a voice session open goes on", { reason });
    return;
  }
  if (vad !== null || vadOpening !== null || state.phase !== "idle") return;
  logInfo(["frontend", "ai"], "a voice session opens", { reason });
  // VOM-FR-VVTY.
  stopSpeech();
  dropHeldRecording();
  clearFeedback();
  patchVoiceState({ transcript: "" });
  void openVad(handleOf(state.activeKey)?.nicknames() ?? []);
}

/** VOM-FR-QKTE, VOM-FR-MUZB, VOM-FR-RBWX: Mute and Unmute keep the session open. */
export function setVadMuted(muted: boolean): void {
  if (vad === null || getVoiceState().muted === muted) return;
  logInfo(["frontend", "ai"], muted ? "a voice session was muted" : "a voice session was unmuted", {});
  patchVoiceState({ muted });
  if (muted) noteMicLevel(0);
}

/** VOM-FR-KQAI, VOM-FR-UXZT: the utterance in progress goes with the session. */
export function discardSpoken(): void {
  if (speaking) speaking.discard = true;
}

/** VOM-FR-JBJI, VOM-FR-ABPU: the utterance in progress joins the session body. */
export function keepSpoken(): void {
  if (speaking) speaking.keepInBody = true;
}

export function resetVoiceVadForTest(): void {
  vad = null;
  closingVad = null;
  vadClosed = null;
  micReleased = null;
  vadOpening = null;
  stopGateWatch?.();
  stopGateWatch = null;
  speaking = null;
}
