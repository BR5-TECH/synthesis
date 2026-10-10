/**
 * Credentials, agent integrations and the conversations held through them.
 *
 * One part of the typed wrappers over the backend's `#[tauri::command]` set;
 * `./index.ts` carries the whole rule these files are written under.
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  ActiveAiApiCatalog,
  AgenticIntegration,
  AgenticTurnKind,
  AgenticVendorId,
  AgenticVerifyConfig,
  AiApiCatalog,
  AiApiIntegration,
  AiApiProviderId,
  Agent,
  AgentDraft,
  AgentTurn,
  ConversationOrigin,
  ProjectAgent,
  GithubTokenBinding,
  GithubTokenRecord,
  ProjectAgenticIntegration,
  ProjectAiApiIntegration,
  ReasoningChoice,
} from "../types";

// --- GitHub tokens (github_tokens.rs) -------------------------------------

/**
 * GTS-FR-08: the stored tokens, each with the verification state the backend
 * resolved. Reads the registry (plus a local keychain probe) — no network — so
 * the Global settings GitHub section renders offline.
 */
export const listGithubTokens = () =>
  invoke<GithubTokenRecord[]>("list_github_tokens");

/**
 * GTS-FR-04: verify the secret against GitHub, then store it — the secret goes
 * into the OS keychain and only its description comes back.
 *
 * This is the one call in the frontend that carries a secret, and it carries it
 * in one direction. Rejects with `invalid_token`, `duplicate_label`,
 * `invalid_host`, `github_unreachable`, or `keychain_unavailable` (see `GITHUB_TOKEN_ERRORS`);
 * on any of them nothing was stored.
 */
export const addGithubToken = (label: string, secret: string, host: string) =>
  invoke<GithubTokenRecord>("add_github_token", { label, secret, host });

/** GTS-FR-07: re-check a stored token and return its refreshed record. */
export const validateGithubToken = (id: string) =>
  invoke<GithubTokenRecord>("validate_github_token", { id });

/** GTS-FR-05: relabel; rejects `duplicate_label` if another record has it. */
export const renameGithubToken = (id: string, label: string) =>
  invoke<GithubTokenRecord>("rename_github_token", { id, label });

/** GTS-FR-09: delete the keychain entry and the registry record together. */
export const removeGithubToken = (id: string) =>
  invoke<void>("remove_github_token", { id });

/**
 * GTS-FR-12: open the browser at GitHub's token-creation page with the scopes
 * Synthesis needs pre-selected. Transmits nothing and receives nothing — the
 * token comes back only by the author pasting it (GHA-FR-06).
 */
export const openGithubTokenCreationPage = (host: string) =>
  invoke<void>("open_github_token_creation_page", { host });

/** GTS-FR-10: how the open project resolves a token, and whether to prompt. */
export const getProjectGithubTokenBinding = () =>
  invoke<GithubTokenBinding>("get_project_github_token_binding");

/** GTS-FR-11: point the open project at a stored token. */
export const setProjectGithubTokenBinding = (tokenId: string) =>
  invoke<GithubTokenBinding>("set_project_github_token_binding", { tokenId });

// --- Agentic integrations (agentic.rs) ------------------------------------

/**
 * AIC-FR-02: one record per supported vendor in a stable order, configured or
 * not — so the level never has to reason about an absent record. Reads the
 * registry, stats each stored path, and probes the keychain for each API
 * record's presence; runs no binary and contacts no network, which is what lets
 * the level render offline and instantly.
 */
export const listAgenticIntegrations = () =>
  invoke<AgenticIntegration[]>("list_agentic_integrations");

/**
 * AIC-FR-03: locate a candidate binary on `PATH` and in the platform's
 * conventional install locations. Persists nothing and executes nothing — the
 * path it returns is a suggestion for the field, not a stored fact. Rejects
 * `not_a_cli_integration` for an API-kind vendor, which has no binary to find.
 */
export const detectAgenticCliBinary = (vendor: AgenticVendorId) =>
  invoke<{ path: string | null }>("detect_agentic_cli_binary", { vendor });

