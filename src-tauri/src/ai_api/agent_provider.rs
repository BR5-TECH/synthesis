//! The provider an agent is served by (AAP-FR-APRV).
//!
//! An agent stores no provider. Its endpoint and key come from the AI API
//! provider the open project resolves to, and this module answers which that is
//! from the registry alone — no network request and no vault read — so a locked
//! keychain stops a conversation but never the editing of an agent.

use super::*;

/// What `get_active_ai_api_catalog` returns (AAP-FR-APRV): how the open project
/// resolved its provider, and that provider's catalogue.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct ActiveAiApiCatalog {
    pub resolution: ProjectResolution,
    /// `None` when no provider resolves.
    pub catalog: Option<AiApiCatalog>,
}

/// The project resolution with every stored key assumed present.
fn resolve_registry_only(
    store: &GlobalSettingsStore,
    project_key: &str,
) -> Result<(Vec<AiApiRecord>, ProjectAiApiIntegration), String> {
    let (records, active) = store.load_ai_api_registry()?;
    let override_provider = if project_key.is_empty() {
        None
    } else {
        store.load_ai_api_override(project_key)?
    };
    let present = KeyPresence::from_pairs(
        records
            .iter()
            // AAP-FR-13: a record that never passed a check does not resolve,
            // whatever else it holds.
            .filter(|r| r.masked_hint.is_some() && r.verified_at.is_some())
            .map(|r| (r.provider.clone(), true)),
    );
    let resolved = resolve_project(
        &records,
        active.as_deref(),
        override_provider.as_deref(),
        &present,
    );
    Ok((records, resolved))
}

/// AAP-FR-APRV: the id of the provider the open project resolves to, or
/// `provider_unconfigured` where nothing is configured and `not_verified` where
/// providers exist but none resolves.
pub fn resolve_agent_provider(
    store: &GlobalSettingsStore,
    project_key: &str,
) -> Result<String, String> {
    let (_, resolved) = resolve_registry_only(store, project_key)?;
    match (resolved.resolution, resolved.provider) {
        (ProjectResolution::NoneConfigured, _) => Err(ERR_PROVIDER_UNCONFIGURED.into()),
        (_, Some(provider)) => Ok(provider),
        (_, None) => Err(ERR_NOT_VERIFIED.into()),
    }
}

/// AAP-FR-APRV: the provider in effect for the project together with its
/// catalogue, read from the registry alone.
pub fn active_catalog_impl(
    store: &GlobalSettingsStore,
    project_key: &str,
) -> Result<ActiveAiApiCatalog, String> {
    let (records, resolved) = resolve_registry_only(store, project_key)?;
    let catalog = resolved.provider.as_deref().and_then(|provider| {
        catalogs_from(&records)
            .into_iter()
            .find(|c| c.provider == provider)
    });
    Ok(ActiveAiApiCatalog {
        resolution: resolved.resolution,
        catalog,
    })
}

#[tauri::command]
pub fn get_active_ai_api_catalog(
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<ActiveAiApiCatalog, String> {
    let result = active_catalog_impl(&store, &project.slot_key());
    match &result {
        Ok(active) => logging::log_debug(
            &app,
            &logging::BUFFER,
            &[Domain::Ai],
            "active ai api catalog read",
            log_fields! {
                "provider" => active.catalog.as_ref().map(|c| c.provider.as_str()).unwrap_or("none"),
                "resolution" => active.resolution,
                "models" => active.catalog.as_ref().map(|c| c.models.len()).unwrap_or(0),
            },
        ),
        Err(error) => logging::log_error(
            &app,
            &logging::BUFFER,
            &[Domain::Ai],
            "active ai api catalog could not be read",
            log_fields! { "error" => error },
        ),
    }
    result
}
