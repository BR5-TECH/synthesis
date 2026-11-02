/**
 * The live level of the microphone of a VAD session, for the waveform
 * (`VOM-voice-mode.md` VOM-FR-WFHN, VOM-FR-ZDLY).
 *
 * The level comes from the WebView microphone stream. It calls no provider and
 * no backend operation. It is apart from the voice state, because it changes
 * many times each second and only the waveform reads it.
 */

let level = 0;
const listeners = new Set<(value: number) => void>();

export function getMicLevel(): number {
  return level;
}

/** A new level, from 0 (silence or flat) to 1. */
export function noteMicLevel(value: number): void {
  level = Math.max(0, Math.min(1, value));
  listeners.forEach((listener) => listener(level));
}

export function subscribeMicLevel(listener: (value: number) => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/**
 * The level of one block of PCM16 audio. The root mean square is scaled up,
 * because speech seldom comes near full scale.
 */
export function levelOfPcm(pcm: ArrayBuffer): number {
  const samples = new Int16Array(pcm, 0, Math.floor(pcm.byteLength / 2));
  if (samples.length === 0) return 0;
  let sum = 0;
  for (const sample of samples) sum += sample * sample;
  const rms = Math.sqrt(sum / samples.length) / 32768;
  return Math.min(1, Math.sqrt(rms) * 1.6);
}

export function resetMicLevelForTest(): void {
  level = 0;
  listeners.clear();
}
