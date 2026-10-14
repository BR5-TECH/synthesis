//! The Tauri commands of the module and their log records.

use super::*;

// ---------------------------------------------------------------------------
// Logging (LGC-logging.md)
// ---------------------------------------------------------------------------
//
// The emit sites sit at the command layer rather than inside the `*_impl`
// functions above, which are pure over their collaborators and are what the
// tests drive. Every command here is a thing the author did, so one record per
// command is the whole account of how a configuration came to be what it is —
// and none of the impls has to carry a sink to produce it.

/// AAP-FR-05: a verification is about to reach an endpoint.
///
/// Takes the key so the record can report whether one was *supplied* — the
/// difference between AAP-FR-32's "re-present what is stored" and a fresh
/// credential, which is the first thing to check when a verification that used
/// to work stops. The key itself goes no further than this boolean; the record
/// is built here rather than at the call site precisely so that is a property of
/// one function a reader can check rather than of every caller.
pub(super) fn log_verify_attempt<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    provider: &str,
    base_url: &str,
    api_key: Option<&str>,
) {
    logging::log_info(
        sink,
        buffer,
        &[Domain::Ai, Domain::Remote],
        "verifying ai api integration",
        log_fields! {
            "provider" => provider,
            // The author's raw field, and it has been validated by nothing at
            // this point: `normalize_base_url`'s refusal of a credential
            // smuggled into a URL happens inside the verification below, which
            // is too late for a record already written. `loggable_base_url` is
            // what makes this field safe — userinfo, query, and fragment gone,
            // the endpoint still recognisable.
            "baseUrl" => crate::ai_shared::loggable_base_url(base_url),
            "keySupplied" => api_key.is_some_and(|k| !k.trim().is_empty()),
        },
    );
}

/// What a verification turned out to be. `ERROR` on failure: the author asked
/// for something and got nothing, and every one of AAP-FR-05's typed refusals
/// names a different correction.
pub(super) fn log_verify_outcome<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    provider: &str,
    result: &Result<AiApiIntegration, String>,
    duration_ms: u64,
) {
    match result {
        Ok(integration) => logging::log_info(
            sink,
            buffer,
            &[Domain::Ai, Domain::Remote],
            "ai api integration verified",
            log_fields! {
                "provider" => provider,
                // Normalised by now, so it carries no userinfo — but a stored
                // URL may still hold a query string, and one gateway or another
                // puts a key there.
                "baseUrl" => integration.base_url.as_deref()
                    .map(crate::ai_shared::loggable_base_url)
                    .unwrap_or_else(|| "none".to_string()),
                "state" => integration.state,
                "keyState" => integration.key_state,
                "models" => integration.models.len(),
                // AAP-FR-10: `catalog` means the probe told us nothing and the
                // list is the bundled one, which is why a model the author
                // expected may be missing from the selector.
                "modelsOrigin" => integration.models_origin,
                "selectedModel" => integration.selected_model.as_deref().unwrap_or("<provider default>"),
                "durationMs" => duration_ms,
            },
        ),
        Err(error) => logging::log_error(
            sink,
            buffer,
            &[Domain::Ai, Domain::Remote],
            "ai api verification failed",
            log_fields! {
                "provider" => provider,
                // One of this module's typed errors (AAP-FR-05). No provider
                // response body and no client error is carried through: both
                // routinely echo the request that produced them, headers and
                // all.
                "error" => error,
                "durationMs" => duration_ms,
            },
        ),
    }
}

/// AAP-FR-OVDB: how the detection of a Custom verification went. Counts and
/// failure kinds only: no record holds the secret, a request body, or a
/// response body. No record where no detection ran.
pub(super) fn log_detection_outcome<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    provider: &str,
    report: Option<&detection::DetectionReport>,
) {
    let Some(report) = report else { return };
    let fields = log_fields! {
        "provider" => provider,
        "models" => report.sent,
        "detected" => report.detected,
        "failed" => report.failed,
        "failureKinds" => report.failure_kinds(),
    };
    logging::log_info(
        sink,
        buffer,
        &[Domain::Ai, Domain::Remote],
        "custom model detection finished",
        fields,
    );
}

