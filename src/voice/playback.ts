/**
 * VOM-FR-SJYH, VOM-FR-QDWW: one queue of audio, with two inputs. A PCM frame
 * becomes an `AudioBuffer` at once, and an encoded file passes through
 * `decodeAudioData`. Both play at exact times, so nothing clicks between frames.
 *
 * VOM-FR-VGQF, VOM-FR-WFTM: an error never passes in silence, and a context
 * that does not run is replaced. `./playbackDiagnostics.ts` writes the log.
 */
import { logWarn } from "../logging";
import {
  audioSettings,
  listAudioDevices,
  matchDevice,
  reportDeviceFallback,
  sinkIdSupported,
} from "./audioDevices";
import {
  frameBytes,
  logContextState,
  logDroppedFrame,
  logPlaybackError,
  logSegment,
  logSpeaker,
  SegmentStats,
} from "./playbackDiagnostics";

/** VOP-FR-XZET. */
const FRAME_PCM = 0;
/** VOP-FR-EMJY: the playback rate of the frames that follow, times 1000. */
export const FRAME_RATE = 3;
const HEADER = 5;
/** A small lead, so that the first frame does not start in the past. */
const LEAD_SECONDS = 0.05;
/** VOM-FR-WFTM: the time a context has to run after a resume. */
export const RUN_LIMIT_MS = 2000;
/** VOM-FR-VGQF. */
export const PLAYBACK_FAILED = "playback_failed";

/** A frame as the channel gives it (VOM-FR-DYTO). */
export type VoiceFrame = ArrayBuffer | ArrayBufferView | number[];

/** A typed reason, which fails the segment with `playback_failed`. */
class PlaybackError extends Error {}

type SinkContext = AudioContext & { setSinkId?: (id: string) => Promise<void> };

let context: SinkContext | null = null;
let nextStart = 0;
let generation = 0;
let decoding: Promise<void> = Promise.resolve();
let pendingDecodes = 0;
const sources = new Map<AudioBufferSourceNode, SegmentStats | null>();
let onDrained: (() => void) | null = null;
let onFailed: ((code: string) => void) | null = null;
/** VOM-FR-QUSH: the rate of the segment that plays now. */
let playbackRate = 1;
/** VOM-FR-FVRS: the clock of the context stands, so queued audio waits. */
let paused = false;
/** VOM-FR-EPBL: the segment whose frames come now. */
let openSegment: SegmentStats | null = null;
/** VOM-FR-KKNN: the saved speaker that the context uses, as a key. */
let appliedSink: string | null = null;
/** VOM-FR-IQNQ: the next segment logs the speaker. */
let announceSpeaker = true;

function createContext(): SinkContext {
  const created: SinkContext = new AudioContext();
  appliedSink = null;
  logContextState(created, "created");
  // VOM-FR-ULPY.
  created.onstatechange = () => logContextState(created, "state");
  return created;
}

/** VOM non-functional: created by the activation of the Voice button. */
export function prepareAudio(): void {
  if (!context || context.state === "closed") context = createContext();
}

/** VOM-FR-IQNQ: a new entry into voice mode logs the speaker again. */
export function notePlaybackEntry(): void {
  announceSpeaker = true;
}

export function setDrainedListener(listener: (() => void) | null): void {
  onDrained = listener;
}

/** VOM-FR-VGQF: a frame that could not play fails its segment. */
export function setPlaybackFailureListener(listener: ((code: string) => void) | null): void {
  onFailed = listener;
}

function idle(): boolean {
  return sources.size === 0 && pendingDecodes === 0;
}

function settle(stats: SegmentStats | null): void {
  if (!stats) return;
  stats.pending -= 1;
  if (stats.inputDone && stats.pending <= 0) logSegment(stats);
}

/** VOM-FR-EPBL: the frames that follow belong to a new segment. */
export function beginSegment(source: string): void {
  endSegmentInput();
  openSegment = new SegmentStats(source);
}

/** VOM-FR-EPBL: every frame of the open segment came. */
export function endSegmentInput(): void {
  const stats = openSegment;
  if (!stats) return;
  openSegment = null;
  stats.inputDone = true;
  if (stats.pending <= 0) logSegment(stats);
}

