/**
 * VOM-FR-YIJS: the microphone, through the WebView.
 *
 * WebView capture on all platforms: on macOS, `echoCancellation` selects the OS
 * voice processing unit, so no native capture code exists. The worklet gives
 * PCM16, because realtime routes take PCM and the HTTP routes take WAV.
 */
// Never inlined: a `data:` URL is no module that every WebView loads.
import workletUrl from "./pcmCapture.worklet.js?url&no-inline";

import { logInfo } from "../logging";
import { audioSettings, listAudioDevices, matchDevice, reportDeviceFallback } from "./audioDevices";

/** VOM non-functional. */
const FRAME_SECONDS = 0.03;

export interface Microphone {
  /** The rate of the audio that `onChunk` receives. */
  rate: number;
  stop(): void;
}

/** A typed reason, which `voiceMessage` turns into text. */
export class MicrophoneError extends Error {}

/** VOM-FR-LDKK: the limit on the load of the worklet and the resume. */
export const MIC_OPEN_LIMIT_MS = 10_000;

/** VOM-FR-LDKK: `step`, or a failure when it does not settle by `deadline`. */
function withinOpenLimit<T>(step: Promise<T>, deadline: number): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const limit = setTimeout(
      () => reject(new MicrophoneError("microphone_unavailable")),
      Math.max(0, deadline - Date.now()),
    );
    step.then(
      (value) => {
        clearTimeout(limit);
        resolve(value);
      },
      (error: unknown) => {
        clearTimeout(limit);
        reject(error);
      },
    );
  });
}

function failureCode(error: unknown): string {
  const name = error instanceof DOMException || error instanceof Error ? error.name : "";
  if (name === "NotAllowedError" || name === "SecurityError") return "microphone_denied";
  if (name === "NotFoundError" || name === "OverconstrainedError") return "microphone_missing";
  return "microphone_unavailable";
}

/**
 * VOM-FR-EQQV, VOM-FR-OGFM: the audio constraints, with the saved microphone
 * as the `ideal` device. A device whose id changed is found by its label.
 */
async function audioConstraints(): Promise<MediaTrackConstraints> {
  const { input, echoCancellation } = audioSettings();
  const constraints: MediaTrackConstraints = {
    echoCancellation,
    noiseSuppression: true,
    autoGainControl: true,
    channelCount: 1,
  };
  if (input) {
    const match = matchDevice(input, await listAudioDevices("audioinput"));
    constraints.deviceId = { ideal: match?.id ?? input.id };
  }
  return constraints;
}

/**
 * VOM-FR-GIES, VOM-FR-IQNQ: log the device in use, and report a saved device
 * that is not the one that opened.
 */
function noteOpenedDevice(stream: MediaStream, echoCancellation: boolean): void {
  const { input } = audioSettings();
  const track = stream.getAudioTracks?.()[0];
  const label = track?.label ?? "";
  const id = track?.getSettings?.().deviceId ?? "";
  const found = input ? id === input.id || (label !== "" && label === input.label) : null;
  logInfo(["frontend"], "the microphone opened", {
    device: label === "" ? "unknown" : label,
    saved: input !== null,
    found,
    echoCancellation,
  });
  if (found === false) reportDeviceFallback("microphone");
}

