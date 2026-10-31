/**
 * Voice mode as a process (`VOM-voice-mode.md`): the press, the recording, the
 * post, and the spoken reply.
 *
 * This holds no React. A composer registers a handle while it is mounted, and
 * the recording goes on through a change of owner surface, so the end of a
 * recording asks who is mounted at that moment (VOM-FR-ABPU).
 */
import {
  appendVoiceCapture,
  cancelVoiceCapture,
  cancelVoiceSpeech,
  finishVoiceCapture,
  retryVoiceCapture,
  speakVoiceSegment,
  startVoiceCapture,
} from "../api";
import { logDebug, logInfo, logWarn } from "../logging";
import { getDiscussionSession, setComposerBody } from "../state/discussionSession";
import {
  INITIAL_VOICE_STATE,
  getVoiceState,
  patchVoiceState,
  type VoiceResponseMode,
} from "../state/voiceSession";
import type { Comment, Discussion, VoiceHotkeyBehaviour } from "../types";
import { ChunkPump, MicrophoneError, openMicrophone, type Microphone } from "./capture";
import {
  isPlaybackIdle,
  playFrame,
  prepareAudio,
  setDrainedListener,
  stopPlayback,
} from "./playback";
import { speakableText, speechSegments } from "./speakableText";
import { canRetry, errorCode } from "./voiceMessages";

/** VOM-FR-DGOG. */
export const SHORT_PRESS_MS = 300;
/** VOM-FR-FTFU. */
export const TAIL_MS = 200;
/** VOM-FR-WSXW. */
export const RECORDING_LIMIT_MS = 10 * 60 * 1000;
/** `ADQ-FR-YQTB`: the identity of a question comment ends so. */
const QUESTION_COMMENT = /:\d{2}-1q$/;

export type PostOutcome = "posted" | "reached_nobody" | "busy" | "failed";

/** What a mounted composer lends to voice mode. */
export interface VoiceComposerHandle {
  /** VOM-FR-RFWC: post the session body, then this text. */
  post(spoken: string): Promise<PostOutcome>;
  /** VOM-FR-TAQJ. Gives the text with its spoken address as a tag. */
  tagged(transcript: string): string;
  /** VOM-FR-ZUWW. */
  blocked(): boolean;
  /** VOM-FR-LNUH. */
  nicknames(): string[];
  /** VOM-FR-UALP. */
  voiceOf(agentId: string): string | null;
}

interface Capture {
  key: string;
  mic: Microphone | null;
  pump: ChunkPump;
  sessionId: string | null;
  pressedAt: number;
  released: boolean;
  discard: boolean;
  /** VOM-FR-ABPU. */
  keepInBody: boolean;
  limit: ReturnType<typeof setTimeout> | null;
}

let behaviour: VoiceHotkeyBehaviour = "hold";
let capture: Capture | null = null;
/**
 * A transcript on its way from the backend. It outlives `capture`, because the
 * author can leave voice mode while it comes.
 */
let awaited: { key: string; sessionId: string; keepInBody: boolean; discard: boolean } | null =
  null;
/** VOM-FR-HUXY: the session whose text shows. It outlives the release. */
let liveSession: string | null = null;
/** VOM-FR-WICV: the session that holds a failed recording. */
let heldSession: { id: string; key: string } | null = null;
let baseline = new Set<string>();
let currentSpeech: string | null = null;
let segmentsSent = false;
let speechCounter = 0;
const composers = new Map<string, VoiceComposerHandle[]>();

export function setVoiceBehaviour(next: VoiceHotkeyBehaviour): void {
  behaviour = next;
}

function handleOf(key: string | null): VoiceComposerHandle | null {
  const mounted = key === null ? undefined : composers.get(key);
  return mounted?.[mounted.length - 1] ?? null;
}

/** VOM-FR-FHYY: a composer of the voice mode is on screen. */
export function voiceComposerMounted(): boolean {
  return handleOf(getVoiceState().activeKey) !== null;
}

function clearFeedback() {
  patchVoiceState({ error: null, canRetry: false, note: null });
}

function dropHeldRecording(): void {
  if (heldSession) {
    void cancelVoiceCapture(heldSession.id).catch(() => undefined);
    heldSession = null;
  }
}

