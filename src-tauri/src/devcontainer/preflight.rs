//! The start preflight of a project whose agent environment is a dev container
//! (DVC-FR-HINW, DVC-FR-ETQN, GSU-FR-GLVQ).
//!
//! It reads configuration and stored verification state alone: it runs no
//! process and contacts no daemon (GSU-FR-HIPF).

use super::{agent_cli_state, cli, config, AgentCliState};
use crate::agentic::FileProbe;
use crate::project_settings::agent_environment::load_agent_environment_from;

/// DVC-FR-ETQN: the config is not set, or names no file.
pub const ERR_DEVCONTAINER_CONFIG_MISSING: &str = "devcontainer_config_missing";
/// DVC-FR-ETQN: the config is not JSON with comments with an object at the top.
pub const ERR_DEVCONTAINER_CONFIG_INVALID: &str = "devcontainer_config_invalid";
/// DVC-FR-ETQN: the resolved vendor's CLI is not in the dev container.
pub const ERR_DEVCONTAINER_AGENT_CLI_MISSING: &str = "devcontainer_agent_cli_missing";
/// DVC-FR-ETQN: the vendor cannot run.
pub const ERR_VENDOR_EXECUTION_UNSUPPORTED: &str = "vendor_execution_unsupported";

/// DVC-FR-HINW: `None` where the project's agent environment is the vendor
/// images, so the checks of GSU-FR-GLVQ apply. Otherwise the result of the dev
/// container checks, in the order of DVC-FR-ETQN.
pub fn check(
    root: &crate::fs::RootFs,
    store: &crate::global_settings::GlobalSettingsStore,
    vendor: &str,
) -> Option<Result<(), String>> {
    let environment = load_agent_environment_from(root);
    if environment.source
        != crate::project_settings::agent_environment::AgentEnvironmentSource::Devcontainer
    {
        return None;
    }
    let record = store.load_devcontainer_cli().unwrap_or_default();
    let backend = store.load_docker_backend().unwrap_or_default();
    Some(check_with(
        root,
        environment.devcontainer_config.as_deref(),
        vendor,
        &record,
        &backend,
        &crate::agentic::RealFileProbe,
    ))
}

/// DVC-FR-HINW / DVC-FR-ETQN: the checks, with every input named.
pub fn check_with(
    root: &crate::fs::RootFs,
    config_path: Option<&str>,
    vendor: &str,
    record: &cli::DevcontainerCliRecord,
    backend: &crate::docker::DockerBackendRecord,
    probe: &dyn FileProbe,
) -> Result<(), String> {
    if !config::RUNNABLE_VENDORS.contains(&vendor) {
        return Err(ERR_VENDOR_EXECUTION_UNSUPPORTED.to_string());
    }
    let relative = config_path.ok_or_else(|| ERR_DEVCONTAINER_CONFIG_MISSING.to_string())?;
    let path = config::validate_config_path(root, relative)
        .map_err(|_| ERR_DEVCONTAINER_CONFIG_MISSING.to_string())?;
    let text = root.read_text(&path).map_err(|_| ERR_DEVCONTAINER_CONFIG_MISSING.to_string())?;
    config::parse_config(&text).map_err(|_| ERR_DEVCONTAINER_CONFIG_INVALID.to_string())?;
    cli::resolve_devcontainer_cli(record).map_err(str::to_string)?;
    cli::resolve_docker_cli(backend, probe).map_err(str::to_string)?;
    if agent_cli_state(root, relative, vendor) == AgentCliState::Missing {
        return Err(ERR_DEVCONTAINER_AGENT_CLI_MISSING.to_string());
    }
    Ok(())
}
