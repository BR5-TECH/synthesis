/**
 * The utterances of voice mode (`VOM-voice-mode.md` VOM-FR-GBCN, VOM-FR-QCZQ):
 * from the start of a recording, or `speech_confirmed`, until the utterance
 * settles.
 *
 * Listening does not wait for transcription. Each utterance transcribes in the
 * background, and the comments post in the order of the utterances. A chunked
 * utterance shows a live bubble (VOM-FR-NCHC) and gives its chunks to the agent
 * as a speech turn while the author still speaks (VOM-FR-RNBS, VOM-FR-OLFT).
 */
import {
  cancelVoiceCapture,
  dispatchSpeechTurn,
  endSpeechTurn,
  steerSpeechTurn,
} from "../api";
import { logDebug, logInfo, logWarn } from "../logging";
import { getDiscussionSession, setComposerBody, upsertDiscussionTurn } from "../state/discussionSession";
import { originFor } from "../state/discussionOrigin";
import {
  dropLivePart,
  noteLiveChunk,
  noteLiveChunkFailed,
  noteLiveText,
  openLivePart,
} from "../state/liveBubble";
import { beginSpeechPost, finishSpeechPost } from "../state/speechPosts";
import { getVoiceState, patchVoiceState } from "../state/voiceSession";
import type { VoiceChunkChange } from "../types";
import { postingHandleOf, type PostOutcome } from "./voiceHandles";
import { canRetry, errorCode } from "./voiceMessages";
import { questionBlockOf, type AnswerTarget } from "./voiceQuestions";
import { afterSettle, latestVoiceDiscussion, noteAnswered } from "./voiceReplies";

/** VOM-FR-QCZQ: one utterance until it settles. Flags read late, so they can change. */
export interface Utterance {
  key: string;
  captureId: string;
  /** VOM-FR-GBCN: chunked, so it has a live bubble and a speech turn. */
  chunked: boolean;
  /** VOM-FR-PINR: the question this utterance answers. */
  answers: AnswerTarget | null;
  /** VOM-FR-ABPU. */
  keepInBody: boolean;
  /** VOM-FR-KQAI, VOM-FR-UXZT. */
  discard: boolean;
  /** "finish voice capture" runs. A capture never finishes twice. */
  finishing: boolean;
  /** Chunks that ended and wait for a transcript. */
  pendingChunks: number;
  /** VOM-FR-RNBS: "dispatch speech turn" was called. */
  speechStarted: boolean;
  /** VOM-FR-MVLA, VOM-FR-REJN: "end speech turn" was called. */
  speechEnded: boolean;
  /** VOM-FR-JBJI: voice mode ended, so the speech reaches no agent now. */
  detached: boolean;
  /** CTA-FR-QWLB: the agents the speech reached. */
  reached: string[];
  /** The speech turn calls, in order. */
  speechCalls: Promise<unknown>;
  /** Resolves when each earlier utterance settled (VOM-FR-QCZQ). */
  previous: Promise<void>;
  settled: boolean;
  done: () => void;
}

const open: Utterance[] = [];
let lastDone: Promise<void> = Promise.resolve();
/** VOM-FR-WICV: the capture that holds a failed recording. */
let held: { id: string; key: string; answers: AnswerTarget | null } | null = null;
/** A Retry in progress. */
let retrying = 0;

/** VOM-FR-ZPOD: an utterance that has not settled. */
export function hasUnsettledUtterance(): boolean {
  return open.length > 0;
}

export function utteranceOf(captureId: string | null): Utterance | null {
  return open.find((u) => u.captureId === captureId) ?? null;
}

export function openUtterances(): readonly Utterance[] {
  return open;
}

/** VOM-FR-VOUG, VOM-FR-QTAQ: what transcribes in the background now. */
function syncTranscribing(): void {
  const count =
    retrying + open.reduce((n, u) => n + u.pendingChunks + (u.finishing ? 1 : 0), 0);
  if (getVoiceState().transcribing !== count) patchVoiceState({ transcribing: count });
}

/** VOM-FR-PINR: an open utterance answers this question already. */
export function answerTaken(target: AnswerTarget): boolean {
  return open.some(
    (u) =>
      !u.discard &&
      u.answers?.setId === target.setId &&
      u.answers.position === target.position,
  );
}

