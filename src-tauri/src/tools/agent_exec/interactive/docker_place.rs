//! EAC-FR-LIQF / EAC-FR-LQJC: an interactive launch in a Docker container,
//! through the verified Docker backend.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use super::super::descriptor;
use super::super::*;
use super::compose::{self, Composed};
use super::{on_end, CleanupOnce, InteractiveLaunchRequest, InteractiveProgram, Parts};
use crate::terminals::process::ProcessHandle;

/// The vendor whose image a Docker interactive launch runs: the vendor that
/// AIC resolves for a `work` turn for a shell, or the given vendor.
pub(super) fn image_vendor(
    store: &GlobalSettingsStore,
    integrations: &AgenticIntegrations,
    project_key: &str,
    program: &InteractiveProgram,
) -> Result<String, String> {
    match program {
        InteractiveProgram::AgentCli(vendor) => Ok(vendor.id().to_string()),
        InteractiveProgram::Shell => {
            let invocation = agentic::resolve_agentic_invocation(
                store,
                integrations,
                project_key,
                Some(TurnKind::Work.selection_key()),
            )?;
            match invocation {
                AgenticInvocation::Cli { vendor, .. } => Ok(vendor),
                AgenticInvocation::Api { .. } => {
                    Err("the project's agentic integration is not a CLI".to_string())
                }
            }
        }
    }
}

/// Removes the session-state directory of a launch, where it has one.
fn remove_session_dir(fs: &Arc<FsAccess>, composed: &Composed) -> impl FnOnce() + Send + 'static {
    let fs = fs.clone();
    let dir = composed.session_dir.clone();
    move || {
        if let Some(dir) = dir {
            let _ = fs.delete_path(&dir, true);
        }
    }
}

impl AgentCliExecutor {
    pub(super) async fn launch_docker_interactive(
        &self,
        parts: &Parts<'_>,
        fs: &Arc<FsAccess>,
        directory: &Path,
        request: &InteractiveLaunchRequest,
    ) -> Result<ProcessHandle, String> {
        let vendor =
            image_vendor(&parts.store, &parts.integrations, &parts.project_key, &request.program)?;
        let (docker_in_docker, images) = project_launch_inputs(parts.project_root.clone());
        let image = images.image_for(&vendor)?;
        let credential = match compose::takes_credential(&request.program) {
            Some(vendor) => Some(agentic::resolve_agent_launch_credential(
                &parts.store,
                &parts.integrations,
                vendor.id(),
            )?),
            None => None,
        };
        let session_root = super::session_root()?;
        let composed = self.compose_interactive(
            fs,
            directory,
            &request.program,
            compose::ComposeExtras {
                credential,
                session_root: Some(&session_root),
                terminal_id: &request.terminal_id,
                docker_in_docker: Some(&docker_in_docker),
            },
        )?;
        let name = descriptor::unique_container_name();
        let spec = compose::container_spec(name.clone(), image, &composed, &request.program);
        let backend = crate::docker::resolve_docker_backend(&parts.store)
            .map_err(|_| "the Docker backend is not verified".to_string())?;
        let cleanup = CleanupOnce::new(remove_session_dir(fs, &composed));
        let started = match backend {
            crate::docker::ResolvedDockerBackend::Cli { path } => {
                run_cli(&path, &spec, &composed.env, directory, request, cleanup.clone())
            }
            crate::docker::ResolvedDockerBackend::Engine { endpoint } => {
                engine::run(&endpoint, &spec, &composed.env, request.cols, request.rows).await
            }
        };
        // A launch that fails after it composed a session-state directory
        // removes that directory.
        let handle = started.inspect_err(|_| cleanup.run())?;
        Ok(on_end(handle, move |_| cleanup.run()))
    }
}

