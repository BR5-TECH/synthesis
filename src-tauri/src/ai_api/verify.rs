//! Verification of one provider (AAP-FR-05, AAP-FR-06, AAP-FR-10).

use super::*;

/// AAP-FR-05 / FR-06 / FR-10: reach the endpoint, confirm it answers as a
/// conversational API, learn its models, and only then commit the configuration.
pub fn verify_integration_impl(
    store: &GlobalSettingsStore,
    ai: &AiApiIntegrations,
    provider: &str,
    base_url: &str,
    api_key: Option<&str>,
) -> Result<AiApiIntegration, String> {
    let descriptor = provider_descriptor(provider).ok_or(ERR_UNKNOWN_PROVIDER)?;

    // Both checks happen before any network work: an empty field and a typo are
    // reported instantly and are never mistaken for an unreachable host.
    let normalized = normalize_base_url(base_url).map_err(|e| match e {
        BaseUrlError::Empty => ERR_BASE_URL_EMPTY.to_string(),
        BaseUrlError::Invalid => ERR_BASE_URL_INVALID.to_string(),
    })?;
    // AAP-FR-KRVT: the gateway root carries no `/v1`, so a route is never built
    // as `/v1/v1/...`.
    if provider == "custom" && custom_gateway::has_v1_suffix(&normalized) {
        return Err(ERR_BASE_URL_INVALID.into());
    }
    // AAP-FR-32: an empty field means the key is unchanged, not withdrawn. The
    // field is never populated from the record (AII-FR-07), so treating empty
    // as "no key" would make a configured provider impossible to verify again
    // without the author holding their key a second time — which is the one
    // thing storing it was meant to avoid.
    let supplied = api_key.map(str::trim).filter(|k| !k.is_empty());
    let key = match &supplied {
        Some(k) => Some((*k).to_string()),
        None => ai
            .secrets
            .get(provider)
            .map_err(|_| ERR_KEYCHAIN_UNAVAILABLE.to_string())?
            .filter(|k| !k.is_empty()),
    };
    if descriptor.key_required && key.is_none() {
        return Err(ERR_KEY_MISSING.into());
    }
    let key = key.as_deref();

    let probed = ai
        .prober_for(provider)
        .probe(&ProbeRequest {
            base_url: &normalized,
            api_key: key,
            auth: descriptor.auth,
            models_path: descriptor.models_path,
            models_format: descriptor.models_format,
        })
        .map_err(api_probe_error)?;

    // AAP-FR-QRAX / AAP-FR-KJSL: a Custom list that verified is followed by
    // detection. A failure gives a model no detected values and changes
    // nothing else, so the verification goes on whatever detection reports.
    let probed = if provider == custom_limits::CUSTOM {
        detection::detect_all(&*ai.detector, &normalized, key, probed)
    } else {
        probed
    };

    // AAP-FR-10: an endpoint that lists nothing degrades to the bundled catalog
    // rather than failing a verification that otherwise succeeded.
    let (models, models_origin) = if probed.is_empty() {
        (catalog_models(descriptor), ModelsOrigin::Catalog)
    } else {
        // AAP-FR-35: what the probe declared about each model's input
        // modalities stands; what it said nothing about the shipped catalog
        // settles; and what neither declares carries `false`.
        (settle_image_input(probed, descriptor), ModelsOrigin::Probed)
    };

    let _guard = ai.write_lock.lock().map_err(|e| format!("ai api registry poisoned: {e}"))?;

    // AAP-FR-06: a key the author supplied goes to the keychain first. A
    // keychain that refuses leaves the registry untouched, so a failed write
    // cannot produce a record describing a key that was never stored.
    //
    // AAP-FR-32: nothing is written when the verification re-presented the
    // stored key — the entry is already what it should be — and no verification
    // deletes one. Withdrawing a key is what `clear_ai_api_integration` is for,
    // so an author checking a working provider cannot lose their credential by
    // leaving the field alone.
    if let Some(supplied) = supplied {
        ai.secrets.set(provider, supplied).map_err(|_| ERR_KEYCHAIN_UNAVAILABLE.to_string())?;
    }

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
        record.base_url = Some(normalized);
        record.masked_hint = key.map(mask_hint);
        record.verified_at = Some(now_iso8601());
        // AAP-FR-12: a selection the refreshed list no longer offers falls back
        // to the provider's own default rather than requesting a model that
        // endpoint does not serve.
        if let Some(selected) = record.selected_model.clone() {
            if !models.iter().any(|m| m.id == selected) {
                record.selected_model = None;
            }
        }
        // AAP-FR-29: and the reasoning choice is re-checked against the refreshed
        // descriptors, so a level the model has stopped offering — or that fell
        // away with the model itself just above — does not survive as a depth
        // the endpoint would refuse.
        if let Some(choice) = record.selected_reasoning.clone() {
            if !reasoning_survives(provider, &models, record.selected_model.as_deref(), &choice) {
                record.selected_reasoning = None;
            }
        }
        record.models = models;
        record.models_origin = models_origin;
    }

    store.save_ai_api_registry(records.clone(), active.clone())?;
    let present = provider_presence(&*ai.secrets, &records);
    integrations_from(&records, active.as_deref(), &present)
        .into_iter()
        .find(|i| i.provider == provider)
        .ok_or_else(|| ERR_UNKNOWN_PROVIDER.to_string())
}
