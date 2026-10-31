/**
 * VOM-FR-YIJS: the microphone, through the WebView.
 *
 * WebView capture on all platforms: on macOS, `echoCancellation` selects the OS
 * voice processing unit, so no native capture code exists. The worklet gives
 * PCM16, because realtime routes take PCM and the HTTP routes take WAV.
 */
// Never inlined: a `data:` URL is no module that every WebView loads.
import workletUrl from "./pcmCapture.worklet.js?url&no-inline";

export interface Microphone {
  /** The rate of the audio that `onChunk` receives. */
  rate: number;
  stop(): void;
}

/** A typed reason, which `voiceMessage` turns into text. */
export class MicrophoneError extends Error {}

function failureCode(error: unknown): string {
  const name = error instanceof DOMException || error instanceof Error ? error.name : "";
  if (name === "NotAllowedError" || name === "SecurityError") return "microphone_denied";
  if (name === "NotFoundError" || name === "OverconstrainedError") return "microphone_missing";
  return "microphone_unavailable";
}

export async function openMicrophone(onChunk: (pcm: ArrayBuffer) => void): Promise<Microphone> {
  if (!navigator.mediaDevices?.getUserMedia) {
    throw new MicrophoneError("microphone_unavailable");
  }
  let stream: MediaStream;
  try {
    stream = await navigator.mediaDevices.getUserMedia({
      audio: {
        echoCancellation: true,
        noiseSuppression: true,
        autoGainControl: true,
        channelCount: 1,
      },
    });
  } catch (error) {
    throw new MicrophoneError(failureCode(error));
  }
  const stopTracks = () => stream.getTracks().forEach((track) => track.stop());
  let context: AudioContext | null = null;
  try {
    const running = new AudioContext();
    context = running;
    await running.audioWorklet.addModule(workletUrl);
    await running.resume();
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

  private merged(): Uint8Array {
    const total = this.backlog.reduce((sum, chunk) => sum + chunk.byteLength, 0);
    const out = new Uint8Array(total);
    let at = 0;
    for (const chunk of this.backlog) {
      out.set(chunk, at);
      at += chunk.byteLength;
    }
    this.backlog = [];
    return out;
  }

  private pump(): void {
    if (this.running || this.target === null || this.backlog.length === 0) return;
    const sessionId = this.target;
    this.running = this.send(sessionId, this.merged())
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
