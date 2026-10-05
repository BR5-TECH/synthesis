//! The tests of the Docker backend: how an endpoint is parsed, how a record
//! resolves to a backend, and what the commands of this module agree to.

use super::*;
use std::path::PathBuf;

#[derive(Default)]
struct StubProbe {
    existing: Vec<PathBuf>,
    executable: Vec<PathBuf>,
}

impl FileProbe for StubProbe {
    fn exists(&self, path: &Path) -> bool {
        self.existing.iter().any(|p| p == path)
    }
    fn is_executable(&self, path: &Path) -> bool {
        self.executable.iter().any(|p| p == path)
    }
}

struct StubRunner {
    identity: Result<CliOutput, RunError>,
    daemon: Result<CliOutput, RunError>,
}

impl StubRunner {
    fn new(identity: &str, daemon_stdout: &str, daemon_ok: bool) -> Self {
        Self {
            identity: Ok(CliOutput {
                stdout: identity.to_string(),
                stderr: String::new(),
                success: true,
            }),
            daemon: Ok(CliOutput {
                stdout: daemon_stdout.to_string(),
                stderr: String::new(),
                success: daemon_ok,
            }),
        }
    }
}

impl CliRunner for StubRunner {
    fn run(
        &self,
        _path: &Path,
        args: &[&str],
        _timeout: Duration,
    ) -> Result<CliOutput, RunError> {
        let source = if args == CLI_IDENTITY_ARGS {
            &self.identity
        } else {
            &self.daemon
        };
        match source {
            Ok(output) => Ok(output.clone()),
            Err(RunError::NotFound) => Err(RunError::NotFound),
            Err(RunError::NotExecutable) => Err(RunError::NotExecutable),
            Err(RunError::TimedOut) => Err(RunError::TimedOut),
            Err(RunError::Failed(e)) => Err(RunError::Failed(e.clone())),
        }
    }
}

struct StubEngine(Result<String, EngineError>);

impl DockerEngine for StubEngine {
    fn server_version(
        &self,
        _endpoint: &DockerEndpoint,
        _timeout: Duration,
    ) -> Result<String, EngineError> {
        match &self.0 {
            Ok(v) => Ok(v.clone()),
            Err(EngineError::TimedOut) => Err(EngineError::TimedOut),
            Err(EngineError::Unreachable(e)) => Err(EngineError::Unreachable(e.clone())),
        }
    }
}

fn cli_config(path: &str) -> DockerBackendConfig {
    DockerBackendConfig {
        mode: DockerBackendMode::DockerCli,
        endpoint: DockerEndpoint::Automatic,
        cli_path: Some(path.to_string()),
    }
}

fn engine_config(endpoint: DockerEndpoint) -> DockerBackendConfig {
    DockerBackendConfig {
        mode: DockerBackendMode::Bollard,
        endpoint,
        cli_path: None,
    }
}

/// GSS-FR-37 (GSS-FR-35, GSS-FR-36): a machine that has configured nothing
/// reaches Docker the way the platform does, and reads as unverified.
#[test]
fn an_unconfigured_record_defaults_to_bollard_automatic_and_unverified() {
    let record = DockerBackendRecord::default();
    let out = record.outbound();
    assert_eq!(out.mode, DockerBackendMode::Bollard);
    assert_eq!(out.endpoint, DockerEndpoint::Automatic);
    assert_eq!(out.cli_path, None);
    assert_eq!(out.state, DockerBackendState::Unverified);
    assert_eq!(out.server_version, None);
}

