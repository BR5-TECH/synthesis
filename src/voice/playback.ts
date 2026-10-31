/**
 * VOM-FR-SJYH, VOM-FR-QDWW: one queue of audio, with two inputs. A PCM frame
 * becomes an `AudioBuffer` at once, and an encoded file passes through
 * `decodeAudioData`. Both play at exact times, so nothing clicks between frames.
 */

/** VOP-FR-XZET. */
const FRAME_PCM = 0;
const HEADER = 5;
/** A small lead, so that the first frame does not start in the past. */
const LEAD_SECONDS = 0.05;

let context: AudioContext | null = null;
let nextStart = 0;
let generation = 0;
let decoding: Promise<void> = Promise.resolve();
let pendingDecodes = 0;
const sources = new Set<AudioBufferSourceNode>();
let onDrained: (() => void) | null = null;

/** VOM non-functional: created by the activation of the Voice button. */
export function prepareAudio(): void {
  context ??= new AudioContext();
}

export function setDrainedListener(listener: (() => void) | null): void {
  onDrained = listener;
}

function schedule(buffer: AudioBuffer): void {
  if (!context) return;
  const source = context.createBufferSource();
  source.buffer = buffer;
  source.connect(context.destination);
  const start = Math.max(nextStart, context.currentTime + LEAD_SECONDS);
  source.onended = () => {
    sources.delete(source);
    if (sources.size === 0 && pendingDecodes === 0) onDrained?.();
  };
  sources.add(source);
  source.start(start);
  nextStart = start + buffer.duration;
}

function bufferOfPcm(payload: ArrayBuffer, rate: number): AudioBuffer | null {
  if (!context) return null;
  const pcm = new Int16Array(payload);
  if (pcm.length === 0) return null;
  const buffer = context.createBuffer(1, pcm.length, rate);
  const channel = buffer.getChannelData(0);
  for (let i = 0; i < pcm.length; i += 1) channel[i] = pcm[i] / 32768;
  return buffer;
}

/** VOP-FR-XZET: queue one frame. Frames play in the order of these calls. */
export function playFrame(frame: ArrayBuffer): void {
  prepareAudio();
  const context_ = context;
  if (!context_ || frame.byteLength < HEADER) return;
  void context_.resume();
  const view = new DataView(frame);
  const kind = view.getUint8(0);
  const rate = view.getUint32(1, true);
  const payload = frame.slice(HEADER);
  const mine = generation;
  pendingDecodes += 1;
  // A chain, so that a PCM frame cannot pass a file that is still decoding.
  decoding = decoding
    .then(async () => {
      const buffer =
        kind === FRAME_PCM ? bufferOfPcm(payload, rate) : await context_.decodeAudioData(payload);
      if (buffer && mine === generation) schedule(buffer);
    })
    .catch(() => undefined)
    .finally(() => {
      pendingDecodes -= 1;
      if (sources.size === 0 && pendingDecodes === 0) onDrained?.();
    });
}

/** VOM-FR-QDWW: silence at once, and frames in flight are dropped. */
/** Nothing plays, and nothing waits to play. */
export function isPlaybackIdle(): boolean {
  return sources.size === 0 && pendingDecodes === 0;
}

export function stopPlayback(): void {
  generation += 1;
  nextStart = 0;
  for (const source of sources) {
    source.onended = null;
    try {
      source.stop();
    } catch {
      // A source that already ended has nothing to stop.
    }
  }
  sources.clear();
}

export function resetPlaybackForTest(): void {
  stopPlayback();
  context = null;
  decoding = Promise.resolve();
  pendingDecodes = 0;
  onDrained = null;
}
