//! EAC-FR-LIQF / EAC-FR-HTEB / EAC-FR-DQZP: what an interactive launch runs
//! and what it mounts, composed apart from the backend that starts it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::super::descriptor::{self, BindMount, ContainerSpec};
use super::super::devcontainer_pool::RunningDevcontainer;
use super::super::*;
use super::InteractiveProgram;
use crate::terminals::model::CliVendor;

/// EAC-FR-HTEB: a login shell, `/bin/bash -l` where the image holds it and
/// `/bin/sh -l` otherwise.
pub const SHELL_SCRIPT: &str =
    "if [ -x /bin/bash ]; then exec /bin/bash -l; else exec /bin/sh -l; fi";

/// EAC-FR-HTEB: the program that replaces the image's entrypoint for a shell.
pub const SHELL_PROGRAM: &str = "/bin/sh";

/// EAC-FR-HTEB: the variable every interactive process gets.
pub fn terminal_literals() -> Vec<(String, String)> {
    vec![("TERM".to_string(), "xterm-256color".to_string())]
}

/// EAC-FR-HTEB: the entrypoint override and the arguments of `program`. A
/// shell replaces the entrypoint; an agent CLI runs after the image's own
/// entrypoint with its executable name and no argument.
pub fn program_args(program: &InteractiveProgram) -> (Option<String>, Vec<String>) {
    match program {
        InteractiveProgram::Shell => {
            (Some(SHELL_PROGRAM.to_string()), vec!["-c".to_string(), SHELL_SCRIPT.to_string()])
        }
        InteractiveProgram::AgentCli(vendor) => (None, vec![vendor.executable().to_string()]),
    }
}

/// The whole argument vector of `program` where no entrypoint runs first: in a
/// dev container and in a VM.
pub fn program_argv(program: &InteractiveProgram) -> Vec<String> {
    let (entrypoint, args) = program_args(program);
    entrypoint.into_iter().chain(args).collect()
}

/// EAC-FR-LQJC: the `docker run` vector of an interactive launch. It is the
/// vector of EAC-FR-13 with `--tty` after `--interactive`, so the headless
/// vector stays as it is.
pub fn interactive_run_args(spec: &ContainerSpec) -> Vec<String> {
    let mut args = spec.docker_run_args();
    let at = args.iter().position(|a| a == "--interactive").map(|i| i + 1).unwrap_or(1);
    args.insert(at, "--tty".to_string());
    args
}

/// DVC-FR-WRXL: the `docker exec` vector of an interactive session in a dev
/// container. Each credential goes by name (`-e NAME`), never with its value.
pub fn interactive_exec_args(
    running: &RunningDevcontainer,
    checkout: &str,
    env_file: &Path,
    env_names: &[String],
    argv: &[String],
) -> Vec<String> {
    let mut args = vec!["exec".to_string(), "-i".to_string(), "-t".to_string()];
    if !running.remote_user.is_empty() {
        args.extend(["-u".to_string(), running.remote_user.clone()]);
    }
    args.extend([
        "-w".to_string(),
        checkout.to_string(),
        "--env-file".to_string(),
        env_file.to_string_lossy().to_string(),
    ]);
    for name in env_names {
        args.extend(["-e".to_string(), name.clone()]);
    }
    args.push(running.container_id.clone());
    args.extend(argv.iter().cloned());
    args
}

/// The credential of an agent CLI, as a launch passes it (EAC-FR-DQZP).
#[derive(Default)]
pub struct CredentialParts {
    pub env_names: Vec<String>,
    pub env: BTreeMap<String, SecretString>,
    pub mounts: Vec<BindMount>,
    pub env_literals: Vec<(String, String)>,
}

/// EAC-FR-DQZP: the launch credential of AIC-FR-30, in the form a turn gets
/// it. A value goes to the environment of the client process alone.
pub fn credential_parts(credential: AgentLaunchCredential) -> Result<CredentialParts, String> {
    let mut parts = CredentialParts::default();
    match credential {
        AgentLaunchCredential::ClaudeEnvironment(variables) => {
            for variable in variables {
                parts.env_names.push(variable.name.clone());
                parts.env.insert(variable.name, variable.value);
            }
        }
        AgentLaunchCredential::CodexConfigMount { source, target } => {
            let source = descriptor::mount_source(&source)
                .ok_or_else(|| agentic::ERR_CODEX_CONFIG_MISSING.to_string())?;
            let target = target.to_string_lossy().into_owned();
            parts.mounts.push(BindMount { source, target: target.clone(), read_only: false });
            if let Some(codex) = descriptor::descriptor_for("codex") {
                if let descriptor::SessionStateMount::SharedWithCredentialMount { env } =
                    codex.session_state
                {
                    parts.env_literals.push((env.to_string(), target));
                }
            }
        }
    }
    Ok(parts)
}

/// What an interactive container is given beyond its image and its program.
#[derive(Default)]
pub struct Composed {
    /// EAC-FR-ZKMR: where the checkout stands in the container.
    pub workspace: String,
    pub aligned: bool,
    /// The checkout first, then the other mounts.
    pub mounts: Vec<BindMount>,
    pub env_names: Vec<String>,
    pub env: BTreeMap<String, SecretString>,
    pub env_literals: Vec<(String, String)>,
    pub groups: Vec<u32>,
    pub extra_hosts: Vec<String>,
    /// EAC-FR-DQZP: the session-state directory this launch owns, removed when
    /// it ends.
    pub session_dir: Option<PathBuf>,
}

