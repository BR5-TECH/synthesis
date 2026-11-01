/**
 * Voice mode as a process (`VOM-voice-mode.md`): entry and exit, the saved
 * modes, the press, and the Push-to-Talk recording.
 *
 * This holds no React. Voice mode is bound to one discussion and not to a
 * mounted composer (VOM-FR-BCRD): a composer lends a handle while it is
 * mounted, and the handle stays retained when it unmounts
 * (`./voiceHandles.ts`). The utterances, the VAD session, and the spoken replies
 * live in `./voiceUtterances.ts`, `./voiceVad.ts`, and `./voiceReplies.ts`.
 */
import { appendVoiceCapture, finishVoiceCapture, retryVoiceCapture, startVoiceCapture } from "../api";
import { logDebug, logInfo, logWarn } from "../logging";
import { loadAppPreferences, patchAppPreferences, peekAppPreferences } from "../state/appPreferences";
import { clearLiveBubble } from "../state/liveBubble";
import { ownerLabelOf } from "../state/ownerAvailability";
import {
  INITIAL_VOICE_STATE,
  getVoiceState,
  patchVoiceState,
  type VoiceInputMode,
  type VoiceResponseMode,
} from "../state/voiceSession";
import type { AppPreferences, Discussion, DiscussionTarget, VoiceHotkeyBehaviour } from "../types";
import { ChunkPump, MicrophoneError, openMicrophone, type Microphone } from "./capture";
import { prepareAudio } from "./playback";
import {
  addHandle,
  forgetRetained,
  handleOf,
  moveHandles,
  postingHandleOf,
  removeHandle,
  resetVoiceHandlesForTest,
  type VoiceComposerHandle,
} from "./voiceHandles";
import { errorCode } from "./voiceMessages";
import {
  answerTargetOf,
  forgetRetainedQuestions,
  resetVoiceQuestionsForTest,
  type AnswerTarget,
} from "./voiceQuestions";
import {
  forgetSpokenQuestion,
  latestVoiceDiscussion,
  resetVoiceRepliesForTest,
  setBaseline,
  stopSpeech,
} from "./voiceReplies";
import {
  answerTaken,
  beginUtterance,
  chunkedFor,
  detachUtterance,
  discardUtterance,
  dropHeldRecording,
  finishUtterance,
  heldRecording,
  moveUtterances,
  noteUtteranceText,
  openUtterances,
  resetVoiceUtterancesForTest,
  retryHeld,
  utteranceOf,
  type Utterance,
} from "./voiceUtterances";
import {
  TAIL_MS,
  discardSpoken,
  endVadSession,
  keepSpoken,
  pressVad,
  pushVadGate,
  resetVoiceVadForTest,
  vadIsBusy,
} from "./voiceVad";

export { noteChunk } from "./voiceUtterances";
export { noteActivity } from "./voiceVad";
export { noteDiscussionChanged, noteQuestionShown, stopSpeech } from "./voiceReplies";
export type { PostOutcome, VoiceComposerHandle } from "./voiceHandles";
export { TAIL_MS };

/** VOM-FR-DGOG. */
export const SHORT_PRESS_MS = 300;
/** VOM-FR-WSXW. */
export const RECORDING_LIMIT_MS = 10 * 60 * 1000;

interface Capture {
  key: string;
  mic: Microphone | null;
  pump: ChunkPump;
  utterance: Utterance | null;
  pressedAt: number;
  released: boolean;
  discard: boolean;
  /** VOM-FR-ABPU. */
  keepInBody: boolean;
  /** VOM-FR-PINR. */
  answers: AnswerTarget | null;
  limit: ReturnType<typeof setTimeout> | null;
}

let behaviour: VoiceHotkeyBehaviour = "hold";
let capture: Capture | null = null;
/** VOM-FR-HUXY: the capture whose live text shows in the voice area. */
let liveSession: string | null = null;
/** VOM-FR-KEMU: the saved response mode. The effective one can be Text. */
let savedResponse: VoiceResponseMode = "text";
/** Counts entries, so a late read of the preferences applies to its own entry. */
let entry = 0;
/** A toggle changed since the entry. */
let toggled = false;

export function setVoiceBehaviour(next: VoiceHotkeyBehaviour): void {
  behaviour = next;
}

/** VOM-FR-WPRW, VOM-FR-FHYY: voice mode is on and can record. */
export function voiceComposerMounted(): boolean {
  return handleOf(getVoiceState().activeKey) !== null;
}

function clearFeedback(): void {
  patchVoiceState({ error: null, canRetry: false, note: null });
}

// --- Saved modes ----------------------------------------------------------