// --- Spoken replies -------------------------------------------------------

/** VOM-FR-QDWW: silence at once. */
export function stopSpeech(): void {
  if (currentSpeech) {
    void cancelVoiceSpeech(currentSpeech).catch(() => undefined);
    currentSpeech = null;
  }
  stopPlayback();
  segmentsSent = false;
  if (getVoiceState().playing) patchVoiceState({ playing: false });
}

function drained(): void {
  if (segmentsSent) {
    currentSpeech = null;
    segmentsSent = false;
    patchVoiceState({ playing: false });
  }
}

async function speak(comment: Comment, key: string | null): Promise<void> {
  stopSpeech();
  const text = speakableText(comment.body);
  if (text === "") return;
  speechCounter += 1;
  const id = `speech-${speechCounter}`;
  currentSpeech = id;
  patchVoiceState({ playing: true });
  setDrainedListener(drained);
  const author = comment.author;
  const voiceName = author.kind === "agent" ? (handleOf(key)?.voiceOf(author.agentId) ?? null) : null;
  try {
    for (const segment of speechSegments(text)) {
      if (currentSpeech !== id) return;
      // A frame can still be in the channel after Stop.
      await speakVoiceSegment(id, segment, voiceName, (frame) => {
        if (currentSpeech === id) playFrame(frame);
      });
    }
    if (currentSpeech === id) {
      segmentsSent = true;
      logDebug(["frontend", "ai"], "a voice reply was sent to playback", {});
      // A reply with no frames, or one that already played, gets no later event.
      if (isPlaybackIdle()) drained();
    }
  } catch (error) {
    if (currentSpeech !== id) return;
    // VOM-FR-QPFQ: the text of the comment stays as it is.
    const code = errorCode(error);
    logWarn(["frontend", "ai"], "a voice reply failed", { code });
    stopSpeech();
    patchVoiceState({ error: code, canRetry: false });
  }
}

/**
 * VOM-FR-ZLAY, VOM-FR-ILJW, VOM-FR-VSCQ, VOM-FR-RKOG,
 * VOM-FR-EGNX: a discussion changed. Each new comment joins the baseline, so
 * a switch to response mode Voice never speaks an old comment.
 */
export function noteDiscussionChanged(discussion: Discussion): void {
  const state = getVoiceState();
  if (state.activeKey === null || discussion.id !== state.discussionId) return;
  const fresh = discussion.comments.filter((c) => !baseline.has(c.id));
  fresh.forEach((c) => baseline.add(c.id));
  if (state.responseMode !== "voice" || handleOf(state.activeKey) === null) return;
  const spoken = fresh.filter(
    (c) => c.author.kind === "agent" && !QUESTION_COMMENT.test(c.id),
  );
  const newest = spoken[spoken.length - 1];
  if (newest) void speak(newest, state.activeKey);
}

// --- Entering and leaving -------------------------------------------------

/** VOM-FR-OSTM, VOM-FR-ILJW. */
export function enterVoiceMode(key: string, discussion: Discussion | null): void {
  if (getVoiceState().activeKey !== null) leaveVoiceMode();
  prepareAudio();
  baseline = new Set(discussion?.comments.map((c) => c.id) ?? []);
  patchVoiceState({
    activeKey: key,
    discussionId: discussion?.id ?? null,
    transcript: "",
    error: null,
    canRetry: false,
    note: null,
  });
  logInfo(["frontend"], "voice mode entered", { key });
}

/** VOM-FR-JBJI. */
export function leaveVoiceMode(): void {
  stopSpeech();
  dropHeldRecording();
  keepWhatIsComing();
  patchVoiceState({
    activeKey: null,
    discussionId: null,
    responseMode: "text",
    transcript: "",
    error: null,
    canRetry: false,
    note: null,
  });
}

/** VOM-FR-ILJW: the first post of an opening composer created a discussion. */
export function bindVoiceToDiscussion(key: string, discussion: Discussion): void {
  if (getVoiceState().activeKey !== key) return;
  baseline = new Set(discussion.comments.map((c) => c.id));
  patchVoiceState({ activeKey: discussion.id, discussionId: discussion.id });
}

export function setResponseMode(mode: VoiceResponseMode): void {
  if (mode === "text") stopSpeech();
  patchVoiceState({ responseMode: mode });
}

