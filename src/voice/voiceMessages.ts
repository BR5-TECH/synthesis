/** VOM-FR-ZFSA, VOM-FR-JJMY, VAI-FR-HJHX: a typed code as text. */

const MESSAGES: Record<string, string> = {
  microphone_denied:
    "The microphone is not allowed. Allow it in the Privacy settings of the operating system.",
  microphone_missing: "No microphone was found.",
  microphone_unavailable: "The microphone is not available.",
  provider_unreachable: "The Voice AI provider is not reachable.",
  provider_rejected: "The Voice AI provider refused the request. Check its key in Settings.",
  transcription_failed: "The transcription failed.",
  voice_session_lost: "The connection to the Voice AI provider was lost.",
  voice_session_active: "A recording is already in progress.",
  no_active_provider: "No Voice AI provider is active.",
  voice_detector_unavailable: "The voice detector could not start.",
  voice_detector_failed: "The voice detector stopped, so the session closed.",
  voice_no_audio: "The microphone stopped sending audio, so the session closed.",
  speech_failed: "The reply could not be spoken.",
  voice_segment_too_long: "The reply could not be spoken.",
  base_url_empty: "Enter a base URL.",
  base_url_invalid: "This base URL is not valid. A Custom URL does not end in /v1.",
  key_missing: "Enter a key.",
  unreachable: "The server is not reachable.",
  rejected: "The server refused this key.",
  no_transcription_route: "This server has no transcription route for this model.",
  no_speech_route: "This server has no speech route for this TTS model.",
  keychain_unavailable: "The application secret vault is not available.",
  not_verified: "Verify this provider first.",
  route_not_detected: "Verification did not find this route.",
};

export function voiceMessage(code: string): string {
  if (code.startsWith("tls_untrusted")) return "The certificate of the server is not trusted.";
  return MESSAGES[code] ?? "The voice operation failed.";
}

/** VOM-FR-WICV: a failure of the provider, where the recording is still held. */
export function canRetry(code: string): boolean {
  return [
    "provider_unreachable",
    "provider_rejected",
    "transcription_failed",
    "voice_session_lost",
  ].includes(code);
}

/**
 * A rejection as a code. A command rejects with a code, and anything else can
 * hold text that must not reach a log.
 */
export function errorCode(error: unknown): string {
  if (typeof error === "string" && /^[a-z_]+(:\S+)?$/.test(error)) return error;
  return "unknown";
}