/**
 * AIC-FR-06: verify the backend and — only on success — commit the
 * configuration. The `config` carries the fields the vendor accepts: a `path`
 * for Codex and OpenCode, a `path` with an optional `oauthToken` for Claude
 * Code, and a `baseUrl` with an `apiKey` for an API agent; the wrong shape
 * rejects `wrong_config_kind` rather than being half-honoured (AIC-FR-25).
 *
 * Claude Code's token reaches the keychain only after the path verifies, and
 * replaces the previously stored one in a single step (AIC-FR-28). Omitting it
 * re-verifies with the token already held, which is what lets the section check
 * a configured integration without ever reading the token (AIC-FR-26).
 *
 * This is the one call in this level that runs another program or leaves the
 * machine. Rejects with any of the typed errors in `AI_ERRORS`; on all of them
 * the registry and the keychain are exactly as they were.
 */
export const verifyAgenticIntegration = (
  vendor: AgenticVendorId,
  config: AgenticVerifyConfig,
) => invoke<AgenticIntegration>("verify_agentic_integration", { vendor, config });

/**
 * AIC-FR-10: persist the chosen model, for one turn kind or for the record's
 * default.
 *
 * A `null` `turnKind` sets the default every kind falls back to; a named one
 * sets that kind's override. A `null` `modelId` clears rather than stores a
 * null: against the default it selects the backend's own, and against a kind it
 * returns that kind to the default. An id absent from the record's list rejects
 * `unknown_model`, and a kind outside the closed set rejects
 * `unknown_turn_kind`; neither rejection changes anything stored.
 */
export const setAgenticIntegrationModel = (
  vendor: AgenticVendorId,
  turnKind: AgenticTurnKind | null,
  modelId: string | null,
) =>
  invoke<AgenticIntegration>("set_agentic_integration_model", {
    vendor,
    turnKind,
    modelId,
  });

/**
 * AIC-FR-10: persist the chosen reasoning effort, for one turn kind or for the
 * record's default.
 *
 * A `null` `turnKind` sets the default every kind falls back to; a named one
 * sets that kind's override. A `null` `effortId` clears rather than stores a
 * null: against the default it selects the backend's own, and against a kind it
 * returns that kind to the default.
 */
export const setAgenticIntegrationEffort = (
  vendor: AgenticVendorId,
  turnKind: AgenticTurnKind | null,
  effortId: string | null,
) =>
  invoke<AgenticIntegration>("set_agentic_integration_effort", {
    vendor,
    turnKind,
    effortId,
  });

/**
 * AIC-FR-12: make this the user-global active agentic integration, across both
 * kinds. Rejects `not_verified` for one whose state is anything other than
 * `verified`. Returns the whole list, because activating one clears any other.
 */
export const setActiveAgenticIntegration = (vendor: AgenticVendorId) =>
  invoke<AgenticIntegration[]>("set_active_agentic_integration", { vendor });

/**
 * AIC-FR-14: return a vendor to `unconfigured`, deleting an API-kind vendor's
 * keychain entry with it. Idempotent.
 */
export const clearAgenticIntegration = (vendor: AgenticVendorId) =>
  invoke<AgenticIntegration[]>("clear_agentic_integration", { vendor });

/** AIC-FR-16: which agentic integration the open project resolves to, and how. */
export const getProjectAgenticIntegration = () =>
  invoke<ProjectAgenticIntegration>("get_project_agentic_integration");

/**
 * AIC-FR-17: override the open project's agentic integration, or clear the
 * override with `null` so it inherits the user-global choice again. Rejects
 * `not_verified` for a vendor that is not currently verified.
 */
export const setProjectAgenticIntegration = (vendor: AgenticVendorId | null) =>
  invoke<ProjectAgenticIntegration>("set_project_agentic_integration", {
    vendor,
  });

// --- AI API integrations (ai_api.rs) --------------------------------------

/**
 * AAP-FR-02: one record per supported provider in a stable order, configured or
 * not. Reads the registry and probes the keychain for each record's presence;
 * makes no network request, so the level renders offline and instantly.
 */
export const listAiApiIntegrations = () =>
  invoke<AiApiIntegration[]>("list_ai_api_integrations");

/**
 * Every provider's identity and model catalogue, read from the registry alone —
 * no network request and, unlike `listAiApiIntegrations`, no keychain probe.
 *
 * This is what a surface calls when it wants to *name* a model or know which
 * providers can carry an agent, rather than to manage a credential. Listing
 * personas must not depend on the keychain answering (AGR-FR-16),
 * and on a machine whose keychain asks first, calling the credential listing
 * for a model label would prompt the author to see a list of names.
 */
