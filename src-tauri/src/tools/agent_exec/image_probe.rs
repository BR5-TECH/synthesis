//! EAC-FR-WYYZ: `probe_agent_image`, the one other entry point of the
//! executor. It runs one fixed probe command in one container of the image of
//! EAC-FR-38, under the runtime and the launch conditions of EAC-FR-13 and
//! EAC-FR-14, and removes the container. It mounts no execution directory, no
//! credential, no session state, and no supplementary mount, and it gives the
//! container no network.
//!
//! The image probe of `../core/OFP-openspec-failure-policy.md` OFP-FR-BZDP is
//! its one caller. The probe is not a turn: it starts no agent CLI, resolves
//! no credential, and emits no agent activity (OFP-FR-JBSO).

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use super::descriptor::{self, ContainerSpec, HELPER_NETWORK};
use super::runtime::{CancellationToken, RunEnd, RunRequest, RuntimeError};
use super::{AgentCliExecutor, AgentExecutionError, LaunchContext};
use crate::agentic::{self, AgenticInvocation};
use crate::log_fields;
use crate::logging::{self, Domain, LogSink, BUFFER};

/// EAC-FR-WYYZ: the command is stopped after 30 seconds.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(30);
/// EAC-FR-WYYZ: the standard output is kept up to 4 KiB.
pub const PROBE_OUTPUT_LIMIT: usize = 4 * 1024;
/// How much standard error the runtime captures before the last 4 KiB are
/// kept (EAC-FR-WYYZ).
const PROBE_STDERR_CAPTURE: usize = 64 * 1024;

/// EAC-FR-WYYZ: the probe commands. `openspec --version` is the one; a caller
/// cannot name a program or an argument of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeCommand {
    OpenSpecVersion,
}

impl ProbeCommand {
    /// The program that replaces the image entrypoint.
    pub fn entrypoint(self) -> &'static str {
        match self {
            ProbeCommand::OpenSpecVersion => "openspec",
        }
    }

    pub fn args(self) -> &'static [&'static str] {
        match self {
            ProbeCommand::OpenSpecVersion => &["--version"],
        }
    }
}

/// EAC contract surface `ImageProbeRequest`.
#[derive(Clone, Debug)]
pub struct ImageProbeRequest {
    pub command: ProbeCommand,
    pub cancellation: CancellationToken,
}

/// What `probe_agent_image` answers: the probe ran, or the isolation of the
/// run cannot run it (OFP-FR-RKMB).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageProbeOutcome {
    Ran(ImageProbe),
    /// OFP-FR-RKMB: the effective isolation is Microsandbox. A sandbox process
    /// runs the image entry point, and the probe must replace it, so no probe
    /// runs and no Docker runtime is reached.
    Skipped,
}

/// EAC contract surface `ImageProbe`: how the probe ended, its exit status,
/// its standard output up to 4 KiB, and the last 4 KiB of its standard error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageProbe {
    pub end: RunEnd,
    pub exit_status: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// EAC-FR-WYYZ: the probe container. The image, the host user, no mount, no
/// variable, no network, and the probe program as the entrypoint. The
/// security options are those every container of the descriptor gets
/// (EAC-FR-13, EAC-FR-14).
pub(super) fn probe_spec(image: &str, command: ProbeCommand) -> ContainerSpec {
    let (host_uid, host_gid) = descriptor::host_ids();
    ContainerSpec {
        name: descriptor::unique_container_name(),
        image: image.to_string(),
        host_uid,
        host_gid,
        workdir: "/".to_string(),
        mounts: Vec::new(),
        groups: Vec::new(),
        extra_hosts: Vec::new(),
        env_names: Vec::new(),
        env_literals: Vec::new(),
        vendor_args: command.args().iter().map(|arg| (*arg).to_string()).collect(),
        entrypoint: Some(command.entrypoint().to_string()),
        network: HELPER_NETWORK,
    }
}

impl AgentCliExecutor {
    /// EAC-FR-WYYZ: run the probe command in one container of the project's
    /// image for the resolved vendor, and remove the container.
    ///
    /// OFP-FR-RKMB: where the effective isolation is Microsandbox, the probe
    /// does not run and does not go to Docker in its place: it answers
    /// [`ImageProbeOutcome::Skipped`].
    pub async fn probe_agent_image<S>(
        &self,
        sink: &S,
        context: &LaunchContext<'_>,
        request: ImageProbeRequest,
    ) -> Result<ImageProbeOutcome, AgentExecutionError>
    where
        S: LogSink + Clone + Send + Sync + 'static,
    {
        let started = Instant::now();
        let result = match self.probe_isolation_is_sandbox(context) {
            Ok(true) => Ok(ImageProbeOutcome::Skipped),
            Ok(false) => self.probe(context, request).await,
            Err(_) => {
                logging::log_warn(
                    sink,
                    &BUFFER,
                    &[Domain::Backend],
                    "agent image probe could not read the isolation",
                    log_fields! { "reason" => "isolation_unreadable" },
                );
                Err(AgentExecutionError::RuntimeUnavailable)
            }
        };
        if let Err(error) = &result {
            logging::log_warn(
                sink,
                &BUFFER,
                &[Domain::Backend],
                "agent image probe refused before launch",
                log_fields! {
                    "reason" => error.kind(),
                    "duration_ms" => started.elapsed().as_millis() as u64,
                },
            );
        }
        result
    }

