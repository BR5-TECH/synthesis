// Agentic integrations (AIC-agentic-integrations.md / AII-ai-integrations.md)
//
// Split out of one `types.ts` that had grown past four thousand lines.
// Every name is re-exported from `./index`, so `from "../types"` still
// resolves to the same set and no import site moved.

// --- Agentic integrations (AIC-agentic-integrations.md / AII-ai-integrations.md)

/**
 * The five supported agentic vendors (AIC-FR-01): three CLIs installed on this
 * machine and two agent-execution endpoints reached over HTTP. Ids are stable
 * and are what every command below is keyed by; the display name travels on the
 * record so the UI never has to carry its own table.
 */
export type AgenticVendorId =
  | "claude_code"
  | "codex"
  | "opencode"
  | "claude_agent_api"
  | "custom_agent_api";

/**
 * Which kind of backend a vendor is, and therefore which configuration fields
 * its record carries (AIC-FR-25). This is what lets one tab strip render both
 * kinds without guessing which fields are meaningful.
 */
export type AgenticKind = "cli" | "api";

/** Where a stored binary path came from (AIC-FR-07). CLI kind only. */
export type AgenticPathOrigin = "detected" | "user_supplied" | "unset";

/** Whether an API-kind record's key is readable (AIC-FR-15). */
export type KeyState = "set" | "unset" | "unavailable";

/**
 * A record's state (AIC-FR-15), derived by the backend from the filesystem or
 * the keychain rather than stored — a binary deleted, or a keychain wiped, while
 * the app was closed reads as such on the next launch without anything having
 * written that down.
 */
export type AgenticState =
  | "unconfigured"
  | "verified"
  | "missing"
  | "key_unavailable";

/**
 * Whether a model list came from the backend itself or the application's bundled
 * catalog (AIC-FR-08 / AAP-FR-10). Rendered beneath the model selector so the
 * author knows whether they are looking at what their backend actually offers.
 */
export type ModelsOrigin = "probed" | "catalog";

/**
 * What a model declares about the reasoning it can be asked for (AAP-FR-24).
 *
 * Read from the provider, never authored here — it is what keeps the reasoning
 * offered to the author compatible with the model in front of them. The ladder
 * is per *model*, not per provider: the levels one model accepts are routinely
 * not the levels another accepts.
 *
 * `supportedEfforts` is null for a model that reasons but declares no levels of
 * its own, which the selector renders as off/on rather than as a ladder
 * (AII-FR-43).
 */
export interface ModelReasoning {
  /** Reasoning cannot be switched off, so no `off` entry is offered. */
  mandatory: boolean;
  /** Whether the model reasons when nothing is asked of it. */
  defaultEnabled?: boolean | null;
  /** This model's own ladder, in the order it declares (AII-FR-42). */
  supportedEfforts?: string[] | null;
  /** A member of `supportedEfforts` whenever both are present. */
  defaultEffort?: string | null;
}

export interface ModelOption {
  id: string;
  label: string;
  /** Absent where the model declares no reasoning at all (AAP-FR-24). */
  reasoning?: ModelReasoning | null;
  /**
   * AAP-FR-RTMZ: the route a Custom gateway model supports. Absent or null
   * means both routes. Only Custom gateway models carry a value.
   */
  mode?: ModelRoute | null;
}

/** The routes a Custom gateway model can be called on (AAP-FR-RTMZ). */
export type ModelRoute = "chat" | "responses";

/**
 * What the author asked for (AAP-FR-27). Three shapes rather than a bare
 * string, because "reason at high" and "reason at whatever depth you like" are
 * different requests, and a model offering no ladder can only express the
 * second.
 */
export type ReasoningChoice =
  | { kind: "off" }
  | { kind: "on" }
  | { kind: "effort"; effort: string };

export interface EffortOption {
  id: string;
  label: string;
}

/**
 * AIC-FR-02: the ONLY representation of an agentic integration that crosses the
 * IPC boundary. The only key-derived field is `maskedHint`, the last four
 * characters (AIC-FR-20).
 *
 * Field names must match `AgenticIntegration` in `src-tauri/src/agentic.rs`
 * byte-for-byte: the Rust struct carries `#[serde(default)]` and does not deny
 * unknown fields, so a misspelled key here is not an error — it is silently
 * `undefined` at runtime.
 */