/// GSS-FR-38 (GSS-FR-38): every CLI-mode refusal, told apart from the next.
#[test]
fn cli_verification_distinguishes_the_executable_from_the_daemon() {
    let engine = StubEngine(Err(EngineError::Unreachable("unused".into())));
    let probe = StubProbe::default();
    let runner = StubRunner::new("Docker version 27.1.1", "27.1.1", true);

    // Empty path.
    assert_eq!(
        verify_backend(&cli_config("  "), &probe, &runner, &engine, VERIFY_TIMEOUT).unwrap_err(),
        ERR_CLI_PATH_EMPTY
    );
    // Nothing there.
    assert_eq!(
        verify_backend(&cli_config("/nope/docker"), &probe, &runner, &engine, VERIFY_TIMEOUT)
            .unwrap_err(),
        ERR_CLI_NOT_FOUND
    );
    // There, but not runnable.
    let probe = StubProbe {
        existing: vec![PathBuf::from("/bin/docker")],
        executable: vec![],
    };
    assert_eq!(
        verify_backend(&cli_config("/bin/docker"), &probe, &runner, &engine, VERIFY_TIMEOUT)
            .unwrap_err(),
        ERR_CLI_NOT_EXECUTABLE
    );

    let probe = StubProbe {
        existing: vec![PathBuf::from("/bin/docker")],
        executable: vec![PathBuf::from("/bin/docker")],
    };
    // Runnable, and not Docker.
    let other = StubRunner::new("GNU coreutils ls 9.1", "", false);
    assert_eq!(
        verify_backend(&cli_config("/bin/docker"), &probe, &other, &engine, VERIFY_TIMEOUT)
            .unwrap_err(),
        ERR_NOT_THE_DOCKER_CLI
    );
    // Docker, and the daemon is down — the distinction GSS-FR-38 exists for.
    let down = StubRunner::new("Docker version 27.1.1", "", false);
    assert_eq!(
        verify_backend(&cli_config("/bin/docker"), &probe, &down, &engine, VERIFY_TIMEOUT)
            .unwrap_err(),
        ERR_DAEMON_UNREACHABLE
    );
    // Docker, and the daemon answered.
    assert_eq!(
        verify_backend(&cli_config("/bin/docker"), &probe, &runner, &engine, VERIFY_TIMEOUT)
            .unwrap(),
        "27.1.1"
    );
}

/// GSS-FR-38 (GSS-FR-38): a CLI that answers `--version` but whose daemon
/// prints nothing is not a success, however cleanly the client ran.
#[test]
fn a_silent_daemon_is_never_reported_as_a_success() {
    let probe = StubProbe {
        existing: vec![PathBuf::from("/bin/docker")],
        executable: vec![PathBuf::from("/bin/docker")],
    };
    let runner = StubRunner::new("Docker version 27.1.1", "   \n", true);
    let engine = StubEngine(Err(EngineError::Unreachable("unused".into())));
    assert_eq!(
        verify_backend(&cli_config("/bin/docker"), &probe, &runner, &engine, VERIFY_TIMEOUT)
            .unwrap_err(),
        ERR_DAEMON_UNREACHABLE
    );
}

/// GSS-FR-38 (GSS-FR-38): the connection's own bound never decides which of
/// the two engine failures the author reads.
///
/// `verify_backend` tells a daemon that never answers (`timed_out`) from a
/// daemon that refused (`daemon_unreachable`) by the deadline it puts
/// *around* the version call. A connection whose own request bound could
/// fire first would report the refusal for the timeout, which is the wrong
/// correction to ask the author for. The bound is asserted here because the
/// two values sit in different modules and nothing else compares them.
#[test]
fn a_connection_is_bounded_wider_than_the_deadline_that_classifies_it() {
    assert!(
        connect_bound(VERIFY_TIMEOUT) > VERIFY_TIMEOUT,
        "the outer deadline must be the one that fires"
    );
    // A bound of zero is no bound at all to a connector that reads it as a
    // count of seconds, so the floor holds for every input.
    assert!(connect_bound(Duration::from_millis(1)) > Duration::ZERO);
    // Saturating, never wrapping: an absurd deadline stays absurd rather
    // than becoming a short one.
    assert_eq!(connect_bound(Duration::MAX), Duration::MAX);
}

