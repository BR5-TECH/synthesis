//! EAC-FR-DKRS, EAC-FR-HWQN, EAC-FR-GKSW and EAC-FR-QWNT: what Docker-in-Docker
//! adds to one container.
//!
//! The four additions and no others: the socket mount, the socket's group, and
//! — where the host gateway toggle is on — the host mapping and the Testcontainers
//! variable. The user, the capabilities and the security options stay as
//! EAC-FR-13 and EAC-FR-14 set them. Nothing here reads a value from a task, an
//! instruction or an agent answer.

use std::collections::BTreeMap;
use std::time::Duration;

use super::descriptor::{
    self, BindMount, ContainerSpec, DOCKER_HOST_GATEWAY_MAPPING, DOCKER_SOCKET_TARGET,
    HELPER_NETWORK, SOCKET_GROUP_ARGS, SOCKET_GROUP_ENTRYPOINT, TESTCONTAINERS_HOST_OVERRIDE_ENV,
    TESTCONTAINERS_HOST_OVERRIDE_VALUE,
};
use super::runtime::{CancellationToken, DockerRuntime, RunEnd, RunRequest, SocketGroupProbe};
use super::{AgentExecutionError, DockerInDockerLaunch};
use crate::log_fields;
use crate::logging::{self, Domain, LogSink, BUFFER};

/// EAC-FR-GKSW: the bound of the helper container.
pub(super) const SOCKET_GROUP_TIMEOUT: Duration = Duration::from_secs(30);
/// EAC-FR-GKSW: the helper prints one number. Anything longer is not one.
const SOCKET_GROUP_OUTPUT_LIMIT: usize = 4096;

/// What one launch adds to its container specification.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct DockerInDockerAdditions {
    pub mounts: Vec<BindMount>,
    pub groups: Vec<u32>,
    pub extra_hosts: Vec<String>,
    pub env_literals: Vec<(String, String)>,
    /// EAC-FR-GKSW: the host path the socket is mounted from, which the helper
    /// container mounts too. `None` where nothing is added.
    pub socket_source: Option<String>,
}

/// EAC-FR-HWQN: checks the socket and reads its group at this launch.
///
/// Disabled settings add nothing, and read no path. An enabled setting whose
/// path is not a Unix socket fails as `DockerSocketUnavailable` before any
/// container exists.
pub(super) fn additions(
    settings: &DockerInDockerLaunch,
) -> Result<DockerInDockerAdditions, AgentExecutionError> {
    if !settings.enabled {
        return Ok(DockerInDockerAdditions::default());
    }
    let unavailable = || AgentExecutionError::DockerSocketUnavailable(settings.socket_path.clone());
    let path = std::path::Path::new(&settings.socket_path);
    let group = crate::fs::probe_unix_socket(path).map_err(|_| unavailable())?;
    let source = descriptor::mount_source(path).ok_or_else(unavailable)?;

    let mut added = DockerInDockerAdditions {
        mounts: vec![BindMount {
            source: source.clone(),
            target: DOCKER_SOCKET_TARGET.to_string(),
            read_only: false,
        }],
        groups: vec![group],
        socket_source: Some(source.clone()),
        ..DockerInDockerAdditions::default()
    };
    if settings.host_gateway {
        added.extra_hosts.push(DOCKER_HOST_GATEWAY_MAPPING.to_string());
        added.env_literals.push((
            TESTCONTAINERS_HOST_OVERRIDE_ENV.to_string(),
            TESTCONTAINERS_HOST_OVERRIDE_VALUE.to_string(),
        ));
    }
    Ok(added)
}

/// EAC-FR-GKSW: the helper container that prints the group of the socket as a
/// container sees it. The image of the launch, its user, its security options,
/// the socket at its fixed target, no network, and `stat` as the program.
pub(super) fn socket_group_spec(probe: &SocketGroupProbe) -> ContainerSpec {
    ContainerSpec {
        name: descriptor::unique_container_name(),
        image: probe.image.clone(),
        host_uid: probe.host_uid,
        host_gid: probe.host_gid,
        workdir: "/".to_string(),
        mounts: vec![BindMount {
            source: probe.socket_source.clone(),
            target: DOCKER_SOCKET_TARGET.to_string(),
            read_only: false,
        }],
        groups: Vec::new(),
        extra_hosts: Vec::new(),
        env_names: Vec::new(),
        env_literals: Vec::new(),
        vendor_args: SOCKET_GROUP_ARGS.iter().map(|arg| (*arg).to_string()).collect(),
        entrypoint: Some(SOCKET_GROUP_ENTRYPOINT.to_string()),
        network: HELPER_NETWORK,
    }
}

