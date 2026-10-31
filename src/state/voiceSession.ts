/**
 * Voice mode, in memory (`VOM-voice-mode.md` VOM-FR-MQHD).
 *
 * A module store, so that a recording outlives the composer that started it and
 * a reply can arrive with no composer mounted. Exactly one composer is in voice
 * mode (VOM-FR-OSTM). `./../voice/voiceController.ts` changes this store.
 */
import { useSyncExternalStore } from "react";

export type VoicePhase = "idle" | "opening" | "recording" | "transcribing" | "posting";
export type VoiceResponseMode = "text" | "voice";
export type VoiceNote = "no_speech" | "no_agent" | "kept_in_body";
/** VOM-FR-IKIK. */
export type VoiceInputMode = "ptt" | "vad";

export interface VoiceState {
  /** The session key of the composer in voice mode. */
  activeKey: string | null;
  discussionId: string | null;
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
  /** VOM-FR-MQHD: Push-to-Talk each time voice mode starts. */
  inputMode: VoiceInputMode;
  /** VOM-FR-PDZR: a VAD session is open, so the microphone is open. */
  vadOpen: boolean;
  /** VOM-FR-VOUG: from `speech_start` to the end of the utterance. */
  hearing: boolean;
  /** VOM-FR-FVRS: the author's speech paused the reply. `playing` stays. */
  playbackPaused: boolean;
}

export const INITIAL_VOICE_STATE: VoiceState = Object.freeze({
  activeKey: null,
  discussionId: null,
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

export function resetVoiceStateForTest(): void {
  state = INITIAL_VOICE_STATE;
}
