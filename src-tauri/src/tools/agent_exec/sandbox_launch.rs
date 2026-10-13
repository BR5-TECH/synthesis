//! The sandbox half of one launch (EAC-FR-SNMS through EAC-FR-PZNB).
//!
//! Split out of `launch.rs` for size alone. Everything here is a method of
//! [`AgentCliExecutor`] and reads the module's own private items through
//! `super`.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::sandbox::{
    self, ImageExporter, SandboxLaunch, SandboxLaunchError, SandboxMount, SandboxPool,
    SandboxProcess, SandboxSpec,
};
use super::*;
use crate::isolation::{ResolvedIsolation, SandboxSettings};

/// The program an invocation activity names for a sandbox launch (EAC-FR-34).
pub(super) const SANDBOX_PROGRAM: &str = "microsandbox";

/// What a sandbox launch is handed by the shared part of the launch.
pub(super) struct SandboxRun<'a> {
    pub settings: &'a SandboxSettings,
    pub scope: &'a sandbox::SandboxScope,
    pub image: &'a str,
    pub mounts: &'a [BindMount],
    pub container: &'a descriptor::ContainerSpec,
    pub env: &'a BTreeMap<String, SecretString>,
    pub stdin: &'a [u8],
    pub timeout: Duration,
    pub cancel: runtime::CancellationToken,
    pub observer: &'a dyn runtime::StreamObserver,
}

impl AgentCliExecutor {
    /// EAC-FR-SNMS / EAC-FR-QYBJ: the sandbox settings this launch runs under,
    /// or `None` for a Docker launch.
    ///
    /// Only a launch that carries a scope is a candidate, and a launch that
    /// carries the semantic-rebase mount or is a `semantic_rebase` turn never
    /// is: its masking is a Docker launch rule (EAC-FR-41).
    pub(super) fn sandbox_settings(
        &self,
        context: &LaunchContext<'_>,
        request: &AgentExecutionRequest,
    ) -> Result<Option<SandboxSettings>, AgentExecutionError> {
        if request.sandbox_scope.is_none()
            || request.supplementary_mount.is_some()
            || request.turn_kind == TurnKind::SemanticRebase
        {
            return Ok(None);
        }
        let isolation = match &self.isolation {
            Some(isolation) => isolation.clone(),
            None => crate::isolation::resolve_isolation(context.store)
                .map_err(AgentExecutionError::LaunchFailed)?,
        };
        Ok(match isolation {
            ResolvedIsolation::None => None,
            ResolvedIsolation::Microsandbox(settings) => Some(settings),
        })
    }

    /// EAC-FR-SNMS: the invocation a sandbox launch reports (EAC-FR-34).
    pub(super) fn sandbox_invocation(
        vm: &str,
        container: &descriptor::ContainerSpec,
    ) -> Vec<String> {
        let mut argv = vec!["exec".to_string(), vm.to_string(), "--".to_string()];
        argv.extend(container.vendor_args.iter().cloned());
        argv
    }

    /// EAC-FR-QULL: the verified Docker backend the export fallback reads, or
    /// `None` where there is none.
    fn image_exporter(&self, store: &GlobalSettingsStore) -> Option<Arc<dyn ImageExporter>> {
        if self.sandbox_pool.is_some() {
            // A test binds its own pool and reaches no Docker backend.
            return None;
        }
        match crate::docker::resolve_docker_backend(store).ok()? {
            crate::docker::ResolvedDockerBackend::Cli { path } => {
                Some(Arc::new(runtime::HostDockerCli::with_program(path)))
            }
            crate::docker::ResolvedDockerBackend::Engine { endpoint } => {
                Some(Arc::new(runtime::BollardDocker::new(endpoint)))
            }
        }
    }

    fn pool(&self) -> &SandboxPool {
        match &self.sandbox_pool {
            Some(pool) => pool,
            None => sandbox::production_pool(),
        }
    }