/// EAC-FR-GKSW: the number the helper printed, or why there is none.
pub(super) fn parse_socket_group(stdout: &[u8]) -> Result<u32, String> {
    let text = String::from_utf8_lossy(stdout);
    let text = text.trim();
    text.parse::<u32>()
        .map_err(|_| format!("the helper printed no group number ({} bytes)", text.len()))
}

/// EAC-FR-GKSW: runs the helper container through `runtime` and reads the group
/// it prints. It stops when `cancel` is cancelled. A helper that does not end
/// with its number is removed, as EAC-FR-26 removes every container, also where
/// the runtime created it and then failed to start it.
pub(super) async fn read_socket_group<R>(
    runtime: &R,
    probe: &SocketGroupProbe,
    cancel: CancellationToken,
) -> Result<u32, String>
where
    R: DockerRuntime + ?Sized,
{
    let spec = socket_group_spec(probe);
    let argv = spec.docker_run_args();
    let env = BTreeMap::new();
    let outcome = runtime
        .run(RunRequest {
            argv: &argv,
            container: Some(&spec),
            env: &env,
            stdin: &[],
            timeout: SOCKET_GROUP_TIMEOUT,
            cancel,
            stdout_limit: SOCKET_GROUP_OUTPUT_LIMIT,
            stderr_limit: SOCKET_GROUP_OUTPUT_LIMIT,
            observer: None,
        })
        .await;
    let read = match outcome {
        Err(error) => Err(format!("the helper container did not run: {error:?}")),
        // The Engine backend removes a container that ends at once
        // (`auto_remove`) before its exit code can be read, and then reports
        // `Terminated` with no code. What the helper printed still says what it
        // did, so a number on its output is its answer.
        Ok(outcome) if outcome.end == RunEnd::Terminated && outcome.exit_code.is_none() => {
            match parse_socket_group(&outcome.stdout.bytes) {
                Ok(group) => return Ok(group),
                Err(_) => {
                    Err("the helper container ended with no exit code and no group".to_string())
                }
            }
        }
        Ok(outcome) if outcome.end != RunEnd::Exited => {
            Err(format!("the helper container ended as {:?}", outcome.end))
        }
        Ok(outcome) if outcome.exit_code != Some(0) => {
            let said = String::from_utf8_lossy(&outcome.stderr.bytes);
            let said: String = said.trim().chars().take(200).collect();
            Err(format!("the helper exited with {:?}: {said}", outcome.exit_code))
        }
        Ok(outcome) => return parse_socket_group(&outcome.stdout.bytes),
    };
    let _ = runtime.remove_container(&spec.name).await;
    read
}