/** VOM-FR-KEMU: saved Voice with no speech route is Text, and the saved value stays. */
function effectiveResponse(saved: VoiceResponseMode): VoiceResponseMode {
  return saved === "voice" && getVoiceState().providerSpeaks ? "voice" : "text";
}

function applySaved(prefs: AppPreferences): void {
  savedResponse = prefs.voiceResponseMode === "voice" ? "voice" : "text";
  const inputMode: VoiceInputMode = prefs.voiceInputMode === "vad" ? "vad" : "ptt";
  patchVoiceState({ inputMode, responseMode: effectiveResponse(savedResponse) });
}

/** VOM-FR-KEMU: a toggle change saves at once. */
function save(change: Partial<AppPreferences>): void {
  toggled = true;
  patchAppPreferences(change).catch((error: unknown) => {
    logWarn(["frontend"], "a voice mode preference was not saved", { code: errorCode(error) });
  });
}

// --- Entering and leaving -------------------------------------------------

/** STB-FR-PYRC: the owner name of a conversation tab, without `Chat:`. */
function ownerNameOf(target: DiscussionTarget): string {
  const label = ownerLabelOf(target);
  if (target.kind !== "artifact") return label;
  const cut = label.lastIndexOf("/");
  return cut === -1 ? label : label.slice(cut + 1);
}

let voiceTarget: DiscussionTarget | null = null;

/** VOM-FR-OSTM, VOM-FR-ILJW, VOM-FR-KEMU, VOM-FR-XDUY. */
export function enterVoiceMode(
  key: string,
  discussion: Discussion | null,
  target: DiscussionTarget | null = discussion?.target ?? null,
): void {
  if (getVoiceState().activeKey !== null) leaveVoiceMode();
  prepareAudio();
  setBaseline(discussion);
  forgetSpokenQuestion();
  voiceTarget = target;
  entry += 1;
  toggled = false;
  const mine = entry;
  patchVoiceState({
    activeKey: key,
    discussionId: discussion?.id ?? null,
    ownerName: target ? ownerNameOf(target) : null,
    transcript: "",
    error: null,
    canRetry: false,
    note: null,
  });
  const cached = peekAppPreferences();
  if (cached) {
    applySaved(cached);
  } else {
    patchVoiceState({ inputMode: "ptt", responseMode: "text" });
    void loadAppPreferences()
      .then((prefs) => {
        // A toggle that the author used before the read came wins.
        if (entry === mine && !toggled && getVoiceState().activeKey !== null) applySaved(prefs);
      })
      .catch((error: unknown) => {
        logWarn(["frontend"], "the voice mode preferences could not be read", {
          code: errorCode(error),
        });
      });
  }
  logInfo(["frontend"], "voice mode entered", { key });
}

/** The reveal of the voice discussion, for the status bar (STB-FR-OSOL). */
export function voiceReveal(): {
  discussion: Discussion | null;
  discussionId: string | null;
  target: DiscussionTarget;
} | null {
  const { activeKey, discussionId } = getVoiceState();
  if (activeKey === null || voiceTarget === null) return null;
  return { discussion: latestVoiceDiscussion(), discussionId, target: voiceTarget };
}

/** What a recording in progress does when voice mode ends. */
function endCapture(kind: "keep" | "discard"): void {
  if (!capture) return;
  if (kind === "discard") capture.discard = true;
  else capture.keepInBody = true;
  endRecording();
}

function closeVoiceState(): void {
  forgetRetained();
  forgetRetainedQuestions();
  forgetSpokenQuestion();
  voiceTarget = null;
  liveSession = null;
  patchVoiceState({
    activeKey: null,
    discussionId: null,
    ownerName: null,
    transcript: "",
    error: null,
    canRetry: false,
    note: null,
  });
}

/** VOM-FR-JBJI, VOM-FR-ABPU: what is on its way joins the session body. */
export function leaveVoiceMode(): void {
  stopSpeech();
  dropHeldRecording();
  endCapture("keep");
  keepSpoken();
  openUtterances().forEach(detachUtterance);
  void endVadSession("release");
  closeVoiceState();
}

/**
 * VOM-FR-UXZT, VOM-FR-LSBQ: the Stop control of the status bar. It discards a
 * recording and its chunks, drops the live bubble, and stops playback.
 */
export function stopVoiceMode(): void {
  if (getVoiceState().activeKey === null) return;
  logInfo(["frontend"], "voice mode stopped from the status bar", {});
  discardAll();
  closeVoiceState();
}

function discardAll(): void {
  stopSpeech();
  endCapture("discard");
  discardSpoken();
  [...openUtterances()].forEach(discardUtterance);
  void endVadSession("discard");
  dropHeldRecording();
  clearLiveBubble();
}

