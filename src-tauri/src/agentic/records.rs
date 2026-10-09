//! The wire and persisted shapes of an agentic integration
//! (`AIC-agentic-integration-config.md`).

use super::*;

// ---------------------------------------------------------------------------
// Wire and persisted types
// ---------------------------------------------------------------------------

/// Where a stored CLI path came from (AIC-FR-07).
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PathOrigin {
    Detected,
    UserSupplied,
    #[default]
    Unset,
}

/// Whether a credential-holding record's credential is readable (AIC-FR-15) —
/// an API-kind vendor's key or Claude Code's OAuth token. Always `Unset` for a
/// vendor that holds nothing, which is Codex and OpenCode (AIC-FR-25).
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KeyState {
    Set,
    #[default]
    Unset,
    Unavailable,
}

/// A record's state (AIC-FR-15). Derived at read time, never persisted: a binary
/// deleted, or a keychain wiped, while the app was closed must read as such on
/// the next launch without anything having written that fact down.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationState {
    #[default]
    Unconfigured,
    Verified,
    /// CLI kind: a stored path that no longer exists on disk.
    Missing,
    /// API kind: a stored configuration whose key can no longer be read.
    KeyUnavailable,
}

/// Claude Code's authentication mode (AIC-FR-WNQR). A record that stores no
/// mode reads as `Subscription`.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    #[default]
    Subscription,
    CustomGateway,
}

impl AuthMode {
    fn is_subscription(&self) -> bool {
        *self == AuthMode::Subscription
    }

    /// The wire spelling, which `VerifyConfig::auth_mode` carries.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "subscription" => Some(AuthMode::Subscription),
            "custom_gateway" => Some(AuthMode::CustomGateway),
            _ => None,
        }
    }
}

/// The persisted half of an integration — what `synthesis.toml` carries
/// (`GSS-global-settings-storage.md` GSS-FR-14).
///
/// `#[serde(default)]` matters: a store written before a field existed must
/// still load rather than sending the whole file through GSS-FR-13's
/// repair-to-defaults, which would wipe recents and every registry.
///
/// Field order is load-bearing for TOML: every scalar is declared before
/// `models`, which serialises as an array of tables.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct AgenticRecord {
    pub vendor: String,
    /// CLI kind only (AIC-FR-25).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binary_path: Option<String>,
    /// CLI kind only (AIC-FR-25).
    pub path_origin: PathOrigin,
    /// API kind only (AIC-FR-25).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// The last four characters of this vendor's credential — an API-kind key or
    /// Claude Code's OAuth token — which is the only credential-derived text
    /// that reaches this store (AIC-FR-20). `None` for a vendor that holds no
    /// credential, and for an API deployment configured with no key at all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub masked_hint: Option<String>,
    /// Claude Code only (AIC-FR-WNQR).
    #[serde(skip_serializing_if = "AuthMode::is_subscription")]
    pub auth_mode: AuthMode,
    /// Claude Code only: the normalized gateway base URL (AIC-FR-YXAB).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway_base_url: Option<String>,
    /// Claude Code only: the variable name the gateway token is passed under
    /// (AIC-FR-CVPW).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway_token_var: Option<String>,
    /// Claude Code only: the last four characters of the gateway token, and
    /// the only text derived from it that reaches this store (AIC-FR-20).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway_masked_hint: Option<String>,
    /// Claude Code only: true when the author accepted the gateway with no
    /// gateway check (AIC-FR-KWMV).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub gateway_check_skipped: bool,
    /// Claude Code only: the author's `NAME=value` entries (AIC-FR-XTEZ). Author
    /// configuration, not credentials.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub env_vars: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified_at: Option<String>,
    /// The model every turn kind uses unless `model_overrides` names one for it
    /// (AIC-FR-10). `None` means the backend's own default, not "unconfigured".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_model: Option<String>,
    /// AIC-FR-10: the turn kinds that use a model other than `selected_model`,
    /// keyed by the identifier of `crate::tools::agent_exec::TurnKind`.
    ///
    /// Independent of `effort_overrides` above it and of both defaults: a kind
    /// may take a model of its own and the default effort, an effort of its own
    /// and the default model, both, or neither. A kind absent from this map
    /// takes `selected_model`, so a record written before any kind was
    /// distinguished resolves one model for every kind and needs no migration.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub model_overrides: std::collections::BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_effort: Option<String>,
    /// AIC-FR-10: the turn kinds that use something other than
    /// `selected_effort`, keyed by the identifier of
    /// `crate::tools::agent_exec::TurnKind`.
    ///
    /// A kind absent from this map takes `selected_effort`, so a record written
    /// before any kind was distinguished resolves one effort for every kind and
    /// needs no migration. A `BTreeMap` so the serialized file keeps a stable
    /// key order rather than churning on save.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub effort_overrides: std::collections::BTreeMap<String, String>,
    pub models_origin: ModelsOrigin,
    pub models: Vec<ModelOption>,
}

