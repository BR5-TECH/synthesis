//! The dev container environment of a project (`DVC-devcontainer-environment.md`).
//!
//! A project can name a `devcontainer.json` as the environment its agentic CLI
//! work runs in. This module owns the state the Settings window shows and
//! changes: the agent environment (stored project-local by
//! `project_settings::agent_environment`), the configs of the project, the
//! agent CLI state, the build, and the dev container CLI of this machine. The
//! start of a container and the turns in it are the executor's
//! (`tools::agent_exec::devcontainer_launch`).

pub mod build;
pub mod cli;
pub mod commands;
pub mod config;
pub mod operations;
pub mod preflight;
pub mod process;

use serde::Serialize;

use crate::project_settings::agent_environment::{
    load_agent_environment_from, load_devcontainer_probe_from, write_agent_environment_to,
    AgentEnvironment, AgentEnvironmentSource, ProbeFinding,
};

/// DVC-FR-BCDZ: where the starter config is written.
pub const STARTER_CONFIG_PATH: &str = ".devcontainer/devcontainer.json";

/// DVC-FR-WZSY: what the application knows of a vendor CLI in the dev
/// container.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentCliState {
    Found,
    Missing,
    Unknown,
}

/// DVC-FR-WZSY: one row of the agent CLI state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCliRow {
    pub vendor: String,
    pub state: AgentCliState,
    pub known_feature: Option<String>,
}

/// The state "load agent environment" returns.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentEnvironmentState {
    pub environment: AgentEnvironment,
    pub config_exists: bool,
    pub agent_cli: Vec<AgentCliRow>,
}

/// DVC-FR-WXYK / DVC-FR-JVKA / DVC-FR-BCDZ: why a change of the agent
/// environment is refused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum AgentEnvironmentError {
    ConfigPathInvalid {
        reason: config::ConfigPathProblem,
    },
    ConfigMissing,
    ConfigInvalid {
        message: String,
        line: Option<usize>,
    },
    ConfigExists,
    NoKnownFeature,
    /// The project store could not be read or written.
    StoreFailed {
        message: String,
    },
}

impl From<config::ConfigInvalid> for AgentEnvironmentError {
    fn from(invalid: config::ConfigInvalid) -> Self {
        AgentEnvironmentError::ConfigInvalid { message: invalid.message, line: invalid.line }
    }
}

fn store_failed(message: impl ToString) -> AgentEnvironmentError {
    AgentEnvironmentError::StoreFailed { message: message.to_string() }
}

/// DVC-FR-WZSY / DVC-FR-PFKJ: the agent CLI state of `vendor` for the config at
/// `relative`. A probe counts only for the config it probed, at the hash it had
/// then.
pub fn agent_cli_state(root: &crate::fs::RootFs, relative: &str, vendor: &str) -> AgentCliState {
    let Ok(path) = config::validate_config_path(root, relative) else {
        return AgentCliState::Unknown;
    };
    if let Some(probe) = load_devcontainer_probe_from(root) {
        let current = config::config_hash(root, &path).ok();
        if probe.config == relative && current.as_deref() == Some(probe.config_hash.as_str()) {
            match probe.finding(vendor) {
                Some(ProbeFinding::Found) => return AgentCliState::Found,
                Some(ProbeFinding::Missing) => return AgentCliState::Missing,
                None => {}
            }
        }
    }
    let holds = root
        .read_text(&path)
        .ok()
        .and_then(|text| config::parse_config(&text).ok())
        .is_some_and(|parsed| config::holds_known_feature(&parsed, vendor));
    if holds {
        AgentCliState::Found
    } else {
        AgentCliState::Unknown
    }
}

/// DVC-FR-WZSY: the state the Settings window renders.
pub fn state_of(root: &crate::fs::RootFs) -> AgentEnvironmentState {
    let environment = load_agent_environment_from(root);
    let selected = environment.devcontainer_config.clone();
    let config_exists = selected
        .as_deref()
        .is_some_and(|relative| config::validate_config_path(root, relative).is_ok());
    let agent_cli = config::RUNNABLE_VENDORS
        .iter()
        .map(|vendor| AgentCliRow {
            vendor: vendor.to_string(),
            state: match selected.as_deref() {
                Some(relative) => agent_cli_state(root, relative, vendor),
                None => AgentCliState::Unknown,
            },
            known_feature: config::known_feature(vendor).map(str::to_string),
        })
        .collect();
    AgentEnvironmentState { environment, config_exists, agent_cli }
}

/// DVC-FR-WXYK: validate and store the agent environment.
pub fn save(
    root: &crate::fs::RootFs,
    environment: AgentEnvironment,
) -> Result<AgentEnvironmentState, AgentEnvironmentError> {
    let config = environment
        .devcontainer_config
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(str::to_string);
    if let Some(relative) = &config {
        config::validate_config_path(root, relative)
            .map_err(|reason| AgentEnvironmentError::ConfigPathInvalid { reason })?;
    }
    if environment.source == AgentEnvironmentSource::Devcontainer && config.is_none() {
        return Err(AgentEnvironmentError::ConfigMissing);
    }
    let record = AgentEnvironment { source: environment.source, devcontainer_config: config };
    write_agent_environment_to(root, &record).map_err(store_failed)?;
    Ok(state_of(root))
}

/// DVC-FR-BCDZ: write the starter config, then select it.
pub fn create_config(
    root: &crate::fs::RootFs,
) -> Result<AgentEnvironmentState, AgentEnvironmentError> {
    let path = root.path().join(STARTER_CONFIG_PATH);
    if root.file_info(&path).is_ok() {
        return Err(AgentEnvironmentError::ConfigExists);
    }
    let name = root
        .path()
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".to_string());
    root.write_text_atomic(&path, &config::starter_config(&name)).map_err(store_failed)?;
    save(
        root,
        AgentEnvironment {
            source: AgentEnvironmentSource::Devcontainer,
            devcontainer_config: Some(STARTER_CONFIG_PATH.to_string()),
        },
    )
}

/// DVC-FR-ABQZ / DVC-FR-JVKA: add the known Feature of `vendor` to the selected
/// config.
pub fn add_feature(
    root: &crate::fs::RootFs,
    vendor: &str,
) -> Result<AgentEnvironmentState, AgentEnvironmentError> {
    if config::known_feature(vendor).is_none() {
        return Err(AgentEnvironmentError::NoKnownFeature);
    }
    let environment = load_agent_environment_from(root);
    let relative = environment.devcontainer_config.ok_or(AgentEnvironmentError::ConfigMissing)?;
    let path = config::validate_config_path(root, &relative)
        .map_err(|reason| AgentEnvironmentError::ConfigPathInvalid { reason })?;
    let text = root.read_text(&path).map_err(store_failed)?;
    match config::add_agent_feature(&text, vendor) {
        Ok(Some(changed)) => root.write_text_atomic(&path, &changed).map_err(store_failed)?,
        Ok(None) => {}
        Err(config::AddFeatureError::NoKnownFeature) => {
            return Err(AgentEnvironmentError::NoKnownFeature);
        }
        Err(config::AddFeatureError::Invalid(invalid)) => return Err(invalid.into()),
    }
    Ok(state_of(root))
}

#[cfg(test)]
mod tests;