function schedule(target: AudioContext, buffer: AudioBuffer, stats: SegmentStats | null): void {
  const source = target.createBufferSource();
  source.buffer = buffer;
  // VOM-FR-QUSH: the rate changes the length that the buffer takes.
  source.playbackRate.value = playbackRate;
  source.connect(target.destination);
  const start = Math.max(nextStart, target.currentTime + LEAD_SECONDS);
  source.onended = () => {
    sources.delete(source);
    settle(stats);
    if (idle()) onDrained?.();
  };
  sources.set(source, stats);
  if (stats) stats.pending += 1;
  source.start(start);
  nextStart = start + buffer.duration / playbackRate;
  stats?.noteScheduled(buffer.duration / playbackRate);
}

function bufferOfPcm(target: AudioContext, payload: ArrayBuffer, rate: number): AudioBuffer | null {
  const pcm = new Int16Array(payload, 0, Math.floor(payload.byteLength / 2));
  if (pcm.length === 0) return null;
  const buffer = target.createBuffer(1, pcm.length, rate);
  const channel = buffer.getChannelData(0);
  for (let i = 0; i < pcm.length; i += 1) channel[i] = pcm[i] / 32768;
  return buffer;
}

/** A step, or `false` when it does not settle within the limit. */
function within(step: Promise<unknown>, ms: number): Promise<boolean> {
  return new Promise((resolve) => {
    const limit = setTimeout(() => resolve(false), ms);
    step.then(
      () => {
        clearTimeout(limit);
        resolve(true);
      },
      () => {
        clearTimeout(limit);
        resolve(false);
      },
    );
  });
}

async function runs(target: AudioContext): Promise<boolean> {
  if (target.state === "running") return true;
  if (target.state === "closed") return false;
  await within(target.resume(), RUN_LIMIT_MS);
  return (target.state as string) === "running";
}

/** The sources of a context that went: they never end, so they count no more. */
function dropSources(): void {
  for (const [source, stats] of sources) {
    source.onended = null;
    try {
      source.stop();
    } catch {
      // A source of a closed context has nothing to stop.
    }
    settle(stats);
  }
  sources.clear();
  nextStart = 0;
}

/**
 * VOM-FR-WFTM: the context that plays, resumed. A closed context, or one that
 * does not run in time, is replaced one time.
 */
async function runningContext(): Promise<AudioContext> {
  // A closed context stays here, so that its replacement is logged below.
  context ??= createContext();
  const current = context;
  // VOM-FR-FVRS: a paused reply keeps its context suspended on purpose.
  if (paused || (await runs(current))) return current;
  if (context !== current) return context as SinkContext;
  logWarn(["frontend"], "the audio context of playback did not run, so it was replaced", {
    state: current.state ?? "unknown",
  });
  dropSources();
  if (current.state !== "closed") void current.close().catch(() => undefined);
  context = createContext();
  logContextState(context, "replaced");
  await syncSpeaker(context);
  if (await runs(context)) return context;
  throw new PlaybackError("context_not_running");
}

/**
 * VOM-FR-KKNN, VOM-FR-GIES, VOM-FR-IQNQ: send the audio to the saved speaker
 * where the WebView can, and log the speaker at the first segment of an entry.
 */
async function syncSpeaker(target: SinkContext): Promise<void> {
  const { output } = audioSettings();
  const key = output ? `${output.id}\n${output.label}` : "";
  const supported = sinkIdSupported() && typeof target.setSinkId === "function";
  if (key === appliedSink && !announceSpeaker) return;
  const wasDefault = appliedSink === null || appliedSink === "";
  appliedSink = key;
  let device = "system default";
  let found: boolean | null = null;
  if (output && supported) {
    const match = matchDevice(output, await listAudioDevices("audiooutput"));
    found = false;
    if (match) {
      try {
        await target.setSinkId?.(match.id);
        found = true;
        device = match.label;
      } catch (error) {
        logPlaybackError(error, -1);
      }
    }
    if (!found) {
      await target.setSinkId?.("").catch(() => undefined);
      reportDeviceFallback("speaker");
    }
  } else if (supported && !wasDefault) {
    await target.setSinkId?.("").catch(() => undefined);
  }
  if (announceSpeaker) {
    announceSpeaker = false;
    logSpeaker({ device, saved: output !== null, found, sinkIdSupported: supported });
  }
}

