/**
 * Spoken replies and spoken questions of voice mode (`VOM-voice-mode.md`
 * VOM-FR-ZLAY, VOM-FR-HBLE, VOM-FR-ZPOD, VOM-FR-UNXR).
 *
 * Both work whether or not a surface of the voice discussion is mounted
 * (VOM-FR-BCRD): the reply comes from the shell's discussion event, and the
 * voice of the agent comes from the mounted or the retained handle.
 */
import { logDebug } from "../logging";
import { getVoiceState, patchVoiceState } from "../state/voiceSession";
import type { Discussion, PendingQuestion, Participant } from "../types";
import { pausePlayback, resumePlayback } from "./playback";
import { cancelFiller, resetVoiceFillerForTest, tryFiller } from "./voiceFiller";
import {
  commentSpeech,
  dropReply,
  fillerSounds,
  resetVoiceSpeechForTest,
  speakAs,
  type Speech,
} from "./voiceSpeech";
import { questionSetOf } from "../state/questionSets";
import { questionSpeech, type AnswerTarget } from "./voiceQuestions";
import { hasUnsettledUtterance } from "./voiceUtterances";

let baseline = new Set<string>();
/** VOM-FR-ZPOD: the newest reply that waits for the author. */
let pendingReply: {
  speech: Speech;
  key: string | null;
  /** A question waits only while its set stands. */
  set?: { discussionId: string; setId: string };
} | null = null;
/** VOM-FR-UALK: the question spoken last, so a re-read speaks nothing. */
let spokenQuestion: AnswerTarget | null = null;
/** VOM-FR-UNXR: an utterance posted or answered since the reply paused. */
let answeredSincePause = false;
/** The voice discussion as the shell last read it. */
let latest: Discussion | null = null;

/** VOM-FR-ILJW: the comments at entry are never spoken. */
export function setBaseline(discussion: Discussion | null): void {
  baseline = new Set(discussion?.comments.map((c) => c.id) ?? []);
  latest = discussion;
}

/** The voice discussion as it stands, when the shell has read it. */
export function latestVoiceDiscussion(): Discussion | null {
  const { discussionId } = getVoiceState();
  return latest !== null && latest.id === discussionId ? latest : null;
}

export function forgetSpokenQuestion(): void {
  spokenQuestion = null;
}

/** VOM-FR-QDWW, VOM-FR-POBZ: silence at once, and no reply or filler is left to speak. */
export function stopSpeech(): void {
  pendingReply = null;
  cancelFiller("stop");
  dropReply();
}

/** VOM-FR-ZPOD: speech that comes while the author speaks waits for them. */
function speakOrWait(
  speech: Speech,
  key: string | null,
  set?: { discussionId: string; setId: string },
): void {
  if (getVoiceState().hearing || hasUnsettledUtterance()) {
    pendingReply = { speech, key, set };
    return;
  }
  void speakAs(speech, key);
}

/**
 * VOM-FR-ZLAY, VOM-FR-ILJW, VOM-FR-VSCQ, VOM-FR-RKOG: a discussion changed. Each
 * new comment joins the baseline, so a switch to response mode Voice never
 * speaks an old comment. No mounted surface is needed (VOM-FR-BCRD).
 */
export function noteDiscussionChanged(discussion: Discussion): void {
  const state = getVoiceState();
  if (state.activeKey === null || discussion.id !== state.discussionId) return;
  latest = discussion;
  const fresh = discussion.comments.filter((c) => !baseline.has(c.id));
  fresh.forEach((c) => baseline.add(c.id));
  // VOM-FR-POBZ: a filler never speaks after the reply arrived.
  if (fresh.some((c) => c.author.kind === "agent")) cancelFiller("reply");
  if (state.responseMode !== "voice") return;
  const newest = fresh.map(commentSpeech).filter((s) => s !== null).pop();
  if (!newest) return;
  if (newest.text === "") logDebug(["frontend", "ai"], "an agent comment has nothing to speak", {});
  speakOrWait(newest, state.activeKey);
}

/**
 * VOM-FR-HBLE, VOM-FR-UALK: the block shows a question. `arrived` is the first
 * show of a mount: a set that voice mode already spoke speaks nothing then, so a
 * remount is silent. The check waits a tick, because the block mounts in the
 * commit that also mounts the session control.
 */
export function noteQuestionShown(
  discussionId: string,
  setId: string,
  question: PendingQuestion,
  askedBy: Participant,
  arrived: boolean,
): void {
  const { activeKey, discussionId: voiceDiscussion } = getVoiceState();
  if (activeKey === null || discussionId !== voiceDiscussion) return;
  const known = spokenQuestion?.setId === setId;
  const same = known && spokenQuestion?.position === question.position;
  spokenQuestion = { setId, position: question.position };
  if (same || (known && arrived)) return;
  // VOM-FR-POBZ: a new question cancels the filler.
  cancelFiller("question");
  queueMicrotask(() => {
    const state = getVoiceState();
    if (spokenQuestion?.setId !== setId || spokenQuestion.position !== question.position) return;
    if (state.responseMode !== "voice") return;
    const text = questionSpeech(question);
    if (text === "") return;
    logDebug(["frontend", "ai"], "a question speaks", { position: question.position });
    speakOrWait(
      { text, agentId: askedBy.kind === "agent" ? askedBy.agentId : null },
      state.activeKey,
      { discussionId, setId },
    );
  });
}

/** VOM-FR-FVRS: the author started to speak while a reply plays. */
export function bargeIn(): void {
  // VOM-FR-UCQN: a barge-in drops a filler, and it never resumes.
  if (fillerSounds()) {
    dropReply();
    return;
  }
  const state = getVoiceState();
  if (state.playing && !state.playbackPaused) {
    pausePlayback();
    answeredSincePause = false;
    patchVoiceState({ playbackPaused: true });
  }
}

/** VOM-FR-UNXR: an utterance posted a comment or gave a spoken answer. */
export function noteAnswered(): void {
  answeredSincePause = true;
}

/**
 * VOM-FR-KVHG, VOM-FR-UNXR, VOM-FR-ZPOD: an utterance settled, or a misfire
 * came. With nothing left to settle, a waiting reply speaks, or a paused reply
 * is dropped or resumes.
 */
export function afterSettle(): void {
  if (getVoiceState().hearing || hasUnsettledUtterance()) return;
  const waiting = pendingReply;
  pendingReply = null;
  const answered = answeredSincePause;
  answeredSincePause = false;
  // VOM-FR-LKKN: a waiting question whose set went says nothing.
  const gone = waiting?.set && questionSetOf(waiting.set.discussionId)?.setId !== waiting.set.setId;
  if (waiting && !gone) {
    void speakAs(waiting.speech, waiting.key);
  } else if (getVoiceState().playbackPaused) {
    if (answered) {
      dropReply();
    } else {
      resumePlayback();
      patchVoiceState({ playbackPaused: false });
    }
  }
  // VOM-FR-LKXN: a due filler waits until nothing is left to settle.
  tryFiller();
}

export function resetVoiceRepliesForTest(): void {
  baseline = new Set();
  pendingReply = null;
  spokenQuestion = null;
  answeredSincePause = false;
  latest = null;
  resetVoiceSpeechForTest();
  resetVoiceFillerForTest();
}
