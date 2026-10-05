//! The Tauri command surface of the agentic integrations
//! (`AIC-agentic-integration-config.md`).

use super::*;

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_agentic_integrations(
    store: State<'_, GlobalSettingsStore>,
    ai: State<'_, AgenticIntegrations>,
) -> Result<Vec<AgenticIntegration>, String> {
    list_integrations_impl(&store, &ai)
}

#[tauri::command]
pub fn detect_agentic_cli_binary(
    vendor: String,
    ai: State<'_, AgenticIntegrations>,
) -> Result<DetectedBinary, String> {
    detect_binary_impl(&ai, &vendor)
}

#[tauri::command]
pub fn verify_agentic_integration(
    vendor: String,
    config: VerifyConfig,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    ai: State<'_, AgenticIntegrations>,
) -> Result<AgenticIntegration, String> {
    log_verify_attempt(&app, &logging::BUFFER, &vendor, &config);
    let started = std::time::Instant::now();
    let result = verify_integration_impl(&store, &ai, &vendor, &config);
    // The duration is what separates `timed_out` from a slow binary that
    // answered, and it is the only measurement of the one child process this
    // module spawns.
    log_verify_outcome(
        &app,
        &logging::BUFFER,
        &vendor,
        &result,
        started.elapsed().as_millis() as u64,
    );
    result
}

#[tauri::command]
pub fn set_agentic_integration_model(
    vendor: String,
    turn_kind: Option<String>,
    model_id: Option<String>,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    ai: State<'_, AgenticIntegrations>,
) -> Result<AgenticIntegration, String> {
    let result = set_model_impl(&store, &ai, &vendor, turn_kind.as_deref(), model_id.as_deref());
    if let Err(error) = &result {
        log_selection_refused(
            &app,
            &logging::BUFFER,
            &vendor,
            "model",
            turn_kind.as_deref(),
            error,
        );
    }
    result
}

#[tauri::command]
pub fn set_agentic_integration_effort(
    vendor: String,
    turn_kind: Option<String>,
    effort_id: Option<String>,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    ai: State<'_, AgenticIntegrations>,
) -> Result<AgenticIntegration, String> {
    let result = set_effort_impl(&store, &ai, &vendor, turn_kind.as_deref(), effort_id.as_deref());
    if let Err(error) = &result {
        log_selection_refused(
            &app,
            &logging::BUFFER,
            &vendor,
            "effort",
            turn_kind.as_deref(),
            error,
        );
    }
    result
}

#[tauri::command]
pub fn set_active_agentic_integration(
    vendor: String,
    store: State<'_, GlobalSettingsStore>,
    ai: State<'_, AgenticIntegrations>,
) -> Result<Vec<AgenticIntegration>, String> {
    set_active_impl(&store, &ai, &vendor)
}

#[tauri::command]
pub fn clear_agentic_integration(
    vendor: String,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    ai: State<'_, AgenticIntegrations>,
) -> Result<Vec<AgenticIntegration>, String> {
    let result = clear_integration_impl(&store, &ai, &vendor);
    log_clear_outcome(&app, &logging::BUFFER, &vendor, &result);
    result
}

/// GSU-FR-GLVQ / AIC-FR-33: which vendor the open project resolves to, for a
/// caller that needs the **identity** alone — the identity a project's committed
/// Docker image entry is selected by.
///
/// Best-effort: a project that resolves nothing, and a store that cannot be
/// read, both answer `None`, which the image preflight reads as "not this
/// preflight's business" rather than as a refusal naming a vendor there is none
/// of. This module holds no image reference, no digest, no Dockerfile path, and
/// no Docker backend value — the entry itself is the project's
/// (`PSS-project-settings-storage.md` PSS-FR-22) and the backend is the
/// machine's (`GSS-global-settings-storage.md` GSS-FR-35).
pub fn resolve_project_vendor<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    project_key: &str,
) -> Option<String> {
    use tauri::Manager;
    let store = app.try_state::<GlobalSettingsStore>()?;
    let ai = app.try_state::<AgenticIntegrations>()?;
    get_project_impl(&store, &ai, project_key)
        .ok()
        .and_then(|resolved| resolved.vendor)
}

#[tauri::command]
pub fn get_project_agentic_integration(
    store: State<'_, GlobalSettingsStore>,
    ai: State<'_, AgenticIntegrations>,
    project: State<'_, ProjectState>,
) -> Result<ProjectAgenticIntegration, String> {
    get_project_impl(&store, &ai, &project.slot_key())
}

#[tauri::command]
pub fn set_project_agentic_integration(
    vendor: Option<String>,
    store: State<'_, GlobalSettingsStore>,
    ai: State<'_, AgenticIntegrations>,
    project: State<'_, ProjectState>,
) -> Result<ProjectAgenticIntegration, String> {
    set_project_impl(&store, &ai, &project.slot_key(), vendor.as_deref())
}

