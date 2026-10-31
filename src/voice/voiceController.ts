/**
 * Voice mode as a process (`VOM-voice-mode.md`): the press, the recording, the
 * VAD session, the post, and the spoken reply.
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
  subscribeVoiceState,
  type VoiceInputMode,
  type VoiceResponseMode,
  type VoiceState,
} from "../state/voiceSession";
import type {
  Comment,
  Discussion,
  VoiceActivityChange,
  VoiceHotkeyBehaviour,
} from "../types";
import { ChunkPump, MicrophoneError, openMicrophone, type Microphone } from "./capture";
import {
  isPlaybackIdle,
  pausePlayback,
  playFrame,
  prepareAudio,
  resumePlayback,
  setDrainedListener,
  stopPlayback,
} from "./playback";
import { speakableText, speechSegments } from "./speakableText";
import { VadSession, type VadGate } from "./vadSession";
import { canRetry, errorCode } from "./voiceMessages";

/** VOM-FR-DGOG. */
export const SHORT_PRESS_MS = 300;
/** VOM-FR-FTFU. */
export const TAIL_MS = 200;
/** VOM-FR-WSXW. */
export const RECORDING_LIMIT_MS = 10 * 60 * 1000;
/** VOM-FR-GZPA. */
export const SURFACE_GRACE_MS = 2000;
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

/** One transcript on its way. Flags read after the wait, so they can still change. */
interface Job {
  key: string;
  sessionId: string;
  keepInBody: boolean;
  discard: boolean;
}