/** VOM-FR-NERG: chunks need a discussion as the origin of a speech turn. */
export function chunkedFor(key: string, answers: AnswerTarget | null): boolean {
  const { discussionId } = getVoiceState();
  return answers === null && discussionId !== null && key === discussionId;
}

/** A recording or a VAD utterance holds a capture now. */
export function beginUtterance(
  key: string,
  captureId: string,
  chunked: boolean,
  answers: AnswerTarget | null,
): Utterance {
  let done = () => undefined as void;
  const settledHere = new Promise<void>((resolve) => {
    done = resolve;
  });
  const utterance: Utterance = {
    key,
    captureId,
    chunked,
    answers,
    keepInBody: false,
    discard: false,
    finishing: false,
    pendingChunks: 0,
    speechStarted: false,
    speechEnded: false,
    detached: false,
    reached: [],
    speechCalls: Promise.resolve(),
    previous: lastDone,
    settled: false,
    done,
  };
  lastDone = settledHere;
  open.push(utterance);
  if (chunked) openLivePart(key, captureId);
  return utterance;
}

// --- Speech turns ---------------------------------------------------------

function chainSpeech(u: Utterance, call: () => Promise<unknown>): void {
  u.speechCalls = u.speechCalls.then(call).catch((error: unknown) => {
    logWarn(["frontend", "ai"], "a speech turn call was refused", { code: errorCode(error) });
  });
}

/** VOM-FR-RNBS: the first chunk starts the speech turn of each agent it reaches. */
function startSpeech(u: Utterance, text: string): void {
  const handle = postingHandleOf(u.key);
  const discussion = latestVoiceDiscussion();
  if (!handle?.targets || discussion === null || discussion.id !== u.key) return;
  u.speechStarted = true;
  const tagged = handle.tagged(text);
  const targets = handle.targets(tagged);
  u.reached = targets;
  const origin = originFor(discussion);
  logInfo(["frontend", "ai"], "a speech turn starts", { agents: targets.length });
  chainSpeech(u, () =>
    Promise.all(
      targets.map((nickname) =>
        dispatchSpeechTurn({ nickname, origin, speechId: u.captureId, text: tagged })
          .then((turn) => upsertDiscussionTurn(u.key, turn))
          .catch((error: unknown) => {
            // The speech did not reach this agent, so the comment at the end
            // of the utterance reaches it as a typed comment does.
            u.reached = u.reached.filter((n) => n !== nickname);
            logWarn(["frontend", "ai"], "a speech turn was not dispatched, so the comment reaches the agent", {
              nickname,
              code: errorCode(error),
            });
          }),
      ),
    ),
  );
}

/** VOM-FR-OLFT: a later chunk steers the turns of the speech. */
function steerSpeech(u: Utterance, text: string): void {
  chainSpeech(u, () => steerSpeechTurn(u.captureId, text));
}

/** VOM-FR-MVLA, VOM-FR-REJN. */
function endSpeech(u: Utterance, commentId: string | null): void {
  if (!u.speechStarted || u.speechEnded) return;
  u.speechEnded = true;
  logDebug(["frontend", "ai"], "a speech turn ends", { withComment: commentId !== null });
  chainSpeech(u, () => endSpeechTurn(u.captureId, commentId));
}

// --- Chunks -----------------------------------------------------------------

/** VOM-FR-NZXN, VOM-FR-NCHC, VOM-FR-MBHB: one chunk event of a capture held here. */
export function noteChunk(change: VoiceChunkChange): void {
  const u = utteranceOf(change.captureId);
  if (u === null || !u.chunked || u.discard) return;
  if (u.detached) {
    // VOM-FR-JBJI: a late chunk steers nothing and shows nothing.
    if (change.state !== "chunk_end") u.pendingChunks = Math.max(0, u.pendingChunks - 1);
    else u.pendingChunks += 1;
    syncTranscribing();
    return;
  }
  switch (change.state) {
    case "chunk_end":
      u.pendingChunks += 1;
      break;
    case "chunk_transcript": {
      u.pendingChunks = Math.max(0, u.pendingChunks - 1);
      const text = (change.text ?? "").trim();
      if (text !== "") {
        noteLiveChunk(u.captureId, change.index, text);
        if (u.speechStarted) steerSpeech(u, text);
        else startSpeech(u, text);
      }
      break;
    }
    case "chunk_failed":
      u.pendingChunks = Math.max(0, u.pendingChunks - 1);
      logWarn(["frontend", "ai"], "a voice chunk failed", { code: change.code ?? "unknown" });
      noteLiveChunkFailed(u.captureId, change.index, change.code ?? "unknown");
      break;
  }
  syncTranscribing();
}

