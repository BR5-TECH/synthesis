/**
 * Voice AI (`VOP-voice-ai-providers.md`).
 *
 * One part of the typed wrappers over the backend's `#[tauri::command]` set;
 * `./index.ts` carries the whole rule these files are written under.
 */
import { Channel, invoke } from "@tauri-apps/api/core";
import type {
  VoiceAiIntegrations,
  VoiceAiRecord,
  VoiceCaptureStart,
  VoiceModels,
  VoiceProviderId,
  VoiceSpeechRoute,
  VoiceTranscriptionRoute,
} from "../types";

export const listVoiceAiIntegrations = () =>
  invoke<VoiceAiIntegrations>("list_voice_ai_integrations");

export const verifyVoiceAiIntegration = (
  provider: VoiceProviderId,
  baseUrl: string,
  apiKey: string | null,
  sttModel: string,
  ttsModel: string,
) =>
  invoke<VoiceAiRecord>("verify_voice_ai_integration", {
    provider,
    baseUrl,
    apiKey,
    sttModel,
    ttsModel,
  });

export const setVoiceAiRoutes = (
  provider: VoiceProviderId,
  transcriptionRoute: VoiceTranscriptionRoute,
  speechRoute: VoiceSpeechRoute | null,
) =>
  invoke<VoiceAiRecord>("set_voice_ai_routes", { provider, transcriptionRoute, speechRoute });

export const setVoiceAiDefaultVoice = (provider: VoiceProviderId, voiceName: string) =>
  invoke<VoiceAiRecord>("set_voice_ai_default_voice", { provider, voiceName });

export const setActiveVoiceAiIntegration = (provider: VoiceProviderId) =>
  invoke<VoiceAiIntegrations>("set_active_voice_ai_integration", { provider });

export const clearVoiceAiIntegration = (provider: VoiceProviderId) =>
  invoke<VoiceAiIntegrations>("clear_voice_ai_integration", { provider });

export const listVoiceAiModels = (
  provider: VoiceProviderId,
  baseUrl: string,
  apiKey: string | null,
) => invoke<VoiceModels>("list_voice_ai_models", { provider, baseUrl, apiKey });

/** VOP-FR-XJRF: `null` means that voice names are free text. */
export const listVoiceAiVoices = (provider?: VoiceProviderId) =>
  invoke<string[] | null>("list_voice_ai_voices", { provider: provider ?? null });

export const startVoiceCapture = (captureRate: number, keyterms: readonly string[]) =>
  invoke<VoiceCaptureStart>("start_voice_capture", { captureRate, keyterms });

/** VOP-FR-DXPK: PCM16 bytes as a raw body, so no JSON array is built. */
export const appendVoiceCapture = (sessionId: string, bytes: Uint8Array) =>
  invoke<void>("append_voice_capture", bytes, { headers: { "x-voice-session": sessionId } });

export const finishVoiceCapture = (sessionId: string) =>
  invoke<string>("finish_voice_capture", { sessionId });

export const retryVoiceCapture = (sessionId: string) =>
  invoke<string>("retry_voice_capture", { sessionId });

export const cancelVoiceCapture = (sessionId: string) =>
  invoke<void>("cancel_voice_capture", { sessionId });

/** VOP-FR-MOSY, VOP-FR-XZET: resolves when the segment ends. */
export const speakVoiceSegment = (
  speechId: string,
  text: string,
  voiceName: string | null,
  onFrame: (frame: ArrayBuffer) => void,
) => {
  const channel = new Channel<ArrayBuffer>();
  channel.onmessage = onFrame;
  return invoke<void>("speak_voice_segment", { speechId, text, voiceName, onFrame: channel });
};

export const cancelVoiceSpeech = (speechId: string) =>
  invoke<void>("cancel_voice_speech", { speechId });