/** VOP-FR-XZET: queue one frame. Frames play in the order of these calls. */
export function playFrame(frame: VoiceFrame): void {
  const bytes = frameBytes(frame);
  if (!bytes || bytes.buffer.byteLength < HEADER) {
    // VOM-FR-OXLD: a frame that cannot play never goes in silence.
    logDroppedFrame(frame, bytes?.buffer.byteLength ?? null);
    return;
  }
  prepareAudio();
  if (!paused) void context?.resume().catch(() => undefined);
  const view = new DataView(bytes.buffer);
  const kind = view.getUint8(0);
  const rate = view.getUint32(1, true);
  const payload = bytes.buffer.slice(HEADER);
  const stats = openSegment;
  stats?.noteFrame(kind, bytes.form, bytes.buffer.byteLength, payload);
  const mine = generation;
  pendingDecodes += 1;
  if (stats) stats.pending += 1;
  // A chain, so that a PCM frame cannot pass a file that is still decoding.
  decoding = decoding
    .then(async () => {
      if (mine !== generation) return;
      // VOM-FR-QUSH: in the chain, so the rate applies to the frames after it.
      if (kind === FRAME_RATE) {
        if (rate > 0) playbackRate = rate / 1000;
        return;
      }
      if (context) await syncSpeaker(context);
      const target = await runningContext();
      const buffer =
        kind === FRAME_PCM
          ? bufferOfPcm(target, payload, rate)
          : await target.decodeAudioData(payload);
      if (buffer && mine === generation) schedule(target, buffer, stats);
    })
    .catch((error: unknown) => {
      if (mine !== generation) return;
      // VOM-FR-VGQF, VOM-FR-OXLD.
      logPlaybackError(error, kind);
      if (stats) stats.failed = true;
      onFailed?.(PLAYBACK_FAILED);
    })
    .finally(() => {
      pendingDecodes -= 1;
      settle(stats);
      if (idle()) onDrained?.();
    });
}

/** Nothing plays, and nothing waits to play. */
export function isPlaybackIdle(): boolean {
  return idle();
}

/** VOM-FR-FVRS: silence at once. Nothing is lost, and frames still queue. */
export function pausePlayback(): void {
  if (!context || paused) return;
  paused = true;
  void context.suspend().catch(() => undefined);
}

/** VOM-FR-KVHG: the audio goes on from where it paused. */
export function resumePlayback(): void {
  if (!paused) return;
  paused = false;
  void context?.resume().catch(() => undefined);
}

/** VOM-FR-QDWW: silence at once, and frames in flight are dropped. */
export function stopPlayback(): void {
  // A context that stays suspended would keep each later reply silent.
  resumePlayback();
  generation += 1;
  nextStart = 0;
  // VOM-FR-QUSH: Stop resets the rate.
  playbackRate = 1;
  const stopped = new Set<SegmentStats>();
  for (const [source, stats] of sources) {
    source.onended = null;
    if (stats) stopped.add(stats);
    try {
      source.stop();
    } catch {
      // A source that already ended has nothing to stop.
    }
  }
  sources.clear();
  if (openSegment) stopped.add(openSegment);
  openSegment = null;
  // VOM-FR-NFQT: a stopped segment is no sign of a fault.
  for (const stats of stopped) {
    stats.stopped = true;
    stats.inputDone = true;
    logSegment(stats);
  }
}

/** VAI-FR-BPUR: about 1 second of a 440 Hz tone, as one PCM frame. */
function toneFrame(): ArrayBuffer {
  const rate = 24_000;
  const samples = rate;
  const out = new Uint8Array(HEADER + samples * 2);
  const view = new DataView(out.buffer);
  view.setUint8(0, FRAME_PCM);
  view.setUint32(1, rate, true);
  const fade = rate * 0.05;
  for (let i = 0; i < samples; i += 1) {
    const envelope = Math.min(1, i / fade, (samples - i) / fade);
    const value = Math.sin((2 * Math.PI * 440 * i) / rate) * 0.3 * envelope;
    view.setInt16(HEADER + i * 2, Math.round(value * 32767), true);
  }
  return out.buffer;
}

/**
 * VAI-FR-BPUR: the test sound, through the playback of voice mode. It resolves
 * when the tone ended or was stopped, and rejects with `playback_failed`. It
 * settles on its own segment, so it changes no listener of another speech.
 */
export function playTestTone(): Promise<void> {
  stopPlayback();
  prepareAudio();
  announceSpeaker = true;
  return new Promise<void>((resolve, reject) => {
    beginSegment("tone");
    const stats = openSegment as SegmentStats;
    stats.onEnd = (ended) => (ended.failed ? reject(PLAYBACK_FAILED) : resolve());
    playFrame(toneFrame());
    endSegmentInput();
  });
}

export function resetPlaybackForTest(): void {
  stopPlayback();
  context = null;
  paused = false;
  decoding = Promise.resolve();
  pendingDecodes = 0;
  onDrained = null;
  onFailed = null;
  openSegment = null;
  appliedSink = null;
  announceSpeaker = true;
}
