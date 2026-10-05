/**
 * The current project identity every discussion surface labels the local
 * participant's comments with (`CMT-comments.md` CMT-FR-ZCAE).
 *
 * A comment written without a GitHub token is stamped with the fixed local
 * participant and keeps that stamp for good (CMS-FR-KTHN). What a surface shows
 * for it is not stored anywhere: it is the GitHub login the project resolves to
 * now, or **Me** while none resolves. This store holds that login, reads it
 * through `"resolve comment author identity"`, and re-reads it when
 * `"github tokens changed"` arrives (GTS-FR-AEQO) or the project changes. No
 * comment record is read or rewritten to do so.
 *
 * The login is kept while nothing renders, and re-read whenever the first
 * subscriber arrives, so a re-parented conversation shows the last known label
 * rather than flashing **Me** (CVP-FR-TIBK).
 */
import { useCallback, useSyncExternalStore } from "react";
import { resolveCommentAuthorIdentity } from "../api";
import { onGithubTokensChanged } from "../events";
import { logDebug } from "../logging";
import {
  isLocalParticipant,
  LOCAL_PARTICIPANT_NAME,
  participantName,
  type Participant,
} from "../types";

/** The login the project resolves to, or `null` while none resolves. */
let login: string | null = null;
/** Bumped by every read, so only the newest answer is applied. */
let generation = 0;
let listening = false;
let unlisten: (() => void) | null = null;

const listeners = new Set<() => void>();

function emit(): void {
  for (const listener of listeners) listener();
}

async function refresh(): Promise<void> {
  const mine = ++generation;
  let next: string | null = null;
  try {
    const identity = await resolveCommentAuthorIdentity();
    if (identity.kind === "human" && !isLocalParticipant(identity)) {
      next = identity.login;
    }
  } catch {
    // Every refusal — a binding still required, an unverified token, a project
    // that is not open — shows **Me**. It is a state of the display rather than
    // an error of this store, so it is not logged.
  }
  if (mine !== generation || next === login) return;
  login = next;
  emit();
}

function startListening(): void {
  if (listening) return;
  listening = true;
  void Promise.resolve(
    onGithubTokensChanged(() => {
      logDebug(["frontend"], "github tokens changed; re-reading project identity");
      void refresh();
    }),
  )
    .then((stop) => {
      const off = typeof stop === "function" ? stop : null;
      if (listeners.size === 0) {
        off?.();
        listening = false;
      } else {
        unlisten = off;
      }
    })
    .catch(() => {
      listening = false;
    });
}

function subscribe(listener: () => void): () => void {
  const first = listeners.size === 0;
  listeners.add(listener);
  if (first) {
    startListening();
    void refresh();
  }
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0 && unlisten) {
      unlisten();
      unlisten = null;
      listening = false;
    }
  };
}

function snapshot(): string | null {
  return login;
}

/**
 * CVP-FR-49: the project changed, so the login of the outgoing project goes and
 * the incoming project's binding is read.
 */
export function resetProjectIdentity(): void {
  login = null;
  emit();
  if (listeners.size > 0) void refresh();
}

/** The login the project resolves to now, followed in React; `null` for none. */
export function useProjectIdentityLogin(): string | null {
  return useSyncExternalStore(subscribe, snapshot, snapshot);
}

/**
 * The one author label every comment surface renders (CMT-FR-ZCAE): the local
 * participant reads as the current project identity or **Me**, and every other
 * participant reads as it was stamped.
 */
export function useParticipantLabel(): (author: Participant) => string {
  const projectLogin = useProjectIdentityLogin();
  return useCallback(
    (author: Participant) =>
      isLocalParticipant(author)
        ? (projectLogin ?? LOCAL_PARTICIPANT_NAME)
        : participantName(author),
    [projectLogin],
  );
}