/// EAC-FR-LQJC: `docker run -i -t` on a host pseudo-terminal. A terminate
/// removes the container.
#[cfg(unix)]
fn run_cli(
    docker: &str,
    spec: &descriptor::ContainerSpec,
    env: &BTreeMap<String, SecretString>,
    directory: &Path,
    request: &InteractiveLaunchRequest,
    cleanup: Arc<CleanupOnce>,
) -> Result<ProcessHandle, String> {
    use crate::terminals::process::{spawn_host, terminal_env, HostCommand};
    // EAC-FR-15: a value goes to the environment of the client process, and
    // the vector names the variable alone.
    let mut client_env = terminal_env();
    for (name, value) in env {
        client_env.push((name.clone(), value.expose().to_string()));
    }
    let command = HostCommand {
        program: docker.to_string(),
        args: compose::interactive_run_args(spec),
        cwd: directory.to_string_lossy().into_owned(),
        env: client_env,
        env_remove: Vec::new(),
    };
    let docker = docker.to_string();
    let name = spec.name.clone();
    let on_terminate = Box::new(move || {
        remove_container_now(&docker, &name);
        cleanup.run();
    });
    spawn_host(&command, request.cols, request.rows, Some(spec.name.clone()), Some(on_terminate))
        .map_err(|e| format!("docker did not start: {}", e.kind()))
}

#[cfg(not(unix))]
fn run_cli(
    _docker: &str,
    _spec: &descriptor::ContainerSpec,
    _env: &BTreeMap<String, SecretString>,
    _directory: &Path,
    _request: &InteractiveLaunchRequest,
    _cleanup: Arc<CleanupOnce>,
) -> Result<ProcessHandle, String> {
    Err("platform_unsupported".to_string())
}

/// `docker rm -f <name>`, on a thread of its own so a terminate returns at
/// once. A container that is not there is not an error.
pub(super) fn remove_container_now(docker: &str, name: &str) {
    let docker = docker.to_string();
    let name = name.to_string();
    let removal = super::removals::begin();
    let _ = std::thread::Builder::new().name("terminal-rm".into()).spawn(move || {
        let _removal = removal;
        let _ = std::process::Command::new(docker)
            .args(["rm", "-f", &name])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    });
}

/// EAC-FR-LQJC: the Docker Engine backend, over Bollard.
mod engine {
    use super::*;

    /// Removes a container that was created and did not start.
    #[cfg(not(test))]
    async fn remove_created(docker: &bollard::Docker, name: &str) {
        let _removal = super::super::removals::begin();
        let options =
            bollard::query_parameters::RemoveContainerOptionsBuilder::default().force(true).build();
        let _ = docker.remove_container(name, Some(options)).await;
    }

    #[cfg(test)]
    pub(super) async fn run(
        _endpoint: &crate::docker::DockerEndpoint,
        _spec: &descriptor::ContainerSpec,
        _env: &BTreeMap<String, SecretString>,
        _cols: u16,
        _rows: u16,
    ) -> Result<ProcessHandle, String> {
        Err("no Docker Engine in tests".to_string())
    }