/// AAP-FR-02: how the registry reads right now.
///
/// `DEBUG`, because this runs whenever the settings level renders and says
/// nothing new most times it does. It earns its slot on the one occasion it
/// matters: a provider that was working reads `key_unavailable` because the
/// keychain no longer answers for it (AAP-FR-09), which is invisible everywhere
/// else until an agent fails to answer and nobody can say why.
pub(super) fn log_listing<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    result: &Result<Vec<AiApiIntegration>, String>,
) {
    match result {
        Ok(integrations) => logging::log_debug(
            sink,
            buffer,
            &[Domain::Ai],
            "ai api integrations listed",
            log_fields! {
                "total" => integrations.len(),
                "verified" => integrations.iter().filter(|i| i.state == IntegrationState::Verified).count(),
                "keyUnavailable" => integrations.iter().filter(|i| i.state == IntegrationState::KeyUnavailable).count(),
                "active" => integrations.iter().find(|i| i.active).map(|i| i.provider.as_str()).unwrap_or("none"),
            },
        ),
        Err(error) => logging::log_error(
            sink,
            buffer,
            &[Domain::Ai],
            "ai api integrations could not be listed",
            log_fields! { "error" => error },
        ),
    }
}

/// AAP-FR-16: which provider the open project actually calls, and whether an
/// override it still names is being ignored.
///
/// `DEBUG` on the same terms as the listing: read on every render, and the one
/// thing worth knowing from it — `override_unavailable`, where the author
/// believes they are calling one provider and the application is calling another
/// — is otherwise reported nowhere but in the settings panel they are not
/// looking at.
pub(super) fn log_project_resolution<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    resolved: &ProjectAiApiIntegration,
) {
    logging::log_debug(
        sink,
        buffer,
        &[Domain::Ai],
        "project ai api integration resolved",
        log_fields! {
            "provider" => resolved.provider.as_deref().unwrap_or("none"),
            "resolution" => resolved.resolution,
            "override" => resolved.override_provider.as_deref().unwrap_or("none"),
        },
    );
}