export const listAiApiCatalogs = () =>
  invoke<AiApiCatalog[]>("list_ai_api_catalogs");

/**
 * AAP-FR-APRV: the catalogue of the AI API provider active for the open
 * project, with the project override applied. Registry-only: no network
 * request and no keychain probe. `catalog` is null when no provider resolves.
 * This is what the agent surfaces read, because an agent stores no provider.
 */
export const getActiveAiApiCatalog = () =>
  invoke<ActiveAiApiCatalog>("get_active_ai_api_catalog");

/**
 * AAP-FR-05 / FR-06: call the endpoint, confirm it answers as a conversational
 * API, and — only on success — commit the base URL and the key.
 *
 * `apiKey` is `null` only when the author typed nothing and a key is already
 * stored; with no stored key every provider, Custom included, rejects `key_missing`
 * without making a request. Every other failure is distinguishable —
 * `unreachable`, `rejected`, `not_an_ai_endpoint`, `timed_out` — because each
 * calls for a different correction by the author.
 */
export const verifyAiApiIntegration = (
  provider: AiApiProviderId,
  baseUrl: string,
  apiKey: string | null,
) =>
  invoke<AiApiIntegration>("verify_ai_api_integration", {
    provider,
    baseUrl,
    apiKey,
  });

/**
 * AAP-FR-11: persist the chosen model. `null` selects the provider's own
 * default; an id absent from the record's list rejects `unknown_model`.
 */
export const setAiApiModel = (
  provider: AiApiProviderId,
  modelId: string | null,
) => invoke<AiApiIntegration>("set_ai_api_model", { provider, modelId });

/**
 * AAP-FR-27: persist the reasoning the author asked for. `null` means the
 * selected model's own default. A choice the selected model cannot honour is
 * refused rather than stored — `no_model_selected`, `reasoning_unsupported`,
 * `unknown_effort`, or `reasoning_mandatory`.
 */
export const setAiApiReasoning = (
  provider: AiApiProviderId,
  choice: ReasoningChoice | null,
) => invoke<AiApiIntegration>("set_ai_api_reasoning", { provider, choice });

/**
 * AAP-FR-FGNK: store how long one conversation turn on this provider may run,
 * in milliseconds. `null` clears it, so the project's execution timeout or the
 * default applies. A value outside 30,000 to 3,600,000 is refused with
 * `turn_timeout_out_of_range`.
 */
export const setAiApiTurnTimeout = (
  provider: AiApiProviderId,
  timeoutMs: number | null,
) => invoke<AiApiIntegration>("set_ai_api_turn_timeout", { provider, timeoutMs });

/**
 * AAP-FR-13: make this the user-global active API integration. Rejects
 * `not_verified` for anything not currently verified. Returns the whole list,
 * because activating one clears any other.
 */
export const setActiveAiApiIntegration = (provider: AiApiProviderId) =>
  invoke<AiApiIntegration[]>("set_active_ai_api_integration", { provider });

/**
 * AAP-FR-15: delete the provider's keychain entry and its record together.
 * Idempotent.
 */
export const clearAiApiIntegration = (provider: AiApiProviderId) =>
  invoke<AiApiIntegration[]>("clear_ai_api_integration", { provider });

/** AAP-FR-16: which API integration the open project resolves to, and how. */
export const getProjectAiApiIntegration = () =>
  invoke<ProjectAiApiIntegration>("get_project_ai_api_integration");

/**
 * AAP-FR-17: override the open project's API integration, or clear the override
 * with `null` so it inherits the user-global choice again. Rejects
 * `not_verified` for a provider that is not currently verified.
 */
export const setProjectAiApiIntegration = (provider: AiApiProviderId | null) =>
  invoke<ProjectAiApiIntegration>("set_project_ai_api_integration", {
    provider,
  });

// --- Conversational agents (agents.rs / AGR-agent-registry.md) -------------

/** AGR-FR-02: every described persona, ordered by nickname without regard to
 * case. Answers before any project is open — a definition belongs to the
 * machine. */
export const listAgents = () => invoke<Agent[]>("list_agents");

/**
 * AGR-FR-09: validate the whole draft, then write. A rejected creation leaves
 * the registry byte-for-byte as it was, and every refusal is one of
 * `AGENT_ERRORS`.
 */