/** VOM-FR-HUXY: live text of a chunked utterance shows in the live bubble. */
export function noteUtteranceText(captureId: string, text: string): boolean {
  const u = utteranceOf(captureId);
  if (u === null || !u.chunked) return false;
  noteLiveText(captureId, text);
  return true;
}

// --- Posting ----------------------------------------------------------------

function joinToBody(key: string, text: string): void {
  const body = getDiscussionSession(key).body;
  setComposerBody(key, body.trim() === "" ? text : `${body.trimEnd()}\n${text}`);
}

type Delivery = { commentId: string | null; answered: boolean };

/**
 * VOM-FR-WYIE, VOM-FR-RENV, VOM-FR-ABPU, VOM-FR-BVNX, VOM-FR-PINR. A comment that
 * reached no agent also posted.
 */
async function deliver(u: Utterance, text: string): Promise<Delivery> {
  const none = { commentId: null, answered: false };
  const spoken = text.trim();
  if (spoken === "") {
    patchVoiceState({ note: "no_speech" });
    return none;
  }
  // VOM-FR-FUFE: the words as spoken, with the session body left as it is.
  // VOM-FR-UXYH: an answer whose set went falls through and joins the body.
  if (u.answers !== null) {
    const block = questionBlockOf(u.key);
    if (!u.keepInBody && block?.setId === u.answers.setId) {
      patchVoiceState({ transcript: "", note: null });
      logInfo(["frontend", "ai"], "a spoken answer filled a question", {
        position: u.answers.position,
      });
      block.answer(u.answers.position, spoken);
      return { commentId: null, answered: true };
    }
    logInfo(["frontend", "ai"], "a spoken answer found no question", {});
    joinToBody(u.key, spoken);
    patchVoiceState({ transcript: "", note: "kept_in_body" });
    return none;
  }
  // The speech calls settle first, so `reached` holds only the agents that the
  // speech reached (CTA-FR-SZOC).
  await u.speechCalls;
  const handle = postingHandleOf(u.key);
  // VOM-FR-ABPU: a question set that came after the recording started.
  const setStands = questionBlockOf(u.key) !== null;
  if (u.keepInBody || !handle || handle.blocked() || setStands) {
    joinToBody(u.key, spoken);
    patchVoiceState({ transcript: "", note: "kept_in_body" });
    return none;
  }
  let outcome: PostOutcome = "failed";
  const tagged = handle.tagged(spoken);
  // VOM-FR-RFWC: the handle posts the session body, then the transcript.
  const held = getDiscussionSession(u.key).body.trimEnd();
  beginSpeechPost(u.key, u.reached, held === "" ? tagged : `${held}\n${tagged}`);
  try {
    outcome = await handle.post(tagged);
  } catch (error) {
    logWarn(["frontend", "ai"], "a voice transcript did not post", { code: errorCode(error) });
  }
  const commentId = finishSpeechPost();
  if (outcome === "busy" || outcome === "failed") {
    joinToBody(u.key, spoken);
    patchVoiceState({ transcript: "", note: "kept_in_body" });
    return none;
  }
  patchVoiceState({ transcript: "", note: outcome === "reached_nobody" ? "no_agent" : null });
  return { commentId, answered: true };
}

/** VOM-FR-QCZQ: the one exit of each utterance, on every path. */
export function settleUtterance(u: Utterance, delivery: Delivery | null): void {
  if (u.settled) return;
  u.settled = true;
  const index = open.indexOf(u);
  if (index !== -1) open.splice(index, 1);
  endSpeech(u, delivery?.commentId ?? null);
  dropLivePart(u.captureId);
  if (delivery?.answered) noteAnswered();
  u.done();
  syncTranscribing();
  afterSettle();
}

