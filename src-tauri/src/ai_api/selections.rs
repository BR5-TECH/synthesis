//! The selections an author makes for one provider: its model, its reasoning,
//! the active provider, and Clear (AAP-FR-11, AAP-FR-13, AAP-FR-15, AAP-FR-27).

use super::*;

/// AAP-FR-11: persist the chosen model. `None` selects the provider's own
/// default; an id absent from the record's current list is refused.
pub fn set_model_impl(
    store: &GlobalSettingsStore,
    ai: &AiApiIntegrations,
    provider: &str,
    model_id: Option<&str>,
) -> Result<AiApiIntegration, String> {
    let descriptor = provider_descriptor(provider).ok_or(ERR_UNKNOWN_PROVIDER)?;
    let _guard = ai.write_lock.lock().map_err(|e| format!("ai api registry poisoned: {e}"))?;
    let (mut records, active) = store.load_ai_api_registry()?;
    let index = match records.iter().position(|r| r.provider == provider) {
        Some(i) => i,
        None => {
            records.push(AiApiRecord::empty(provider));
            records.len() - 1
        }
    };
    {
        let record = &mut records[index];
        // The record's *current* list, which is the probed one after a
        // verification and the bundled catalog before it — matching what the
        // selector was populated from.
        let available = available_models(record, descriptor);
        match model_id {
            None => record.selected_model = None,
            Some(id) => {
                if !available.iter().any(|m| m.id == id) {
                    return Err(ERR_UNKNOWN_MODEL.into());
                }
                record.selected_model = Some(id.to_string());
            }
        }
        // AAP-FR-29: the reasoning choice belongs to the model it was made
        // against. A new model that cannot honour it returns the record to that
        // model's own default rather than carrying across a depth it would
        // refuse — including the case of clearing the model entirely, where no
        // capability is known at all.
        if let Some(choice) = record.selected_reasoning.clone() {
            if !reasoning_survives(provider, &available, record.selected_model.as_deref(), &choice)
            {
                record.selected_reasoning = None;
            }
        }
    }
    store.save_ai_api_registry(records.clone(), active.clone())?;
    let present = provider_presence(&*ai.secrets, &records);
    integrations_from(&records, active.as_deref(), &present)
        .into_iter()
        .find(|i| i.provider == provider)
        .ok_or_else(|| ERR_UNKNOWN_PROVIDER.to_string())
}

/// AAP-FR-27: persist the reasoning the author asked for. `None` means the
/// selected model's own default; a choice the selected model cannot honour is
/// refused rather than stored.
pub fn set_reasoning_impl(
    store: &GlobalSettingsStore,
    ai: &AiApiIntegrations,
    provider: &str,
    choice: Option<ReasoningChoice>,
) -> Result<AiApiIntegration, String> {
    let descriptor = provider_descriptor(provider).ok_or(ERR_UNKNOWN_PROVIDER)?;
    let _guard = ai.write_lock.lock().map_err(|e| format!("ai api registry poisoned: {e}"))?;
    let (mut records, active) = store.load_ai_api_registry()?;
    let index = match records.iter().position(|r| r.provider == provider) {
        Some(i) => i,
        None => {
            records.push(AiApiRecord::empty(provider));
            records.len() - 1
        }
    };
    {
        let record = &mut records[index];
        match choice {
            None => record.selected_reasoning = None,
            // AAP-FR-27 / AAP-FR-WSIC: the Custom choice is the provider-level
            // default for every model, so it needs no selected model.
            Some(choice) if provider == custom_limits::CUSTOM => {
                if let Some(err) = custom_limits::custom_reasoning_error(&choice) {
                    return Err(err.into());
                }
                record.selected_reasoning = Some(choice);
            }
            Some(choice) => {
                let available = if record.models.is_empty() {
                    catalog_models(descriptor)
                } else {
                    record.models.clone()
                };
                // Distinguishing "no model chosen" from "this model cannot
                // reason" matters to the author: the first is fixed by picking a
                // model, the second by picking a different one.
                let Some(selected) = record.selected_model.as_deref() else {
                    return Err(ERR_NO_MODEL_SELECTED.into());
                };
                let model = available.iter().find(|m| m.id == selected);
                if model.is_none() {
                    return Err(ERR_NO_MODEL_SELECTED.into());
                }
                if let Some(err) = reasoning_error(model, &choice) {
                    return Err(err.into());
                }
                record.selected_reasoning = Some(choice);
            }
        }
    }
    store.save_ai_api_registry(records.clone(), active.clone())?;
    let present = provider_presence(&*ai.secrets, &records);
    integrations_from(&records, active.as_deref(), &present)
        .into_iter()
        .find(|i| i.provider == provider)
        .ok_or_else(|| ERR_UNKNOWN_PROVIDER.to_string())
}

/// AAP-FR-13: record the user-global active provider. At most one is ever
/// recorded — activating one clears any other.
pub fn set_active_impl(
    store: &GlobalSettingsStore,
    ai: &AiApiIntegrations,
    provider: &str,
) -> Result<Vec<AiApiIntegration>, String> {
    provider_descriptor(provider).ok_or(ERR_UNKNOWN_PROVIDER)?;
    let _guard = ai.write_lock.lock().map_err(|e| format!("ai api registry poisoned: {e}"))?;
    let (records, _) = store.load_ai_api_registry()?;
    let present = provider_presence(&*ai.secrets, &records);
    let verified = records
        .iter()
        .any(|r| r.provider == provider && state_for(r, &present) == IntegrationState::Verified);
    if !verified {
        return Err(ERR_NOT_VERIFIED.into());
    }
    store.save_ai_api_registry(records.clone(), Some(provider.to_string()))?;
    Ok(integrations_from(&records, Some(provider), &present))
}

/// AAP-FR-15: delete the provider's keychain entry and its record together.
/// Idempotent. A cleared provider that was active leaves nothing active; a
/// project override naming it is left recorded and stops resolving.
pub fn clear_integration_impl(
    store: &GlobalSettingsStore,
    ai: &AiApiIntegrations,
    provider: &str,
) -> Result<Vec<AiApiIntegration>, String> {
    provider_descriptor(provider).ok_or(ERR_UNKNOWN_PROVIDER)?;
    let _guard = ai.write_lock.lock().map_err(|e| format!("ai api registry poisoned: {e}"))?;
    ai.secrets.delete(provider).map_err(|_| ERR_KEYCHAIN_UNAVAILABLE.to_string())?;
    let (mut records, active) = store.load_ai_api_registry()?;
    records.retain(|r| r.provider != provider);
    let active = match active {
        Some(a) if a == provider => None,
        other => other,
    };
    store.save_ai_api_registry(records.clone(), active.clone())?;
    let present = provider_presence(&*ai.secrets, &records);
    Ok(integrations_from(&records, active.as_deref(), &present))
}
