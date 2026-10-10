import {
  isValidClaudeOauthToken,
  type AgenticIntegration,
  type AgenticVendorId,
  type AgenticVerifyConfig,
  type ClaudeAuthMode,
} from "../types";
import type { Busy, Status } from "./aiIntegrationsShared";
import {
  DEFAULT_GATEWAY_TOKEN_VAR,
  canVerifyGateway,
  envEdited,
  envTextOf,
  parseEnvText,
  type GatewayFields,
} from "./claudeGateway";

/** The same, for the agentic level's API-kind vendors (AII-FR-19). */
export function defaultAgenticBaseUrl(vendor: AgenticVendorId): string {
  return vendor === "claude_agent_api" ? "https://api.anthropic.com/v1" : "";
}

// ---------------------------------------------------------------------------
// The Agentic level
// ---------------------------------------------------------------------------

/** The candidate configuration of an agentic tab, whichever kind it is. */
export interface AgenticDraft extends GatewayFields {
  /**
   * Claude Code's open sub-tab (AII-FR-IUUM). A candidate like the rest of the
   * draft: choosing a sub-tab commits nothing until Verify succeeds.
   */
  authMode: ClaudeAuthMode;
  path: string;
  baseUrl: string;
  apiKey: string;
  /**
   * Claude Code's OAuth token (AII-FR-49). Lives here and nowhere else, and only
   * until the submission carrying it resolves — nothing persists it, carries it
   * across a tab change, or restores it when the tab reopens (AII-FR-53).
   */
  oauthToken: string;
}

/**
 * AII-FR-49: does this tab render an OAuth token field?
 *
 * Read off the record rather than compared against a vendor id, so the rule is
 * "the vendor says it needs a credential" rather than "the vendor is called
 * Claude Code" — which is the same fact the backend derives its own behaviour
 * from (AIC-FR-26), and keeps the two from drifting apart.
 */
export function rendersOauthTokenField(i: AgenticIntegration): boolean {
  return i.kind === "cli" && i.keyRequired;
}

/**
 * AII-FR-50: the inline structural complaint, or `""` when there is nothing to
 * say. An empty field is not an error — it means "keep the stored token", which
 * AII-FR-51 judges separately.
 *
 * Never quotes the value it rejected: a near-miss token is one character from
 * the real thing, and echoing it into the DOM would put it exactly where the
 * rest of this section takes care never to.
 */
export function oauthTokenValidation(value: string): string {
  if (value.trim() === "") return "";
  return isValidClaudeOauthToken(value.trim())
    ? ""
    : "A token starts with sk-ant-oat01- followed by letters, digits, hyphens, or underscores.";
}

/**
 * AII-FR-51: may Verify be activated in this tab?
 *
 * For Claude Code the tab must be able to produce a complete configuration —
 * either a token that passes AII-FR-50, or an empty field with a token already
 * in the keychain. Anything else would submit a payload the backend refuses as
 * `token_missing` or `token_malformed`, so the guard is here rather than in the
 * error handler. Every other tab has nothing extra to satisfy.
 */
export function canVerifyAgentic(
  i: AgenticIntegration,
  draft: AgenticDraft,
): boolean {
  if (!rendersOauthTokenField(i)) return true;
  // AII-FR-51 / AII-FR-HOKG: an environment line the backend would refuse
  // leaves the configuration incomplete in either sub-tab.
  if (parseEnvText(draft.envText).error !== "") return false;
  if (draft.authMode === "custom_gateway") return canVerifyGateway(i, draft);
  const typed = draft.oauthToken.trim();
  if (typed !== "") return isValidClaudeOauthToken(typed);
  return i.keyState === "set";
}

/**
 * AII-FR-22 / AII-FR-IUUM / AII-FR-EJMG: has the open candidate moved away from
 * what last verified? Only the Claude Code tab has more to compare than a path.
 */
export function claudeDraftEdited(
  i: AgenticIntegration,
  draft: AgenticDraft,
): boolean {
  if (!rendersOauthTokenField(i)) return false;
  const storedMode: ClaudeAuthMode = i.authMode ?? "subscription";
  if (draft.authMode !== storedMode) return true;
  if (envEdited(i, draft.envText)) return true;
  if (draft.authMode === "subscription") return draft.oauthToken.trim() !== "";
  return (
    draft.gatewayToken.trim() !== "" ||
    draft.gatewayUrl.trim() !== (i.gatewayBaseUrl ?? "") ||
    draft.gatewayTokenVar.trim() !==
      (i.gatewayTokenVar ?? DEFAULT_GATEWAY_TOKEN_VAR)
  );
}

/** The candidate a record opens with: nothing typed, every stored fact shown. */
export function draftForIntegration(i: AgenticIntegration): AgenticDraft {
  return {
    path: i.binaryPath ?? "",
    baseUrl: i.baseUrl ?? defaultAgenticBaseUrl(i.vendor),
    apiKey: "",
    oauthToken: "",
    authMode: i.authMode ?? "subscription",
    gatewayUrl: i.gatewayBaseUrl ?? "",
    gatewayTokenVar: i.gatewayTokenVar ?? DEFAULT_GATEWAY_TOKEN_VAR,
    gatewayToken: "",
    envText: envTextOf(i.envVars),
  };
}

/**
 * AII-FR-20 / FR-21 / FR-22 / FR-27: the Agentic level's verification status
 * line. Same precedence rules as `apiStatus`, and the same reason for them.
 *
 * Pure and exported so both kinds' precedence is testable directly.
 */
