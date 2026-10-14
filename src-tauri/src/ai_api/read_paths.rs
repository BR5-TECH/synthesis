//! The read paths for a key and an endpoint (AAP-FR-19, AAP-FR-33, AAP-FR-34).

use super::*;

/// AAP-FR-19: one of the two read paths for a key and an endpoint in the
/// application, and the one selected by what the *project* resolves to.
///
/// Deliberately **not** a `#[tauri::command]`: it is not registered in
/// `generate_handler!`, so no frontend `invoke` can reach it. Its one consumer
/// is the graduation loop (`crate::graduation::driver`, GRL-FR-EKXT), which is
/// the main consumer of the model and the reasoning an integration stores
/// (AAP-FR-31). An agent's turn takes the other path,
/// `resolve_ai_api_endpoint`, naming its own provider and carrying its own
/// model and reasoning (AGC-FR-14). A stored model never reaches an agent, a
/// stored reasoning reaches one only as the Custom default (AAP-FR-34), and an
/// agent's selection never reaches a graduation run.
///
/// It refuses with the same typed distinction `get_project_ai_api_integration`
/// reports rather than returning a key when the project resolves no integration.
pub fn resolve_ai_api_call(
    store: &GlobalSettingsStore,
    ai: &AiApiIntegrations,
    project_key: &str,
) -> Result<AiApiCall, String> {
    let resolved = get_project_impl(store, ai, project_key)?;
    let provider = match (resolved.resolution, resolved.provider.as_deref()) {
        (ProjectResolution::NoneConfigured, _) => return Err(ERR_NONE_CONFIGURED.into()),
        (ProjectResolution::NoneSelected, _) => return Err(ERR_NONE_SELECTED.into()),
        (_, Some(provider)) => provider.to_string(),
        // `override_unavailable` with nothing active: the project names a
        // provider that no longer works and there is no fallback.
        (_, None) => return Err(ERR_NONE_SELECTED.into()),
    };
    let (records, _) = store.load_ai_api_registry()?;
    let record = records.iter().find(|r| r.provider == provider).ok_or(ERR_NONE_SELECTED)?;
    let base_url = record.base_url.clone().filter(|u| !u.is_empty()).ok_or(ERR_NONE_SELECTED)?;
    // The one place a key is read back out, and only for a caller that is about
    // to present it to the endpoint it belongs to.
    let api_key = if record.masked_hint.is_some() {
        ai.secrets.get(&provider).map_err(|_| ERR_KEYCHAIN_UNAVAILABLE.to_string())?
    } else {
        None
    };
    let provider_name = provider.clone();
    Ok(AiApiCall {
        provider,
        base_url,
        api_key,
        model_id: record.selected_model.clone(),
        // AAP-FR-31: the one place the stored choice becomes the reasoning a
        // request carries. `None` sends nothing at all.
        reasoning: record.selected_reasoning.clone(),
        // AAP-FR-35: the flag of the model this project resolved to. A record
        // that names no model resolves to the provider's own default, whose
        // capability nothing here declares — so it is `false`, on the same terms
        // every undeclared capability is.
        accepts_image_input: provider_descriptor(&provider_name)
            .map(|d| available_models(record, d))
            .unwrap_or_default()
            .iter()
            .find(|m| Some(m.id.as_str()) == record.selected_model.as_deref())
            .is_some_and(|m| m.accepts_image_input),
        // AAP-FR-RTMZ: the route metadata of the same model.
        model_mode: provider_descriptor(&provider_name)
            .map(|d| available_models(record, d))
            .unwrap_or_default()
            .iter()
            .find(|m| Some(m.id.as_str()) == record.selected_model.as_deref())
            .and_then(|m| m.mode),
        // AAP-FR-TXNM: no read path but `resolve_ai_api_endpoint` returns it.
        turn_timeout_ms: None,
        // AAP-FR-HFQZ / AAP-FR-RANS: the effective values of the same model.
        max_output_tokens: custom_limits::effective_output_limit(record),
        context_size: custom_limits::effective_context_size(
            record,
            provider_descriptor(&provider_name)
                .map(|d| available_models(record, d))
                .unwrap_or_default()
                .iter()
                .find(|m| Some(m.id.as_str()) == record.selected_model.as_deref()),
        ),
    })
}

