//! Claude Code's Custom Gateway mode: field checks, the gateway request, and the
//! launch environment (`AIC-agentic-integrations.md` AIC-FR-WNQR, AIC-FR-UFNB,
//! AIC-FR-CVPW, AIC-FR-IOWS, AIC-FR-PADP, AIC-FR-DRPC, AIC-FR-XTEZ, AIC-FR-SXVA,
//! AIC-FR-XZCS, AIC-FR-ISOC).

use super::*;

/// AIC-FR-20: the vault id of Claude Code's gateway token, beside the vendor id
/// that holds its OAuth token.
pub const GATEWAY_SECRET_ID: &str = "claude_code_gateway";
/// AIC-FR-CVPW: the variable name the gateway token takes when the author names
/// none.
pub const DEFAULT_GATEWAY_TOKEN_VAR: &str = "ANTHROPIC_AUTH_TOKEN";
/// AIC-FR-XZCS: the variable the gateway base URL is passed under.
pub const GATEWAY_BASE_URL_VAR: &str = "ANTHROPIC_BASE_URL";
/// AIC-FR-XZCS: the variables that make Claude Code send Bedrock runtime
/// requests to the gateway and sign none of them with AWS credentials.
pub const BEDROCK_FLAG_VAR: &str = "CLAUDE_CODE_USE_BEDROCK";
pub const BEDROCK_SKIP_AUTH_VAR: &str = "CLAUDE_CODE_SKIP_BEDROCK_AUTH";
/// AIC-FR-XZCS: the variable a Bedrock gateway base URL is passed under.
pub const BEDROCK_BASE_URL_VAR: &str = "ANTHROPIC_BEDROCK_BASE_URL";
/// AIC-FR-XZCS: the variable the subscription token is passed under.
pub const OAUTH_TOKEN_VAR: &str = "CLAUDE_CODE_OAUTH_TOKEN";
/// AIC-FR-PADP: the path appended to the gateway base URL.
const GATEWAY_MODELS_PATH: &str = "/v1/models";
/// AIC-FR-PADP: the variable name that makes the token travel as `x-api-key`.
const API_KEY_VAR: &str = "ANTHROPIC_API_KEY";

/// The longest network cause a typed failure carries.
const CAUSE_LIMIT: usize = 200;

/// Names the Docker client process reads, which a variable passed by name would
/// overwrite there, plus the session-state directory variable the executor owns
/// (AIC-FR-XTEZ).
const RESERVED_NAMES: &[&str] = &[
    "PATH",
    "PATHEXT",
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "PWD",
    "TMPDIR",
    "TEMP",
    "TMP",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "SYSTEMROOT",
    "COMSPEC",
    "SSH_AUTH_SOCK",
    "GLIBC_TUNABLES",
    "GCONV_PATH",
    "LOCPATH",
    "NLSPATH",
    "CLAUDE_CONFIG_DIR",
];
const RESERVED_PREFIXES: &[&str] = &["DOCKER_", "LD_", "DYLD_", "XDG_"];

/// `^[A-Za-z_][A-Za-z0-9_]*$`.
pub fn is_valid_variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// AIC-FR-XTEZ: is this name one the author may not set?
///
/// Compared without regard to case, because the Docker client process runs on
/// platforms whose environment is case-insensitive.
pub fn is_reserved_variable(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    RESERVED_NAMES.contains(&upper.as_str())
        || RESERVED_PREFIXES.iter().any(|p| upper.starts_with(p))
}

/// AIC-FR-CVPW: the token variable name a payload carries, defaulted.
pub(super) fn resolve_token_var(raw: Option<&str>) -> Result<String, String> {
    let name = raw.map(str::trim).filter(|n| !n.is_empty()).unwrap_or(DEFAULT_GATEWAY_TOKEN_VAR);
    // The launch sets these names itself, in one API or the other (AIC-FR-XZCS),
    // so a token under one of them would replace that variable.
    let launch_sets = [
        GATEWAY_BASE_URL_VAR,
        BEDROCK_BASE_URL_VAR,
        BEDROCK_FLAG_VAR,
        BEDROCK_SKIP_AUTH_VAR,
    ];
    if !is_valid_variable_name(name)
        || launch_sets.iter().any(|set| name.eq_ignore_ascii_case(set))
        || is_reserved_variable(name)
    {
        return Err(ERR_TOKEN_VAR_INVALID.into());
    }
    Ok(name.to_string())
}

/// AIC-FR-IOWS: a gateway token is a non-empty run of visible ASCII. Nothing
/// else can travel in an HTTP header, and nothing else is a token.
pub fn is_valid_gateway_token(token: &str) -> bool {
    !token.is_empty() && token.chars().all(|c| c.is_ascii_graphic())
}

