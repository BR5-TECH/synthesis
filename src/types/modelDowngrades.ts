/**
 * Model downgrades (`../../specifications/core/AGC-agent-conversations.md`
 * AGC-FR-UVKF, AGC-FR-PNHM; `GSS-global-settings-storage.md` GSS-FR-DAKR).
 */
import type { ConversationOrigin } from "./agentConversations";

/** AGC-FR-PNHM: the payload of `"model downgrade detected"`. */
export interface ModelDowngrade {
  source: "agent" | "graduation";
  /** The model id the request named, exactly as sent. */
  requestedModel: string;
  /** The model id the response reported, exactly as received. */
  servedModel: string;
  provider: string;
  /** The key of the per-project slot (GSS-FR-18). */
  projectKey: string;
  /** The absolute path of the worktree the work runs in. */
  worktree: string;
  /** Source `agent`: the conversation of the turn. */
  origin: ConversationOrigin | null;
  /** Source `graduation`: the run. */
  runId: string | null;
}

/** GSS-FR-DAKR: one suppression rule. `expiresAt` absent is Forever. */
export interface ModelDowngradeSuppression {
  requestedModel: string;
  servedModel: string;
  /** RFC 3339 UTC. */
  expiresAt?: string | null;
}

/** GSS-FR-UUET / NTF-FR-JXQL: the four durations of the Suppress menu. */
export type SuppressionDuration = "hour" | "day" | "week" | "forever";
