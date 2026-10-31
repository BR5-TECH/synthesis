// Voice AI (voice_ai.rs / VOP-voice-ai-providers.md). Field names match the
// Rust records byte-for-byte.

export type VoiceProviderId = "openai" | "openrouter" | "custom";
export type VoiceTranscriptionRoute = "realtime" | "http";
export type VoiceSpeechRoute = "realtime" | "openai_speech" | "llama_tts";
export type VoiceDialect = "openai" | "llamacpp" | "vllm";

/** VOP-FR-SURK: what detection found. */
export interface VoiceCapabilities {
  dialect?: VoiceDialect;
  /** `live` gives text while recording, and `turn` gives it after the commit. */
  realtimeTranscription?: "live" | "turn";
  realtimeInputRate?: number;
  httpTranscription: boolean;
  realtimeSpeech: boolean;
  openaiSpeech: boolean;
  llamaTts: boolean;
}

/** VOP-FR-SURK: one provider. It holds no key. */
export interface VoiceAiRecord {
  provider: VoiceProviderId;
  baseUrl: string;
  maskedHint?: string;
  sttModel: string;
  ttsModel: string;
  transcriptionRoute: VoiceTranscriptionRoute;
  speechRoute?: VoiceSpeechRoute;
  defaultVoice: string;
  /** VOP-FR-AVXY: the server's message for a refused model. */
  capabilityNote?: string;
  verifiedAt?: string;
  routesChosen: boolean;
  capabilities?: VoiceCapabilities;
}

export interface VoiceAiIntegrations {
  active: VoiceProviderId | null;
  records: VoiceAiRecord[];
}

export interface VoiceCaptureStart {
  sessionId: string;
  route: VoiceTranscriptionRoute;
  live: boolean;
}

export interface VoiceModels {
  sttModels: string[];
  ttsModels: string[];
}

/** VAI-FR-HVNJ. */
export type VoiceHotkeyBehaviour = "hold" | "toggle";
