//! The Tauri commands of `DVC-devcontainer-environment.md` ("Contract
//! surface").

use serde::Serialize;
use tauri::State;

use super::build::{self, BuildEvents, DevcontainerBuildRegistry};
use super::cli::{self, DevcontainerCli, DevcontainerCliFailure};
use super::{config, AgentEnvironmentError, AgentEnvironmentState};
use crate::global_settings::GlobalSettingsStore;
use crate::log_fields;
use crate::logging::{self, Domain, BUFFER};
use crate::project::ProjectState;
use crate::project_settings::agent_environment::AgentEnvironment;

/// DVC-FR-OXCZ: the id of a build that started.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartedDevcontainerBuild {
    pub operation_id: String,
}

/// DVC-FR-XZCD: what detection found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedDevcontainerCli {
    pub path: Option<String>,
}

fn store_error(message: String) -> AgentEnvironmentError {
    AgentEnvironmentError::StoreFailed { message }
}

#[project_scope_macros::project_scoped]
#[project_scope_macros::blocking_command]
pub fn load_agent_environment(
    project: crate::project_scope::Scoped<'_, ProjectState>,
) -> Result<AgentEnvironmentState, AgentEnvironmentError> {
    let root = project.require_root().map_err(store_error)?;
    Ok(super::state_of(&root))
}

#[project_scope_macros::project_scoped]
#[project_scope_macros::blocking_command]
pub fn save_agent_environment(
    environment: AgentEnvironment,
    app: tauri::AppHandle,
    project: crate::project_scope::Scoped<'_, ProjectState>,
) -> Result<AgentEnvironmentState, AgentEnvironmentError> {
    let root = project.require_root().map_err(store_error)?;
    let source = format!("{:?}", environment.source);
    let saved = super::save(&root, environment);
    match &saved {
        Ok(state) => logging::log_info(
            &app,
            &BUFFER,
            &[Domain::Backend],
            "agent environment saved",
            log_fields! {
                "source" => source,
                "config" => state.environment.devcontainer_config.clone().unwrap_or_default()
            },
        ),
        Err(error) => logging::log_warn(
            &app,
            &BUFFER,
            &[Domain::Backend],
            "agent environment refused",
            log_fields! { "source" => source, "reason" => format!("{error:?}") },
        ),
    }
    saved
}

#[project_scope_macros::project_scoped]
#[project_scope_macros::blocking_command]
pub fn list_devcontainer_configs(
    project: crate::project_scope::Scoped<'_, ProjectState>,
) -> Result<Vec<String>, String> {
    let root = project.require_root()?;
    Ok(config::list_configs(&root))
}

#[project_scope_macros::project_scoped]
#[project_scope_macros::blocking_command]
pub fn create_devcontainer_config(
    app: tauri::AppHandle,
    project: crate::project_scope::Scoped<'_, ProjectState>,
) -> Result<AgentEnvironmentState, AgentEnvironmentError> {
    let root = project.require_root().map_err(store_error)?;
    let created = super::create_config(&root);
    let outcome = match &created {
        Ok(_) => "created".to_string(),
        Err(error) => format!("{error:?}"),
    };
    logging::log_info(
        &app,
        &BUFFER,
        &[Domain::Backend],
        "dev container config creation",
        log_fields! { "outcome" => outcome },
    );
    created
}

#[project_scope_macros::project_scoped]
#[project_scope_macros::blocking_command]
pub fn add_devcontainer_agent_feature(
    vendor: String,
    app: tauri::AppHandle,
    project: crate::project_scope::Scoped<'_, ProjectState>,
) -> Result<AgentEnvironmentState, AgentEnvironmentError> {
    let root = project.require_root().map_err(store_error)?;
    let added = super::add_feature(&root, &vendor);
    let outcome = match &added {
        Ok(_) => "added".to_string(),
        Err(error) => format!("{error:?}"),
    };
    logging::log_info(
        &app,
        &BUFFER,
        &[Domain::Backend],
        "dev container agent feature",
        log_fields! { "vendor" => vendor.as_str(), "outcome" => outcome },
    );
    added
}

/// DVC-FR-RTBA: the two build events, through the Tauri bus. An emit that
/// cannot be delivered is dropped: a report must never fail the build.
struct AppBuildEvents(tauri::AppHandle);

impl BuildEvents for AppBuildEvents {
    fn progress(&self, payload: build::DevcontainerBuildProgress) {
        use crate::project_scope::events::EmitToProject;
        let _ = self.0.emit_project(build::DEVCONTAINER_BUILD_PROGRESS, payload);
    }

