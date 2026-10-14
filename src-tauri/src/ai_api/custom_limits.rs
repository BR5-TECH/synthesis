//! The provider-level values of the Custom provider (AAP-FR-AYKU), its fixed
//! reasoning set (AAP-FR-WSIC), and the effective values that the read paths
//! return (AAP-FR-RANS, AAP-FR-HFQZ).
//!
//! A local model has limits that its model list does not state, so the Custom
//! record holds defaults that apply to every Custom model.

use super::*;
use crate::ai_shared::ModelReasoning;

/// The id of the Custom provider.
pub const CUSTOM: &str = "custom";

/// AAP-FR-ITBL: the value is not a whole number from 1 to `TOKEN_LIMIT_MAX`.
pub const ERR_TOKEN_LIMIT_INVALID: &str = "token_limit_invalid";
/// AAP-FR-ITBL: the provider is not Custom, so it holds no token limits.
pub const ERR_LIMIT_UNSUPPORTED: &str = "limit_unsupported";

/// AAP-FR-ITBL: the largest token value a Custom field accepts.
pub const TOKEN_LIMIT_MAX: u64 = u32::MAX as u64;

/// AAP-FR-WSIC: the reasoning levels every Custom model offers, in order.
pub const CUSTOM_EFFORTS: [&str; 4] = ["none", "low", "medium", "high"];

/// AAP-FR-WSIC: the one reasoning descriptor of every Custom model.
pub fn custom_reasoning() -> ModelReasoning {
    ModelReasoning {
        mandatory: false,
        default_enabled: None,
        supported_efforts: Some(CUSTOM_EFFORTS.iter().map(|e| e.to_string()).collect()),
        default_effort: None,
    }
}

/// AAP-FR-WSIC: give every model of a Custom list the fixed descriptor. The
/// detected capabilities do not change it.
pub fn with_custom_reasoning(models: Vec<ModelOption>) -> Vec<ModelOption> {
    models.into_iter().map(|m| m.with_reasoning(Some(custom_reasoning()))).collect()
}

/// AAP-FR-WSIC: a Custom reasoning choice is an effort of the fixed set and
/// nothing else.
pub fn custom_reasoning_error(choice: &ReasoningChoice) -> Option<&'static str> {
    match choice {
        ReasoningChoice::Effort { effort } if CUSTOM_EFFORTS.contains(&effort.as_str()) => None,
        _ => Some(ERR_UNKNOWN_EFFORT),
    }
}

/// AAP-FR-RANS: the effective context size of one Custom model — the detected
/// value, else the provider's max context size, else `None`. `None` for every
/// other provider.
pub fn effective_context_size(record: &AiApiRecord, model: Option<&ModelOption>) -> Option<u64> {
    if record.provider != CUSTOM {
        return None;
    }
    model.and_then(|m| m.detected_context_size).or(record.max_context_size)
}

/// AAP-FR-HFQZ: the effective output limit — the Custom provider's max output
/// tokens, else `None`, which leaves the loop's default.
pub fn effective_output_limit(record: &AiApiRecord) -> Option<u64> {
    if record.provider != CUSTOM {
        return None;
    }
    record.max_output_tokens
}

/// AAP-FR-34: the reasoning an agent's call carries. For Custom, an agent
/// with no reasoning of its own takes the provider-level default.
pub fn effective_agent_reasoning(
    record: &AiApiRecord,
    reasoning: Option<ReasoningChoice>,
) -> Option<ReasoningChoice> {
    if record.provider == CUSTOM {
        reasoning.or_else(|| record.selected_reasoning.clone())
    } else {
        reasoning
    }
}

/// Which of the two token values a command sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenLimit {
    MaxOutputTokens,
    MaxContextSize,
}

/// AAP-FR-ITBL: store one token value of the Custom provider, or clear it on
/// `None`. A value out of range and a provider other than Custom are refused,
/// and nothing changes.
pub fn set_token_limit_impl(
    store: &GlobalSettingsStore,
    ai: &AiApiIntegrations,
    provider: &str,
    limit: TokenLimit,
    tokens: Option<u64>,
) -> Result<AiApiIntegration, String> {
    provider_descriptor(provider).ok_or(ERR_UNKNOWN_PROVIDER)?;
    if provider != CUSTOM {
        return Err(ERR_LIMIT_UNSUPPORTED.into());
    }
    if let Some(value) = tokens {
        if !(1..=TOKEN_LIMIT_MAX).contains(&value) {
            return Err(ERR_TOKEN_LIMIT_INVALID.into());
        }
    }
    let _guard = ai.write_lock.lock().map_err(|e| format!("ai api registry poisoned: {e}"))?;
    let (mut records, active) = store.load_ai_api_registry()?;
    let index = match records.iter().position(|r| r.provider == provider) {
        Some(i) => i,
        None => {
            records.push(AiApiRecord::empty(provider));
            records.len() - 1
        }
    };
    match limit {
        TokenLimit::MaxOutputTokens => records[index].max_output_tokens = tokens,
        TokenLimit::MaxContextSize => records[index].max_context_size = tokens,
    }
    store.save_ai_api_registry(records.clone(), active.clone())?;
    let present = provider_presence(&*ai.secrets, &records);
    integrations_from(&records, active.as_deref(), &present)
        .into_iter()
        .find(|i| i.provider == provider)
        .ok_or_else(|| ERR_UNKNOWN_PROVIDER.to_string())
}

/// The value as a log record shows it: the number, or "default" when cleared.
fn shown(tokens: Option<u64>) -> String {
    tokens.map(|t| t.to_string()).unwrap_or_else(|| "default".to_string())
}

#[tauri::command]
pub fn set_ai_api_max_output_tokens(
    provider: String,
    tokens: Option<u64>,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    ai: State<'_, AiApiIntegrations>,
) -> Result<AiApiIntegration, String> {
    let result = set_token_limit_impl(&store, &ai, &provider, TokenLimit::MaxOutputTokens, tokens);
    log_config_outcome(
        &app,
        &logging::BUFFER,
        "ai api max output tokens set",
        &result,
        log_fields! { "provider" => &provider, "tokens" => shown(tokens) },
    );
    result
}

#[tauri::command]
pub fn set_ai_api_max_context_size(
    provider: String,
    tokens: Option<u64>,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    ai: State<'_, AiApiIntegrations>,
) -> Result<AiApiIntegration, String> {
    let result = set_token_limit_impl(&store, &ai, &provider, TokenLimit::MaxContextSize, tokens);
    log_config_outcome(
        &app,
        &logging::BUFFER,
        "ai api max context size set",
        &result,
        log_fields! { "provider" => &provider, "tokens" => shown(tokens) },
    );
    result
}