    #[cfg(not(test))]
    pub(super) async fn run(
        endpoint: &crate::docker::DockerEndpoint,
        spec: &descriptor::ContainerSpec,
        env: &BTreeMap<String, SecretString>,
        cols: u16,
        rows: u16,
    ) -> Result<ProcessHandle, String> {
        use bollard::query_parameters::{
            AttachContainerOptionsBuilder, CreateContainerOptionsBuilder,
            RemoveContainerOptionsBuilder, ResizeContainerTTYOptionsBuilder,
        };
        use futures_util::StreamExt;
        use tokio::io::AsyncWriteExt;
        use tokio::sync::{mpsc, oneshot};

        use crate::terminals::process::{ProcessControl, OUTPUT_QUEUE_CHUNKS};

        let failed = |e: bollard::errors::Error| format!("the Docker Engine refused: {e}");
        let docker = crate::docker::connect(
            endpoint,
            crate::docker::connect_bound(std::time::Duration::from_secs(30)),
        )?;
        let mut config = super::super::super::runtime::engine_config(
            spec,
            super::super::super::runtime::engine_env(spec, env),
        );
        // EAC-FR-LQJC: a terminal and an open stdin, in place of the closed
        // stdin of a turn.
        config.tty = Some(true);
        config.stdin_once = Some(false);
        let create = CreateContainerOptionsBuilder::default().name(&spec.name).build();
        docker.create_container(Some(create), config).await.map_err(failed)?;
        let attach = AttachContainerOptionsBuilder::default()
            .stream(true)
            .stdin(true)
            .stdout(true)
            .stderr(true)
            .build();
        let attached = match docker.attach_container(&spec.name, Some(attach)).await {
            Ok(attached) => attached,
            Err(e) => {
                remove_created(&docker, &spec.name).await;
                return Err(failed(e));
            }
        };
        if let Err(e) = docker
            .start_container(&spec.name, None::<bollard::query_parameters::StartContainerOptions>)
            .await
        {
            // A container that never started is not removed by `auto_remove`.
            remove_created(&docker, &spec.name).await;
            return Err(failed(e));
        }
        // The wait is registered before any output is read, so the exit code
        // is known before `auto_remove` takes the container away.
        let wait_docker = docker.clone();
        let wait_name = spec.name.clone();
        let wait = tokio::spawn(async move {
            let mut wait = wait_docker.wait_container(
                &wait_name,
                None::<bollard::query_parameters::WaitContainerOptions>,
            );
            match wait.next().await {
                Some(Ok(response)) => Some(response.status_code as i32),
                _ => None,
            }
        });
        let resize = |cols: u16, rows: u16| {
            ResizeContainerTTYOptionsBuilder::default()
                .h(i32::from(rows))
                .w(i32::from(cols))
                .build()
        };
        let _ = docker.resize_container_tty(&spec.name, resize(cols, rows)).await;

        enum Request {
            Input(Vec<u8>),
            Resize(u16, u16),
            Remove(super::super::removals::Removal),
        }
        struct EngineControl(mpsc::UnboundedSender<Request>);
        impl ProcessControl for EngineControl {
            fn write(&self, data: &[u8]) -> std::io::Result<()> {
                self.0
                    .send(Request::Input(data.to_vec()))
                    .map_err(|_| std::io::ErrorKind::BrokenPipe.into())
            }
            fn resize(&self, cols: u16, rows: u16) -> std::io::Result<()> {
                self.0
                    .send(Request::Resize(cols, rows))
                    .map_err(|_| std::io::ErrorKind::BrokenPipe.into())
            }
            fn terminate(&self) {
                // Counted from the terminate, so an exit that waits sees it.
                let _ = self.0.send(Request::Remove(super::super::removals::begin()));
            }
        }

        let (requests_tx, mut requests_rx) = mpsc::unbounded_channel::<Request>();
        let mut input = attached.input;
        let control_docker = docker.clone();
        let name = spec.name.clone();
        tokio::spawn(async move {
            while let Some(request) = requests_rx.recv().await {
                match request {
                    Request::Input(data) => {
                        let _ = input.write_all(&data).await;
                        let _ = input.flush().await;
                    }
                    Request::Resize(cols, rows) => {
                        let _ =
                            control_docker.resize_container_tty(&name, resize(cols, rows)).await;
                    }
                    Request::Remove(_removal) => {
                        let options = RemoveContainerOptionsBuilder::default().force(true).build();
                        let _ = control_docker.remove_container(&name, Some(options)).await;
                    }
                }
            }
        });

        let (output_tx, output_rx) = mpsc::channel::<Vec<u8>>(OUTPUT_QUEUE_CHUNKS);
        let mut output = attached.output;
        let (exit_tx, exit_rx) = oneshot::channel::<Option<i32>>();
        tokio::spawn(async move {
            while let Some(Ok(chunk)) = output.next().await {
                use bollard::container::LogOutput;
                let bytes = match chunk {
                    LogOutput::StdOut { message }
                    | LogOutput::StdErr { message }
                    | LogOutput::Console { message } => message,
                    LogOutput::StdIn { .. } => continue,
                };
                if output_tx.send(bytes.to_vec()).await.is_err() {
                    break;
                }
            }
            drop(output_tx);
            let code = wait.await.ok().flatten();
            let _ = exit_tx.send(code);
        });

        Ok(ProcessHandle {
            output: output_rx,
            exit: exit_rx,
            control: Arc::new(EngineControl(requests_tx)),
            environment_name: Some(spec.name.clone()),
        })
    }
}
