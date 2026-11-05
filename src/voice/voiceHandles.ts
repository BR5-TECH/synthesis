/**
 * What the surfaces of a discussion lend to voice mode (`VOM-voice-mode.md`
 * VOM-FR-BCRD).
 *
 * A composer registers a handle while it is mounted. Voice mode is bound to the
 * discussion and not to a mounted composer, so when the last composer of the
 * voice discussion unmounts, its handle stays as the **retained** handle. The
 * retained handle posts, tags, and names voices as the composer did, through the
 * same path as a typed comment (VOM-FR-WYIE).
 */
import { getVoiceState } from "../state/voiceSession";
import type { Discussion, DiscussionTarget, VoiceOptions } from "../types";

export type PostOutcome = "posted" | "reached_nobody" | "busy" | "failed";

/** What a mounted composer lends to voice mode. */
export interface VoiceComposerHandle {
  /** VOM-FR-RFWC: post the session body, then this text. */
  post(spoken: string): Promise<PostOutcome>;
  /**
   * VOM-FR-YTKV, VOM-FR-FUFE: post this text alone as the comment of a spoken
   * answer. No session body, no quote, and no attachment go with it, and the
   * session stays as it is. Absent on a surface that cannot post.
   */
  postAnswer?(text: string): Promise<PostOutcome>;
  /** VOM-FR-TAQJ, VOM-FR-SBSS. Gives the text with its spoken address as a tag. */
  tagged(transcript: string): string;
  /** VOM-FR-ZUWW. */
  blocked(): boolean;
  /** VOM-FR-LNUH. */
  nicknames(): string[];
  /** VOM-FR-QWMK: what the discussion is about, for its document language. */
  target?(): DiscussionTarget | null;
  /** VOM-FR-UALP. */
  voiceOf(agentId: string): string | null;
  /** VOM-FR-UALP, AGR-FR-EPYX: the speed and the style of the agent's voice. */
  voiceOptionsOf?(agentId: string): VoiceOptions | null;
  /**
   * VOM-FR-RNBS, CTA-FR-QWLB: the agents that a typed comment of this text
   * reaches. Absent on a surface that cannot post.
   */
  targets?(text: string): string[];
  /**
   * False for a surface that cannot post, such as the session control of a
   * question set (VOM-FR-VGLT). Such a handle is never retained.
   */
  posts?: boolean;
}

const mounted = new Map<string, VoiceComposerHandle[]>();
/** VOM-FR-BCRD: the handle of the voice discussion while none is mounted. */
const retained = new Map<string, VoiceComposerHandle>();
/**
 * VOM-FR-NERG: a handle that a first post moved, to the discussion that the post
 * created. It is kept per handle and not per composer, so each moved handle
 * posts into that discussion, also the handle of a composer that did not post.
 * A composer that mounts later for the same target key is not moved.
 */
let bound = new WeakMap<VoiceComposerHandle, Discussion>();

/** The newest mounted handle of a key, or else its retained handle. */
export function handleOf(key: string | null): VoiceComposerHandle | null {
  if (key === null) return null;
  const list = mounted.get(key);
  return list?.[list.length - 1] ?? retained.get(key) ?? null;
}

/** The newest handle of a key that can post. */
export function postingHandleOf(key: string | null): VoiceComposerHandle | null {
  if (key === null) return null;
  const list = (mounted.get(key) ?? []).filter((h) => h.posts !== false);
  return list[list.length - 1] ?? retained.get(key) ?? null;
}

/** A surface of this key is on screen. */
export function surfaceMounted(key: string | null): boolean {
  return key !== null && (mounted.get(key)?.length ?? 0) > 0;
}

export function addHandle(key: string, handle: VoiceComposerHandle): void {
  mounted.set(key, [...(mounted.get(key) ?? []), handle]);
  if (handle.posts !== false) retained.delete(key);
}

/** VOM-FR-NERG: the discussion that the first post created, for a handle that it moved. */
export function boundDiscussionOf(handle: VoiceComposerHandle): Discussion | null {
  return bound.get(handle) ?? null;
}

export function removeHandle(registeredAs: string, handle: VoiceComposerHandle): void {
  // A handle whose key moved (VOM-FR-NERG) is held under the discussion id.
  const key =
    [...mounted.keys()].find((k) => mounted.get(k)?.includes(handle)) ?? registeredAs;
  const left = (mounted.get(key) ?? []).filter((h) => h !== handle);
  if (left.length > 0) mounted.set(key, left);
  else mounted.delete(key);
  const postingLeft = left.some((h) => h.posts !== false);
  if (handle.posts !== false && !postingLeft && getVoiceState().activeKey === key) {
    retained.set(key, handle);
  }
}

/**
 * VOM-FR-NERG: the first post of an opening composer moves the key. Its
 * handles now stand for the created discussion, behind a composer of that
 * discussion that mounts later, and a handle that unmounts stays as the
 * retained handle of that discussion.
 */
export function moveHandles(from: string, created: Discussion): void {
  const to = created.id;
  const list = mounted.get(from);
  if (list) {
    list.forEach((h) => bound.set(h, created));
    mounted.delete(from);
    mounted.set(to, [...list, ...(mounted.get(to) ?? [])]);
  }
  const handle = retained.get(from);
  if (!handle) return;
  bound.set(handle, created);
  retained.delete(from);
  if (!surfaceMounted(to)) retained.set(to, handle);
}

/** Voice mode ended, so no handle is kept for it. */
export function forgetRetained(): void {
  retained.clear();
}

export function resetVoiceHandlesForTest(): void {
  mounted.clear();
  retained.clear();
  bound = new WeakMap();
}