/// EAC-FR-LIQF: the container specification of a Docker interactive launch.
pub fn container_spec(
    name: String,
    image: String,
    composed: &Composed,
    program: &InteractiveProgram,
) -> ContainerSpec {
    let (host_uid, host_gid) = descriptor::host_ids();
    let (entrypoint, vendor_args) = program_args(program);
    ContainerSpec {
        name,
        image,
        host_uid,
        host_gid,
        workdir: composed.workspace.clone(),
        mounts: composed.mounts.clone(),
        groups: composed.groups.clone(),
        extra_hosts: composed.extra_hosts.clone(),
        env_names: composed.env_names.clone(),
        env_literals: composed.env_literals.clone(),
        vendor_args,
        entrypoint,
        network: descriptor::NETWORK,
    }
}

/// EAC-FR-DQZP: whether `program` gets the launch credential. A shell gets
/// none, and OpenCode has none to get (EAC-FR-QGYS).
pub fn takes_credential(program: &InteractiveProgram) -> Option<CliVendor> {
    match program {
        InteractiveProgram::AgentCli(vendor @ (CliVendor::ClaudeCode | CliVendor::Codex)) => {
            Some(*vendor)
        }
        _ => None,
    }
}

/// EAC-FR-DQZP: the dedicated session-state mount of `vendor`, where it has
/// one: the environment name and the container target.
pub fn dedicated_session_state(vendor: CliVendor) -> Option<(&'static str, &'static str)> {
    let descriptor = descriptor::descriptor_for(vendor.id())?;
    match descriptor.session_state {
        descriptor::SessionStateMount::Dedicated { env, target } => Some((env, target)),
        _ => None,
    }
}

/// The session-state directory of one interactive launch, under `root`.
pub fn session_dir_for(root: &Path, terminal_id: &str) -> PathBuf {
    let safe: String = terminal_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' })
        .collect();
    root.join(format!("terminal-{safe}"))
}

/// What an interactive launch adds to the checkout mount.
pub struct ComposeExtras<'a> {
    /// The launch credential of an agent CLI (EAC-FR-DQZP).
    pub credential: Option<AgentLaunchCredential>,
    /// Where the session-state directory of an agent CLI is made.
    pub session_root: Option<&'a Path>,
    pub terminal_id: &'a str,
    /// EAC-FR-DKRS: the project's settings, for a Docker launch alone.
    pub docker_in_docker: Option<&'a DockerInDockerLaunch>,
}

impl AgentCliExecutor {
    /// EAC-FR-LIQF / EAC-FR-DQZP: the mounts and the environment of one
    /// interactive launch in a container: the checkout, the launch credential
    /// and a fresh session-state directory for an agent CLI, the read-only
    /// repository metadata of EAC-FR-FNFV, and Docker-in-Docker where
    /// `docker_in_docker` enables it.
    pub(super) fn compose_interactive(
        &self,
        fs: &FsAccess,
        execution_directory: &Path,
        program: &InteractiveProgram,
        extras: ComposeExtras<'_>,
    ) -> Result<Composed, String> {
        let ComposeExtras { credential, session_root, terminal_id, docker_in_docker } = extras;
        let source = descriptor::mount_source(execution_directory)
            .ok_or_else(|| "the checkout cannot be mounted".to_string())?;
        let aligned = descriptor::container_path(execution_directory);
        let workspace = aligned.clone().unwrap_or_else(|| WORKSPACE_TARGET.to_string());
        let mut composed = Composed {
            workspace: workspace.clone(),
            aligned: aligned.is_some(),
            mounts: vec![BindMount { source, target: workspace.clone(), read_only: false }],
            env_literals: terminal_literals(),
            ..Default::default()
        };

        if let Some(credential) = credential {
            let parts = credential_parts(credential)?;
            composed.env_names.extend(parts.env_names);
            composed.env.extend(parts.env);
            composed.mounts.extend(parts.mounts);
            composed.env_literals.extend(parts.env_literals);
        }

        if let (InteractiveProgram::AgentCli(vendor), Some(root)) = (program, session_root) {
            if let Some((env, target)) = dedicated_session_state(*vendor) {
                let dir = session_dir_for(root, terminal_id);
                fs.create_dir(&dir).map_err(|e| format!("session state: {e}"))?;
                let source = descriptor::mount_source(&dir)
                    .ok_or_else(|| "the session state cannot be mounted".to_string())?;
                composed.mounts.push(BindMount {
                    source,
                    target: target.to_string(),
                    read_only: false,
                });
                composed.env_literals.push((env.to_string(), target.to_string()));
                composed.session_dir = Some(dir);
            }
        }

        if let Some((mounts, env, _linked)) =
            self.repository_mounts(fs, execution_directory, &workspace, composed.aligned)
        {
            composed.mounts.extend(mounts);
            composed.env_literals.extend(env);
        }

        if let Some(settings) = docker_in_docker {
            let dind = docker_in_docker::additions(settings).map_err(|e| e.kind().to_string())?;
            composed.mounts.extend(dind.mounts);
            composed.env_literals.extend(dind.env_literals);
            composed.groups = dind.groups;
            composed.extra_hosts = dind.extra_hosts;
        }
        Ok(composed)
    }
}
