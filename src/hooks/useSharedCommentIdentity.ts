/**
 * The identity a comment is attributed to, shared across every conversational
 * surface.
 *
 * It is **project-wide**: `"resolve comment author identity"` answers the same
 * for every conversation in the project (CMS-FR-13). A surface that read it on
 * mount would therefore re-read it every time a conversation is moved — and
 * `../../specifications/ui/CVP-conversation-presentation.md`'s non-functional
 * requirement is explicit that a transition is a re-parenting of a live
 * conversation rather than a remount of it: no transition re-reads anything, and
 * none of them costs a visible flash of an empty surface.
 *
 * So the read happens once per session and every presentation subscribes to the
 * result. Session-only, like the presentations themselves.
 */
import { useSyncExternalStore } from "react";
import { resolveCommentAuthorIdentity } from "../api";
import { onGithubTokensChanged } from "../events";
import type { Participant } from "../types";

interface SharedIdentity {
  identity: Participant | null;
  /** The typed error the last resolution rejected with, when it did. */
  identityError: string | null;
}

let state: SharedIdentity = {
  identity: null,
  identityError: null,
};
let asked = false;

const listeners = new Set<() => void>();
let unlistenTokens: (() => void) | null = null;
let listeningTokens = false;

function emit(next: SharedIdentity): void {
  state = next;
  for (const listener of listeners) listener();
}

function snapshot(): SharedIdentity {
  return state;
}

function load(): void {
  if (asked) return;
  asked = true;
  void resolveCommentAuthorIdentity()
    .then((identity) => emit({ ...state, identity, identityError: null }))
    .catch((e: unknown) => {
      // CMT-FR-24 … CMT-FR-26: the typed refusal is what the surface routes on —
      // a token to select, one to add, or a provider to wait for — so it is kept
      // rather than flattened to "unavailable".
      const text =
        typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
      emit({ ...state, identity: null, identityError: text });
    });
}

/**
 * CVP-FR-49: torn down with every presentation on a project or worktree change —
 * an identity resolves against the project's own credential binding.
 */
export function resetSharedCommentIdentity(): void {
  asked = false;
  emit({ identity: null, identityError: null });
}

/** Re-resolve after the author has done something about a refusal (CMT-FR-25). */
export function retrySharedCommentIdentity(): void {
  asked = false;
  load();
}

/**
 * GTS-FR-AEQO: a token or a binding changed, so the identity every presentation
 * writes as is read again. Listened to while any presentation subscribes.
 */
function listenForTokenChanges(): void {
  if (listeningTokens) return;
  listeningTokens = true;
  void Promise.resolve(onGithubTokensChanged(() => retrySharedCommentIdentity()))
    .then((stop) => {
      const off = typeof stop === "function" ? stop : null;
      if (listeners.size === 0) {
        off?.();
        listeningTokens = false;
      } else {
        unlistenTokens = off;
      }
    })
    .catch(() => {
      listeningTokens = false;
    });
}

export function useSharedCommentIdentity(): SharedIdentity {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      listenForTokenChanges();
      // Kicked off from subscribe rather than from an effect, so a caller gets
      // it by subscribing at all — every consumer of this wants the identity.
      load();
      return () => {
        listeners.delete(listener);
        if (listeners.size === 0 && unlistenTokens) {
          unlistenTokens();
          unlistenTokens = null;
          listeningTokens = false;
        }
      };
    },
    snapshot,
    snapshot,
  );
}