export function agenticStatus(
  integration: AgenticIntegration,
  draft: AgenticDraft,
  busy: Busy,
  error: string,
  detectionFoundNothing: boolean,
  detectedPath: string | null = null,
): Status {
  if (busy === "verifying") return { text: "Verifying…", tone: "muted" };
  // AII-FR-17: detection belongs to the Subscription sub-tab. A Custom Gateway
  // opened while it runs does not report it.
  const gatewayOpen = rendersOauthTokenField(integration) && draft.authMode === "custom_gateway";
  if (busy === "detecting" && !gatewayOpen) {
    return { text: "Looking for the binary…", tone: "muted" };
  }
  if (error) return { text: error, tone: "warn" };

  if (integration.kind === "cli") {
    // AII-FR-DKDC / AIC-FR-QHLN: a Custom Gateway runs no binary, so its status
    // follows the gateway fields and the gateway token alone.
    if (rendersOauthTokenField(integration) && draft.authMode === "custom_gateway") {
      if (claudeDraftEdited(integration, draft) || integration.state === "unconfigured") {
        return { text: "Not verified — verify this gateway before use.", tone: "muted" };
      }
      if (integration.state === "key_unavailable") {
        return {
          text: "The stored gateway token can no longer be read. Enter one and verify again.",
          tone: "warn",
        };
      }
      return { text: "Bedrock gateway · token stored · verified", tone: "ok" };
    }
    const stored = integration.binaryPath ?? "";
    const candidate = draft.path.trim();
    // A newly typed token is an edit like any other: it returns the tab to an
    // unverified presentation until Verify commits it (AII-FR-22). So is a
    // change of sub-tab, of a gateway field, or of an environment entry.
    const edited = candidate !== stored || claudeDraftEdited(integration, draft);

    if (edited || integration.state === "unconfigured") {
      if (candidate === "") {
        return detectionFoundNothing
          ? {
              text: "No binary found. Enter the path to it, or browse for it.",
              tone: "muted",
            }
          : { text: "No binary configured yet.", tone: "muted" };
      }
      // AII-FR-17: a value detection produced is marked as detected. It is still
      // only a candidate — detection persists nothing — so it says so in the
      // same breath, but the author should not have to wonder where it came from.
      if (detectedPath !== null && candidate === detectedPath.trim()) {
        return { text: "Detected — verify this path before use.", tone: "muted" };
      }
      return { text: "Not verified — verify this path before use.", tone: "muted" };
    }

    // AII-FR-27: a stored configuration that has degraded keeps everything it
    // had and says so, rather than being cleared or silently re-detected.
    if (integration.state === "missing") {
      return {
        text: "The binary is no longer at this path. Verify it again once it is back.",
        tone: "warn",
      };
    }
    if (integration.state === "key_unavailable") {
      return {
        // A gateway record returns above, so only the OAuth token is left here.
        text: "The stored OAuth token can no longer be read. Enter one and verify again.",
        tone: "warn",
      };
    }

    const origin =
      integration.pathOrigin === "detected" ? "detected" : "you supplied this path";
    const version = integration.version ? ` · ${integration.version}` : "";
    return { text: `${origin} · verified${version}`, tone: "ok" };
  }

  // API kind.
  const stored = integration.baseUrl ?? "";
  const urlEdited = draft.baseUrl.trim() !== stored;
  const keyEdited = draft.apiKey.trim() !== "";

  if (urlEdited || keyEdited || integration.state === "unconfigured") {
    if (draft.baseUrl.trim() === "") {
      return { text: "No endpoint configured yet.", tone: "muted" };
    }
    return {
      text: "Not verified — verify this endpoint before use.",
      tone: "muted",
    };
  }

  if (integration.state === "key_unavailable") {
    return {
      text: "The stored key can no longer be read. Verify this endpoint again.",
      tone: "warn",
    };
  }

  const when = integration.verifiedAt ? ` · ${integration.verifiedAt}` : "";
  return { text: `verified${when}`, tone: "ok" };
}


export function emptyDraft(): AgenticDraft {
  return {
    path: "",
    baseUrl: "",
    apiKey: "",
    oauthToken: "",
    authMode: "subscription",
    gatewayUrl: "",
    gatewayTokenVar: DEFAULT_GATEWAY_TOKEN_VAR,
    gatewayToken: "",
    envText: "",
  };
}

/**
 * AIC-FR-26 / AIC-FR-UFNB / AIC-FR-SXVA: the configuration a CLI tab submits.
 *
 * Codex and OpenCode send the path alone. Claude Code sends the open sub-tab's
 * shape and no field of the other: a subscription sends a token only when one
 * was typed, and a gateway sends its URL, its variable name, and a token only
 * when one was typed. The environment entries are sent when they differ from
 * the stored ones, so a re-verification that touched nothing sends none.
 */
export function cliConfigFor(
  integration: AgenticIntegration | undefined,
  draft: AgenticDraft,
): AgenticVerifyConfig {
  if (integration && rendersOauthTokenField(integration) && draft.authMode === "custom_gateway") {
    // AII-FR-DKDC / AIC-FR-QHLN: a gateway verification runs no binary, so it
    // carries no path.
    const config: AgenticVerifyConfig = { authMode: "custom_gateway" };
    config.gatewayBaseUrl = draft.gatewayUrl.trim();
    config.gatewayTokenVar = draft.gatewayTokenVar.trim();
    const token = draft.gatewayToken.trim();
    if (token !== "") config.gatewayToken = token;
    if (envEdited(integration, draft.envText)) {
      config.envVars = parseEnvText(draft.envText).entries;
    }
    return config;
  }
  const config: AgenticVerifyConfig = { path: draft.path };
  if (!integration || !rendersOauthTokenField(integration)) return config;
  const token = draft.oauthToken.trim();
  if (token !== "") config.oauthToken = token;
  if (envEdited(integration, draft.envText)) {
    config.envVars = parseEnvText(draft.envText).entries;
  }
  return config;
}
