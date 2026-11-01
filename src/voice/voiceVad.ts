/**
 * The VAD session of voice mode (`VOM-voice-mode.md` VOM-FR-PDZR to VOM-FR-RTSU,
 * VOM-FR-NZXN to VOM-FR-GTAQ).
 *
 * The backend finds each utterance. The session stays with the voice discussion
 * while no composer of it is mounted (VOM-FR-BCRD), and it listens for the next
 * utterance while earlier ones transcribe (VOM-FR-GBCN).
 */
import { cancelVoiceCapture, finishVoiceCapture } from "../api";
import { logInfo, logWarn } from "../logging";
import { getVoiceState, patchVoiceState, subscribeVoiceState } from "../state/voiceSession";
import type { VoiceActivityChange } from "../types";
import { MicrophoneError } from "./capture";
import { handleOf } from "./voiceHandles";
import { errorCode } from "./voiceMessages";
import { answerTargetOf } from "./voiceQuestions";
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
/** A session that opens. The author can close it before it is open. */
let vadOpening: { cancelled: boolean } | null = null;
let stopGateWatch: (() => void) | null = null;
/** VOM-FR-WRVR: the utterance that the author speaks now. */
let speaking: Utterance | null = null;

function clearFeedback(): void {
  patchVoiceState({ error: null, canRetry: false, note: null });
}

/** VOM-FR-GTAQ, VOP-FR-QZCE: what the detector may do now. */
function vadGate(): VadGate {
  const state = getVoiceState();
  const key = state.activeKey;
  const handle = handleOf(key);
  const answers = key === null ? null : answerTargetOf(key);
  return {
    blocked: handle === null || handle.blocked() || !state.providerReady,
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

/** VOM-FR-CSXI. */
function vadLost(code: string): void {
  logWarn(["frontend", "ai"], "a voice session lost its audio", { code });
  patchVoiceState({ error: code, canRetry: false });
  void endVadSession("release");
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
    const session = await VadSession.open(nicknames, { gate: vadGate, lost: vadLost });
    vadOpening = null;
    if (opening.cancelled) {
      session.stopMicrophone();
      void session.close(true).catch(() => undefined);
      patchVoiceState({ phase: "idle" });
      return;
    }
    vad = session;
    stopGateWatch = subscribeVoiceState(() => vad?.pushGate());
    patchVoiceState({ phase: "idle", vadOpen: true });
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
  void finishUtterance(u, finishVoiceCapture(u.captureId));
}

/** VOM-FR-XEIA: the tail, the last audio, then the close. */
async function closeVad(session: VadSession, kind: "release" | "discard"): Promise<void> {
  const job = speaking;
  if (kind === "release" && job !== null) {
    await new Promise((done) => setTimeout(done, TAIL_MS));
  }
  session.stopMicrophone();
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
  patchVoiceState({ vadOpen: false });
  const closed = closeVad(session, kind).finally(() => {
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

/** VOM-FR-PDZR: the record control opens or closes the session. */
export function pressVad(): void {
  if (vad) {
    void endVadSession("release");
    return;
  }
  if (vadOpening) {
    vadOpening.cancelled = true;
    return;
  }
  const state = getVoiceState();
  const handle = handleOf(state.activeKey);
  if (!handle || state.activeKey === null || handle.blocked() || state.phase !== "idle") return;
  // VOM-FR-VVTY.
  stopSpeech();
  dropHeldRecording();
  clearFeedback();
  patchVoiceState({ transcript: "" });
  void openVad(handle.nicknames());
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
  vadOpening = null;
  stopGateWatch?.();
  stopGateWatch = null;
  speaking = null;
}
