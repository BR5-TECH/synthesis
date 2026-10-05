//! The turn timeout a provider stores (AAP-FR-FGNK).
//!
//! It bounds each conversation turn that runs on the provider
//! (`CVL-conversation-loop.md` CVL-FR-16) and nothing else, so it is
//! independent of the model and the reasoning the record also holds.

use super::*;

/// AAP-FR-FGNK: the shortest turn timeout a provider may store, in
/// milliseconds.
pub const TURN_TIMEOUT_MIN_MS: u64 = 30_000;
/// AAP-FR-FGNK: the longest turn timeout a provider may store, in
/// milliseconds.
pub const TURN_TIMEOUT_MAX_MS: u64 = 3_600_000;

/// AAP-FR-FGNK: store the turn timeout of one provider, or clear it on `None`.
/// A value outside the range is refused and nothing changes. Other providers'
/// records are not touched.
pub fn set_turn_timeout_impl(
    store: &GlobalSettingsStore,
    ai: &AiApiIntegrations,
    provider: &str,
    timeout_ms: Option<u64>,
) -> Result<AiApiIntegration, String> {
    provider_descriptor(provider).ok_or(ERR_UNKNOWN_PROVIDER)?;
    if let Some(ms) = timeout_ms {
        if !(TURN_TIMEOUT_MIN_MS..=TURN_TIMEOUT_MAX_MS).contains(&ms) {
            return Err(ERR_TURN_TIMEOUT_OUT_OF_RANGE.into());
        }
    }
    let _guard = ai
        .write_lock
        .lock()
        .map_err(|e| format!("ai api registry poisoned: {e}"))?;
    let (mut records, active) = store.load_ai_api_registry()?;
    match records.iter_mut().find(|r| r.provider == provider) {
        Some(record) => record.turn_timeout_ms = timeout_ms,
        None => records.push(AiApiRecord {
            turn_timeout_ms: timeout_ms,
            ..AiApiRecord::empty(provider)
        }),
    }
    store.save_ai_api_registry(records.clone(), active.clone())?;
    let present = provider_presence(&*ai.secrets, &records);
    integrations_from(&records, active.as_deref(), &present)
        .into_iter()
        .find(|i| i.provider == provider)
        .ok_or_else(|| ERR_UNKNOWN_PROVIDER.to_string())
}

#[tauri::command]
pub fn set_ai_api_turn_timeout(
    provider: String,
    timeout_ms: Option<u64>,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    ai: State<'_, AiApiIntegrations>,
) -> Result<AiApiIntegration, String> {
    let result = set_turn_timeout_impl(&store, &ai, &provider, timeout_ms);
    log_config_outcome(
        &app,
        &logging::BUFFER,
        "ai api turn timeout set",
        &result,
        log_fields! {
            "provider" => &provider,
            "timeoutMs" => timeout_ms
                .map(|ms| ms.to_string())
                .unwrap_or_else(|| "default".to_string()),
        },
    );
    result
}
