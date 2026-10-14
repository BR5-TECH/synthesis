//! The project override of the AI API integration (AAP-FR-15 .. AAP-FR-18):
//! how the open project resolves an integration, and how the author records or
//! clears the override.

use super::*;

/// AAP-FR-16: resolve the open project's API integration, and say *how* — which
/// is what tells the UI what to write on the line.
pub fn resolve_project(
    records: &[AiApiRecord],
    explicit_active: Option<&str>,
    override_provider: Option<&str>,
    present: &KeyPresence,
) -> ProjectAiApiIntegration {
    let active = active_provider(records, explicit_active, present);
    let is_verified = |provider: &str| {
        records
            .iter()
            .any(|r| r.provider == provider && state_for(r, present) == IntegrationState::Verified)
    };

    if let Some(over) = override_provider {
        if is_verified(over) {
            return ProjectAiApiIntegration {
                provider: Some(over.to_string()),
                resolution: ProjectResolution::Overridden,
                override_provider: Some(over.to_string()),
            };
        }
        // The override is recorded but no longer usable. It is deliberately not
        // erased (AAP-FR-15): re-verifying the provider should restore the
        // author's choice rather than silently having lost it.
        return ProjectAiApiIntegration {
            provider: active,
            resolution: ProjectResolution::OverrideUnavailable,
            override_provider: Some(over.to_string()),
        };
    }

    if let Some(active) = active {
        return ProjectAiApiIntegration {
            provider: Some(active),
            resolution: ProjectResolution::Inherited,
            override_provider: None,
        };
    }

    // Nothing resolves. The distinction the UI needs is whether there is
    // anything to choose between at all: something configured means "pick one",
    // nothing configured means "go and set one up".
    let anything_configured =
        records.iter().any(|r| state_for(r, present) != IntegrationState::Unconfigured);
    ProjectAiApiIntegration {
        provider: None,
        resolution: if anything_configured {
            ProjectResolution::NoneSelected
        } else {
            ProjectResolution::NoneConfigured
        },
        override_provider: None,
    }
}

/// AAP-FR-16: how the open project resolves an API integration right now.
pub fn get_project_impl(
    store: &GlobalSettingsStore,
    ai: &AiApiIntegrations,
    project_key: &str,
) -> Result<ProjectAiApiIntegration, String> {
    let (records, active) = store.load_ai_api_registry()?;
    // An empty key is "no project open" (`ProjectState::slot_key`), whose slot is
    // the shared default one — an override read from there would belong to no
    // project in particular.
    let override_provider =
        if project_key.is_empty() { None } else { store.load_ai_api_override(project_key)? };
    Ok(resolve_project(
        &records,
        active.as_deref(),
        override_provider.as_deref(),
        &provider_presence(&*ai.secrets, &records),
    ))
}

/// AAP-FR-17 / FR-18: record (or clear) the open project's override. A provider
/// that is not currently verified is refused, so an override can only ever name
/// an integration the author has already made work.
pub fn set_project_impl(
    store: &GlobalSettingsStore,
    ai: &AiApiIntegrations,
    project_key: &str,
    provider: Option<&str>,
) -> Result<ProjectAiApiIntegration, String> {
    if project_key.is_empty() {
        return Err(ERR_NO_PROJECT.into());
    }
    match provider {
        None => store.clear_ai_api_override(project_key)?,
        Some(provider) => {
            provider_descriptor(provider).ok_or(ERR_UNKNOWN_PROVIDER)?;
            let (records, _) = store.load_ai_api_registry()?;
            let present = provider_presence(&*ai.secrets, &records);
            let verified = records.iter().any(|r| {
                r.provider == provider && state_for(r, &present) == IntegrationState::Verified
            });
            if !verified {
                return Err(ERR_NOT_VERIFIED.into());
            }
            store.save_ai_api_override(project_key, provider)?;
        }
    }
    get_project_impl(store, ai, project_key)
}