/** VOM-FR-EGNX, VOM-FR-ABPU: the last surface of the composer went. */
function surfaceGone(): void {
  stopSpeech();
  keepWhatIsComing();
}

/** A recording or a transcript on its way stays as unsent text. */
function keepWhatIsComing(): void {
  if (capture) {
    capture.keepInBody = true;
    endRecording();
  }
  if (awaited) awaited.keepInBody = true;
}

export function registerVoiceComposer(key: string, handle: VoiceComposerHandle): () => void {
  composers.set(key, [...(composers.get(key) ?? []), handle]);
  return () => {
    const left = (composers.get(key) ?? []).filter((h) => h !== handle);
    if (left.length > 0) composers.set(key, left);
    else composers.delete(key);
    if (left.length === 0 && getVoiceState().activeKey === key) surfaceGone();
  };
}

// --- Recording ------------------------------------------------------------

function joinToBody(key: string, text: string): void {
  const body = getDiscussionSession(key).body;
  setComposerBody(key, body.trim() === "" ? text : `${body.trimEnd()}\n${text}`);
}

/** VOM-FR-WYIE, VOM-FR-RENV, VOM-FR-ABPU, VOM-FR-BVNX. */
async function deliver(key: string, text: string, keepInBody: boolean): Promise<void> {
  const spoken = text.trim();
  const finished = { phase: "idle" as const, recordingSince: null };
  if (spoken === "") {
    patchVoiceState({ ...finished, transcript: "", note: "no_speech" });
    return;
  }
  const handle = handleOf(key);
  if (keepInBody || !handle || handle.blocked()) {
    joinToBody(key, spoken);
    patchVoiceState({ ...finished, transcript: "", note: "kept_in_body" });
    return;
  }
  patchVoiceState({ phase: "posting", transcript: spoken });
  const outcome = await handle.post(handle.tagged(spoken));
  if (outcome === "busy" || outcome === "failed") {
    joinToBody(key, spoken);
    patchVoiceState({ ...finished, transcript: "", note: "kept_in_body" });
    return;
  }
  patchVoiceState({
    ...finished,
    transcript: "",
    note: outcome === "reached_nobody" ? "no_agent" : null,
  });
}

/** A transcript failed. With the composer gone, no Retry can come. */
function failed(error: unknown, job: NonNullable<typeof awaited>): void {
  const code = errorCode(error);
  logWarn(["frontend", "ai"], "a voice recording failed", { code });
  if (job.discard || getVoiceState().activeKey !== job.key) {
    void cancelVoiceCapture(job.sessionId).catch(() => undefined);
    if (!job.discard) patchVoiceState({ phase: "idle", recordingSince: null, transcript: "" });
    return;
  }
  const retryable = canRetry(code);
  heldSession = retryable ? { id: job.sessionId, key: job.key } : null;
  patchVoiceState({
    phase: "idle",
    recordingSince: null,
    transcript: "",
    error: code,
    canRetry: retryable,
  });
}

/** Wait for the text of one session, then hand it on. */
async function receive(job: NonNullable<typeof awaited>, text: Promise<string>): Promise<void> {
  awaited = job;
  let transcript: string;
  try {
    transcript = await text;
  } catch (error) {
    awaited = null;
    liveSession = null;
    failed(error, job);
    return;
  }
  awaited = null;
  liveSession = null;
  // The session stores went with the teardown.
  if (job.discard) return;
  await deliver(job.key, transcript, job.keepInBody);
}

async function completeRecording(active: Capture): Promise<void> {
  capture = null;
  const sessionId = active.sessionId;
  active.mic?.stop();
  if (active.limit) clearTimeout(active.limit);
  if (sessionId === null) {
    patchVoiceState({ phase: "idle", recordingSince: null });
    return;
  }
  if (active.discard) {
    liveSession = null;
    void cancelVoiceCapture(sessionId).catch(() => undefined);
    patchVoiceState({ phase: "idle", recordingSince: null, transcript: "" });
    return;
  }
  await active.pump.flush();
  patchVoiceState({ phase: "transcribing" });
  await receive(
    { key: active.key, sessionId, keepInBody: active.keepInBody, discard: false },
    finishVoiceCapture(sessionId),
  );
}

