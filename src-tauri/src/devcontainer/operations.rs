//! The work of the build and the CLI verification commands, with every input
//! named, so it is tested without a Tauri application (DVC-FR-OXCZ,
//! DVC-FR-RTBA, DVC-FR-EKAO, DVC-FR-PFKJ, DVC-FR-BAID).

use std::time::Duration;

use super::build::{self, BuildEvents, BuildOutcome, DevcontainerBuildRegistry};
use super::cli::{self, DevcontainerCli, DevcontainerCliFailure, DevcontainerCliVerification};
use super::config;
use crate::agentic::FileProbe;
use crate::global_settings::GlobalSettingsStore;
use crate::log_fields;
use crate::logging::{self, Domain, LogSink, BUFFER};
use crate::project_settings::agent_environment::{
    load_agent_environment_from, write_devcontainer_probe_to, DevcontainerProbe,
};
use crate::tools::agent_exec::runtime::CancellationToken;

/// DVC-FR-OXCZ: a build that every check before its start allowed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedBuild {
    pub spec: build::BuildSpec,
    pub relative: String,
    /// DVC-FR-PFKJ: the hash at the start of the build.
    pub config_hash: String,
}

/// DVC-FR-OXCZ: check what a build needs, in the order of the refusals, and
/// read the hash of the config at the start (DVC-FR-PFKJ). It runs nothing.
pub fn prepare_build(
    root: &crate::fs::RootFs,
    store: &GlobalSettingsStore,
    probe: &dyn FileProbe,
    no_cache: bool,
) -> Result<PreparedBuild, &'static str> {
    let missing = super::preflight::ERR_DEVCONTAINER_CONFIG_MISSING;
    let relative = load_agent_environment_from(root).devcontainer_config.ok_or(missing)?;
    let config_path = config::validate_config_path(root, &relative).map_err(|_| missing)?;
    let record = store.load_devcontainer_cli().unwrap_or_default();
    let cli_path = cli::resolve_devcontainer_cli(&record)?;
    let docker = cli::resolve_docker_cli(&store.load_docker_backend().unwrap_or_default(), probe)?;
    let config_hash = config::config_hash(root, &config_path).map_err(|_| missing)?;
    Ok(PreparedBuild {
        spec: build::BuildSpec {
            cli: cli_path,
            docker,
            workspace: root.path().to_path_buf(),
            config: config_path,
            no_cache,
            image_name: build::image_name_for(root.path()),
        },
        relative,
        config_hash,
    })
}

/// DVC-FR-RTBA / DVC-FR-EKAO: run a claimed build, probe a built image, free
/// the project, then emit the one result.
pub async fn run_claimed_build<S>(
    sink: &S,
    root: &crate::fs::RootFs,
    prepared: PreparedBuild,
    operation_id: &str,
    cancel: &CancellationToken,
    events: &dyn BuildEvents,
    registry: &DevcontainerBuildRegistry,
) where
    S: LogSink + Clone + Send + Sync + 'static,
{
    let begun = std::time::Instant::now();
    let finished =
        build::run_build(&prepared.spec, root.path(), operation_id, cancel, events, registry).await;
    if let (BuildOutcome::Succeeded, Some(image)) = (finished.outcome, finished.image.clone()) {
        let docker = prepared.spec.docker.clone();
        probe_and_store(sink, root, &prepared.relative, prepared.config_hash, &docker, &image)
            .await;
    }
    registry.release(root.path());
    logging::log_info(
        sink,
        &BUFFER,
        &[Domain::Backend],
        "dev container build finished",
        log_fields! {
            "operation_id" => operation_id,
            "outcome" => format!("{:?}", finished.outcome),
            "duration_ms" => begun.elapsed().as_millis() as u64
        },
    );
    events.finished(finished);
}

/// DVC-FR-EKAO: probe the image for each vendor that can run, and store what
/// the probes found with the hash of the config at the build start. A probe
/// that cannot run stores nothing. Returns what was stored.
pub async fn probe_and_store<S>(
    sink: &S,
    root: &crate::fs::RootFs,
    relative: &str,
    config_hash: String,
    docker: &str,
    image: &str,
) -> Option<DevcontainerProbe>
where
    S: LogSink + Clone + Send + Sync + 'static,
{
    let mut probe =
        DevcontainerProbe { config: relative.to_string(), config_hash, ..Default::default() };
    for vendor in config::RUNNABLE_VENDORS {
        let Some(cli_name) = config::cli_name(vendor) else { continue };
        let finding = build::probe_image(docker, image, cli_name).await;
        logging::log_info(
            sink,
            &BUFFER,
            &[Domain::Backend],
            "dev container image probed",
            log_fields! { "vendor" => *vendor, "finding" => format!("{finding:?}") },
        );
        let finding = finding?;
        match *vendor {
            "claude_code" => probe.claude_code = Some(finding),
            "codex" => probe.codex = Some(finding),
            _ => {}
        }
    }
    match write_devcontainer_probe_to(root, &probe) {
        Ok(()) => Some(probe),
        Err(error) => {
            logging::log_warn(
                sink,
                &BUFFER,
                &[Domain::Backend],
                "dev container probe not stored",
                log_fields! { "error" => error },
            );
            None
        }
    }
}

/// DVC-FR-BAID: verify the stored path, or `detected` where the stored path is
/// empty, and store `verified` with the version or `unverified`. A path saved
/// during the check is the author's latest word, so the result of the older
/// path is not stored.
pub async fn verify_and_store<S>(
    sink: &S,
    store: &GlobalSettingsStore,
    detected: Option<String>,
    timeout: Duration,
) -> Result<DevcontainerCli, DevcontainerCliFailure>
where
    S: LogSink + Clone + Send + Sync + 'static,
{
    let failure = |message: String| DevcontainerCliFailure { code: "failed", message };
    let record = store.load_devcontainer_cli().map_err(failure)?;
    let path = if record.path.is_empty() { detected } else { Some(record.path.clone()) };
    let unverified = cli::DevcontainerCliRecord { verified: None, ..record.clone() };
    let Some(path) = path else {
        store.save_devcontainer_cli(&unverified).map_err(failure)?;
        return Err(DevcontainerCliFailure { code: "not_found", message: String::new() });
    };
    let verified = cli::verify(&path, timeout).await;
    let outcome = match &verified {
        Ok(_) => "verified",
        Err(failure) => failure.code,
    };
    logging::log_info(
        sink,
        &BUFFER,
        &[Domain::Backend],
        "dev container CLI verification",
        log_fields! { "path" => path.as_str(), "outcome" => outcome },
    );
    let current = store.load_devcontainer_cli().map_err(failure)?;
    if current.path != record.path {
        return verified.map(|_| current.view());
    }
    match verified {
        Ok(version) => {
            let updated = cli::DevcontainerCliRecord {
                verified: Some(DevcontainerCliVerification {
                    path,
                    version,
                    verified_at: crate::notes::now_rfc3339(),
                }),
                ..record
            };
            store.save_devcontainer_cli(&updated).map_err(failure)?;
            Ok(updated.view())
        }
        Err(error) => {
            store.save_devcontainer_cli(&unverified).map_err(failure)?;
            Err(error)
        }
    }
}

#[cfg(test)]
#[path = "tests/operations.rs"]
mod tests;