/// AIC-FR-XTEZ: split one `NAME=value` entry at its first `=`.
pub fn split_env_entry(entry: &str) -> Option<(&str, &str)> {
    let (name, value) = entry.split_once('=')?;
    if !is_valid_variable_name(name) || value.contains(['\n', '\r', '\0']) {
        return None;
    }
    Some((name, value))
}

/// AIC-FR-XTEZ / AIC-FR-SXVA: check every entry, naming a bad one by its
/// position or its name and never by its value.
pub(super) fn validate_env_vars(entries: &[String]) -> Result<(), String> {
    for (index, entry) in entries.iter().enumerate() {
        let Some((name, _)) = split_env_entry(entry) else {
            return Err(format!("{ERR_ENV_VAR_INVALID}:{}", index + 1));
        };
        if is_reserved_variable(name) {
            return Err(format!("{ERR_ENV_VAR_RESERVED}:{name}"));
        }
    }
    Ok(())
}

/// AIC-FR-PADP: the authentication style the token variable name implies.
pub(super) fn auth_style_for(token_var: &str) -> AuthStyle {
    if token_var == API_KEY_VAR {
        AuthStyle::AnthropicApiKey
    } else {
        AuthStyle::Bearer
    }
}

/// AIC-FR-PADP / AIC-FR-DRPC: send the one `GET <base>/v1/models` and read the
/// answer. Returns the models the gateway listed.
pub(super) fn check_gateway(
    ai: &AgenticIntegrations,
    base_url: &str,
    token_var: &str,
    token: &str,
) -> Result<Vec<ModelOption>, String> {
    ai.prober
        .probe(&ProbeRequest {
            base_url,
            api_key: Some(token),
            auth: auth_style_for(token_var),
            models_path: GATEWAY_MODELS_PATH,
            models_format: crate::ai_shared::ModelsFormat::Lenient,
            report_status: true,
        })
        .map_err(gateway_probe_error)
}

/// AIC-FR-DRPC: the typed failure of a gateway check.
pub(super) fn gateway_probe_error(e: ProbeError) -> String {
    match e {
        ProbeError::Status(code) => format!("{ERR_GATEWAY_STATUS}:{code}"),
        // `Rejected` is a status answer by another name; the check asks for the
        // status itself, so this arm only keeps the match total.
        ProbeError::Rejected => format!("{ERR_GATEWAY_STATUS}:401"),
        ProbeError::Unreachable(cause) => {
            let cause: String = cause
                .chars()
                .map(|c| if c.is_control() { ' ' } else { c })
                .take(CAUSE_LIMIT)
                .collect();
            format!("{ERR_GATEWAY_UNREACHABLE}:{cause}")
        }
        ProbeError::NotExpectedKind => ERR_GATEWAY_NOT_A_MODEL_LIST.to_string(),
        ProbeError::TimedOut => ERR_GATEWAY_TIMED_OUT.to_string(),
        ProbeError::TlsUntrusted(failure) => failure.wire(),
    }
}

/// What a gateway payload settles before anything is run or written.
pub(super) struct GatewayPlan {
    pub base_url: String,
    pub token_var: String,
    /// A token the author supplied, trimmed. `None` keeps the stored one.
    pub supplied_token: Option<String>,
    /// AIC-FR-QHLN: the API shape the gateway serves.
    pub api: GatewayApi,
    /// AIC-FR-KWMV: the author accepted the gateway with no gateway check.
    pub skip_check: bool,
}

impl GatewayPlan {
    /// AIC-FR-PADP / AIC-FR-KWMV / AIC-FR-QHLN: only an Anthropic gateway that
    /// the author did not accept without the check is asked for its models.
    pub fn runs_check(&self) -> bool {
        self.api == GatewayApi::Anthropic && !self.skip_check
    }

    /// AIC-FR-KWMV: the flag a record keeps. A Bedrock gateway is never
    /// checked, so the flag says nothing about it and stays false.
    pub fn check_skipped(&self) -> bool {
        self.api == GatewayApi::Anthropic && self.skip_check
    }
}

/// AIC-FR-UFNB / AIC-FR-CVPW / AIC-FR-IOWS: settle a gateway payload's shape,
/// before the binary is run and before the vault is touched.
pub(super) fn plan_gateway(
    ai: &AgenticIntegrations,
    config: &VerifyConfig,
) -> Result<GatewayPlan, String> {
    let base_url = normalize_base_url(config.gateway_base_url.as_deref().unwrap_or("")).map_err(
        |e| match e {
            BaseUrlError::Empty => ERR_BASE_URL_EMPTY.to_string(),
            BaseUrlError::Invalid => ERR_BASE_URL_INVALID.to_string(),
        },
    )?;
    // AIC-FR-UFNB: a query or a fragment can carry a credential, and the check
    // URL is the base URL with a path appended to it.
    if base_url.contains(['?', '#']) {
        return Err(ERR_BASE_URL_INVALID.into());
    }
    let api = match config.gateway_api.as_deref() {
        None => GatewayApi::Anthropic,
        Some(raw) => GatewayApi::parse(raw).ok_or(ERR_WRONG_CONFIG_KIND)?,
    };
    let token_var = resolve_token_var(config.gateway_token_var.as_deref())?;
    let supplied_token = match config.gateway_token.as_deref() {
        Some(raw) => {
            let token = raw.trim();
            if !is_valid_gateway_token(token) {
                return Err(ERR_TOKEN_MALFORMED.into());
            }
            Some(token.to_string())
        }
        None => {
            let stored = ai
                .secrets
                .has(GATEWAY_SECRET_ID)
                .map_err(|_| ERR_KEYCHAIN_UNAVAILABLE.to_string())?;
            if !stored {
                return Err(ERR_TOKEN_MISSING.into());
            }
            None
        }
    };
    Ok(GatewayPlan {
        base_url,
        token_var,
        supplied_token,
        api,
        skip_check: config.skip_gateway_check == Some(true),
    })
}

