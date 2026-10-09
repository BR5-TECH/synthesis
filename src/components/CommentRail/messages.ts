/**
 * What a refused comment operation and a failed agent turn say to the author.
 *
 * Kept apart from the rail so the rules stay readable and testable on their own.
 */
import { tlsFailureMessage } from "../../tlsError";
import { AGENT_TURN_FAILURES, COMMENT_ERRORS, type AgentTurn } from "../../types";

/**
 * The part of a refused dispatch that is safe to log: the typed failure code,
 * or `"unexpected"`. Any other text can be an argument error that quotes the
 * value it refused, and an origin can hold a quoted passage, which is user
 * content.
 */
export function loggableTurnFailure(raw: string): string {
  return (Object.values(AGENT_TURN_FAILURES) as string[]).includes(raw)
    ? raw
    : "unexpected";
}

/**
 * CTA-FR-EXVN: the one detail a failed contribution names — the host and the
 * cause of a refused certificate. `null` for every other failure, which names
 * nothing but the Retry beside it.
 */
export function failedTurnDetail(turn: AgentTurn): string | null {
  if (turn.failure !== AGENT_TURN_FAILURES.tlsUntrusted || !turn.tlsFailure) {
    return null;
  }
  return tlsFailureMessage(turn.tlsFailure);
}

/**
 * CTA-FR-IGNT: what a failed turn says at the foot of its card.
 *
 * The typed failures are matched on rather than printed, for the same reason
 * CMT-FR-34's are: each names a different thing for the author to do next — fix
 * a provider, unlock a thread, or simply ask again.
 */
export function turnFailureMessage(raw: string): string {
  switch (raw) {
    case AGENT_TURN_FAILURES.agentNotFound:
      return "That agent is not enrolled in this project.";
    case AGENT_TURN_FAILURES.agentUnavailable:
      return "That agent's provider or model is no longer available. Check Global settings → Agents.";
    case AGENT_TURN_FAILURES.discussionLocked:
      return "This thread was locked, so the agent could not answer.";
    case AGENT_TURN_FAILURES.contextUnavailable:
      return "The material under discussion could not be read, so the agent was not asked.";
    case AGENT_TURN_FAILURES.unreachable:
      return "The agent's provider could not be reached.";
    case AGENT_TURN_FAILURES.rejected:
      return "The agent's provider refused the credentials. Re-verify it in Global settings → AI API.";
    case AGENT_TURN_FAILURES.timedOut:
      return "The agent did not answer in time.";
    case AGENT_TURN_FAILURES.emptyReply:
      return "The agent answered with nothing.";
    case AGENT_TURN_FAILURES.keychainUnavailable:
      return "The system keychain is unavailable, so the agent could not be reached.";
    case AGENT_TURN_FAILURES.tlsUntrusted:
      return "The certificate of the agent's provider is not trusted.";
    default:
      return raw;
  }
}

/**
 * CMT-FR-34: what a refused operation says to the author.
 *
 * The wire strings are matched on rather than printed: `discussion_locked` is a fact
 * about what someone else did and is recoverable by reloading, while
 * `invalid_fragment` is not something the author can act on at all. Rendering the
 * slug would leak an implementation detail and tell them neither.
 */
export function commentErrorMessage(raw: string): string {
  switch (raw) {
    case COMMENT_ERRORS.discussionLocked:
      return "This thread was locked, so it takes no new comments.";
    case COMMENT_ERRORS.discussionNotFound:
      return "This thread is no longer in the artifact's comments.";
    case COMMENT_ERRORS.quotedCommentNotInDiscussion:
      return "A quoted message is not part of this thread.";
    case COMMENT_ERRORS.invalidFragment:
      return "That selection could not be anchored. Select the passage again.";
    case COMMENT_ERRORS.artifactNotFound:
      return "This file is no longer in the project, so it cannot be discussed.";
    case COMMENT_ERRORS.draftNotFound:
      return "This draft is no longer in the project.";
    case COMMENT_ERRORS.noteNotFound:
      return "This note is no longer in the project, so it cannot be discussed.";
    case COMMENT_ERRORS.emptyBody:
      return "Write an opening message to start this conversation.";
    case COMMENT_ERRORS.notSupported:
      return "This conversation cannot be started that way.";
    case COMMENT_ERRORS.storeUnavailable:
      return "Conversations for this repository could not be reached, so nothing was read or written.";
    default:
      return raw;
  }
}
