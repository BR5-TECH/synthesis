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

/** VOP-FR-XZET. */
const FRAME_END = 2;

/**
 * VOP-FR-MOSY, VOP-FR-XZET: resolves at the end frame, and not when the command
 * resolves. A large frame comes through a fetch of its own, so it can arrive
 * after the command. A failed command rejects, and it sends no end frame.
 */
export const speakVoiceSegment = (
  speechId: string,
  text: string,
  voiceName: string | null,
  onFrame: (frame: ArrayBuffer) => void,
) =>
  new Promise<void>((resolve, reject) => {
    const channel = new Channel<ArrayBuffer>();
    channel.onmessage = (frame) => {
      if (new Uint8Array(frame)[0] === FRAME_END) resolve();
      else onFrame(frame);
    };
    invoke<void>("speak_voice_segment", { speechId, text, voiceName, onFrame: channel }).catch(
      reject,
    );
  });

export const cancelVoiceSpeech = (speechId: string) =>
  invoke<void>("cancel_voice_speech", { speechId });

/** VOP-FR-WYKO. */
export const openVoiceActivitySession = (captureRate: number, keyterms: readonly string[]) =>
  invoke<{ activityId: string }>("open_voice_activity_session", { captureRate, keyterms });

/** VOP-FR-TADX: the request form of "append voice capture". */
export const feedVoiceActivity = (activityId: string, bytes: Uint8Array) =>
  invoke<void>("feed_voice_activity", bytes, { headers: { "x-voice-session": activityId } });

/** VOP-FR-QZCE: one whole record, so that no push depends on an earlier one. */
export const setVoiceActivityGate = (
  activityId: string,
  gate: {
    seq: number;
    blocked: boolean;
    playing: boolean;
    settled: string | null;
    keyterms: readonly string[];
  },
) => invoke<void>("set_voice_activity_gate", { activityId, ...gate });

/** VOP-FR-FAAB: gives the id of a capture that has not settled. */
export const closeVoiceActivitySession = (activityId: string, discard: boolean) =>
  invoke<{ captureId: string | null }>("close_voice_activity_session", { activityId, discard });