/// AIC-FR-ISOC: a value of this many characters or more is masked in records.
const MASKED_VALUE_MIN_CHARS: usize = 8;

/// AIC-FR-YXAB: a token shorter than this keeps no hint, because the last four
/// characters of a short token are most of it.
const HINT_MIN_TOKEN_CHARS: usize = 8;

/// AIC-FR-YXAB: the hint the registry keeps for a gateway token, if any.
pub(super) fn gateway_hint(token: &str) -> Option<String> {
    (token.chars().count() >= HINT_MIN_TOKEN_CHARS).then(|| mask_hint(token))
}

/// One variable of a Claude Code launch (AIC-FR-XZCS).
///
/// `Debug` is hand-rolled and shows the name alone: the value is a credential
/// or author text.
pub struct LaunchVariable {
    pub name: String,
    pub value: SecretString,
    /// AIC-FR-ISOC: whether the executor masks this value in its records.
    pub masked: bool,
}

impl std::fmt::Debug for LaunchVariable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LaunchVariable")
            .field("name", &self.name)
            .field("value", &"<redacted>")
            .finish()
    }
}

/// AIC-FR-XZCS: add a variable, replacing the value of an earlier one of the
/// same name.
fn set_variable(variables: &mut Vec<LaunchVariable>, name: &str, value: &str, masked: bool) {
    let variable = LaunchVariable {
        name: name.to_string(),
        value: SecretString::new(value.to_string()),
        masked,
    };
    match variables.iter_mut().find(|v| v.name == name) {
        Some(existing) => *existing = variable,
        None => variables.push(variable),
    }
}

/// AIC-FR-XZCS / AIC-FR-ISOC: the variables of one Claude Code launch, in order.
///
/// The credential variables come first. Each `env_vars` entry follows and wins
/// over a variable of the same name; among entries of one name the last wins.
///
/// The registry is a file the author can edit, so what it holds is checked
/// again here. A token variable name that fails the rule of AIC-FR-CVPW falls
/// back to the default name, an entry that fails the rule of AIC-FR-XTEZ is
/// skipped, and a gateway record with no base URL is refused.
pub(super) fn compose_launch_environment(
    record: &AgenticRecord,
    credential: &SecretString,
) -> Result<Vec<LaunchVariable>, String> {
    let mut variables = Vec::new();
    match record.auth_mode {
        AuthMode::Subscription => {
            set_variable(&mut variables, OAUTH_TOKEN_VAR, credential.expose(), true);
        }
        AuthMode::CustomGateway => {
            // Without the URL the token would go to the CLI's default host.
            let url = record
                .gateway_base_url
                .as_deref()
                .filter(|u| !u.is_empty())
                .ok_or(ERR_REGISTRY_UNAVAILABLE)?;
            match record.gateway_api {
                GatewayApi::Anthropic => {
                    set_variable(&mut variables, GATEWAY_BASE_URL_VAR, url, false);
                }
                GatewayApi::Bedrock => {
                    set_variable(&mut variables, BEDROCK_FLAG_VAR, "1", false);
                    set_variable(&mut variables, BEDROCK_SKIP_AUTH_VAR, "1", false);
                    set_variable(&mut variables, BEDROCK_BASE_URL_VAR, url, false);
                }
            }
            let name = resolve_token_var(record.gateway_token_var.as_deref())
                .unwrap_or_else(|_| DEFAULT_GATEWAY_TOKEN_VAR.to_string());
            set_variable(&mut variables, &name, credential.expose(), true);
        }
    }
    for entry in &record.env_vars {
        // A stored entry that no longer parses, or that names a reserved
        // variable, was never written by this module; it is skipped.
        if let Some((name, value)) = split_env_entry(entry) {
            if is_reserved_variable(name) {
                continue;
            }
            let masked = value.chars().count() >= MASKED_VALUE_MIN_CHARS;
            set_variable(&mut variables, name, value, masked);
        }
    }
    Ok(variables)
}