/// GSS-FR-38 (GSS-FR-38): the Docker Engine mode's own two failures, told
/// apart.
#[test]
fn engine_verification_distinguishes_the_endpoint_from_the_daemon() {
    let probe = StubProbe::default();
    let runner = StubRunner::new("", "", false);
    let up = StubEngine(Ok("27.1.1".to_string()));
    let down = StubEngine(Err(EngineError::Unreachable("connection refused".into())));
    let slow = StubEngine(Err(EngineError::TimedOut));

    assert_eq!(
        verify_backend(
            &engine_config(DockerEndpoint::Tcp(String::new())),
            &probe,
            &runner,
            &up,
            VERIFY_TIMEOUT
        )
        .unwrap_err(),
        ERR_ENDPOINT_EMPTY
    );
    assert_eq!(
        verify_backend(
            &engine_config(DockerEndpoint::Tcp("not a url".into())),
            &probe,
            &runner,
            &up,
            VERIFY_TIMEOUT
        )
        .unwrap_err(),
        ERR_ENDPOINT_INVALID
    );
    assert_eq!(
        verify_backend(
            &engine_config(DockerEndpoint::Automatic),
            &probe,
            &runner,
            &down,
            VERIFY_TIMEOUT
        )
        .unwrap_err(),
        ERR_DAEMON_UNREACHABLE
    );
    assert_eq!(
        verify_backend(
            &engine_config(DockerEndpoint::Automatic),
            &probe,
            &runner,
            &slow,
            VERIFY_TIMEOUT
        )
        .unwrap_err(),
        ERR_TIMED_OUT
    );
    assert_eq!(
        verify_backend(
            &engine_config(DockerEndpoint::Automatic),
            &probe,
            &runner,
            &up,
            VERIFY_TIMEOUT
        )
        .unwrap(),
        "27.1.1"
    );
}

/// GSS-FR-38 (GSS-FR-36): the four platform-neutral forms, and what each
/// accepts.
#[test]
fn the_four_endpoint_forms_validate_on_their_own_terms() {
    assert!(validate_endpoint(&DockerEndpoint::Automatic).is_ok());
    assert!(validate_endpoint(&DockerEndpoint::UnixSocket("/var/run/docker.sock".into())).is_ok());
    assert!(
        validate_endpoint(&DockerEndpoint::UnixSocket("unix:///var/run/docker.sock".into()))
            .is_ok()
    );
    assert_eq!(
        validate_endpoint(&DockerEndpoint::UnixSocket("var/run/docker.sock".into())).unwrap_err(),
        ERR_ENDPOINT_INVALID
    );
    assert!(validate_endpoint(&DockerEndpoint::WindowsPipe("//./pipe/docker_engine".into())).is_ok());
    assert!(
        validate_endpoint(&DockerEndpoint::WindowsPipe(r"\\.\pipe\docker_engine".into())).is_ok()
    );
    assert_eq!(
        validate_endpoint(&DockerEndpoint::WindowsPipe("docker_engine".into())).unwrap_err(),
        ERR_ENDPOINT_INVALID
    );
    assert!(validate_endpoint(&DockerEndpoint::Tcp("tcp://192.168.1.4:2375".into())).is_ok());
    assert!(validate_endpoint(&DockerEndpoint::Tcp("https://docker.example:2376".into())).is_ok());
    assert_eq!(
        validate_endpoint(&DockerEndpoint::Tcp("ftp://host".into())).unwrap_err(),
        ERR_ENDPOINT_INVALID
    );
}

/// GSS-FR-39 (GSS-FR-39): a success belongs to the values that earned it,
/// and changing the endpoint away and back does not bring it back.
#[test]
fn changing_the_endpoint_invalidates_the_success_and_going_back_does_not_restore_it() {
    let mut record = DockerBackendRecord::default();
    record.record_success(&engine_config(DockerEndpoint::Automatic), "27.1.1", "2026-01-01T00:00:00Z");
    // A relaunch reads the same record, and a stopped daemon changes
    // nothing about it: reachability is not what the state describes.
    assert_eq!(record.outbound().state, DockerBackendState::Verified);
    assert_eq!(record.outbound().server_version.as_deref(), Some("27.1.1"));
    let reloaded: DockerBackendRecord =
        toml::from_str(&toml::to_string(&record).unwrap()).unwrap();
    assert_eq!(reloaded.outbound().state, DockerBackendState::Verified);

    // The endpoint changes.
    let mut moved = record.clone();
    moved.apply(&engine_config(DockerEndpoint::Tcp("tcp://host:2375".into())));
    assert_eq!(moved.outbound().state, DockerBackendState::Unverified);
    // And back again — still unverified. A success says a daemon answered
    // when it was asked, and nobody has asked this one since.
    moved.apply(&engine_config(DockerEndpoint::Automatic));
    assert_eq!(moved.outbound().state, DockerBackendState::Unverified);
    assert_eq!(moved.outbound().server_version, None);
}