/// A change to the registry: `INFO` when it took, `WARN` with the typed refusal
/// when it did not.
///
/// `WARN` rather than `ERROR` because nothing broke — a rejected model id or an
/// unverified provider is the module declining to store something it could not
/// serve (AAP-FR-11, AAP-FR-13, AAP-FR-27), and the record exists so an author
/// wondering why their choice did not stick can see that it was refused.
pub(super) fn log_config_outcome<S: LogSink + Clone + Send + 'static, T>(
    sink: &S,
    buffer: &'static LogBuffer,
    message: &str,
    result: &Result<T, String>,
    mut fields: Fields,
) {
    match result {
        Ok(_) => logging::log_info(sink, buffer, &[Domain::Ai], message, fields),
        Err(error) => {
            fields.insert("error".to_string(), serde_json::json!(error));
            logging::log_warn(sink, buffer, &[Domain::Ai], message, fields);
        }
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_ai_api_integrations(
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    ai: State<'_, AiApiIntegrations>,
) -> Result<Vec<AiApiIntegration>, String> {
    let result = list_integrations_impl(&store, &ai);
    log_listing(&app, &logging::BUFFER, &result);
    result
}

#[tauri::command]
pub fn list_ai_api_catalogs(
    store: State<'_, GlobalSettingsStore>,
) -> Result<Vec<AiApiCatalog>, String> {
    list_catalogs_impl(&store)
}

#[tauri::command]
pub fn verify_ai_api_integration(
    provider: String,
    base_url: String,
    api_key: Option<String>,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    ai: State<'_, AiApiIntegrations>,
) -> Result<AiApiIntegration, String> {
    log_verify_attempt(&app, &logging::BUFFER, &provider, &base_url, api_key.as_deref());
    let started = std::time::Instant::now();
    let mut report = None;
    let result = verify_integration_reported(
        &store,
        &ai,
        &provider,
        &base_url,
        api_key.as_deref(),
        &mut report,
    );
    // The duration is what separates `timed_out` from a slow endpoint that
    // answered, and it is the only measurement of the one network call this
    // module makes.
    log_verify_outcome(
        &app,
        &logging::BUFFER,
        &provider,
        &result,
        started.elapsed().as_millis() as u64,
    );
    log_detection_outcome(&app, &logging::BUFFER, &provider, report.as_ref());
    result
}

#[tauri::command]
pub fn set_ai_api_model(
    provider: String,
    model_id: Option<String>,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    ai: State<'_, AiApiIntegrations>,
) -> Result<AiApiIntegration, String> {
    let result = set_model_impl(&store, &ai, &provider, model_id.as_deref());
    log_config_outcome(
        &app,
        &logging::BUFFER,
        "ai api model selected",
        &result,
        log_fields! {
            "provider" => &provider,
            "model" => model_id.as_deref().unwrap_or("<provider default>"),
            // AAP-FR-29: a model change can clear the reasoning choice under the
            // author, which is otherwise a silent change to what every call
            // carries.
            "reasoning" => result.as_ref().ok()
                .map(|i| reasoning_label(i.selected_reasoning.as_ref()))
                .unwrap_or_else(|| "unchanged".to_string()),
        },
    );
    result
}

#[tauri::command]
pub fn set_ai_api_reasoning(
    provider: String,
    choice: Option<ReasoningChoice>,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    ai: State<'_, AiApiIntegrations>,
) -> Result<AiApiIntegration, String> {
    let requested = reasoning_label(choice.as_ref());
    let result = set_reasoning_impl(&store, &ai, &provider, choice);
    log_config_outcome(
        &app,
        &logging::BUFFER,
        "ai api reasoning selected",
        &result,
        log_fields! { "provider" => &provider, "reasoning" => requested },
    );
    result
}

#[tauri::command]
pub fn set_active_ai_api_integration(
    provider: String,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    ai: State<'_, AiApiIntegrations>,
) -> Result<Vec<AiApiIntegration>, String> {
    let result = set_active_impl(&store, &ai, &provider);
    log_config_outcome(
        &app,
        &logging::BUFFER,
        "active ai api integration set",
        &result,
        log_fields! { "provider" => &provider },
    );
    result
}

#[tauri::command]
pub fn clear_ai_api_integration(
    provider: String,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    ai: State<'_, AiApiIntegrations>,
) -> Result<Vec<AiApiIntegration>, String> {
    let result = clear_integration_impl(&store, &ai, &provider);
    // AAP-FR-15: the keychain entry is gone and every agent naming this provider
    // stops resolving. Worth a line of its own — an agent that answered
    // yesterday and refuses today is otherwise unexplained.
    log_config_outcome(
        &app,
        &logging::BUFFER,
        "ai api integration cleared",
        &result,
        log_fields! { "provider" => &provider },
    );
    result
}

#[tauri::command]
pub fn get_project_ai_api_integration(
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    ai: State<'_, AiApiIntegrations>,
    project: State<'_, ProjectState>,
) -> Result<ProjectAiApiIntegration, String> {
    let result = get_project_impl(&store, &ai, &project.slot_key());
    if let Ok(resolved) = &result {
        log_project_resolution(&app, &logging::BUFFER, resolved);
    }
    result
}

#[tauri::command]
pub fn set_project_ai_api_integration(
    provider: Option<String>,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    ai: State<'_, AiApiIntegrations>,
    project: State<'_, ProjectState>,
) -> Result<ProjectAiApiIntegration, String> {
    let result = set_project_impl(&store, &ai, &project.slot_key(), provider.as_deref());
    log_config_outcome(
        &app,
        &logging::BUFFER,
        "project ai api override set",
        &result,
        log_fields! {
            "provider" => provider.as_deref().unwrap_or("<inherit>"),
            "resolution" => result.as_ref().ok().map(|r| r.resolution),
        },
    );
    result
}