    fn finished(&self, payload: build::DevcontainerBuildFinished) {
        use crate::project_scope::events::EmitToProject;
        let _ = self.0.emit_project(build::DEVCONTAINER_BUILD_FINISHED, payload);
    }
}

/// DVC-FR-OXCZ: start a local build of the selected config. Every refusal
/// happens before anything starts, and the project's build slot is claimed
/// last.
#[project_scope_macros::project_scoped]
#[project_scope_macros::blocking_command]
pub fn build_devcontainer(
    no_cache: bool,
    app: tauri::AppHandle,
    project: crate::project_scope::Scoped<'_, ProjectState>,
    store: State<'_, GlobalSettingsStore>,
    registry: State<'_, DevcontainerBuildRegistry>,
) -> Result<StartedDevcontainerBuild, String> {
    let root = project.require_root()?;
    let refuse = |code: &str| {
        logging::log_warn(
            &app,
            &BUFFER,
            &[Domain::Backend],
            "dev container build refused",
            log_fields! { "reason" => code },
        );
        code.to_string()
    };
    let prepared =
        super::operations::prepare_build(&root, &store, &crate::agentic::RealFileProbe, no_cache)
            .map_err(&refuse)?;
    static NEXT_BUILD: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = NEXT_BUILD.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let operation_id = format!("devcontainer-build-{sequence}");
    let cancel = registry.claim(root.path(), &operation_id).map_err(|code| refuse(&code))?;
    logging::log_info(
        &app,
        &BUFFER,
        &[Domain::Backend],
        "dev container build started",
        log_fields! {
            "operation_id" => operation_id.as_str(),
            "config" => prepared.relative.as_str(),
            "no_cache" => no_cache
        },
    );
    let started = StartedDevcontainerBuild { operation_id: operation_id.clone() };
    crate::project_scope::context::spawn(async move {
        use tauri::Manager;
        let registry = app.state::<DevcontainerBuildRegistry>();
        let events = AppBuildEvents(app.clone());
        super::operations::run_claimed_build(
            &app,
            &root,
            prepared,
            &operation_id,
            &cancel,
            &events,
            &registry,
        )
        .await;
    });
    Ok(started)
}

#[project_scope_macros::project_scoped]
#[project_scope_macros::blocking_command]
pub fn cancel_devcontainer_build(
    operation_id: String,
    registry: State<'_, DevcontainerBuildRegistry>,
) -> Result<(), String> {
    registry.cancel(&operation_id);
    Ok(())
}

#[project_scope_macros::project_scoped]
#[project_scope_macros::blocking_command]
pub fn load_devcontainer_build_in_flight(
    project: crate::project_scope::Scoped<'_, ProjectState>,
    registry: State<'_, DevcontainerBuildRegistry>,
) -> Result<Option<build::DevcontainerBuildInFlight>, String> {
    match project.require_root() {
        Ok(root) => Ok(registry.in_flight(root.path())),
        Err(_) => Ok(None),
    }
}

#[project_scope_macros::project_scoped]
#[project_scope_macros::blocking_command]
pub fn load_devcontainer_cli(
    store: State<'_, GlobalSettingsStore>,
) -> Result<DevcontainerCli, String> {
    Ok(store.load_devcontainer_cli()?.view())
}

#[project_scope_macros::project_scoped]
#[project_scope_macros::blocking_command]
pub fn save_devcontainer_cli(
    path: String,
    store: State<'_, GlobalSettingsStore>,
) -> Result<DevcontainerCli, String> {
    let record = store.load_devcontainer_cli()?.with_path(&path);
    store.save_devcontainer_cli(&record)?;
    Ok(record.view())
}

/// DVC-FR-BAID: run `--version` on the stored path, or on the detected one
/// where the stored path is empty, and store the result.
#[project_scope_macros::project_scoped]
#[tauri::command]
pub async fn verify_devcontainer_cli(
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
) -> Result<DevcontainerCli, DevcontainerCliFailure> {
    super::operations::verify_and_store(&app, &store, detected_path(), cli::VERIFY_TIMEOUT).await
}

fn detected_path() -> Option<String> {
    let path_var = std::env::var("PATH").ok();
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    cli::detect(path_var.as_deref(), home.as_deref(), &crate::agentic::RealFileProbe)
}

#[project_scope_macros::project_scoped]
#[project_scope_macros::blocking_command]
pub fn detect_devcontainer_cli() -> Result<DetectedDevcontainerCli, String> {
    Ok(DetectedDevcontainerCli { path: detected_path() })
}