export interface AgenticIntegration {
  vendor: AgenticVendorId;
  kind: AgenticKind;
  displayName: string;
  /** CLI kind only; always null for an API agent (AIC-FR-25). */
  binaryPath: string | null;
  pathOrigin: AgenticPathOrigin;
  /** API kind only; always null for a CLI (AIC-FR-25). */
  baseUrl: string | null;
  /**
   * AIC-FR-25: the credential fields follow the credential, not the kind. They
   * describe an API-kind vendor's key **and Claude Code's OAuth token**; Codex
   * and OpenCode hold nothing, so they always read `unset` with a null hint.
   */
  keyState: KeyState;
  /** The last four characters of the credential, and the only text derived from it. */
  maskedHint: string | null;
  /** Whether this vendor requires a credential at all — true for Claude Code. */
  keyRequired: boolean;
  state: AgenticState;
  version: string | null;
  verifiedAt: string | null;
  models: ModelOption[];
  modelsOrigin: ModelsOrigin;
  /**
   * The model every turn kind uses unless `modelOverrides` names one for it.
   * `null` means the backend's own default, not an unconfigured state.
   */
  selectedModel: string | null;
  /**
   * AIC-FR-10: the turn kinds that use a model other than `selectedModel`,
   * keyed by turn kind. A kind absent from this map follows the default, so an
   * integration configured before any kind was distinguished reads back empty.
   * Independent of `effortOverrides`: a kind may take a model of its own and
   * the default effort, an effort of its own and the default model, both, or
   * neither.
   */
  modelOverrides: Record<string, string>;
  /** Empty for a vendor with no notion of reasoning effort (AIC-FR-09). */
  reasoningEfforts: EffortOption[];
  /** The effort every turn kind uses unless `effortOverrides` names one. */
  selectedEffort: string | null;
  /**
   * AIC-FR-10: the turn kinds that use something other than `selectedEffort`,
   * keyed by turn kind. A kind absent from this map follows the default, so an
   * integration configured before any kind was distinguished reads back empty.
   */
  effortOverrides: Record<string, string>;
  active: boolean;
}

/**
 * AIC-FR-10 / EAC-FR-IRRD: the kinds of work a run hands to an agent backend,
 * each able to carry a model and a reasoning effort of its own.
 *
 * The `id` is the canonical task kind the backend is keyed by; the label is what
 * the author reads. `semantic_rebase` is labelled Reconciliation, which is the
 * name the rest of the application gives that turn (AII-FR-23, AII-FR-24).
 *
 * An override the store holds against a kind outside this set is dropped when
 * the record is read (AIC-FR-QVWX), so a selection made by an older build
 * falls back to the record's default rather than naming a kind nothing runs.
 */
export const AGENTIC_TURN_KINDS = [
  { id: "work", label: "Work" },
  { id: "review", label: "Review" },
  { id: "semantic_rebase", label: "Reconciliation" },
] as const;

export type AgenticTurnKind = (typeof AGENTIC_TURN_KINDS)[number]["id"];

/**
 * The kind-shaped configuration a verification is asked to check (AIC-FR-25).
 * Supplying a field that does not belong to the vendor's kind is refused with
 * `wrong_config_kind` rather than silently ignored.
 */
export interface AgenticVerifyConfig {
  path?: string | null;
  baseUrl?: string | null;
  apiKey?: string | null;
  /**
   * Claude Code only (AIC-FR-26). Present when the author supplied a new token;
   * absent when they are keeping the one already in the keychain. Sending it to
   * any other vendor rejects `wrong_config_kind`.
   */
  oauthToken?: string | null;
}

/**
 * AII-FR-50 / AIC-FR-27: the one pattern a Claude Code OAuth token may match.
 *
 * Structural and complete — it decides whether a value is *shaped* like a token
 * and nothing else. Ownership, expiry, and authenticity are deliberately not
 * tested, because doing so would mean a network call the section never makes.
 * The backend re-checks the identical pattern (`is_valid_oauth_token` in
 * `src-tauri/src/agentic.rs`), so a value that slipped past here still cannot
 * reach the keychain.
 */
export const CLAUDE_OAUTH_TOKEN_PATTERN = /^sk-ant-oat01-[A-Za-z0-9-]+$/;

/** Whether `value` is shaped like a Claude Code OAuth token (AII-FR-50). */
export const isValidClaudeOauthToken = (value: string): boolean =>
  CLAUDE_OAUTH_TOKEN_PATTERN.test(value);

/**
 * How the open project resolved an integration — the value that decides what the
 * Project settings line says (AIC-FR-16 / AAP-FR-16). Shared by both levels,
 * because both resolve the same five ways.
 *
 * - `overridden` — this project names an integration of its own, and it works.
 * - `inherited` — no override; the user-global active integration applies,
 *   whether it was chosen explicitly or is the sole verified one.
 * - `override_unavailable` — the project's override was cleared or has stopped
 *   verifying; the resolved id names what is in effect instead, or is null.
 * - `none_selected` — something is configured, but nothing resolves.
 * - `none_configured` — nothing is configured at all; route the author to that
 *   level's own Global settings section (Conversational AI or Agentic AI).
 */
export type AiResolution =
  | "overridden"
  | "inherited"
  | "override_unavailable"
  | "none_selected"
  | "none_configured";

export interface ProjectAgenticIntegration {
  /** The vendor actually in effect, or null when nothing resolves. */
  vendor: AgenticVendorId | null;
  resolution: AiResolution;
  /** The recorded override, whether or not it still resolves. */
  overrideVendor: AgenticVendorId | null;
}
