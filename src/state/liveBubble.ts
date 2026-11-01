/**
 * The live bubble of the voice discussion (`VOM-voice-mode.md` VOM-FR-NCHC,
 * `CVP-conversation-presentation.md` CVP-FR-TGPV, CVP-FR-GHWN).
 *
 * It holds one part for each chunked utterance that has not settled. A part
 * joins the chunk transcripts in index order, and an open chunk can show live
 * text after them (VOM-FR-HUXY). It is local and in memory only: it is not a
 * comment and nothing saves it (CVP-FR-48).
 */
import { useSyncExternalStore } from "react";

export interface LiveChunk {
  index: number;
  /** The chunk transcript. `null` until it comes, or when the chunk failed. */
  text: string | null;
  /** VOM-FR-MBHB: the failure code of a chunk that did not transcribe. */
  failed: string | null;
}

export interface LivePart {
  captureId: string;
  chunks: LiveChunk[];
  /** VOM-FR-HUXY: the live text of the open chunk. */
  liveText: string;
}

export interface LiveBubble {
  discussionId: string | null;
  parts: LivePart[];
}

const EMPTY: LiveBubble = Object.freeze({ discussionId: null, parts: [] });

let bubble: LiveBubble = EMPTY;
const listeners = new Set<() => void>();

function publish(next: LiveBubble): void {
  bubble = next;
  listeners.forEach((listener) => listener());
}

function patchPart(captureId: string, change: (part: LivePart) => LivePart): void {
  if (!bubble.parts.some((p) => p.captureId === captureId)) return;
  publish({
    ...bubble,
    parts: bubble.parts.map((p) => (p.captureId === captureId ? change(p) : p)),
  });
}

export function getLiveBubble(): LiveBubble {
  return bubble;
}

/** A chunked utterance of this discussion started. */
export function openLivePart(discussionId: string, captureId: string): void {
  const parts = bubble.discussionId === discussionId ? bubble.parts : [];
  if (parts.some((p) => p.captureId === captureId)) return;
  publish({ discussionId, parts: [...parts, { captureId, chunks: [], liveText: "" }] });
}

function withChunk(part: LivePart, chunk: LiveChunk): LivePart {
  const chunks = [...part.chunks.filter((c) => c.index !== chunk.index), chunk].sort(
    (a, b) => a.index - b.index,
  );
  return { ...part, chunks, liveText: "" };
}

/** VOM-FR-NCHC: a chunk transcript came. */
export function noteLiveChunk(captureId: string, index: number, text: string): void {
  patchPart(captureId, (part) => withChunk(part, { index, text, failed: null }));
}

/** VOM-FR-MBHB: a chunk failed, and its error shows at its place. */
export function noteLiveChunkFailed(captureId: string, index: number, code: string): void {
  patchPart(captureId, (part) => withChunk(part, { index, text: null, failed: code }));
}

/** VOM-FR-HUXY. */
export function noteLiveText(captureId: string, text: string): void {
  patchPart(captureId, (part) => (part.liveText === text ? part : { ...part, liveText: text }));
}

/** The utterance settled: its comment replaces its part, or the part goes. */
export function dropLivePart(captureId: string): void {
  if (!bubble.parts.some((p) => p.captureId === captureId)) return;
  const parts = bubble.parts.filter((p) => p.captureId !== captureId);
  publish(parts.length === 0 ? EMPTY : { ...bubble, parts });
}

/** VOM-FR-UXZT, VOM-FR-KQAI. */
export function clearLiveBubble(): void {
  if (bubble !== EMPTY) publish(EMPTY);
}

/** A part holds something to show. */
export function partShows(part: LivePart): boolean {
  return part.liveText !== "" || part.chunks.some((c) => c.text || c.failed);
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** CVP-FR-TGPV: the parts that show in this discussion, and in no other. */
export function useLiveBubble(discussionId: string): LivePart[] {
  const current = useSyncExternalStore(subscribe, getLiveBubble);
  return current.discussionId === discussionId ? current.parts : EMPTY.parts;
}

export function resetLiveBubbleForTest(): void {
  bubble = EMPTY;
}
