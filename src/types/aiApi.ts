// AI API integrations (AAP-ai-api-integrations.md / AII-ai-integrations.md)
//
// Split out of one `types.ts` that had grown past four thousand lines.
// Every name is re-exported from `./index`, so `from "../types"` still
// resolves to the same set and no import site moved.

import { AiResolution, KeyState, ModelOption, ModelsOrigin, ReasoningChoice } from "./agentic";

// --- AI API integrations (AAP-ai-api-integrations.md / AII-ai-integrations.md)

/**
 * The four supported conversational providers (AAP-FR-01). `custom` is the one
 * that takes any OpenAI-compatible base URL, which is what makes a locally-run
 * model a first-class choice (AAP-FR-03 / AAP-FR-04).
 */
export type AiApiProviderId = "anthropic" | "openai" | "openrouter" | "custom";

/** A record's state (AAP-FR-09), derived from the keychain rather than stored. */
export type AiApiState = "unconfigured" | "verified" | "key_unavailable";

/**
 * AAP-FR-08: the ONLY representation of a provider that crosses the IPC
 * boundary. `maskedHint` is the only key-derived field.
 *
 * Field names must match `AiApiIntegration` in `src-tauri/src/ai_api.rs`
 * byte-for-byte, for the same reason as the agentic record above.
 */
export interface AiApiIntegration {
  provider: AiApiProviderId;
  displayName: string;
  baseUrl: string | null;
  keyState: KeyState;
  maskedHint: string | null;
  /** True for every provider: Custom needs a gateway secret too (AAP-FR-04). */
  keyRequired: boolean;
  state: AiApiState;
  verifiedAt: string | null;
  models: ModelOption[];
  modelsOrigin: ModelsOrigin;
  /** `null` means the provider's own default, not an unconfigured state. */
  selectedModel: string | null;
  /** `null` means the selected model's own default (AAP-FR-27). */
  selectedReasoning: ReasoningChoice | null;
  /**
   * AAP-FR-FGNK: how long one conversation turn on this provider may run, in
   * milliseconds. `null` means the project's execution timeout, or five minutes.
   */
  turnTimeoutMs: number | null;
  active: boolean;
}

/**
 * A provider's identity and model catalogue, with nothing key-derived in it.
 *
 * Field names must match `AiApiCatalog` in `src-tauri/src/ai_api.rs`.
 *
 * This is what the agent surfaces read. `AiApiIntegration` describes a
 * *credential*, so building one probes the OS keychain per record — right for
 * the Conversational AI level, where keys are managed, and wrong for a list of
 * personas, which reads no key at all (AGR-FR-16). `state` here is
 * computed with the key assumed present, exactly as the backend computes an
 * agent's availability, so the two cannot disagree about the same agent.
 */
export interface AiApiCatalog {
  provider: AiApiProviderId;
  displayName: string;
  /**
   * Computed with the key assumed present, so a locked or wiped keychain still
   * reads `verified`. Still `key_unavailable` for a record that names an
   * endpoint but never held a key for a provider that requires one — a state no
   * key could rescue, and one both sides render as `provider_unverified`.
   */
  state: AiApiState;
  models: ModelOption[];
}

/**
 * AAP-FR-APRV: the catalogue of the AI API provider that is active for the
 * open project, with how the project resolved it. `catalog` is null when no
 * provider resolves. Field names must match `ActiveAiApiCatalog` in
 * `src-tauri/src/ai_api/agent_provider.rs`.
 */
export interface ActiveAiApiCatalog {
  resolution: AiResolution;
  catalog: AiApiCatalog | null;
}

export interface ProjectAiApiIntegration {
  /** The provider actually in effect, or null when nothing resolves. */
  provider: AiApiProviderId | null;
  resolution: AiResolution;
  /** The recorded override, whether or not it still resolves. */
  overrideProvider: AiApiProviderId | null;
}

/**
 * The typed errors both levels reject with, mirroring the constants in
 * `src-tauri/src/agentic.rs` and `src-tauri/src/ai_api.rs`.
 *
 * These are matched on, not just displayed: `notFound`, `notExecutable`, and
 * `notTheExpectedCli` are three different things for the author to do next, as
 * are `unreachable` and `rejected` (AII-FR-09 / AII-FR-20). A divergence from
 * the Rust spelling is silent — which is why both sides keep the list as named
 * constants rather than inline literals.
 */
export const AI_ERRORS = {
  // CLI-kind verification (AIC-FR-05).
  pathEmpty: "path_empty",
  notFound: "not_found",
  notExecutable: "not_executable",
  notTheExpectedCli: "not_the_expected_cli",
  executionFailed: "execution_failed",
  // Claude Code's OAuth token (AIC-FR-26 / AIC-FR-27). Both are backstops: the
  // section will not submit a payload that provokes either (AII-FR-51), so
  // seeing one means the guard was bypassed.
  tokenMissing: "token_missing",
  tokenMalformed: "token_malformed",
  // Endpoint verification, both levels (AIC-FR-22 / AAP-FR-05).
  baseUrlEmpty: "base_url_empty",
  baseUrlInvalid: "base_url_invalid",
  keyMissing: "key_missing",
  unreachable: "unreachable",
  rejected: "rejected",
  notAnAgentEndpoint: "not_an_agent_endpoint",
  notAnAiEndpoint: "not_an_ai_endpoint",
  // Shared.
  timedOut: "timed_out",
  keychainUnavailable: "keychain_unavailable",
  wrongConfigKind: "wrong_config_kind",
  notACliIntegration: "not_a_cli_integration",
  unknownModel: "unknown_model",
  unknownEffort: "unknown_effort",
  unknownTurnKind: "unknown_turn_kind",
  // Reasoning, the AI API level (AAP-FR-27).
  noModelSelected: "no_model_selected",
  reasoningUnsupported: "reasoning_unsupported",
  reasoningMandatory: "reasoning_mandatory",
  notVerified: "not_verified",
  unknownVendor: "unknown_vendor",
  unknownProvider: "unknown_provider",
  // The turn timeout, the AI API level (AAP-FR-FGNK).
  turnTimeoutOutOfRange: "turn_timeout_out_of_range",
  /**
   * AAP-FR-33: the provider has never been configured at all, as distinct from
   * `notVerified` — one is a configuration to write, the other a check to pass.
   */
  providerUnconfigured: "provider_unconfigured",
  /**
   * AAP-FR-34: the provider's keychain entry is gone, as distinct from
   * `keychainUnavailable` — one is a credential to re-enter, the other a
   * keychain to unlock.
   */
  keyUnavailable: "key_unavailable",
} as const;