/** VOM-FR-WRVR: an utterance from `speech_confirmed` until it settles. */
interface Utterance extends Job {
  /** "finish voice capture" runs. A capture never finishes twice. */
  finishing: boolean;
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
let awaited: Job | null = null;
/** VOM-FR-HUXY: the session whose text shows. It outlives the release. */
let liveSession: string | null = null;
/** VOM-FR-WICV: the session that holds a failed recording. */
let heldSession: { id: string; key: string } | null = null;
let baseline = new Set<string>();
let currentSpeech: string | null = null;
let segmentsSent = false;
let speechCounter = 0;
const composers = new Map<string, VoiceComposerHandle[]>();
/** VOM-FR-PDZR: the open session. */
let vad: VadSession | null = null;
/** A session that closes. Its events still count until the close comes back. */
let closingVad: VadSession | null = null;
let vadClosed: Promise<void> | null = null;
/** A session that opens. The author can close it before it is open. */
let vadOpening: { cancelled: boolean } | null = null;
/** The key of the session, which outlives `activeKey` when voice mode ends. */
let vadKey: string | null = null;
let stopGateWatch: (() => void) | null = null;
let surfaceTimer: ReturnType<typeof setTimeout> | null = null;
let utterance: Utterance | null = null;
/** VOM-FR-ZPOD: the newest reply that waits for the author. */
let pendingReply: { comment: Comment; key: string | null } | null = null;

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

/** The reply that sounds or stands paused goes. A reply that waits stays. */
function dropReply(): void {
  if (currentSpeech) {
    void cancelVoiceSpeech(currentSpeech).catch(() => undefined);
    currentSpeech = null;
  }
  stopPlayback();
  segmentsSent = false;
  const { playing, playbackPaused } = getVoiceState();
  if (playing || playbackPaused) patchVoiceState({ playing: false, playbackPaused: false });
}

/** VOM-FR-QDWW: silence at once, and no reply is left to speak. */
export function stopSpeech(): void {
  pendingReply = null;
  dropReply();
}

function drained(): void {
  if (segmentsSent) {
    currentSpeech = null;
    segmentsSent = false;
    patchVoiceState({ playing: false, playbackPaused: false });
  }
}

async function speak(comment: Comment, key: string | null): Promise<void> {
  dropReply();
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
    dropReply();
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
  if (!newest) return;
  // VOM-FR-ZPOD.
  if (state.hearing || utterance !== null) {
    pendingReply = { comment: newest, key: state.activeKey };
    return;
  }
  void speak(newest, state.activeKey);
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
    inputMode: "ptt",
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
    inputMode: "ptt",
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
  // What is on its way follows the composer, or it posts through the opening
  // composer a second time.
  if (capture?.key === key) capture.key = discussion.id;
  if (awaited?.key === key) awaited.key = discussion.id;
  if (heldSession?.key === key) heldSession.key = discussion.id;
  if (utterance?.key === key) utterance.key = discussion.id;
  if (pendingReply?.key === key) pendingReply.key = discussion.id;
  if (vadKey === key) vadKey = discussion.id;
  patchVoiceState({ activeKey: discussion.id, discussionId: discussion.id });
  checkSurfaceSoon();
}

export function setResponseMode(mode: VoiceResponseMode): void {
  if (mode === "text") stopSpeech();
  patchVoiceState({ responseMode: mode });
}

/** VOM-FR-EGNX, VOM-FR-ABPU: the last surface of the composer went. */
function surfaceGone(): void {
  stopSpeech();
  // VOM-FR-GZPA: a session waits to see whether another surface mounts.
  if (vad) checkSurfaceSoon();
  else keepWhatIsComing();
}

/** A recording or a transcript on its way stays as unsent text. */
function keepWhatIsComing(): void {
  if (capture) {
    capture.keepInBody = true;
    endRecording();
  }
  if (awaited) awaited.keepInBody = true;
  if (utterance) utterance.keepInBody = true;
  void endVadSession("release");
}

export function registerVoiceComposer(key: string, handle: VoiceComposerHandle): () => void {
  composers.set(key, [...(composers.get(key) ?? []), handle]);
  noteComposerChanged();
  return () => {
    const left = (composers.get(key) ?? []).filter((h) => h !== handle);
    if (left.length > 0) composers.set(key, left);
    else composers.delete(key);
    if (left.length === 0 && getVoiceState().activeKey === key) surfaceGone();
    noteComposerChanged();
  };
}

/** VOP-FR-QZCE: the composer changed what a session may do. */
export function noteComposerChanged(): void {
  if (surfaceTimer !== null && handleOf(getVoiceState().activeKey) !== null) clearSurfaceCheck();
  vad?.pushGate();
}

// --- Recording ------------------------------------------------------------

function joinToBody(key: string, text: string): void {
  const body = getDiscussionSession(key).body;
  setComposerBody(key, body.trim() === "" ? text : `${body.trimEnd()}\n${text}`);
}

/**
 * VOM-FR-WYIE, VOM-FR-RENV, VOM-FR-ABPU, VOM-FR-BVNX. Gives whether a comment
 * posted, which a comment that reached no agent also did.
 */
async function deliver(key: string, text: string, keepInBody: boolean): Promise<boolean> {
  const spoken = text.trim();
  const finished = { phase: "idle" as const, recordingSince: null };
  if (spoken === "") {
    patchVoiceState({ ...finished, transcript: "", note: "no_speech" });
    return false;
  }
  const handle = handleOf(key);
  if (keepInBody || !handle || handle.blocked()) {
    joinToBody(key, spoken);
    patchVoiceState({ ...finished, transcript: "", note: "kept_in_body" });
    return false;
  }
  patchVoiceState({ phase: "posting", transcript: spoken });
  let outcome: PostOutcome = "failed";
  try {
    outcome = await handle.post(handle.tagged(spoken));
  } catch (error) {
    logWarn(["frontend", "ai"], "a voice transcript did not post", { code: errorCode(error) });
  }
  if (outcome === "busy" || outcome === "failed") {
    joinToBody(key, spoken);
    patchVoiceState({ ...finished, transcript: "", note: "kept_in_body" });
    return false;
  }
  patchVoiceState({
    ...finished,
    transcript: "",
    note: outcome === "reached_nobody" ? "no_agent" : null,
  });
  return true;
}

/** A transcript failed. With the composer gone, no Retry can come. */
function failed(error: unknown, job: Job): void {
  const code = errorCode(error);
  logWarn(["frontend", "ai"], "a voice recording failed", { code });
  // A Retry lost its held audio to a new utterance, and that utterance stands.
  if (utterance !== null && utterance !== job) return;
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

/** Wait for the text of one session, then hand it on. Gives whether it posted. */
async function receive(job: Job, text: Promise<string>): Promise<boolean> {
  awaited = job;
  let transcript: string;
  try {
    transcript = await text;
  } catch (error) {
    awaited = null;
    if (liveSession === job.sessionId) liveSession = null;
    failed(error, job);
    return false;
  }
  awaited = null;
  if (liveSession === job.sessionId) liveSession = null;
  // The session stores went with the teardown.
  if (job.discard) return false;
  return deliver(job.key, transcript, job.keepInBody);
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

// --- VAD sessions ---------------------------------------------------------

/** VOM-FR-GTAQ, VOP-FR-QZCE: what the detector may do now. */
function vadGate(): VadGate {
  const state = getVoiceState();
  const handle = handleOf(state.activeKey);
  return {
    blocked:
      handle === null || handle.blocked() || state.phase !== "idle" || !state.providerReady,
    playing: state.playing && !state.playbackPaused,
    keyterms: handle?.nicknames() ?? [],
  };
}

/** VOM-FR-CSXI. */
function vadLost(code: string): void {
  logWarn(["frontend", "ai"], "a voice session lost its audio", { code });
  patchVoiceState({ error: code, canRetry: false });
  void endVadSession("release");
}

function clearSurfaceCheck(): void {
  if (surfaceTimer !== null) clearTimeout(surfaceTimer);
  surfaceTimer = null;
}

/** VOM-FR-GZPA: an owner mounts its composer after the old one unmounted. */
function checkSurfaceSoon(): void {
  clearSurfaceCheck();
  if (!vad) return;
  surfaceTimer = setTimeout(() => {
    surfaceTimer = null;
    if (handleOf(getVoiceState().activeKey) === null) void endVadSession("release");
  }, SURFACE_GRACE_MS);
}

async function openVad(key: string, handle: VoiceComposerHandle): Promise<void> {
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
    const session = await VadSession.open(handle.nicknames(), { gate: vadGate, lost: vadLost });
    vadOpening = null;
    if (opening.cancelled) {
      session.stopMicrophone();
      void session.close(true).catch(() => undefined);
      patchVoiceState({ phase: "idle" });
      return;
    }
    vad = session;
    vadKey = key;
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

/** VOM-FR-XEIA: the tail, the last audio, then the close. */
async function closeVad(session: VadSession, kind: "release" | "discard"): Promise<void> {
  const job = utterance;
  if (kind === "release" && job !== null && !job.finishing) {
    await new Promise((done) => setTimeout(done, TAIL_MS));
  }
  session.stopMicrophone();
  // A capture that finishes stays, because a failure leaves it for Retry. A
  // teardown during the tail discards too (VOM-FR-KQAI).
  const discard = kind === "discard" || job === null || job.discard;
  let open: string | null = null;
  try {
    open = await session.close(discard);
  } catch (error) {
    logWarn(["frontend", "ai"], "a voice session did not close", { code: errorCode(error) });
    // The capture is still open, and it holds the speech.
    if (!discard) open = job?.sessionId ?? null;
  }
  // The utterance can end by itself during the tail.
  if (job?.finishing) return;
  if (job !== null && open === job.sessionId) {
    await finishUtterance(job);
    return;
  }
  if (job !== null) {
    liveSession = null;
    patchVoiceState({ transcript: "" });
  }
  settleUtterance(job, false);
}

async function endVadSession(kind: "release" | "discard"): Promise<void> {
  // Each exit passes here, and a session that still opens has none of its own.
  if (vadOpening) vadOpening.cancelled = true;
  if (kind === "discard") closingVad?.stopMicrophone();
  const session = vad;
  if (!session) return;
  vad = null;
  closingVad = session;
  clearSurfaceCheck();
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

/**
 * VOM-FR-UNXR, VOM-FR-ZPOD: the one exit of each utterance, on every path. The
 * detector waits for it (VOM-FR-KHGC), and so does a paused reply.
 */
function settleUtterance(job: Utterance | null, posted: boolean): void {
  // With no utterance, the phase belongs to something else, such as a Retry.
  if (job) {
    utterance = null;
    patchVoiceState({ phase: "idle", hearing: false, recordingSince: null });
    vad?.settle(job.sessionId);
  } else {
    patchVoiceState({ hearing: false });
  }
  const waiting = pendingReply;
  pendingReply = null;
  if (waiting) {
    void speak(waiting.comment, waiting.key);
  } else if (getVoiceState().playbackPaused) {
    if (posted) {
      dropReply();
    } else {
      resumePlayback();
      patchVoiceState({ playbackPaused: false });
    }
  }
}

/** VOM-FR-OTRF. */
async function finishUtterance(job: Utterance): Promise<void> {
  if (job.finishing) return;
  job.finishing = true;
  patchVoiceState({ phase: "transcribing", hearing: false });
  let posted = false;
  try {
    posted = await receive(job, finishVoiceCapture(job.sessionId));
  } catch (error) {
    logWarn(["frontend", "ai"], "a voice utterance did not post", { code: errorCode(error) });
  }
  settleUtterance(job, posted);
}

/** VOM-FR-WRVR, VOM-FR-FASJ. */
function utteranceConfirmed(change: VoiceActivityChange): void {
  const key = getVoiceState().activeKey ?? vadKey;
  if (change.captureId === null || key === null) return;
  dropHeldRecording();
  clearFeedback();
  utterance = {
    key,
    sessionId: change.captureId,
    keepInBody: false,
    discard: false,
    finishing: false,
  };
  liveSession = change.captureId;
  patchVoiceState({
    phase: "recording",
    live: change.live ?? false,
    transcript: "",
    recordingSince: Date.now(),
  });
}

/** VOM-FR-OTRF: a capture that nobody here knows would hold the detector. */
function utteranceEnded(change: VoiceActivityChange): void {
  if (utterance !== null && utterance.sessionId === change.captureId) {
    void finishUtterance(utterance);
  } else if (change.captureId !== null) {
    void cancelVoiceCapture(change.captureId).catch(() => undefined);
    vad?.settle(change.captureId);
    settleUtterance(null, false);
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
  if (utterance !== null) {
    liveSession = null;
    patchVoiceState({ transcript: "" });
  }
  patchVoiceState({ error: code, canRetry: false });
  settleUtterance(utterance, false);
}

/** VOM-FR-NZXN: each step of an utterance, as the backend finds it. */
export function noteActivity(change: VoiceActivityChange): void {
  // A close travels apart from the events, so an event can come after it.
  if ((vad ?? closingVad)?.activityId !== change.activityId) return;
  if (vad === null) {
    if (change.state === "speech_end" && utterance) void finishUtterance(utterance);
    return;
  }
  switch (change.state) {
    case "speech_start":
      patchVoiceState({ hearing: true });
      // VOM-FR-FVRS.
      if (getVoiceState().playing && !getVoiceState().playbackPaused) {
        pausePlayback();
        patchVoiceState({ playbackPaused: true });
      }
      break;
    case "speech_confirmed":
      utteranceConfirmed(change);
      break;
    case "speech_end":
      utteranceEnded(change);
      break;
    case "misfire":
      // VOM-FR-XWPZ, VOM-FR-KVHG.
      settleUtterance(null, false);
      break;
    case "failed":
      activityFailed(change);
      break;
  }
}

function pressVad(state: VoiceState): void {
  if (vad) {
    void endVadSession("release");
    return;
  }
  if (vadOpening) {
    vadOpening.cancelled = true;
    return;
  }
  const handle = handleOf(state.activeKey);
  if (!handle || state.activeKey === null || handle.blocked() || state.phase !== "idle") return;
  stopSpeech();
  dropHeldRecording();
  clearFeedback();
  patchVoiceState({ transcript: "" });
  void openVad(state.activeKey, handle);
}

/** VOM-FR-IKIK, VOM-FR-VGCE, VOM-FR-BQBI. */
export function setInputMode(mode: VoiceInputMode): void {
  const state = getVoiceState();
  if (mode === state.inputMode) return;
  if (mode === "vad" && state.phase !== "idle") return;
  patchVoiceState({ inputMode: mode });
  if (mode === "ptt") void endVadSession("release");
}

/** VOM-FR-HHWL, VOM-FR-LIFL, VOM-FR-VVTY, VOM-FR-FASJ, VOM-FR-PDZR. */
export function pressRecord(): void {
  const state = getVoiceState();
  if (state.inputMode === "vad") {
    pressVad(state);
    return;
  }
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
  // VOP-FR-KBPZ: an utterance replaces the held recording, so the two cannot run.
  if (!heldSession || getVoiceState().hearing || utterance !== null) return;
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
  if (!ready) void endVadSession("release");
}

/** VOM-FR-KQAI: the session stores go, so nothing is kept. */
export function endVoiceForTeardown(): void {
  stopSpeech();
  if (capture) {
    capture.discard = true;
    endRecording();
  }
  if (awaited) awaited.discard = true;
  if (utterance) utterance.discard = true;
  void endVadSession("discard");
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
  vad = null;
  closingVad = null;
  vadClosed = null;
  vadOpening = null;
  vadKey = null;
  stopGateWatch?.();
  stopGateWatch = null;
  clearSurfaceCheck();
  utterance = null;
  pendingReply = null;
}
