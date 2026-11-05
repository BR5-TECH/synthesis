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
  /** VOP-FR-TGIB: who changes the speed. Absent reads as `client`. */
  speechSpeed?: "server" | "client";
  /** VOP-FR-COMJ: the speech route takes a style. */
  speechInstructions?: boolean;
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
  /** VOP-FR-HQTQ: 50 to 200. Absent means 100. */
  speechSpeedPercent?: number;
  /** VOP-FR-HQTQ: `""` means no style. */
  speechStyle?: string;
  /** VOP-FR-AVXY: the server's message for a refused model. */
  capabilityNote?: string;
  verifiedAt?: string;
  routesChosen: boolean;
  capabilities?: VoiceCapabilities;
}

/** VOP-FR-STVA, AGR-FR-EPYX: the speed and the style of one voice. */
export interface VoiceOptions {
  /** 50 to 200, or null for the provider's speed. */
  speedPercent: number | null;
  /** `""` for the provider's style. */
  style: string;
}

/** VOP-FR-HQTQ, VAI-FR-USDR. */
export const SPEECH_SPEED_MIN = 50;
export const SPEECH_SPEED_MAX = 200;
export const SPEECH_SPEED_STEP = 10;
export const SPEECH_STYLE_LIMIT = 200;

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

/** GSS-FR-KOAB. */
export type VoiceVadSensitivity = "low" | "medium" | "high";

/** VOP "Records": what `voice-activity-changed` carries. */
/** VOP-FR-DSSU: one step of a chunk of a chunked capture. */
export interface VoiceChunkChange {
  captureId: string;
  /** 0 for the first chunk of the capture. */
  index: number;
  state: "chunk_end" | "chunk_transcript" | "chunk_failed";
  text: string | null;
  code: string | null;
}

export interface VoiceActivityChange {
  activityId: string;
  state: "speech_start" | "speech_confirmed" | "speech_end" | "misfire" | "failed";
  captureId: string | null;
  live: boolean | null;
  code: string | null;
  fatal: boolean | null;
}