/** VOM-FR-FTFU: the release, a short tail, and then the end of input. */
function endRecording(): void {
  const active = capture;
  if (!active || active.released) return;
  active.released = true;
  if (active.sessionId === null) return; // The start path sees `released`.
  if (active.discard) {
    void completeRecording(active);
    return;
  }
  setTimeout(() => void completeRecording(active), TAIL_MS);
}

async function beginRecording(key: string, handle: VoiceComposerHandle): Promise<void> {
  const pump = new ChunkPump(appendVoiceCapture);
  const active: Capture = {
    key,
    mic: null,
    pump,
    sessionId: null,
    pressedAt: Date.now(),
    released: false,
    discard: false,
    keepInBody: false,
    limit: null,
  };
  capture = active;
  patchVoiceState({ phase: "opening" });
  try {
    active.mic = await openMicrophone((pcm) => pump.push(pcm));
    // Nobody waits for this recording now: a short press, or a teardown.
    if (active.discard) {
      active.mic.stop();
      capture = null;
      patchVoiceState({ phase: "idle", recordingSince: null });
      return;
    }
    const start = await startVoiceCapture(active.mic.rate, handle.nicknames());
    active.sessionId = start.sessionId;
    liveSession = start.sessionId;
    pump.attach(start.sessionId);
    patchVoiceState({ phase: "recording", live: start.live, recordingSince: Date.now() });
    active.limit = setTimeout(endRecording, RECORDING_LIMIT_MS);
    logDebug(["frontend", "ai"], "a voice recording started", { live: start.live });
  } catch (error) {
    active.mic?.stop();
    capture = null;
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

/** VOM-FR-HHWL, VOM-FR-LIFL, VOM-FR-VVTY, VOM-FR-FASJ. */
export function pressRecord(): void {
  const state = getVoiceState();
  const handle = handleOf(state.activeKey);
  if (!handle || state.activeKey === null || handle.blocked()) return;
  if (capture) {
    if (behaviour === "toggle") endRecording();
    return;
  }
  if (state.phase !== "idle") return;
  stopSpeech();
  dropHeldRecording();
  clearFeedback();
  patchVoiceState({ transcript: "" });
  void beginRecording(state.activeKey, handle);
}

/** VOM-FR-HHWL, VOM-FR-DGOG, VOM-FR-XQUY: the end of a hold. */
export function releaseRecord(): void {
  if (behaviour !== "hold" || !capture) return;
  if (Date.now() - capture.pressedAt < SHORT_PRESS_MS) capture.discard = true;
  endRecording();
}

/** VOM-FR-WICV. */
export async function retryRecording(): Promise<void> {
  if (!heldSession) return;
  const { id, key } = heldSession;
  heldSession = null;
  patchVoiceState({ phase: "transcribing", error: null, canRetry: false });
  await receive({ key, sessionId: id, keepInBody: false, discard: false }, retryVoiceCapture(id));
}

/** VOM-FR-HUXY. */
export function noteTranscript(sessionId: string, text: string): void {
  if (liveSession === sessionId) patchVoiceState({ transcript: text });
}

/** VOM-FR-WMAP. */
export function noteProvider(ready: boolean, speaks: boolean): void {
  const change: Partial<ReturnType<typeof getVoiceState>> = {
    providerReady: ready,
    providerSpeaks: speaks,
  };
  if (!speaks && getVoiceState().responseMode === "voice") {
    stopSpeech();
    change.responseMode = "text";
  }
  patchVoiceState(change);
}

/** VOM-FR-KQAI: the session stores go, so nothing is kept. */
export function endVoiceForTeardown(): void {
  stopSpeech();
  if (capture) {
    capture.discard = true;
    endRecording();
  }
  if (awaited) awaited.discard = true;
  dropHeldRecording();
  const { providerReady, providerSpeaks } = getVoiceState();
  patchVoiceState({ ...INITIAL_VOICE_STATE, providerReady, providerSpeaks });
}

export function resetVoiceControllerForTest(): void {
  behaviour = "hold";
  capture = null;
  awaited = null;
  liveSession = null;
  heldSession = null;
  baseline = new Set();
  currentSpeech = null;
  segmentsSent = false;
  speechCounter = 0;
  composers.clear();
}