/** VOM-FR-NERG, VOM-FR-ILJW: the first post of an opening composer created a discussion. */
export function bindVoiceToDiscussion(key: string, discussion: Discussion): void {
  if (getVoiceState().activeKey !== key) return;
  setBaseline(discussion);
  // What is on its way follows the composer, or it posts through the opening
  // composer a second time.
  if (capture?.key === key) capture.key = discussion.id;
  moveUtterances(key, discussion.id);
  moveHandles(key, discussion);
  voiceTarget = discussion.target;
  patchVoiceState({ activeKey: discussion.id, discussionId: discussion.id });
  pushVadGate();
}

/** VOM-FR-IAVZ, VOM-FR-KEMU. */
export function setResponseMode(mode: VoiceResponseMode): void {
  if (mode === "text") stopSpeech();
  savedResponse = mode;
  patchVoiceState({ responseMode: mode });
  save({ voiceResponseMode: mode });
}

export function registerVoiceComposer(key: string, handle: VoiceComposerHandle): () => void {
  addHandle(key, handle);
  noteComposerChanged();
  return () => {
    removeHandle(key, handle);
    noteComposerChanged();
  };
}

/** VOP-FR-QZCE: the composer changed what a session may do. */
export function noteComposerChanged(): void {
  pushVadGate();
}

// --- Push-to-Talk recording -------------------------------------------------

async function completeRecording(active: Capture): Promise<void> {
  if (capture === active) capture = null;
  active.mic?.stop();
  if (active.limit) clearTimeout(active.limit);
  const u = active.utterance;
  // The next press can start a recording at once (VOM-FR-QTAQ).
  patchVoiceState({ phase: "idle", recordingSince: null });
  if (u === null) return;
  if (active.discard) {
    if (liveSession === u.captureId) liveSession = null;
    patchVoiceState({ transcript: "" });
    discardUtterance(u);
    return;
  }
  u.keepInBody = u.keepInBody || active.keepInBody;
  await active.pump.flush();
  // A Stop or a teardown during the flush discarded the capture already.
  if (u.discard || u.settled) {
    if (liveSession === u.captureId) liveSession = null;
    return;
  }
  await finishUtterance(u, finishVoiceCapture(u.captureId));
  if (liveSession === u.captureId) liveSession = null;
}

/** VOM-FR-FTFU: the release, a short tail, and then the end of input. */
function endRecording(): void {
  const active = capture;
  if (!active || active.released) return;
  active.released = true;
  if (active.utterance === null) return; // The start path sees `released`.
  if (active.discard) {
    void completeRecording(active);
    return;
  }
  setTimeout(() => void completeRecording(active), TAIL_MS);
}

async function beginRecording(key: string, handle: VoiceComposerHandle): Promise<void> {
  const pump = new ChunkPump(appendVoiceCapture);
  const answers = answerTargetOf(key, answerTaken);
  const active: Capture = {
    key,
    mic: null,
    pump,
    utterance: null,
    pressedAt: Date.now(),
    released: false,
    discard: false,
    keepInBody: false,
    answers,
    limit: null,
  };
  capture = active;
  patchVoiceState({ phase: "opening" });
  try {
    active.mic = await openMicrophone((pcm) => pump.push(pcm));
    // Nobody waits for this recording now: a short press, or a teardown.
    if (active.discard) {
      active.mic.stop();
      if (capture === active) capture = null;
      patchVoiceState({ phase: "idle", recordingSince: null });
      return;
    }
    const chunked = chunkedFor(active.key, answers);
    const start = await startVoiceCapture(active.mic.rate, handle.nicknames(), chunked);
    active.utterance = beginUtterance(active.key, start.sessionId, chunked, answers);
    liveSession = start.sessionId;
    pump.attach(start.sessionId);
    patchVoiceState({ phase: "recording", live: start.live, recordingSince: Date.now() });
    active.limit = setTimeout(endRecording, RECORDING_LIMIT_MS);
    logDebug(["frontend", "ai"], "a voice recording started", { live: start.live, chunked });
  } catch (error) {
    active.mic?.stop();
    if (capture === active) capture = null;
    const code = error instanceof MicrophoneError ? error.message : errorCode(error);
    logWarn(["frontend", "ai"], "a voice recording did not start", { code });
    patchVoiceState({ phase: "idle", error: code, canRetry: false });
    return;
  }
  // The author let go while the microphone opened.
  if (active.released) {
    active.released = false;
    endRecording();
  }
}

