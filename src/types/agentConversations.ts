// Agent conversations (agent_conversations.rs / AGC)
//
// Split out of one `types.ts` that had grown past four thousand lines.
// Every name is re-exported from `./index`, so `from "../types"` still
// resolves to the same set and no import site moved.

import type { TlsCause } from "../tlsError";
import { AI_ERRORS } from "./aiApi";
import type { DiscussionTarget, FragmentTarget } from "./comments";

// --- Agent conversations (agent_conversations.rs / AGC) -------------------

/**
 * AGC-FR-05: which conversation a turn belongs to. The discussion's id, its
 * owner target, and its fragment target where it has one. The kind of `target`
 * selects the backend's context builder, and a fragment target adds a branch to
 * none. The backend reads `target` and `fragmentTarget` again from the
 * discussion that `discussionId` names, so a caller cannot misstate either.
 *
 * The shape is the backend's `ConversationOrigin` struct, field for field:
 * `src/test/contracts/conversationOrigin.json` holds it for both sides' tests.
 */
export interface ConversationOrigin {
  discussionId: string;
  target: DiscussionTarget;
  fragmentTarget?: FragmentTarget | null;
}

/**
 * AGC-FR-28: `awaiting_reply` is the one state that is terminal and outstanding
 * at once.
 *
 * *Terminal*: the turn is over and everything it held — its exchange, its
 * session, its slot — was discarded, so it renders no pending contribution
 * (CTA-FR-KVIF) and its agent is not answering (AGT-FR-05). *Outstanding*:
 * `listAgentTurns` keeps returning it, because it is what tells the next comment
 * in that conversation which agent is owed a reply (CTA-FR-JQDM).
 */
export type AgentTurnState =
  | "running"
  | "delivered"
  | "awaiting_reply"
  | "failed"
  | "cancelled";

/** AGC-FR-15: every way a turn can fail, each calling for its own correction. */
export const AGENT_TURN_FAILURES = {
  /** The nickname resolves to no agent enrolled in this project. */
  agentNotFound: "agent_not_found",
  /** It resolves, but its provider or model no longer serves it. */
  agentUnavailable: "agent_unavailable",
  /** The conversation takes no further contribution. */
  discussionLocked: "discussion_locked",
  /** None of the material the builder needs could be read. */
  contextUnavailable: "context_unavailable",
  unreachable: "unreachable",
  rejected: "rejected",
  timedOut: "timed_out",
  emptyReply: "empty_reply",
  keychainUnavailable: "keychain_unavailable",
  /**
   * AGC-FR-RWPT: the provider's certificate is not trusted. The turn also
   * carries `tlsFailure`, which names the host and the cause.
   */
  tlsUntrusted: "tls_untrusted",
  /**
   * CVL-FR-21: the provider answered with a reply that could not be read.
   * Recoverable: the author can retry, but the loop does not repeat it.
   */
  invalidResponse: "invalid_response",
  /**
   * CVL-FR-21: the app could not build the request, so nothing was sent.
   * Recoverable: the author can retry, but the loop does not repeat it.
   */
  invalidRequest: "invalid_request",
} as const;

export type AgentTurnFailure =
  (typeof AGENT_TURN_FAILURES)[keyof typeof AGENT_TURN_FAILURES];

/** AGC-FR-01: one agent, answering one message, in one conversation. */
export interface AgentTurn {
  /** Unique for the lifetime of the running application (AGC-FR-23). */
  id: string;
  agentId: string;
  nickname: string;
  origin: ConversationOrigin;
  /** The comment that addressed the agent. */
  triggerCommentId: string;
  state: AgentTurnState;
  failure: AgentTurnFailure | null;
  /**
   * AGC-FR-RWPT: the host and the cause of a refused certificate. Present only
   * when `failure` is `tls_untrusted`.
   */
  tlsFailure?: { host: string; cause: TlsCause } | null;
  /**
   * AGC-FR-31: true only while this turn is its conversation's **current**
   * recoverable failure.
   *
   * What tells a card whether to render a failed contribution with a Retry
   * (CTA-FR-MGVJ) or an inline typed failure with none (CTA-FR-IGNT). Not derivable
   * from `failure`: `timed_out` names both the retryable provider-call deadline
   * and the whole-turn one that is not (CVL-FR-17, CVL-FR-18).
   */
  retryPermitted: boolean;
  startedAt: string;
  /** `null` while running. */
  endedAt: string | null;
  /**
   * AGC-FR-33: the tool calls **active** in this turn — those the loop has
   * begun and has not yet finished — ordered by activation.
   *
   * What is happening now and never what has happened: a turn that has made no
   * tool call carries an empty list, and a turn in any terminal state carries
   * an empty list whatever was active when it ended (AGC-FR-34). It is what a
   * pending contribution reads its activity status from (CTA-FR-FBJR,
   * CTA-FR-IWOJ).
   */
  activeToolCalls: ActiveToolCall[];
  /**
   * AGC-FR-37: true where the turn sent **text and safe image metadata** in
   * place of the pictures its material held, because the selected provider and
   * model do not take image content (AGC-FR-36).
   *
   * A **status and not an error**: the conversation is answered, the answer is
   * an ordinary comment, and the turn carries no failure and no offer to retry.
   * It is what the rail renders its unsupported-image notice from
   * (CTA-FR-ARBB), learnt from the terminal `"agent turn state changed"` event
   * and, for an instance mounted after the turn ended, from
   * `listAgentTurnImageNotices` (AGC-FR-39).
   */
  imagesOmitted: boolean;
}

/**
 * AGC-FR-33: one tool call the loop has begun and has not yet finished.
 *
 * The tool's name and its order and nothing else — no argument the model
 * composed, no result a tool produced, and no part of the exchange (CVL-FR-26,
 * CVL-FR-27).
 */
export interface ActiveToolCall {
  /** Unique within the turn, and never reused within it. */
  id: string;
  /**
   * The tool the model called — a portable tool's own name, and a
   * provider-native tool's entry type (TLC-FR-02, TLC-FR-21).
   */
  tool: string;
  /**
   * An integer ascending with activation, unique within the turn, never
   * reused. What orders the calls, so a surface reads which one is the latest
   * from the record rather than from the order events reached it (CTA-FR-KWOF).
   */
  activationSeq: number;
}

/**
 * The typed refusals the agent registry returns. Matched on rather than merely
 * displayed: `nicknameTaken` renders against the nickname field while
 * `providerNotVerified` names the Conversational AI section as where it is
 * corrected (AGT-FR-13, AGT-FR-19).
 */
export const AGENT_ERRORS = {
  nicknameEmpty: "nickname_empty",
  nicknameInvalid: "nickname_invalid",
  /** The nickname is the reserved `@all` handle (AGR-FR-04, AGT-FR-41). */
  nicknameReserved: "nickname_reserved",
  nicknameTaken: "nickname_taken",
  providerUnconfigured: "provider_unconfigured",
  providerNotVerified: "provider_not_verified",
  agentNotFound: "agent_not_found",
  agentUnavailable: "agent_unavailable",
  noProjectOpen: "no_project_open",
  // Shared with the AI API level, which is what validates the selection
  // (AAP-FR-33) — a second spelling here would be silent drift.
  unknownModel: AI_ERRORS.unknownModel,
  unknownEffort: AI_ERRORS.unknownEffort,
  reasoningUnsupported: AI_ERRORS.reasoningUnsupported,
  reasoningMandatory: AI_ERRORS.reasoningMandatory,
} as const;