/// AAP-FR-33: whether a caller's own `(provider, model, reasoning)` is one this
/// module could serve.
///
/// Deliberately **not** a `#[tauri::command]`, on the same terms as the two
/// resolvers: it is the rule an agent's stored selection is checked against
/// (`crate::agents`, AGR-FR-06 / AGR-FR-07), not something a frontend decides.
///
/// Reads the registry and nothing else. In particular it never asks the
/// keychain whether the key is still there — `state_of` is handed `true` for
/// key presence — because a locked keychain must not stop an author *editing*
/// an agent, only calling one. The keychain is consulted exactly once, in
/// `resolve_ai_api_endpoint` below, at the moment a call is actually made.
#[allow(dead_code)]
pub fn validate_ai_api_selection(
    store: &GlobalSettingsStore,
    provider: &str,
    model_id: &str,
    reasoning: Option<&ReasoningChoice>,
) -> Result<(), String> {
    let descriptor = provider_descriptor(provider).ok_or(ERR_UNKNOWN_PROVIDER)?;
    let (records, _) = store.load_ai_api_registry()?;
    let record = records
        .iter()
        .find(|r| r.provider == provider)
        .cloned()
        .unwrap_or_else(|| AiApiRecord::empty(provider));

    // Registry-only reading of AAP-FR-09 and AAP-FR-13. `key_present: true` is
    // the assumption that makes this offline: a record that carries a masked
    // hint was verified with a key, and whether that key is *readable right now*
    // is a question for the call, not for the edit.
    match state_assuming_key(&record, descriptor.key_required) {
        IntegrationState::Unconfigured => return Err(ERR_PROVIDER_UNCONFIGURED.into()),
        IntegrationState::KeyUnavailable => return Err(ERR_NOT_VERIFIED.into()),
        IntegrationState::Verified => {}
    }

    let models = available_models(&record, descriptor);
    let model = models.iter().find(|m| m.id == model_id).ok_or(ERR_UNKNOWN_MODEL)?;
    // AAP-FR-27: `None` is the model's own default and is always serviceable.
    // AAP-FR-WSIC: a Custom choice is an effort of the fixed set.
    if let Some(choice) = reasoning {
        let refused = if provider == custom_limits::CUSTOM {
            custom_limits::custom_reasoning_error(choice)
        } else {
            reasoning_error(Some(model), choice)
        };
        if let Some(err) = refused {
            return Err(err.into());
        }
    }
    Ok(())
}

/// AAP-FR-34: the endpoint and key of a provider the **caller** names, carrying
/// the **caller's** model and reasoning.
///
/// The difference from `resolve_ai_api_call` is the whole point of it existing:
/// that one asks "what does the open project resolve to, and what did the author
/// choose for it"; this one asks "reach *this* provider with *this* model". The
/// two consumers are settled and do not overlap (AAP-FR-19): this path serves an
/// agent's turn alone (`crate::agent_conversations`, AGC-FR-14), which runs each
/// conversation against the configuration its own agent named — its provider,
/// its model, its reasoning, all chosen in the Agents section rather than in the
/// AI API one — and no project override and no stored selection redirects any of
/// them.
///
/// Deliberately **not** a `#[tauri::command]`: it returns a cleartext key.
pub fn resolve_ai_api_endpoint(
    store: &GlobalSettingsStore,
    ai: &AiApiIntegrations,
    provider: &str,
    model_id: &str,
    reasoning: Option<ReasoningChoice>,
) -> Result<AiApiCall, String> {
    // Validated first, so an unserviceable combination is refused *before* a key
    // is read rather than after.
    validate_ai_api_selection(store, provider, model_id, reasoning.as_ref())?;
    let (records, _) = store.load_ai_api_registry()?;
    let record =
        records.iter().find(|r| r.provider == provider).ok_or(ERR_PROVIDER_UNCONFIGURED)?;
    let base_url =
        record.base_url.clone().filter(|u| !u.is_empty()).ok_or(ERR_PROVIDER_UNCONFIGURED)?;
    let descriptor = provider_descriptor(provider);
    let api_key = if record.masked_hint.is_some() {
        match ai.secrets.get(provider) {
            // The store answered and the entry is gone. A distinct outcome from
            // the store refusing to answer: one is a credential to re-enter, the
            // other is a keychain to unlock.
            Ok(None) => return Err(ERR_KEY_UNAVAILABLE.into()),
            Ok(some) => some,
            Err(_) => return Err(ERR_KEYCHAIN_UNAVAILABLE.into()),
        }
    } else {
        None
    };
    Ok(AiApiCall {
        provider: provider.to_string(),
        base_url,
        api_key,
        // The caller's, never the record's. Substituting the provider's stored
        // selection here would make every agent on a provider answer as one.
        model_id: Some(model_id.to_string()),
        // AAP-FR-34: the caller's reasoning. For Custom alone, a caller with
        // none takes the provider-level default (AAP-FR-AYKU).
        reasoning: custom_limits::effective_agent_reasoning(record, reasoning),
        // AAP-FR-35: the flag of the model that was resolved. `validate_ai_api_selection`
        // above neither read nor reported it — an agent is never refused for the
        // pictures a conversation might later carry — so it is read here, from
        // the same model list the validation resolved against.
        accepts_image_input: descriptor
            .map(|d| available_models(record, d))
            .unwrap_or_default()
            .iter()
            .find(|m| m.id == model_id)
            .is_some_and(|m| m.accepts_image_input),
        // AAP-FR-RTMZ: the route metadata of the resolved model.
        model_mode: descriptor
            .map(|d| available_models(record, d))
            .unwrap_or_default()
            .iter()
            .find(|m| m.id == model_id)
            .and_then(|m| m.mode),
        // AAP-FR-TXNM: this read path alone returns it.
        turn_timeout_ms: record.turn_timeout_ms,
        // AAP-FR-HFQZ / AAP-FR-RANS: the effective values of the resolved model.
        max_output_tokens: custom_limits::effective_output_limit(record),
        context_size: custom_limits::effective_context_size(
            record,
            descriptor
                .map(|d| available_models(record, d))
                .unwrap_or_default()
                .iter()
                .find(|m| m.id == model_id),
        ),
    })
}