/// GSS-FR-36 (GSS-FR-36, GSS-FR-39): changing the mode invalidates the
/// success and keeps the other mode's value, and only a fresh verification
/// restores it.
#[test]
fn changing_the_mode_invalidates_the_success_and_keeps_the_other_modes_value() {
    let mut record = DockerBackendRecord::default();
    record.record_success(&cli_config("/bin/docker"), "27.1.1", "2026-01-01T00:00:00Z");
    assert_eq!(record.outbound().state, DockerBackendState::Verified);

    record.apply(&engine_config(DockerEndpoint::Automatic));
    assert_eq!(record.outbound().state, DockerBackendState::Unverified);
    // GSS-FR-36: the CLI path the other mode uses is kept.
    assert_eq!(record.cli_path.as_deref(), Some("/bin/docker"));

    // Back to the mode that earned it, and still unverified until Verify
    // runs again.
    record.apply(&cli_config("/bin/docker"));
    assert_eq!(record.outbound().state, DockerBackendState::Unverified);
    record.record_success(&cli_config("/bin/docker"), "27.2.0", "2026-02-01T00:00:00Z");
    assert_eq!(record.outbound().state, DockerBackendState::Verified);
    assert_eq!(record.outbound().server_version.as_deref(), Some("27.2.0"));

    // And the third of the three: the CLI path itself.
    record.apply(&cli_config("/usr/local/bin/docker"));
    assert_eq!(record.outbound().state, DockerBackendState::Unverified);
}

/// GSS-FR-39 (GSS-FR-39): a `synthesis.toml` an author edited, carrying a
/// success onto a selection it was never earned against, is not read as
/// verified.
#[test]
fn a_success_carried_onto_another_selection_is_not_read_as_one() {
    let mut record = DockerBackendRecord::default();
    record.record_success(&cli_config("/bin/docker"), "27.1.1", "2026-01-01T00:00:00Z");
    // Only the selected value is edited, as a hand-editor would.
    record.cli_path = Some("/somewhere/else/docker".to_string());
    assert_eq!(record.outbound().state, DockerBackendState::Unverified);
    assert_eq!(
        resolve_from_record(&record).unwrap_err(),
        ERR_DOCKER_BACKEND_UNVERIFIED
    );
}

/// GSS-FR-40 (GSS-FR-40): resolution reads the record and asks the daemon
/// nothing, and no frontend call can reach it.
#[test]
fn resolution_returns_the_verified_backend_or_refuses() {
    let unverified = DockerBackendRecord::default();
    assert_eq!(
        resolve_from_record(&unverified).unwrap_err(),
        ERR_DOCKER_BACKEND_UNVERIFIED
    );

    let mut engine = DockerBackendRecord::default();
    engine.record_success(
        &engine_config(DockerEndpoint::UnixSocket("/var/run/docker.sock".into())),
        "27.1.1",
        "2026-01-01T00:00:00Z",
    );
    assert_eq!(
        resolve_from_record(&engine).unwrap(),
        ResolvedDockerBackend::Engine {
            endpoint: DockerEndpoint::UnixSocket("/var/run/docker.sock".into())
        }
    );

    let mut cli = DockerBackendRecord::default();
    cli.record_success(&cli_config("/bin/docker"), "27.1.1", "2026-01-01T00:00:00Z");
    assert_eq!(
        resolve_from_record(&cli).unwrap(),
        ResolvedDockerBackend::Cli {
            path: "/bin/docker".to_string()
        }
    );

    // And no frontend call can reach it: the read path for an actual Docker
    // operation is absent from the handler registry, exactly as
    // `resolve_agentic_invocation` is (GSS-FR-40).
    const LIB: &str = include_str!("../lib.rs");
    assert!(
        !LIB.contains("docker::resolve_docker_backend"),
        "the resolution is registered as a command"
    );
    // The canonical name list lives beside `lib.rs` rather than in it, so
    // this reads the file that actually holds it — a negative assertion
    // against the wrong file is the one that stays green while proving
    // nothing.
    const NAMES: &str = include_str!("../command_names.rs");
    assert!(
        !NAMES.contains("\"resolve_docker_backend\""),
        "the resolution is named in the registered command list"
    );
}