/** VOM-FR-WWZX, VOM-FR-WICV: a whole utterance failed. */
function failed(error: unknown, u: Utterance): void {
  const code = errorCode(error);
  logWarn(["frontend", "ai"], "a voice recording failed", { code });
  if (u.discard || getVoiceState().activeKey !== u.key) {
    void cancelVoiceCapture(u.captureId).catch(() => undefined);
    return;
  }
  // A Retry lost its held audio to a new utterance, and that utterance stands.
  if (!u.chunked && open.some((o) => o !== u && !o.finishing)) return;
  const retryable = canRetry(code);
  held = retryable ? { id: u.captureId, key: u.key, answers: u.answers } : null;
  patchVoiceState({ transcript: "", error: code, canRetry: retryable });
}

/**
 * VOM-FR-OTRF, VOM-FR-GBCN: the transcript comes in the background, and the post
 * waits for each earlier utterance to settle.
 */
export async function finishUtterance(u: Utterance, text: Promise<string>): Promise<void> {
  if (u.finishing || u.settled) {
    // Nobody waits for this transcript, as the utterance was discarded or
    // finishes already. Its rejection must not go unhandled.
    text.catch((error: unknown) => {
      logDebug(["frontend", "ai"], "a transcript of a settled utterance was dropped", {
        code: errorCode(error),
      });
    });
    return;
  }
  u.finishing = true;
  syncTranscribing();
  let transcript: string | null = null;
  let failure: unknown = null;
  try {
    transcript = await text;
  } catch (error) {
    failure = error ?? "unknown";
  }
  await u.previous;
  u.finishing = false;
  u.pendingChunks = 0;
  if (u.discard) {
    settleUtterance(u, null);
    return;
  }
  if (transcript === null) {
    failed(failure, u);
    settleUtterance(u, null);
    return;
  }
  let delivery: Delivery | null = null;
  try {
    delivery = await deliver(u, transcript);
  } catch (error) {
    logWarn(["frontend", "ai"], "a voice utterance did not post", { code: errorCode(error) });
  }
  settleUtterance(u, delivery);
}

/**
 * VOM-FR-JBJI, VOM-FR-ABPU: voice mode ended by the Voice button. The
 * transcript joins the session body, and the speech reaches no agent from now
 * on, so a turn that the chunks started ends with no comment (VOM-FR-REJN).
 */
export function detachUtterance(u: Utterance): void {
  u.keepInBody = true;
  if (u.detached) return;
  u.detached = true;
  if (u.speechStarted) {
    logInfo(["frontend", "ai"], "a speech stops at the end of voice mode", {});
  }
  endSpeech(u, null);
  dropLivePart(u.captureId);
}

/** VOM-FR-UXZT, VOM-FR-KQAI: drop an utterance and its capture. */
export function discardUtterance(u: Utterance): void {
  u.discard = true;
  if (!u.finishing) {
    void cancelVoiceCapture(u.captureId).catch(() => undefined);
    settleUtterance(u, null);
  }
}

// --- Held recording -----------------------------------------------------------

export function heldRecording(): { id: string; key: string; answers: AnswerTarget | null } | null {
  return held;
}

/** VOM-FR-FASJ. */
export function dropHeldRecording(): void {
  if (held) {
    void cancelVoiceCapture(held.id).catch(() => undefined);
    held = null;
  }
}

/** VOM-FR-WICV: a Retry takes the held recording. It is not chunked. */
export async function retryHeld(text: (id: string) => Promise<string>): Promise<void> {
  const job = held;
  if (!job) return;
  held = null;
  retrying += 1;
  syncTranscribing();
  const u = beginUtterance(job.key, job.id, false, job.answers);
  retrying -= 1;
  await finishUtterance(u, text(job.id));
}

/** VOM-FR-BCRD: the first post of an opening composer moved the key. */
export function moveUtterances(from: string, to: string): void {
  open.forEach((u) => {
    if (u.key === from) u.key = to;
  });
  if (held?.key === from) held.key = to;
}

export function resetVoiceUtterancesForTest(): void {
  open.length = 0;
  lastDone = Promise.resolve();
  held = null;
  retrying = 0;
}
