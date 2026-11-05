/**
 * The speech player of voice mode (`VOM-voice-mode.md` VOM-FR-QOXX, VOM-FR-QPFQ):
 * one reply at a time, in order, with what a comment says.
 */
import { cancelVoiceSpeech, speakVoiceSegment } from "../api";
import { logDebug, logWarn } from "../logging";
import { getVoiceState, patchVoiceState } from "../state/voiceSession";
import { spokenPart } from "../text/spokenPart";
import type { Comment, VoiceOptions } from "../types";
import {
  beginSegment,
  endSegmentInput,
  isPlaybackIdle,
  playFrame,
  setDrainedListener,
  setPlaybackFailureListener,
  stopPlayback,
} from "./playback";
import { speakableText, speechSegments } from "./speakableText";
import { handleOf } from "./voiceHandles";
import { errorCode } from "./voiceMessages";

/** `ADQ-FR-YQTB`: the identity of a question comment ends so. */
const QUESTION_COMMENT = /:\d{2}-1q$/;

/** What one speech says, and whose voice says it. */
export interface Speech {
  text: string;
  agentId: string | null;
  /** VOM-FR-UCQN: a filler, which is not a reply. */
  filler?: boolean;
}

let currentSpeech: string | null = null;
/** VOM-FR-UCQN: the speech that sounds now is a filler. */
let currentFiller = false;
let segmentsSent = false;
let speechCounter = 0;
/** VOM-FR-QUSH: a segment set a rate, so the next one sets its own. */
let rateChanged = false;

/** VOP-FR-XZET, VOP-FR-EMJY: the kind of a rate frame, and a frame of rate 1. */
const RATE_FRAME_KIND = 3;
const RATE_ONE = new Uint8Array([RATE_FRAME_KIND, 0xe8, 0x03, 0, 0]).buffer;

/** The reply that sounds or stands paused goes. A reply that waits stays. */
export function dropReply(): void {
  if (currentSpeech) {
    void cancelVoiceSpeech(currentSpeech).catch(() => undefined);
    currentSpeech = null;
  }
  currentFiller = false;
  stopPlayback();
  // VOM-FR-QUSH: Stop resets the rate of playback.
  rateChanged = false;
  segmentsSent = false;
  const { playing, playbackPaused } = getVoiceState();
  if (playing || playbackPaused) patchVoiceState({ playing: false, playbackPaused: false });
}

/** VOM-FR-UCQN: a filler sounds or stands paused now. */
export function fillerSounds(): boolean {
  return currentSpeech !== null && currentFiller;
}

function drained(): void {
  if (segmentsSent) {
    currentSpeech = null;
    currentFiller = false;
    segmentsSent = false;
    patchVoiceState({ playing: false, playbackPaused: false });
  }
}

/**
 * VOM-FR-TRWN, VOM-FR-VSCQ: a comment speaks its spoken part alone. An agent
 * comment with nothing to speak still counts: it replaces a paused reply and
 * then says nothing (VOM-FR-ZPOD).
 */
export function commentSpeech(comment: Comment): Speech | null {
  if (comment.author.kind !== "agent" || QUESTION_COMMENT.test(comment.id)) return null;
  const part = spokenPart(comment.body);
  return { text: part === null ? "" : speakableText(part), agentId: comment.author.agentId };
}

/** VOM-FR-QPFQ, VOM-FR-UCQN: a speech fails, by its provider or by playback. */
function failSpeech(code: string, filler: boolean): void {
  if (filler) {
    // VOM-FR-UCQN: a failed filler stops and shows no error.
    logWarn(["frontend", "ai"], "a voice filler failed", { code });
    dropReply();
    return;
  }
  // VOM-FR-QPFQ: the text of the comment stays as it is.
  logWarn(["frontend", "ai"], "a voice reply failed", { code });
  dropReply();
  patchVoiceState({ error: code, canRetry: false });
}

/** One reply: it replaces what sounds now, and it goes out in order. */
export async function speak(
  { text, filler = false }: Speech,
  voiceName: string | null,
  options: VoiceOptions | null = null,
): Promise<void> {
  dropReply();
  if (text === "") return;
  speechCounter += 1;
  const id = `speech-${speechCounter}`;
  currentSpeech = id;
  currentFiller = filler;
  patchVoiceState({ playing: true });
  setDrainedListener(drained);
  // VOM-FR-VGQF: a frame that cannot play fails the speech, and it never
  // counts as played.
  setPlaybackFailureListener((code) => {
    if (currentSpeech === id) failSpeech(code, filler);
  });
  try {
    for (const segment of speechSegments(text)) {
      if (currentSpeech !== id) return;
      // VOM-FR-EPBL: the log of playback counts each segment apart.
      beginSegment(filler ? "filler" : "reply");
      // A frame can still be in the channel after Stop.
      let first = true;
      const onFrame = (frame: ArrayBuffer | number[]) => {
        if (currentSpeech !== id) return;
        // VOM-FR-QUSH: a segment with no rate frame plays at rate 1.
        if (first && rateChanged && new Uint8Array(frame)[0] !== RATE_FRAME_KIND) {
          playFrame(RATE_ONE.slice(0));
          rateChanged = false;
        }
        if (new Uint8Array(frame)[0] === RATE_FRAME_KIND) rateChanged = true;
        first = false;
        playFrame(frame);
      };
      // VOM-FR-UALP: the options of the agent, where it sets any.
      await (options
        ? speakVoiceSegment(id, segment, voiceName, onFrame, options)
        : speakVoiceSegment(id, segment, voiceName, onFrame));
      if (currentSpeech === id) endSegmentInput();
    }
    if (currentSpeech === id) {
      segmentsSent = true;
      logDebug(["frontend", "ai"], "a voice reply was sent to playback", {});
      // A reply with no frames, or one that already played, gets no later event.
      if (isPlaybackIdle()) drained();
    }
  } catch (error) {
    if (currentSpeech !== id) return;
    failSpeech(errorCode(error), filler);
  }
}

/**
 * VOM-FR-UALP, VOM-FR-UFBT: speak with the voice and the options that the
 * handle of the discussion knows for the agent.
 */
export function speakAs(speech: Speech, key: string | null): Promise<void> {
  const { agentId } = speech;
  if (agentId === null) return speak(speech, null);
  const handle = handleOf(key);
  return speak(speech, handle?.voiceOf(agentId) ?? null, handle?.voiceOptionsOf?.(agentId) ?? null);
}

export function resetVoiceSpeechForTest(): void {
  currentSpeech = null;
  currentFiller = false;
  rateChanged = false;
  segmentsSent = false;
  speechCounter = 0;
}
