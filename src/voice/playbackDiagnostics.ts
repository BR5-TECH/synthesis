/**
 * What playback writes to the log (`VOM-voice-mode.md` VOM-FR-ULPY,
 * VOM-FR-EPBL, VOM-FR-NFQT, VOM-FR-OXLD).
 *
 * A reply that the author does not hear leaves no error, so these records are
 * what tells its cause. They hold no text and no audio, except the first 4
 * bytes of an encoded file.
 */
import { logDebug, logInfo, logWarn } from "../logging";

/** VOM-FR-DYTO: how a frame came through the channel. */
export type FrameForm = "binary" | "array";

/** VOP-FR-XZET: the frame kinds, for the counts of the log. */
const KIND_NAMES: Record<number, string> = { 0: "pcm", 1: "file", 3: "rate" };

/** What the segment log says about the microphone (VOM-FR-EPBL). */
export interface PlaybackEnvironment {
  microphoneOpen: boolean;
  vadOpen: boolean;
}

let environment: () => PlaybackEnvironment = () => ({ microphoneOpen: false, vadOpen: false });

/** Voice mode tells playback whether the microphone is open. */
export function setPlaybackEnvironmentProbe(probe: (() => PlaybackEnvironment) | null): void {
  environment = probe ?? (() => ({ microphoneOpen: false, vadOpen: false }));
}

/** VOM-FR-DYTO: the bytes of a frame, from an `ArrayBuffer` or a byte array. */
export function frameBytes(frame: unknown): { buffer: ArrayBuffer; form: FrameForm } | null {
  if (frame instanceof ArrayBuffer) return { buffer: frame, form: "binary" };
  if (ArrayBuffer.isView(frame)) {
    const view = new Uint8Array(frame.buffer, frame.byteOffset, frame.byteLength);
    return { buffer: view.slice().buffer, form: "binary" };
  }
  if (Array.isArray(frame)) return { buffer: Uint8Array.from(frame as number[]).buffer, form: "array" };
  return null;
}

/** VOM-FR-EPBL: 4 bytes as text, with each byte that is not printable as `.`. */
export function signatureOf(payload: ArrayBuffer): string {
  return Array.from(new Uint8Array(payload, 0, Math.min(4, payload.byteLength)))
    .map((byte) => (byte >= 0x20 && byte <= 0x7e ? String.fromCharCode(byte) : "."))
    .join("");
}

/** The counts of one segment, from its first frame to its end. */
export class SegmentStats {
  readonly source: string;
  readonly frames: Record<string, number> = {};
  readonly forms: Record<FrameForm, number> = { binary: 0, array: 0 };
  bytes = 0;
  signature = "";
  /** The summed duration of the scheduled audio, at its rate, in seconds. */
  audioSeconds = 0;
  /** `performance.now()` of the first scheduled audio. */
  firstScheduled: number | null = null;
  /** Decodes and sources of this segment that did not end. */
  pending = 0;
  inputDone = false;
  stopped = false;
  failed = false;
  logged = false;
  /** Called one time, after the record of the segment (VAI-FR-BPUR). */
  onEnd: ((stats: SegmentStats) => void) | null = null;

  constructor(source: string) {
    this.source = source;
  }

  noteFrame(kind: number, form: FrameForm, bytes: number, payload: ArrayBuffer): void {
    const name = KIND_NAMES[kind] ?? `kind${kind}`;
    this.frames[name] = (this.frames[name] ?? 0) + 1;
    this.forms[form] += 1;
    this.bytes += bytes;
    if (name === "file" && this.signature === "") this.signature = signatureOf(payload);
  }

  noteScheduled(seconds: number): void {
    this.audioSeconds += seconds;
    this.firstScheduled ??= performance.now();
  }
}

/** VOM-FR-EPBL, VOM-FR-NFQT: one record when a segment ends. */
export function logSegment(stats: SegmentStats): void {
  if (stats.logged) return;
  stats.logged = true;
  const elapsedSeconds =
    stats.firstScheduled === null ? 0 : (performance.now() - stats.firstScheduled) / 1000;
  const { microphoneOpen, vadOpen } = environment();
  const fields = {
    source: stats.source,
    pcmFrames: stats.frames.pcm ?? 0,
    fileFrames: stats.frames.file ?? 0,
    rateFrames: stats.frames.rate ?? 0,
    binaryFrames: stats.forms.binary,
    arrayFrames: stats.forms.array,
    bytes: stats.bytes,
    signature: stats.signature,
    audioSeconds: Number(stats.audioSeconds.toFixed(3)),
    elapsedSeconds: Number(elapsedSeconds.toFixed(3)),
    stopped: stats.stopped,
    failed: stats.failed,
    microphoneOpen,
    vadOpen,
  };
  // VOM-FR-NFQT: audio that ended in less than half of its length did not play.
  const tooShort =
    !stats.stopped && !stats.failed && stats.audioSeconds > 0 && elapsedSeconds < stats.audioSeconds / 2;
  if (tooShort) logWarn(["frontend"], "a voice segment ended before its audio could play", fields);
  else logDebug(["frontend"], "a voice segment ended", fields);
  stats.onEnd?.(stats);
}

/** VOM-FR-OXLD: a frame that playback could not read, by its shape alone. */
export function logDroppedFrame(frame: unknown, bytes: number | null): void {
  logWarn(["frontend"], "a voice frame was dropped", {
    form: frame instanceof ArrayBuffer ? "binary" : Array.isArray(frame) ? "array" : typeof frame,
    bytes,
  });
}

/** VOM-FR-ULPY: the creation and each state change of the audio context. */
export function logContextState(context: AudioContext, event: "created" | "state" | "replaced"): void {
  const latency = context as AudioContext & { outputLatency?: number; baseLatency?: number };
  logDebug(["frontend"], "the audio context of playback changed", {
    event,
    state: context.state ?? "unknown",
    sampleRate: context.sampleRate ?? 0,
    baseLatency: typeof latency.baseLatency === "number" ? latency.baseLatency : null,
    outputLatency: typeof latency.outputLatency === "number" ? latency.outputLatency : null,
    sinkIdSupported:
      typeof (context as { setSinkId?: unknown }).setSinkId === "function",
  });
}

/** VOM-FR-OXLD: a playback error, with its name and the frame kind. */
export function logPlaybackError(error: unknown, kind: number): void {
  const name = error instanceof Error || error instanceof DOMException ? error.name : typeof error;
  logWarn(["frontend"], "voice playback failed", {
    error: name,
    frameKind: KIND_NAMES[kind] ?? `kind${kind}`,
  });
}

/** VOM-FR-IQNQ: the speaker in use, at the first segment of an entry. */
export function logSpeaker(fields: {
  device: string;
  saved: boolean;
  found: boolean | null;
  sinkIdSupported: boolean;
}): void {
  logInfo(["frontend"], "the speaker of voice playback", fields);
}

export function resetPlaybackDiagnosticsForTest(): void {
  environment = () => ({ microphoneOpen: false, vadOpen: false });
}