/** `onLost`: the track ended with no call to `stop`, as when a device goes. */
export async function openMicrophone(
  onChunk: (pcm: ArrayBuffer) => void,
  onLost?: () => void,
): Promise<Microphone> {
  if (!navigator.mediaDevices?.getUserMedia) {
    throw new MicrophoneError("microphone_unavailable");
  }
  let stream: MediaStream;
  const audio = await audioConstraints();
  try {
    stream = await navigator.mediaDevices.getUserMedia({ audio });
  } catch (error) {
    throw new MicrophoneError(failureCode(error));
  }
  noteOpenedDevice(stream, audio.echoCancellation !== false);
  const stopTracks = () =>
    stream.getTracks().forEach((track) => {
      track.onended = null;
      track.onmute = null;
      track.stop();
    });
  // A muted track sends silence. Only the log says so.
  stream.getTracks().forEach((track) => {
    track.onmute = () => logInfo(["frontend"], "the microphone track was muted", {});
  });
  let context: AudioContext | null = null;
  try {
    const running = new AudioContext();
    context = running;
    // VOM-FR-LDKK: one limit for both steps, from the resolve of `getUserMedia`.
    const deadline = Date.now() + MIC_OPEN_LIMIT_MS;
    await withinOpenLimit(running.audioWorklet.addModule(workletUrl), deadline);
    await withinOpenLimit(running.resume(), deadline);
    const source = running.createMediaStreamSource(stream);
    const node = new AudioWorkletNode(running, "pcm-capture");
    node.port.onmessage = (event: MessageEvent<ArrayBuffer>) => onChunk(event.data);
    source.connect(node);
    // The node must reach the destination to run, and silence keeps the
    // author's own voice out of the speakers.
    const silence = running.createGain();
    silence.gain.value = 0;
    node.connect(silence);
    silence.connect(running.destination);
    if (onLost) stream.getTracks().forEach((track) => (track.onended = onLost));
    return {
      rate: running.sampleRate,
      stop() {
        node.port.onmessage = null;
        stopTracks();
        void running.close();
      },
    };
  } catch {
    stopTracks();
    void context?.close();
    throw new MicrophoneError("microphone_unavailable");
  }
}

function joined(chunks: Uint8Array[]): Uint8Array {
  const total = chunks.reduce((sum, chunk) => sum + chunk.byteLength, 0);
  const out = new Uint8Array(total);
  let at = 0;
  for (const chunk of chunks) {
    out.set(chunk, at);
    at += chunk.byteLength;
  }
  return out;
}

/**
 * VOM non-functional: a VAD session sends frames of about 30 ms. The worklet
 * gives a block of about 3 ms, and a whole session of those would be hundreds of
 * calls each second. This counts bytes and holds no timer.
 */
export class FrameBatcher {
  private held: Uint8Array[] = [];
  private size = 0;
  private frameBytes = 0;

  constructor(private readonly emit: (pcm: ArrayBuffer) => void) {}

  /** Audio held until now goes out with the next block. */
  setRate(rate: number): void {
    this.frameBytes = Math.round(rate * FRAME_SECONDS) * 2;
  }

  push(pcm: ArrayBuffer): void {
    this.held.push(new Uint8Array(pcm));
    this.size += pcm.byteLength;
    if (this.frameBytes > 0 && this.size >= this.frameBytes) this.flush();
  }

  flush(): void {
    if (this.size === 0) return;
    const out = joined(this.held);
    this.held = [];
    this.size = 0;
    this.emit(out.buffer as ArrayBuffer);
  }
}

/**
 * VOM non-functional: chunks reach the backend in order. Tauri runs async
 * commands in parallel, so one chunk is in flight at a time and a backlog
 * merges into the next chunk.
 */
export class ChunkPump {
  private backlog: Uint8Array[] = [];
  private running: Promise<void> | null = null;
  private target: string | null = null;

  constructor(private readonly send: (sessionId: string, bytes: Uint8Array) => Promise<unknown>) {}

  push(pcm: ArrayBuffer): void {
    this.backlog.push(new Uint8Array(pcm));
    this.pump();
  }

  /** The session opened, so the audio held so far can go. */
  attach(sessionId: string): void {
    this.target = sessionId;
    this.pump();
  }

  private pump(): void {
    if (this.running || this.target === null || this.backlog.length === 0) return;
    const sessionId = this.target;
    const bytes = joined(this.backlog);
    this.backlog = [];
    this.running = this.send(sessionId, bytes)
      .then(() => undefined, () => undefined)
      .finally(() => {
        this.running = null;
        this.pump();
      });
  }

  /** VOM non-functional: all audio is sent before the session ends. */
  async flush(): Promise<void> {
    while (this.running || this.backlog.length > 0) {
      this.pump();
      if (!this.running) return;
      await this.running;
    }
  }
}