/// GSS-FR-35, GSS-FR-36 (GSS-FR-37): detection reports a path and commits nothing.
#[test]
fn detection_finds_the_first_docker_on_the_search_path() {
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let docker = bin.join("docker");
    std::fs::write(&docker, "#!/bin/sh\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let probe = crate::agentic::RealFileProbe;
    let found = detect_docker_cli(Some(bin.to_str().unwrap()), None, &probe);
    assert_eq!(found.as_deref(), Some(docker.to_str().unwrap()));

    // A machine running the suite may genuinely have Docker in one of the
    // conventional locations detection also searches, so the assertion is
    // that nothing was found *in the named directory* rather than that
    // nothing was found at all.
    let empty = tempfile::tempdir().unwrap();
    let elsewhere = detect_docker_cli(Some(empty.path().to_str().unwrap()), None, &probe);
    assert!(
        !elsewhere
            .as_deref()
            .is_some_and(|p| p.starts_with(empty.path().to_str().unwrap())),
        "detection reported a binary in a directory that holds none"
    );
    // And it committed nothing either way.
    assert!(!empty.path().join("docker").exists());
}

/// GSS-FR-35 (GSS-FR-35): the record round-trips through TOML, scalars
/// before its one sub-table, carrying every Docker field and no secret.
#[test]
fn the_record_round_trips_through_toml() {
    let mut record = DockerBackendRecord::default();
    record.record_success(
        &cli_config("/usr/local/bin/docker"),
        "27.1.1",
        "2026-01-01T00:00:00Z",
    );
    let text = toml::to_string(&record).unwrap();
    let parsed: DockerBackendRecord = toml::from_str(&text).unwrap();
    assert_eq!(parsed, record);
    assert_eq!(parsed.outbound().state, DockerBackendState::Verified);
    // Every Docker field the user-global store holds is in the text.
    for value in [
        "docker_cli",
        "/usr/local/bin/docker",
        "27.1.1",
        "2026-01-01T00:00:00Z",
    ] {
        assert!(text.contains(value), "{value} is missing from {text}");
    }
    // GSS-FR-35: and none of it is a project-scoped value, so nothing here
    // has any business in a committed project file. The record carries no
    // project key at all, which is what makes that true by shape.
    assert!(!text.contains("project"), "{text}");
}

/// GSS-FR-35 (GSS-FR-36): the wire shape of the endpoint is the tagged one
/// the contract defines.
#[test]
fn the_endpoint_serialises_as_the_contract_spells_it() {
    assert_eq!(
        serde_json::to_string(&DockerEndpoint::Automatic).unwrap(),
        "\"automatic\""
    );
    assert_eq!(
        serde_json::to_string(&DockerEndpoint::UnixSocket("/var/run/docker.sock".into()))
            .unwrap(),
        "{\"unix_socket\":\"/var/run/docker.sock\"}"
    );
    let parsed: DockerEndpoint =
        serde_json::from_str("{\"tcp\":\"tcp://host:2375\"}").unwrap();
    assert_eq!(parsed, DockerEndpoint::Tcp("tcp://host:2375".into()));
}