impl AgenticRecord {
    pub(super) fn empty(vendor: &str) -> Self {
        Self {
            vendor: vendor.to_string(),
            ..Default::default()
        }
    }
}

/// The single representation of an integration that crosses the IPC boundary
/// (AIC-FR-02). Carries no key material beyond `masked_hint` (AIC-FR-20).
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct AgenticIntegration {
    pub vendor: String,
    pub kind: VendorKind,
    pub display_name: String,
    pub binary_path: Option<String>,
    pub path_origin: PathOrigin,
    pub base_url: Option<String>,
    pub key_state: KeyState,
    pub masked_hint: Option<String>,
    /// AIC-FR-WNQR: `None` for every vendor but Claude Code.
    pub auth_mode: Option<AuthMode>,
    pub gateway_base_url: Option<String>,
    pub gateway_token_var: Option<String>,
    pub gateway_key_state: KeyState,
    pub gateway_masked_hint: Option<String>,
    /// AIC-FR-KWMV: `false` for every vendor but Claude Code (AIC-FR-25).
    pub gateway_check_skipped: bool,
    pub env_vars: Vec<String>,
    pub key_required: bool,
    pub state: IntegrationState,
    pub version: Option<String>,
    pub verified_at: Option<String>,
    pub models: Vec<ModelOption>,
    pub models_origin: ModelsOrigin,
    pub selected_model: Option<String>,
    /// AIC-FR-10: the model each turn kind uses where it does not use
    /// `selected_model`. Empty where every kind follows the default.
    pub model_overrides: std::collections::BTreeMap<String, String>,
    pub reasoning_efforts: Vec<EffortOption>,
    pub selected_effort: Option<String>,
    /// AIC-FR-10: the effort each turn kind uses where it does not use
    /// `selected_effort`. Empty where every kind follows the default.
    pub effort_overrides: std::collections::BTreeMap<String, String>,
    pub active: bool,
}

/// How the open project resolved its integration (AIC-FR-16) — the value that
/// tells the UI what to say.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectResolution {
    /// An override naming a currently-verified integration.
    Overridden,
    /// No override; the user-global active integration applies.
    Inherited,
    /// An override whose integration was cleared or has stopped verifying.
    OverrideUnavailable,
    /// Something is configured, but nothing resolves.
    NoneSelected,
    /// No vendor is configured at all.
    #[default]
    NoneConfigured,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct ProjectAgenticIntegration {
    /// The vendor actually in effect, or `None` when nothing resolves.
    pub vendor: Option<String>,
    pub resolution: ProjectResolution,
    /// The recorded override, whether or not it still resolves.
    pub override_vendor: Option<String>,
}

/// What an actual invocation needs, tagged by the resolved integration's kind
/// so a caller cannot mistake a binary path for an endpoint (AIC-FR-19).
/// Returned only by `resolve_agentic_invocation`, which is not reachable from
/// the frontend.
///
/// `Debug` is hand-rolled below rather than derived, because the `Api` variant
/// carries a cleartext key (AIC-FR-20).
#[derive(Clone, PartialEq, Eq)]
pub enum AgenticInvocation {
    Cli {
        vendor: String,
        binary_path: String,
        model_id: Option<String>,
        effort_id: Option<String>,
    },
    Api {
        vendor: String,
        base_url: String,
        api_key: Option<String>,
        model_id: Option<String>,
        effort_id: Option<String>,
    },
}

