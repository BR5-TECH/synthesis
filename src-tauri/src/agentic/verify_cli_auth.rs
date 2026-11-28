//! What a CLI verification means for the vendor's credential, settled before
//! any binary runs or anything is written (`AIC-agentic-integrations.md`
//! AIC-FR-25, AIC-FR-26, AIC-FR-27, AIC-FR-UFYH, AIC-FR-ISWS, AIC-FR-BXFL).
//!
//! Each vendor accepts one payload shape per authentication mode. A field of
//! another shape is refused as `wrong_config_kind` rather than ignored, so a UI
//! bug cannot verify against a stale value and a credential meant for one
//! vendor cannot reach another.

use super::*;

/// The credential side of a CLI verification, once its payload is accepted.
pub(super) struct CliAuthPlan {
    /// Claude Code's mode. `Subscription` for every other vendor.
    pub claude_mode: AuthMode,
    /// Claude Code, gateway mode only.
    pub gateway: Option<GatewayPlan>,
    /// Codex's mode. `LoginFolder` for every other vendor.
    pub codex_mode: CodexAuthMode,
    /// The token to store at the vendor's own vault id after the binary
    /// verifies: Claude Code's OAuth token or Codex's access token. `None`
    /// keeps what is stored, or means the vendor stores nothing.
    pub supplied_token: Option<String>,
}

/// AIC-FR-UFNB: does the payload carry a field of Claude Code's gateway shape?
pub(super) fn carries_gateway_fields(config: &VerifyConfig) -> bool {
    config.gateway_base_url.is_some()
        || config.gateway_token_var.is_some()
        || config.gateway_token.is_some()
}

/// AIC-FR-25: does the payload carry a field that only Claude Code has?
pub(super) fn carries_claude_only_fields(config: &VerifyConfig) -> bool {
    carries_gateway_fields(config) || config.env_vars.is_some() || config.oauth_token.is_some()
}

/// AIC-FR-26 / AIC-FR-27: settle Claude Code's OAuth token.
///
/// Returns the token to store, or `None` when the author keeps the one in the
/// vault. A token that is not shaped like one is `token_malformed`, and a
/// `{ path }` payload with nothing stored is `token_missing`.
fn resolve_oauth_token(
    ai: &AgenticIntegrations,
    descriptor: &AgenticVendor,
    oauth_token: Option<&str>,
) -> Result<Option<String>, String> {
    match oauth_token {
        Some(raw) => {
            // Trimmed before the check so a token pasted with a stray newline is
            // not rejected as malformed; an empty field still fails, because
            // `""` matches nothing the pattern accepts.
            let token = raw.trim();
            if !is_valid_oauth_token(token) {
                return Err(ERR_TOKEN_MALFORMED.into());
            }
            Ok(Some(token.to_string()))
        }
        None => {
            if !stored(ai, descriptor.vendor)? {
                return Err(ERR_TOKEN_MISSING.into());
            }
            Ok(None)
        }
    }
}

/// Whether the vault holds the credential at `id`.
fn stored(ai: &AgenticIntegrations, id: &str) -> Result<bool, String> {
    ai.secrets.has(id).map_err(|_| ERR_KEYCHAIN_UNAVAILABLE.to_string())
}

/// AIC-FR-WNQR / AIC-FR-UFNB / AIC-FR-SXVA: Claude Code's payload.
fn plan_claude(
    ai: &AgenticIntegrations,
    descriptor: &AgenticVendor,
    config: &VerifyConfig,
) -> Result<CliAuthPlan, String> {
    if config.access_token.is_some() {
        return Err(ERR_WRONG_CONFIG_KIND.into());
    }
    let mode = match config.auth_mode.as_deref() {
        None => AuthMode::Subscription,
        Some(raw) => AuthMode::parse(raw).ok_or(ERR_WRONG_CONFIG_KIND)?,
    };
    let gateway = match mode {
        AuthMode::Subscription => {
            if carries_gateway_fields(config) {
                return Err(ERR_WRONG_CONFIG_KIND.into());
            }
            None
        }
        AuthMode::CustomGateway => {
            if config.oauth_token.is_some() {
                return Err(ERR_WRONG_CONFIG_KIND.into());
            }
            Some(plan_gateway(ai, config)?)
        }
    };
    if let Some(entries) = config.env_vars.as_deref() {
        validate_env_vars(entries)?;
    }
    // AIC-FR-QHLN: a gateway verification stores no OAuth token.
    let supplied_token = match gateway {
        Some(_) => None,
        None => resolve_oauth_token(ai, descriptor, config.oauth_token.as_deref())?,
    };
    Ok(CliAuthPlan {
        claude_mode: mode,
        gateway,
        codex_mode: CodexAuthMode::LoginFolder,
        supplied_token,
    })
}

/// AIC-FR-UFYH / AIC-FR-ISWS / AIC-FR-BXFL: Codex's payload.
fn plan_codex(
    ai: &AgenticIntegrations,
    descriptor: &AgenticVendor,
    config: &VerifyConfig,
) -> Result<CliAuthPlan, String> {
    if carries_claude_only_fields(config) {
        return Err(ERR_WRONG_CONFIG_KIND.into());
    }
    let mode = match config.auth_mode.as_deref() {
        None => CodexAuthMode::LoginFolder,
        Some(raw) => CodexAuthMode::parse(raw).ok_or(ERR_WRONG_CONFIG_KIND)?,
    };
    let supplied_token = match (mode, config.access_token.as_deref()) {
        (CodexAuthMode::LoginFolder, Some(_)) => return Err(ERR_WRONG_CONFIG_KIND.into()),
        // AIC-FR-OFEO: a login-folder verification never touches the token.
        (CodexAuthMode::LoginFolder, None) => None,
        (CodexAuthMode::AccessToken, Some(raw)) => {
            let token = trim_codex_access_token(raw);
            if !is_valid_codex_access_token(token) {
                return Err(ERR_TOKEN_MALFORMED.into());
            }
            Some(token.to_string())
        }
        (CodexAuthMode::AccessToken, None) => {
            if !stored(ai, descriptor.vendor)? {
                return Err(ERR_TOKEN_MISSING.into());
            }
            None
        }
    };
    Ok(CliAuthPlan {
        claude_mode: AuthMode::Subscription,
        gateway: None,
        codex_mode: mode,
        supplied_token,
    })
}

/// AIC-FR-25 / AIC-FR-26 / AIC-FR-ISWS: accept or refuse a CLI payload's
/// credential fields, before the binary runs and before the vault is touched.
pub(super) fn plan_cli_auth(
    ai: &AgenticIntegrations,
    descriptor: &AgenticVendor,
    config: &VerifyConfig,
) -> Result<CliAuthPlan, String> {
    if config.base_url.is_some() || config.api_key.is_some() {
        return Err(ERR_WRONG_CONFIG_KIND.into());
    }
    if descriptor.requires_oauth_token() {
        return plan_claude(ai, descriptor, config);
    }
    if descriptor.holds_access_token() {
        return plan_codex(ai, descriptor, config);
    }
    // A vendor that holds no credential takes `{ path }` alone.
    if carries_claude_only_fields(config)
        || config.auth_mode.is_some()
        || config.access_token.is_some()
    {
        return Err(ERR_WRONG_CONFIG_KIND.into());
    }
    Ok(CliAuthPlan {
        claude_mode: AuthMode::Subscription,
        gateway: None,
        codex_mode: CodexAuthMode::LoginFolder,
        supplied_token: None,
    })
}
