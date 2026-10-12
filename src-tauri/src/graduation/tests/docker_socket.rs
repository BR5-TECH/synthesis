//! The Docker socket preflight (GSU-FR-GLVQ, GSU-FR-SQRC, GSU-FR-HIPF,
//! GSU-FR-RQDS, GSU-FR-LMPW).

use super::*;
use crate::project_settings::DockerInDockerSettings;

impl Fixture {
    /// Store the project's Docker-in-Docker settings, as the Docker section of
    /// Project settings does.
    fn docker_in_docker(&self, enabled: bool, socket_path: &str) {
        let access = self.app.state::<crate::fs::FsAccessState>().get().expect("an instance");
        let root = crate::fs::RootFs::new(self.root(), access);
        crate::project_settings::save_docker_in_docker_settings_to(
            &root,
            DockerInDockerSettings {
                enabled,
                socket_path: socket_path.to_string(),
                host_gateway: false,
            },
        )
        .expect("stored settings");
    }

    fn start_on(
        &self,
        draft: &str,
        stream: &crate::streams::WorkStream,
    ) -> Result<GraduationRun, String> {
        crate::graduation::start_graduation(
            self.app.clone(),
            draft.to_string(),
            stream.id.clone(),
            StandingWork::Keep,
            None,
        )
    }
}

/// GSU-FR-RQDS, GSU-FR-SQRC, GSU-FR-ELZO: a start whose project enables
/// Docker-in-Docker is refused when the socket is missing. The refusal names the
/// path, and no run, no queue entry and no draft lock exists.
#[test]
fn a_missing_socket_refuses_the_start_and_creates_nothing() {
    let fx = Fixture::new();
    fx.allow_execution();
    let missing = fx.root().join("no-such.sock");
    let path = missing.to_string_lossy().into_owned();
    fx.docker_in_docker(true, &path);
    let stream = fx.stream("editor");
    let draft = fx.draft("task");

    let refused = fx.start_on(&draft, &stream).unwrap_err();

    assert_eq!(refused, format!("{ERR_DOCKER_SOCKET_UNAVAILABLE}: {path}"));
    let queue = crate::graduation::list_graduation_queue(fx.app.clone()).expect("the queue");
    assert!(queue.runs.is_empty(), "no run exists");
    assert!(crate::graduation::require_unlocked_draft(&fx.app, &draft).is_ok(), "no draft lock");
}

/// GSU-FR-RQDS: a path that exists and is not a Unix socket is refused the same
/// way.
#[test]
fn a_file_that_is_not_a_socket_refuses_the_start() {
    let fx = Fixture::new();
    fx.allow_execution();
    let plain = fx.root().join("plain.sock");
    std::fs::write(&plain, b"x").unwrap();
    let path = plain.to_string_lossy().into_owned();
    fx.docker_in_docker(true, &path);
    let stream = fx.stream("editor");
    let draft = fx.draft("task");

    let refused = fx.start_on(&draft, &stream).unwrap_err();

    assert_eq!(refused, format!("{ERR_DOCKER_SOCKET_UNAVAILABLE}: {path}"));
}

/// GSU-FR-RQDS, GSU-FR-LMPW: with Docker-in-Docker disabled no path is read, and
/// with a real socket the start passes.
#[cfg(unix)]
#[test]
fn a_disabled_setting_reads_no_path_and_a_real_socket_passes() {
    let fx = Fixture::new();
    fx.allow_execution();
    let stream = fx.stream("editor");
    fx.hold_stream(&stream.id);
    let draft = fx.draft("task");

    fx.docker_in_docker(false, "/does/not/exist.sock");
    let run = fx.start_on(&draft, &stream).expect("a disabled setting refuses nothing");
    crate::graduation::discard_graduation_run(fx.app.clone(), run.id.clone()).expect("discarded");

    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("docker.sock");
    let _listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    fx.docker_in_docker(true, socket.to_str().unwrap());
    fx.start_on(&draft, &stream).expect("a socket passes the preflight");
}

/// GSU-FR-RQDS: the direct start takes the same check, after the worktree
/// checks and before anything exists.
#[test]
fn a_direct_start_is_refused_for_a_missing_socket() {
    let fx = Fixture::new();
    fx.allow_execution();
    let path = fx.root().join("no-such.sock").to_string_lossy().into_owned();
    fx.docker_in_docker(true, &path);
    let draft = fx.committed_draft("direct");

    let refused = crate::graduation::start_direct_graduation(
        fx.app.clone(),
        draft.clone(),
        fx.root().to_string_lossy().into_owned(),
        fx.branch(),
    )
    .unwrap_err();

    assert_eq!(refused, format!("{ERR_DOCKER_SOCKET_UNAVAILABLE}: {path}"));
    let queue = crate::graduation::list_graduation_queue(fx.app.clone()).expect("the queue");
    assert!(queue.runs.is_empty());
    assert!(crate::graduation::require_unlocked_draft(&fx.app, &draft).is_ok());
}

/// GSU-FR-RQDS, GSU-FR-IRAC: a restart takes the same check afresh, so a socket
/// that has gone away since the first start refuses the restart and creates no
/// second run.
#[cfg(unix)]
#[test]
fn a_restart_takes_the_socket_check_afresh() {
    let fx = Fixture::new();
    fx.allow_execution();
    let stream = fx.stream("editor");
    fx.hold_stream(&stream.id);
    let draft = fx.draft("task");
    let first = fx.start_on(&draft, &stream).expect("a run");
    let discarded = crate::graduation::discard_graduation_run(fx.app.clone(), first.id.clone())
        .expect("a discarded run");

    let path = fx.root().join("gone.sock").to_string_lossy().into_owned();
    fx.docker_in_docker(true, &path);
    let refused = crate::graduation::restart_graduation_run(
        fx.app.clone(),
        discarded.id.clone(),
        stream.id.clone(),
        StandingWork::Keep,
        None,
    )
    .unwrap_err();

    assert_eq!(refused, format!("{ERR_DOCKER_SOCKET_UNAVAILABLE}: {path}"));
    let queue = crate::graduation::list_graduation_queue(fx.app.clone()).expect("the queue");
    assert!(queue.runs.iter().all(|run| run.restarted_from_run_id.is_none()));

    // A comma cannot be mounted, so a path that holds one is refused too.
    fx.docker_in_docker(false, "");
    let comma = fx.root().join("a,b.sock").to_string_lossy().into_owned();
    std::fs::write(
        fx.root().join(".synthesis/local.toml"),
        format!("[docker_in_docker]\nenabled = true\nsocketPath = {comma:?}\n"),
    )
    .unwrap();
    let refused = fx.start_on(&fx.draft("another"), &stream).unwrap_err();
    assert_eq!(refused, format!("{ERR_DOCKER_SOCKET_UNAVAILABLE}: {comma}"));
}
