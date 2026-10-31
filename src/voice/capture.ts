/**
 * VOM-FR-YIJS: the microphone, through the WebView.
 *
 * WebView capture on all platforms: on macOS, `echoCancellation` selects the OS
 * voice processing unit, so no native capture code exists. The worklet gives
 * PCM16, because realtime routes take PCM and the HTTP routes take WAV.
 */
// Never inlined: a `data:` URL is no module that every WebView loads.
import workletUrl from "./pcmCapture.worklet.js?url&no-inline";

/** VOM non-functional. */
const FRAME_SECONDS = 0.03;

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

/** `onLost`: the track ended with no call to `stop`, as when a device goes. */
export async function openMicrophone(
  onChunk: (pcm: ArrayBuffer) => void,
  onLost?: () => void,
): Promise<Microphone> {
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
  const stopTracks = () =>
    stream.getTracks().forEach((track) => {
      track.onended = null;
      track.stop();
    });
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