    async fn probe(
        &self,
        context: &LaunchContext<'_>,
        request: ImageProbeRequest,
    ) -> Result<ImageProbeOutcome, AgentExecutionError> {
        // EAC-FR-03, EAC-FR-38: the vendor and its image, as a launch finds
        // them. No credential is asked for.
        let invocation = agentic::resolve_agentic_invocation(
            context.store,
            context.integrations,
            context.project_key,
            None,
        )
        .map_err(AgentExecutionError::IntegrationUnresolved)?;
        let vendor = match invocation {
            AgenticInvocation::Cli { vendor, .. } => vendor,
            AgenticInvocation::Api { vendor, .. } => {
                return Err(AgentExecutionError::UnsupportedIntegration(vendor));
            }
        };
        descriptor::descriptor_for(&vendor)
            .ok_or_else(|| AgentExecutionError::UnsupportedIntegration(vendor.clone()))?;
        let image = context
            .images
            .image_for(&vendor)
            .map_err(AgentExecutionError::VendorImageUnresolved)?;

        let runtime = self.runtime_for(context.store)?;
        if runtime.ensure_available().await.is_err() {
            return Err(AgentExecutionError::RuntimeUnavailable);
        }
        runtime.ensure_image(&image).await.map_err(|error| match error {
            RuntimeError::ImageUnavailable => AgentExecutionError::ImageUnavailable(vendor.clone()),
            RuntimeError::Unavailable => AgentExecutionError::RuntimeUnavailable,
            RuntimeError::LaunchFailed(reason) => AgentExecutionError::LaunchFailed(reason),
        })?;

        let spec = probe_spec(&image, request.command);
        let argv = spec.docker_run_args();
        let env = BTreeMap::new();
        let outcome = runtime
            .run(RunRequest {
                argv: &argv,
                container: Some(&spec),
                env: &env,
                stdin: &[],
                timeout: PROBE_TIMEOUT,
                cancel: request.cancellation,
                stdout_limit: PROBE_OUTPUT_LIMIT,
                stderr_limit: PROBE_STDERR_CAPTURE,
                observer: None,
                final_event: None,
            })
            .await;
        // EAC-FR-26: the container goes on every path out.
        let _ = runtime.remove_container(&spec.name).await;
        let outcome = outcome.map_err(|error| match error {
            RuntimeError::Unavailable => AgentExecutionError::RuntimeUnavailable,
            RuntimeError::ImageUnavailable => AgentExecutionError::ImageUnavailable(vendor),
            RuntimeError::LaunchFailed(reason) => AgentExecutionError::LaunchFailed(reason),
        })?;
        let stderr = &outcome.stderr.bytes;
        Ok(ImageProbeOutcome::Ran(ImageProbe {
            end: outcome.end,
            exit_status: outcome.exit_code,
            stdout: outcome.stdout.bytes,
            stderr: stderr[stderr.len().saturating_sub(PROBE_OUTPUT_LIMIT)..].to_vec(),
        }))
    }

    /// OFP-FR-RKMB, EAC-FR-SNMS: whether a turn of this project runs in a
    /// micro VM. That is the effective isolation of a graduation launch: the
    /// author's isolation, with the host-socket choice of Docker-in-Docker
    /// giving up the VM.
    ///
    /// An isolation setting that cannot be read is an error of a setting, not
    /// a probe outcome (OFP-FR-XBNJ). The caller answers `RuntimeUnavailable`
    /// for it, and the dispatch then states the error on its own terms.
    fn probe_isolation_is_sandbox(&self, context: &LaunchContext<'_>) -> Result<bool, String> {
        let isolation = match &self.isolation {
            Some(isolation) => isolation.clone(),
            None => crate::isolation::resolve_isolation(context.store)?,
        };
        let dind = &context.docker_in_docker;
        Ok(matches!(
            crate::isolation::effective_isolation(isolation, dind.enabled, dind.under_microsandbox),
            crate::isolation::ResolvedIsolation::Microsandbox(_)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // EAC-FR-WYYZ: the probe container mounts nothing, carries no variable,
    // has no network, and runs `openspec --version` in place of the image
    // entrypoint.
    #[test]
    fn the_probe_container_mounts_nothing_and_runs_the_fixed_command() {
        let spec = probe_spec("example/agent:1", ProbeCommand::OpenSpecVersion);
        assert!(spec.mounts.is_empty());
        assert!(spec.env_names.is_empty());
        assert!(spec.env_literals.is_empty());
        assert!(spec.groups.is_empty());
        assert_eq!(spec.network, HELPER_NETWORK);
        assert_eq!(spec.entrypoint.as_deref(), Some("openspec"));
        assert_eq!(spec.vendor_args, ["--version"]);
        let argv = spec.docker_run_args();
        assert!(!argv.iter().any(|arg| arg == "--mount" || arg == "-v" || arg == "--volume"));
        assert!(argv.iter().any(|arg| arg == "example/agent:1"));
    }
}