    /// EAC-FR-IGBY: where exports are kept. A test roots it under its own
    /// directory, as it roots session state there.
    fn export_root(&self) -> Result<PathBuf, AgentExecutionError> {
        match &self.session_state_root {
            Some(root) => Ok(root.join(sandbox::EXPORT_CACHE_DIRNAME)),
            None => crate::fs::app_data_dir()
                .map(|dir| dir.join(sandbox::EXPORT_CACHE_DIRNAME))
                .map_err(|_| {
                    AgentExecutionError::SandboxLaunchFailed(sandbox::SandboxFailure {
                        cause: sandbox::SandboxFailureCause::ImageExportFailed,
                        message: "no application data directory".to_string(),
                    })
                }),
        }
    }

    /// EAC-FR-UEFI / EAC-FR-ZIYG / EAC-FR-PZNB: run the turn as one process in
    /// the scope's VM. The VM outlives the call.
    pub(super) async fn run_in_sandbox<S>(
        &self,
        sink: &S,
        context: &LaunchContext<'_>,
        vm: &str,
        run: SandboxRun<'_>,
    ) -> Result<runtime::RuntimeOutcome, AgentExecutionError>
    where
        S: LogSink + Clone + Send + Sync + 'static,
    {
        let mounts: Vec<SandboxMount> = run
            .mounts
            .iter()
            .map(|mount| SandboxMount {
                source: mount.source.clone(),
                target: mount.target.clone(),
                read_only: mount.read_only,
            })
            .collect();
        let spec = SandboxSpec::new(run.image.to_string(), run.settings, mounts);
        // EAC-FR-15: every value on this process alone. The literals are paths
        // and names this application chose, never a secret.
        let mut env: Vec<(String, SecretString)> = run
            .env
            .iter()
            .map(|(name, value)| (name.clone(), SecretString::new(value.expose().to_string())))
            .collect();
        for (name, value) in &run.container.env_literals {
            env.push((name.clone(), SecretString::new(value.clone())));
        }
        let launch = SandboxLaunch {
            vm,
            spec: &spec,
            settings: run.settings,
            exporter: self.image_exporter(context.store),
            fs: context.fs,
            export_root: self.export_root()?,
        };
        let process = SandboxProcess {
            vendor_args: &run.container.vendor_args,
            workdir: &run.container.workdir,
            user: run.container.user(),
            env,
            stdin: run.stdin,
            timeout: run.timeout,
            cancel: run.cancel,
            stdout_limit: protocol::LIMIT_STDOUT,
            stderr_limit: protocol::LIMIT_STDERR,
            observer: Some(run.observer),
        };
        logging::log_info(
            sink,
            &BUFFER,
            &[Domain::Ai, Domain::Backend],
            "agent execution runs in a sandbox VM",
            log_fields! {
                "vm" => vm,
                // The queue key names a stream id or a worktree; its digest is
                // the VM name above, so the key itself is not logged.
                "scope_kind" => if run.scope.queue_key.starts_with("worktree:") {
                    "worktree"
                } else {
                    "stream"
                },
                "cpus" => spec.cpus,
                "memory_gib" => spec.memory_gib,
                "mounts" => spec.mounts.len(),
                "block_local_network" => spec.network.block_local_network,
                "allowed_hosts" => spec.network.allowed.len(),
            },
        );
        self.pool().launch(sink, launch, process).await.map_err(|error| match error {
            SandboxLaunchError::Sandbox(failure) => {
                AgentExecutionError::SandboxLaunchFailed(failure)
            }
            SandboxLaunchError::Runtime(RuntimeError::Unavailable) => {
                AgentExecutionError::RuntimeUnavailable
            }
            SandboxLaunchError::Runtime(RuntimeError::ImageUnavailable) => {
                AgentExecutionError::SandboxLaunchFailed(sandbox::SandboxFailure {
                    cause: sandbox::SandboxFailureCause::ImageUnavailable,
                    message: "the image is not available".to_string(),
                })
            }
            SandboxLaunchError::Runtime(RuntimeError::LaunchFailed(reason)) => {
                AgentExecutionError::LaunchFailed(reason)
            }
        })
    }
}