/// EAC-FR-GKSW / EAC-FR-QWNT: the group the launch adds for the socket. The
/// group a container sees, or the group of the host file where the helper
/// gives none.
pub(super) async fn container_socket_group<S>(
    sink: &S,
    runtime: &dyn DockerRuntime,
    probe: &SocketGroupProbe,
    host_group: u32,
    cancel: CancellationToken,
) -> u32
where
    S: LogSink + Clone + Send + Sync + 'static,
{
    match runtime.socket_group(probe, cancel).await {
        Ok(group) => {
            logging::log_info(
                sink,
                &BUFFER,
                &[Domain::Ai, Domain::Backend],
                "agent container gets the group of the Docker socket as a container sees it",
                log_fields! { "container_group" => group, "host_group" => host_group },
            );
            group
        }
        Err(cause) => {
            logging::log_warn(
                sink,
                &BUFFER,
                &[Domain::Ai, Domain::Backend],
                "the group of the Docker socket in a container is unknown; the host group is used",
                log_fields! { "host_group" => host_group, "cause" => cause },
            );
            host_group
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn settings(path: &str, host_gateway: bool) -> DockerInDockerLaunch {
        DockerInDockerLaunch {
            enabled: true,
            socket_path: path.to_string(),
            host_gateway,
            ..Default::default()
        }
    }

    // EAC-FR-DKRS: disabled settings add nothing and read no path.
    #[test]
    fn disabled_settings_add_nothing() {
        let off = DockerInDockerLaunch {
            enabled: false,
            socket_path: "/does/not/exist.sock".to_string(),
            host_gateway: true,
            ..Default::default()
        };
        assert_eq!(additions(&off).unwrap(), DockerInDockerAdditions::default());
    }

    // EAC-FR-DKRS: the socket, its group, and nothing about the host gateway.
    #[test]
    fn a_socket_adds_its_mount_and_its_group_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("d.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let path_text = path.to_str().unwrap();

        let added = additions(&settings(path_text, false)).unwrap();
        assert_eq!(
            added.mounts,
            vec![BindMount {
                source: path_text.to_string(),
                target: DOCKER_SOCKET_TARGET.to_string(),
                read_only: false,
            }]
        );
        assert_eq!(added.groups.len(), 1);
        assert!(added.extra_hosts.is_empty());
        assert!(added.env_literals.is_empty());
    }

    // EAC-FR-DKRS: the host gateway adds the mapping and the variable.
    #[test]
    fn the_host_gateway_adds_the_mapping_and_the_variable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("d.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&path).unwrap();

        let added = additions(&settings(path.to_str().unwrap(), true)).unwrap();
        assert_eq!(added.extra_hosts, vec![DOCKER_HOST_GATEWAY_MAPPING.to_string()]);
        assert_eq!(
            added.env_literals,
            vec![("TESTCONTAINERS_HOST_OVERRIDE".to_string(), "host.docker.internal".to_string())]
        );
    }

    // EAC-FR-HWQN: a missing path and a plain file both fail, naming the path.
    #[test]
    fn a_missing_socket_and_a_plain_file_fail_naming_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("none.sock");
        // A directory is an entry that is not a socket, and needs no file write.
        let plain = dir.path().to_path_buf();
        for path in [missing, plain] {
            let text = path.to_str().unwrap().to_string();
            match additions(&settings(&text, false)) {
                Err(AgentExecutionError::DockerSocketUnavailable(named)) => {
                    assert_eq!(named, text)
                }
                other => panic!("expected DockerSocketUnavailable, got {other:?}"),
            }
        }
    }

    use super::super::runtime::{BoxFuture, CapturedStream, RuntimeError, RuntimeOutcome};
    use std::sync::Mutex;

    /// A runtime that answers the helper run with one fixed outcome.
    struct HelperRuntime {
        outcome: Result<RuntimeOutcome, RuntimeError>,
        timeouts: Mutex<Vec<Duration>>,
        cancels: Mutex<Vec<CancellationToken>>,
        names: Mutex<Vec<String>>,
        removed: Mutex<Vec<String>>,
    }

    fn answering(outcome: Result<RuntimeOutcome, RuntimeError>) -> HelperRuntime {
        HelperRuntime {
            outcome,
            timeouts: Mutex::new(Vec::new()),
            cancels: Mutex::new(Vec::new()),
            names: Mutex::new(Vec::new()),
            removed: Mutex::new(Vec::new()),
        }
    }

    fn ended(end: RunEnd, exit_code: Option<i32>, stdout: &str, stderr: &str) -> RuntimeOutcome {
        RuntimeOutcome {
            end,
            exit_code,
            stdout: CapturedStream { bytes: stdout.as_bytes().to_vec(), truncated: false },
            stderr: CapturedStream { bytes: stderr.as_bytes().to_vec(), truncated: false },
        }
    }

    impl DockerRuntime for HelperRuntime {
        fn ensure_available(&self) -> BoxFuture<'_, Result<(), RuntimeError>> {
            Box::pin(async { Ok(()) })
        }
        fn ensure_image<'a>(&'a self, _image: &'a str) -> BoxFuture<'a, Result<(), RuntimeError>> {
            Box::pin(async { Ok(()) })
        }
        fn run<'a>(
            &'a self,
            request: RunRequest<'a>,
        ) -> BoxFuture<'a, Result<RuntimeOutcome, RuntimeError>> {
            self.timeouts.lock().unwrap().push(request.timeout);
            self.cancels.lock().unwrap().push(request.cancel.clone());
            let name = request.container.map(|c| c.name.clone()).unwrap_or_default();
            self.names.lock().unwrap().push(name);
            let outcome = self.outcome.clone();
            Box::pin(async move { outcome })
        }
        fn remove_container<'a>(
            &'a self,
            name: &'a str,
        ) -> BoxFuture<'a, Result<(), RuntimeError>> {
            self.removed.lock().unwrap().push(name.to_string());
            Box::pin(async { Ok(()) })
        }
    }

    fn probe() -> SocketGroupProbe {
        SocketGroupProbe {
            image: "example/agent:1".to_string(),
            socket_source: "/var/run/docker.sock".to_string(),
            host_uid: 501,
            host_gid: 20,
        }
    }

    fn read(runtime: &HelperRuntime) -> Result<u32, String> {
        tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(read_socket_group(
            runtime,
            &probe(),
            CancellationToken::new(),
        ))
    }

    // EAC-FR-GKSW: a clean exit that prints a number is the group, read under
    // the bound of 30 seconds.
    #[test]
    fn a_clean_helper_answers_its_number() {
        let runtime = answering(Ok(ended(RunEnd::Exited, Some(0), "0\n", "")));
        assert_eq!(read(&runtime), Ok(0));
        assert_eq!(*runtime.timeouts.lock().unwrap(), vec![Duration::from_secs(30)]);
        assert!(runtime.removed.lock().unwrap().is_empty());
    }

    // EAC-FR-QWNT, EAC-FR-26: a non-zero exit, output that is not a number, and
    // a runtime that refuses the helper each give a cause and no group. A helper
    // that gives no number is removed, also where the runtime refused it after
    // it created the container.
    #[test]
    fn a_failed_helper_gives_a_cause() {
        let exit = answering(Ok(ended(RunEnd::Exited, Some(1), "", "stat: not found")));
        let error = read(&exit).unwrap_err();
        assert!(error.contains("stat: not found"), "{error}");
        assert_eq!(*exit.removed.lock().unwrap(), *exit.names.lock().unwrap());

        let noise = answering(Ok(ended(RunEnd::Exited, Some(0), "root\n", "")));
        assert!(read(&noise).is_err());

        let empty = answering(Ok(ended(RunEnd::Exited, Some(0), "", "")));
        assert!(read(&empty).is_err());

        let refused = answering(Err(RuntimeError::LaunchFailed("no such image".into())));
        assert!(read(&refused).unwrap_err().contains("no such image"));
        assert_eq!(*refused.removed.lock().unwrap(), *refused.names.lock().unwrap());
    }

    // EAC-FR-GKSW, EAC-FR-26: the helper runs under the token of the launch, so
    // a cancelled launch stops it, and its container is removed.
    #[test]
    fn a_cancelled_launch_stops_the_helper() {
        let runtime = answering(Ok(ended(RunEnd::Cancelled, None, "", "")));
        let cancel = CancellationToken::new();
        cancel.cancel();
        let error = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(read_socket_group(&runtime, &probe(), cancel.clone()))
            .unwrap_err();
        assert!(error.contains("Cancelled"), "{error}");
        assert!(runtime.cancels.lock().unwrap().iter().all(|seen| seen.is_cancelled()));
        assert_eq!(*runtime.removed.lock().unwrap(), *runtime.names.lock().unwrap());
    }

    // EAC-FR-QWNT, EAC-FR-26: a helper that times out gives no group, and its
    // container is removed.
    #[test]
    fn a_timed_out_helper_is_removed() {
        let runtime = answering(Ok(ended(RunEnd::TimedOut, None, "", "")));
        assert!(read(&runtime).is_err());
        assert_eq!(*runtime.removed.lock().unwrap(), *runtime.names.lock().unwrap());
    }

    // EAC-FR-GKSW, EAC-FR-QWNT: a helper that the Engine backend removed before
    // its exit code could be read answers with the number it printed. With no
    // number, it gives a cause and is removed.
    #[test]
    fn a_helper_removed_before_its_exit_code_answers_its_number() {
        let removed_early = answering(Ok(ended(RunEnd::Terminated, None, "0\n", "")));
        assert_eq!(read(&removed_early), Ok(0));
        assert!(removed_early.removed.lock().unwrap().is_empty());

        let silent = answering(Ok(ended(RunEnd::Terminated, None, "", "")));
        assert!(read(&silent).is_err());
        assert_eq!(*silent.removed.lock().unwrap(), *silent.names.lock().unwrap());

        let killed = answering(Ok(ended(RunEnd::Terminated, Some(137), "0\n", "")));
        assert!(read(&killed).is_err());
    }

    // EAC-FR-GKSW: surrounding white space is not part of the number.
    #[test]
    fn the_number_is_read_without_white_space() {
        assert_eq!(parse_socket_group(b"  993\n"), Ok(993));
        assert!(parse_socket_group(b"-1").is_err());
        assert!(parse_socket_group(b"1 2").is_err());
    }
}