export const createAgent = (draft: AgentDraft) =>
  invoke<Agent>("create_agent", { draft });

/** AGR-FR-10: replace every field under the validation `createAgent` applies,
 * keeping the id and `createdAt`. */
export const updateAgent = (id: string, draft: AgentDraft) =>
  invoke<Agent>("update_agent", { id, draft });

/**
 * AGR-FR-11: remove the definition and every project's enrolment of it in one
 * operation, cancelling any turn it has in flight. Idempotent. Returns the
 * registry as it stands afterwards.
 */
export const deleteAgent = (id: string) =>
  invoke<Agent[]>("delete_agent", { id });

/** AGR-FR-13: only the open project's enrolled agents, each carrying its
 * availability. */
export const listProjectAgents = () =>
  invoke<ProjectAgent[]>("list_project_agents");

/** AGR-FR-14: add the agent to the open project's enrolment. Idempotent. */
export const enrolProjectAgent = (agentId: string) =>
  invoke<ProjectAgent[]>("enrol_project_agent", { agentId });

/**
 * AGR-FR-15: withdraw the agent from the open project, leaving its description
 * untouched and cancelling any turn it has in flight *here*. Idempotent.
 */
export const removeProjectAgent = (agentId: string) =>
  invoke<ProjectAgent[]>("remove_project_agent", { agentId });

// --- Agent conversations (agent_conversations.rs / AGC) -------------------

/**
 * AGC-FR-02: register a turn and return it `running` without waiting for the
 * model. The refusals it can return are the ones knowable without a call —
 * `agent_not_found`, `agent_unavailable`, `discussion_locked`; every other outcome
 * arrives as an `agent-turn-state-changed` event on the registered turn.
 *
 * AGC-FR-03: never refused because another turn is already running.
 */
export const dispatchAgentTurn = (args: {
  nickname: string;
  origin: ConversationOrigin;
  triggerCommentId: string;
}) => invoke<AgentTurn>("dispatch_agent_turn", args);

/** AGC-FR-20: abandon the model call and terminate the turn `cancelled`,
 * appending nothing. */
export const cancelAgentTurn = (turnId: string) =>
  invoke<AgentTurn>("cancel_agent_turn", { turnId });

/**
 * AGC-FR-22: the turns still in flight, most recently started first. An
 * `origin` narrows the result to one conversation; `null` returns every turn in
 * flight anywhere, which is what the chrome roster reads (AGT-FR-05).
 */
export const listAgentTurns = (origin: ConversationOrigin | null = null) =>
  invoke<AgentTurn[]>("list_agent_turns", { origin });

/**
 * AGC-FR-31: the recovery registry — at most one turn per conversation, each
 * `failed` and each carrying `retryPermitted`, most recently failed first.
 *
 * What a card reads when it mounts, so a conversation reopened while a failure
 * stands renders its failed contribution and Retry again rather than appearing
 * to have delivered (CTA-FR-RHPP). A turn returned here is not outstanding, so
 * `listAgentTurns` returns it under neither form.
 */
export const listRecoverableAgentTurnFailures = (
  origin: ConversationOrigin | null = null,
) => invoke<AgentTurn[]>("list_recoverable_agent_turn_failures", { origin });

/**
 * AGC-FR-39: the notice registry — at most one turn per conversation, each
 * having ended with `imagesOmitted` true, most recent first.
 *
 * What a conversation reads when it mounts, so an instance opened after a turn
 * ended still knows to say that its pictures were not sent (CTA-FR-XSGX) —
 * exactly as `listRecoverableAgentTurnFailures` is what it reads to recover an
 * offer to retry. What it answers is the current state of each conversation
 * rather than a history of it.
 */
export const listAgentTurnImageNotices = (
  origin: ConversationOrigin | null = null,
) => invoke<AgentTurn[]>("list_agent_turn_image_notices", { origin });

/**
 * AGC-FR-32: start a **new** turn for the failed one's agent, origin, and
 * trigger comment (CTA-FR-QDDG).
 *
 * Appends nothing: the human comment that addressed the agent is neither
 * reposted nor changed. Refuses any turn that is not its conversation's current
 * recoverable failure, and a refusal leaves that offer standing.
 */
export const retryAgentTurn = (turnId: string) =>
  invoke<AgentTurn>("retry_agent_turn", { turnId });