impl std::fmt::Debug for AgenticInvocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cli {
                vendor,
                binary_path,
                model_id,
                effort_id,
            } => f
                .debug_struct("AgenticInvocation::Cli")
                .field("vendor", vendor)
                .field("binary_path", binary_path)
                .field("model_id", model_id)
                .field("effort_id", effort_id)
                .finish(),
            Self::Api {
                vendor,
                base_url,
                api_key,
                model_id,
                effort_id,
            } => f
                .debug_struct("AgenticInvocation::Api")
                .field("vendor", vendor)
                .field("base_url", base_url)
                .field("api_key", &redacted(api_key))
                .field("model_id", model_id)
                .field("effort_id", effort_id)
                .finish(),
        }
    }
}

/// The candidate path `detect_agentic_cli_binary` found, if any (AIC-FR-03).
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct DetectedBinary {
    pub path: Option<String>,
}

/// The kind-shaped configuration a verification is asked to check.
///
/// One struct with optional fields rather than an enum because it arrives from
/// the frontend as JSON; the kind check (AIC-FR-25) is explicit in
/// `verify_integration_impl` and returns `wrong_config_kind` rather than
/// silently ignoring a field that does not belong to the vendor's kind.
///
/// Deserialize-only, and `Debug` is hand-rolled below: this is the one shape in
/// which a cleartext credential crosses into the backend, so it must never be
/// serialisable back out or printable (AIC-FR-20).
///
/// The shapes the vendors accept (AIC-FR-26): `{ path }` for Codex and OpenCode,
/// `{ path, oauth_token }` or `{ path }` for Claude Code, and
/// `{ base_url, api_key }` for an API-kind vendor. Anything else is
/// `wrong_config_kind`.
#[derive(Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct VerifyConfig {
    pub path: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    /// Claude Code only. `Some` means the author supplied a new token; `None`
    /// means they are keeping the one already stored (AIC-FR-26).
    pub oauth_token: Option<String>,
    /// Claude Code only: `subscription` or `custom_gateway`. Absent means
    /// `subscription` (AIC-FR-WNQR, AIC-FR-UFNB).
    pub auth_mode: Option<String>,
    /// Claude Code, gateway mode only (AIC-FR-UFNB).
    pub gateway_base_url: Option<String>,
    /// Claude Code, gateway mode only (AIC-FR-CVPW).
    pub gateway_token_var: Option<String>,
    /// Claude Code, gateway mode only: `Some` is a new token, `None` keeps the
    /// stored one (AIC-FR-IOWS).
    pub gateway_token: Option<String>,
    /// Claude Code only: `Some` replaces the stored list, `None` keeps it
    /// (AIC-FR-SXVA).
    pub env_vars: Option<Vec<String>>,
    /// Claude Code, gateway mode only: `Some(true)` verifies the binary and
    /// sends no request to the gateway (AIC-FR-KWMV).
    pub skip_gateway_check: Option<bool>,
}

impl std::fmt::Debug for VerifyConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VerifyConfig")
            .field("path", &self.path)
            .field("base_url", &self.base_url)
            .field("api_key", &redacted(&self.api_key))
            .field("oauth_token", &redacted(&self.oauth_token))
            .field("auth_mode", &self.auth_mode)
            .field("gateway_base_url", &self.gateway_base_url)
            .field("gateway_token_var", &self.gateway_token_var)
            .field("gateway_token", &redacted(&self.gateway_token))
            // A count, never the entries: a value is author text and may
            // carry anything (AIC-FR-SXVA).
            .field("env_vars", &self.env_vars.as_ref().map(Vec::len))
            .field("skip_gateway_check", &self.skip_gateway_check)
            .finish()
    }
}

