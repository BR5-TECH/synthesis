/**
 * The speech player of voice mode (`VOM-voice-mode.md` VOM-FR-QOXX, VOM-FR-QPFQ):
 * one reply at a time, in order, with what a comment says.
 */
import { cancelVoiceSpeech, speakVoiceSegment } from "../api";
import { logDebug, logWarn } from "../logging";
import { getVoiceState, patchVoiceState } from "../state/voiceSession";
import { spokenPart } from "../text/spokenPart";
import type { Comment } from "../types";
import { isPlaybackIdle, playFrame, setDrainedListener, stopPlayback } from "./playback";
import { speakableText, speechSegments } from "./speakableText";
import { errorCode } from "./voiceMessages";

/** `ADQ-FR-YQTB`: the identity of a question comment ends so. */
const QUESTION_COMMENT = /:\d{2}-1q$/;

/** What one speech says, and whose voice says it. */
export interface Speech {
  text: string;
  agentId: string | null;
}

let currentSpeech: string | null = null;
let segmentsSent = false;
let speechCounter = 0;

/** The reply that sounds or stands paused goes. A reply that waits stays. */
export function dropReply(): void {
  if (currentSpeech) {
    void cancelVoiceSpeech(currentSpeech).catch(() => undefined);
    currentSpeech = null;
  }
  stopPlayback();
  segmentsSent = false;
  const { playing, playbackPaused } = getVoiceState();
  if (playing || playbackPaused) patchVoiceState({ playing: false, playbackPaused: false });
}

function drained(): void {
  if (segmentsSent) {
    currentSpeech = null;
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

/** One reply: it replaces what sounds now, and it goes out in order. */
export async function speak({ text }: Speech, voiceName: string | null): Promise<void> {
  dropReply();
  if (text === "") return;
  speechCounter += 1;
  const id = `speech-${speechCounter}`;
  currentSpeech = id;
  patchVoiceState({ playing: true });
  setDrainedListener(drained);
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

export function resetVoiceSpeechForTest(): void {
  currentSpeech = null;
  segmentsSent = false;
  speechCounter = 0;
}