/** VOM-FR-IKIK, VOM-FR-VGCE, VOM-FR-BQBI, VOM-FR-KEMU. */
export function setInputMode(mode: VoiceInputMode): void {
  const state = getVoiceState();
  if (mode === state.inputMode) return;
  if (mode === "vad" && (state.phase !== "idle" || capture !== null)) return;
  patchVoiceState({ inputMode: mode });
  save({ voiceInputMode: mode });
  if (mode === "ptt") void endVadSession("release");
}

/** VOM-FR-HHWL, VOM-FR-LIFL, VOM-FR-VVTY, VOM-FR-FASJ, VOM-FR-PDZR. */
export function pressRecord(): void {
  const state = getVoiceState();
  if (state.inputMode === "vad") {
    pressVad();
    return;
  }
  const handle = handleOf(state.activeKey);
  if (!handle || state.activeKey === null || handle.blocked()) return;
  if (capture) {
    if (behaviour === "toggle") endRecording();
    return;
  }
  if (state.phase !== "idle" || vadIsBusy()) return;
  // VOM-FR-VVTY: barge-in in Push-to-Talk.
  stopSpeech();
  dropHeldRecording();
  clearFeedback();
  patchVoiceState({ transcript: "" });
  void beginRecording(state.activeKey, postingHandleOf(state.activeKey) ?? handle);
}

/** VOM-FR-HHWL, VOM-FR-DGOG, VOM-FR-XQUY: the end of a hold. */
export function releaseRecord(): void {
  if (behaviour !== "hold" || !capture) return;
  if (Date.now() - capture.pressedAt < SHORT_PRESS_MS) capture.discard = true;
  endRecording();
}

/** VOM-FR-WICV. */
export async function retryRecording(): Promise<void> {
  // VOP-FR-KBPZ: an utterance replaces the held recording, so the two cannot run.
  if (!heldRecording() || getVoiceState().hearing || capture !== null) return;
  patchVoiceState({ error: null, canRetry: false });
  await retryHeld((id) => retryVoiceCapture(id));
}

/** VOM-FR-HUXY: a chunked utterance shows its live text in the live bubble. */
export function noteTranscript(sessionId: string, text: string): void {
  if (noteUtteranceText(sessionId, text)) return;
  const u = utteranceOf(sessionId);
  if (liveSession === sessionId || (u !== null && !u.chunked)) patchVoiceState({ transcript: text });
}

/** VOM-FR-WMAP, VOM-FR-KEMU. */
export function noteProvider(ready: boolean, speaks: boolean): void {
  patchVoiceState({ providerReady: ready, providerSpeaks: speaks });
  if (getVoiceState().activeKey !== null) {
    const effective = effectiveResponse(savedResponse);
    if (effective === "text") stopSpeech();
    patchVoiceState({ responseMode: effective });
  } else if (!speaks && getVoiceState().responseMode === "voice") {
    patchVoiceState({ responseMode: "text" });
  }
  if (!ready) void endVadSession("release");
}

/** CVP-FR-51, VOM-FR-LSBQ: the owner of the voice discussion was deleted. */
export function noteOwnerDeleted(target: DiscussionTarget): void {
  if (voiceTarget === null || getVoiceState().activeKey === null) return;
  const same =
    (voiceTarget.kind === "draft" && target.kind === "draft" && voiceTarget.draftId === target.draftId) ||
    (voiceTarget.kind === "artifact" &&
      target.kind === "artifact" &&
      // LCM-FR-11: a folder that holds the file goes with it.
      (voiceTarget.artifactId === target.artifactId ||
        voiceTarget.artifactId.startsWith(`${target.artifactId.replace(/\/+$/, "")}/`))) ||
    (voiceTarget.kind === "note" && target.kind === "note" && voiceTarget.noteId === target.noteId);
  if (!same) return;
  logInfo(["frontend"], "voice mode ended with its owner", { kind: target.kind });
  discardAll();
  closeVoiceState();
}

/** VOM-FR-KQAI: the session stores go, so nothing is kept. The saved modes stay. */
export function endVoiceForTeardown(): void {
  discardAll();
  forgetRetained();
  forgetRetainedQuestions();
  voiceTarget = null;
  liveSession = null;
  const { providerReady, providerSpeaks, inputMode, responseMode } = getVoiceState();
  patchVoiceState({ ...INITIAL_VOICE_STATE, providerReady, providerSpeaks, inputMode, responseMode });
}

export function resetVoiceControllerForTest(): void {
  behaviour = "hold";
  capture = null;
  liveSession = null;
  savedResponse = "text";
  voiceTarget = null;
  entry = 0;
  toggled = false;
  resetVoiceHandlesForTest();
  resetVoiceVadForTest();
  resetVoiceUtterancesForTest();
  resetVoiceRepliesForTest();
  resetVoiceQuestionsForTest();
}
