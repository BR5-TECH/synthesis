/**
 * The microphone and the speaker that the author chose (`VOM-voice-mode.md`
 * VOM-FR-EQQV, VOM-FR-KKNN, VOM-FR-GIES, VOM-FR-OGFM).
 *
 * The choices are app preferences (`GSS-global-settings-storage.md`
 * GSS-FR-EKHX, GSS-FR-THBY). A device id of the WebView can change, so a saved
 * device is found by its id, else by its label.
 */
import { peekAppPreferences } from "../state/appPreferences";
import type { VoiceDevice } from "../types";

export type AudioDeviceKind = "audioinput" | "audiooutput";

/** One device that the WebView lists. */
export interface AudioDeviceOption {
  id: string;
  label: string;
}

/** What capture and playback use. `null` id is the system default. */
export interface DeviceChoice {
  id: string | null;
  /** The saved device, where one is saved. */
  saved: VoiceDevice | null;
}

/** The saved microphone and speaker, and the echo cancellation. */
export interface AudioSettings {
  input: VoiceDevice | null;
  output: VoiceDevice | null;
  echoCancellation: boolean;
}

/**
 * The saved values, from the record that this window already holds. A window
 * that has not read the record uses the defaults.
 */
export function audioSettings(): AudioSettings {
  const prefs = peekAppPreferences();
  return {
    input: prefs?.voiceInputDevice ?? null,
    output: prefs?.voiceOutputDevice ?? null,
    // GSS-FR-THBY: on until the author turns it off.
    echoCancellation: prefs?.voiceEchoCancellation !== false,
  };
}

/** The devices of one kind. A WebView with no device list gives none. */
export async function listAudioDevices(kind: AudioDeviceKind): Promise<AudioDeviceOption[]> {
  const media = typeof navigator === "undefined" ? undefined : navigator.mediaDevices;
  if (!media?.enumerateDevices) return [];
  try {
    const devices = await media.enumerateDevices();
    return devices
      .filter((device) => device.kind === kind)
      .map((device) => ({ id: device.deviceId, label: device.label }));
  } catch {
    return [];
  }
}

/** VOM-FR-GIES: the present device that matches, by id, else by label. */
export function matchDevice(
  saved: VoiceDevice | null,
  devices: AudioDeviceOption[],
): AudioDeviceOption | null {
  if (!saved) return null;
  return (
    devices.find((device) => device.id !== "" && device.id === saved.id) ??
    devices.find((device) => device.label !== "" && device.label === saved.label) ??
    null
  );
}

/** The list gives labels only after the microphone is allowed (VAI-FR-XOOE). */
export function hasLabels(devices: AudioDeviceOption[]): boolean {
  return devices.some((device) => device.label !== "");
}

/** VAI-FR-YTLU, VOM-FR-KKNN: the WebView can send audio to a chosen device. */
export function sinkIdSupported(): boolean {
  return (
    typeof AudioContext !== "undefined" &&
    typeof (AudioContext.prototype as { setSinkId?: unknown }).setSinkId === "function"
  );
}

/** VAI-FR-XADO: a change of the device list. Returns the unsubscribe. */
export function onDeviceChange(listener: () => void): () => void {
  const media = typeof navigator === "undefined" ? undefined : navigator.mediaDevices;
  if (!media?.addEventListener) return () => undefined;
  media.addEventListener("devicechange", listener);
  return () => media.removeEventListener("devicechange", listener);
}

/** VOM-FR-GIES: the device kind of a fallback note. */
export type FallbackKind = "microphone" | "speaker";

let fallbackListener: ((kind: FallbackKind) => void) | null = null;

/** Voice mode shows the note of VOM-FR-GIES. One listener at a time. */
export function setDeviceFallbackListener(listener: ((kind: FallbackKind) => void) | null): void {
  fallbackListener = listener;
}

/** VOM-FR-GIES: a saved device was not found, so the system default plays. */
export function reportDeviceFallback(kind: FallbackKind): void {
  fallbackListener?.(kind);
}

export function resetAudioDevicesForTest(): void {
  fallbackListener = null;
}
