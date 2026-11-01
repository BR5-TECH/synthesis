/**
 * Voice mode, in memory (`VOM-voice-mode.md` VOM-FR-MQHD).
 *
 * A module store, so that voice mode stays with its discussion while no
 * composer of it is mounted (VOM-FR-BCRD). Exactly one discussion is in voice
 * mode (VOM-FR-OSTM). `./../voice/voiceController.ts` changes this store. The
 * input mode and the response mode are also saved as app preferences
 * (VOM-FR-KEMU), and this store holds their effective values.
 */
import { useSyncExternalStore } from "react";

export type VoicePhase = "idle" | "opening" | "recording" | "transcribing" | "posting";
export type VoiceResponseMode = "text" | "voice";
export type VoiceNote = "no_speech" | "no_agent" | "kept_in_body";
/** VOM-FR-IKIK. */
export type VoiceInputMode = "ptt" | "vad";

export interface VoiceState {
  /** The session key of the voice discussion (VOM-FR-BCRD). */
  activeKey: string | null;
  discussionId: string | null;
  /** STB-FR-PYRC: the display name of the voice discussion's owner. */
  ownerName: string | null;
  /**
   * VOM-FR-GBCN: chunks and utterances that transcribe in the background, while
   * the microphone can stay open.
   */
  transcribing: number;
  phase: VoicePhase;
  /** VOM-FR-HUXY: the route gives text while recording. */
  live: boolean;
  transcript: string;
  /** A typed code. */
  error: string | null;
  canRetry: boolean;
  note: VoiceNote | null;
  responseMode: VoiceResponseMode;
  playing: boolean;
  /** VOM-FR-SRGN: when recording started, in `Date.now()` terms. */
  recordingSince: number | null;
  /** VOM-FR-UXKL: a verified provider is active. */
  providerReady: boolean;
  /** VOM-FR-IAVZ: that provider has a speech route. */
  providerSpeaks: boolean;
  /** VOM-FR-KEMU: the saved input mode, applied at entry. */
  inputMode: VoiceInputMode;
  /** VOM-FR-PDZR: a VAD session is open, so the microphone is open. */
  vadOpen: boolean;
  /** VOM-FR-MUZB, VOM-FR-RBWX: the open VAD session is muted. */
  muted: boolean;
  /** VOM-FR-VOUG: from `speech_start` to the end of the utterance. */
  hearing: boolean;
  /** VOM-FR-FVRS: the author's speech paused the reply. `playing` stays. */
  playbackPaused: boolean;
}

export const INITIAL_VOICE_STATE: VoiceState = Object.freeze({
  activeKey: null,
  discussionId: null,
  ownerName: null,
  transcribing: 0,
  phase: "idle",
  live: false,
  transcript: "",
  error: null,
  canRetry: false,
  note: null,
  responseMode: "text",
  playing: false,
  recordingSince: null,
  providerReady: false,
  providerSpeaks: false,
  inputMode: "ptt",
  vadOpen: false,
  muted: false,
  hearing: false,
  playbackPaused: false,
});

let state: VoiceState = INITIAL_VOICE_STATE;
const listeners = new Set<() => void>();

export function getVoiceState(): VoiceState {
  return state;
}

export function patchVoiceState(change: Partial<VoiceState>): void {
  state = { ...state, ...change };
  listeners.forEach((listener) => listener());
}

export function subscribeVoiceState(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function useVoiceState(): VoiceState {
  return useSyncExternalStore(subscribeVoiceState, getVoiceState);
}

/** VOM-FR-XDUY, STB-FR-KWMV: the state that the status bar indicator shows. */
export type VoiceIndicatorState =
  | "paused"
  | "hearing"
  | "transcribing"
  | "speaking"
  | "listening"
  | "idle";

export function voiceIndicatorState(s: VoiceState): VoiceIndicatorState {
  if (s.playbackPaused) return "paused";
  if (s.hearing || s.phase === "recording" || s.phase === "opening") return "hearing";
  if (s.transcribing > 0 || s.phase === "transcribing" || s.phase === "posting") {
    return "transcribing";
  }
  if (s.playing) return "speaking";
  if (s.vadOpen) return "listening";
  return "idle";
}

export function resetVoiceStateForTest(): void {
  state = INITIAL_VOICE_STATE;
}
