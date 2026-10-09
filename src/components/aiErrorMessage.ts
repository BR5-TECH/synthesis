import { AI_ERRORS } from "../types";
import { tlsErrorMessage } from "../tlsError";

/**
 * AII-FR-DKDC, AII-FR-HOKG: the failures of Claude Code's Custom Gateway. Their
 * wire text carries a detail after the first colon — a status, a network cause,
 * a variable name, or an entry number — so they are read by prefix. The detail
 * is text the backend chose; nothing the author typed is quoted back.
 */
function gatewayErrorMessage(raw: string): string | null {
  const colon = raw.indexOf(":");
  const code = colon === -1 ? raw : raw.slice(0, colon);
  const detail = colon === -1 ? "" : raw.slice(colon + 1);
  switch (code) {
    case AI_ERRORS.gatewayStatus: {
      const hint =
        detail === "401" || detail === "403"
          ? " Check the token and the token variable name."
          : detail === "404"
            ? " Check the base URL."
            : "";
      return `The gateway answered with HTTP ${detail}.${hint}`;
    }
    case AI_ERRORS.gatewayUnreachable:
      return `The gateway could not be reached: ${detail || "no cause given"}`;
    case AI_ERRORS.gatewayNotAModelList:
      return "The gateway answered, but not with a model list.";
    case AI_ERRORS.tokenVarInvalid:
      return "The token variable name is not valid. Use letters, digits, and underscores, and do not start with a digit.";
    case AI_ERRORS.envVarInvalid:
      return `Environment variable entry ${detail} is not written as NAME=value.`;
    case AI_ERRORS.envVarReserved:
      return `${detail} is set by Synthesis and cannot be used as an environment variable here.`;
    default:
      return null;
  }
}

/**
 * Render a typed backend rejection as text that says what to do about it.
 *
 * The failures are deliberately distinguishable — a wrong path, a permissions
 * problem, an unreachable host, and a refused key each call for a different
 * response from the author (AII-FR-09 / AII-FR-20).
 */
export function aiErrorMessage(e: unknown): string {
  const raw = typeof e === "string" ? e : e instanceof Error ? e.message : "";
  // AII-FR-09 / AII-FR-20: a refused certificate names its host and its cause.
  const tls = tlsErrorMessage(raw);
  if (tls) return tls;
  const gateway = gatewayErrorMessage(raw);
  if (gateway) return gateway;
  switch (raw) {
    case AI_ERRORS.pathEmpty:
      return "Enter the path to the CLI before verifying.";
    case AI_ERRORS.notFound:
      return "Nothing exists at that path.";
    case AI_ERRORS.notExecutable:
      return "That file cannot be run. Check its permissions.";
    case AI_ERRORS.tokenMissing:
      return "This integration needs an OAuth token. Enter one and verify again.";
    case AI_ERRORS.tokenMalformed:
      // Deliberately says nothing about the value that provoked it (AII-FR-50).
      return "That is not a valid OAuth token.";
    case AI_ERRORS.notTheExpectedCli:
      return "That program is not this vendor's CLI.";
    case AI_ERRORS.executionFailed:
      return "That file could not be run at all.";
    case AI_ERRORS.baseUrlEmpty:
      return "Enter the endpoint's base URL before verifying.";
    case AI_ERRORS.baseUrlInvalid:
      return "That is not a valid URL. It should start with http:// or https://.";
    case AI_ERRORS.keyMissing:
      return "This provider needs an API key.";
    case AI_ERRORS.unreachable:
      return "That endpoint could not be reached. Check the URL and your network.";
    case AI_ERRORS.rejected:
      return "The endpoint refused that key.";
    case AI_ERRORS.notAnAiEndpoint:
      return "That URL answered, but not as a conversational API.";
    case AI_ERRORS.notAnAgentEndpoint:
      return "That URL answered, but not as an agent-execution endpoint.";
    case AI_ERRORS.timedOut:
      return "It did not answer in time and was stopped.";
    case AI_ERRORS.keychainUnavailable:
      return "The system keychain would not answer, so the key was not stored.";
    case AI_ERRORS.wrongConfigKind:
      return "That configuration does not match this integration's kind.";
    case AI_ERRORS.notACliIntegration:
      return "This integration has no binary to look for.";
    case AI_ERRORS.unknownModel:
      return "That model is not one this backend offers.";
    case AI_ERRORS.unknownEffort:
      return "That reasoning effort is not one this backend offers.";
    case AI_ERRORS.unknownTurnKind:
      return "That is not a kind of task this backend is given.";
    case AI_ERRORS.noModelSelected:
      return "Choose a model before setting how much it should reason.";
    case AI_ERRORS.reasoningUnsupported:
      return "That model does not support reasoning.";
    case AI_ERRORS.reasoningMandatory:
      return "That model always reasons and cannot be turned off.";
    case AI_ERRORS.notVerified:
      return "Verify this integration before using it.";
    case AI_ERRORS.turnTimeoutOutOfRange:
      return "Enter a whole number of seconds from 30 to 3600.";
    case AI_ERRORS.unknownVendor:
    case AI_ERRORS.unknownProvider:
      return "That integration is not one Synthesis supports.";
    default:
      return raw || "operation failed";
  }
}
