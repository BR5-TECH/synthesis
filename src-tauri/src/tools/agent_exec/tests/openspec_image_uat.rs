//! OAT-FR-NVHI, OAT-FR-ISHM, OAT-FR-TJAV: the OpenSpec image acceptance test.
//!
//! It builds the Claude Code agent image of the repository through the
//! project image build (PSS-FR-26), and runs the production image probe of
//! OFP-FR-BZDP against it with real Docker. It also runs the probe against
//! two images made from the same base: one that holds no `openspec`, and one
//! whose `openspec` reports a version below 1. Nothing here mocks a Docker
//! call or an executor step.
//!
//! Ignored by default, and run by no pipeline lane: it needs a Docker daemon
//! and the base image of the Dockerfile. Run it with:
//!
//! ```text
//! task uat:openspec-image
//! ```

use super::dind_uat::{docker_cli, repository_root};
use super::*;
use crate::docker::{DockerBackendConfig, DockerBackendMode, DockerBackendSeams, DockerEndpoint};
use crate::graduation::driver::image_probe::{verdict_of, ProbeVerdict};
use crate::isolation::{IsolationMode, IsolationRecord};
use crate::project_settings::images::{
    self, BuildTerminal, ImageBuildRegistry, ProjectVendorImage,
};
use crate::tools::agent_exec::image_probe::{ImageProbeRequest, ProbeCommand};

/// The Dockerfile of the agent image, relative to the repository root.
const AGENT_DOCKERFILE: &str = "docker/agent-images/claude-code/Dockerfile";

/// One image of the test: the folder of its build context in the project,
/// and its local name.
struct Image {
    folder: &'static str,
    name: &'static str,
}

const AGENT: Image = Image { folder: "oat-agent", name: "synthesis-oat-agent" };
const NO_CLI: Image = Image { folder: "oat-no-cli", name: "synthesis-oat-no-cli" };
const OLD_CLI: Image = Image { folder: "oat-old-cli", name: "synthesis-oat-old-cli" };

/// The first `FROM` line of the agent Dockerfile, so the two small images
/// reuse its base and reach no other image (OAT-FR-ISHM).
fn base_line(dockerfile: &str) -> String {
    dockerfile
        .lines()
        .find(|line| line.starts_with("FROM "))
        .expect("the agent Dockerfile names a base image")
        .to_string()
}

/// AVI-FR-RZFR: the OpenSpec version that the Dockerfile pins.
fn pinned_version(dockerfile: &str) -> String {
    dockerfile
        .lines()
        .find_map(|line| line.strip_prefix("ARG OPENSPEC_VERSION="))
        .expect("the agent Dockerfile pins OPENSPEC_VERSION")
        .trim()
        .to_string()
}

/// The build contexts of the three images, written into the project.
fn write_contexts(project: &Path, dockerfile: &str) {
    let write = |image: &Image, body: &str| {
        let folder = project.join(image.folder);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("Dockerfile"), body).unwrap();
    };
    write(&AGENT, dockerfile);
    let base = base_line(dockerfile);
    write(&NO_CLI, &format!("{base}\n"));
    write(
        &OLD_CLI,
        &format!(
            "{base}\nRUN printf '#!/bin/sh\\necho 0.9.0\\n' > /usr/local/bin/openspec \
             && chmod 0755 /usr/local/bin/openspec\n"
        ),
    );
}

/// OAT-FR-TJAV: the Docker CLI and a daemon that answers, or a failure that
/// names the missing one.
fn docker_ready() -> String {
    let docker = std::panic::catch_unwind(docker_cli).unwrap_or_else(|_| {
        panic!("no `docker` executable was found; install Docker to run uat:openspec-image")
    });
    let answered = std::process::Command::new(&docker)
        .args(["version", "--format", "{{.Server.Version}}"])
        .output()
        .is_ok_and(|out| out.status.success());
    assert!(answered, "the Docker daemon does not answer `{docker} version`; start Docker");
    docker
}

/// The project settings of the test: the Docker backend verified against the
/// real daemon (GSS-FR-38), isolation None, and `image` as the project image
/// (PSS-FR-22).
fn store_settings(harness: &Harness, root: &crate::fs::RootFs, docker: &str, image: &Image) {
    crate::docker::verify_docker_backend_with(
        &Silent,
        &crate::logging::BUFFER,
        &harness.store,
        &DockerBackendSeams::default(),
        DockerBackendConfig {
            mode: DockerBackendMode::DockerCli,
            endpoint: DockerEndpoint::Automatic,
            cli_path: Some(docker.to_string()),
        },
    )
    .expect("the Docker backend verifies");
    harness
        .store
        .save_isolation(&IsolationRecord {
            mode: IsolationMode::None,
            ..IsolationRecord::default()
        })
        .expect("the isolation saves");
    crate::project_settings::save_project_vendor_image_to(
        root,
        "claude_code",
        &ProjectVendorImage {
            image_name: image.name.to_string(),
            tag: Some("test".to_string()),
            dockerfile: Some(format!("{}/Dockerfile", image.folder)),
            build_context: Some(image.folder.to_string()),
        },
    )
    .expect("the project image saves");
}

/// Keeps the last build output, so a failed build says why.
#[derive(Default)]
struct BuildLog(StdMutex<Vec<String>>);

impl images::BuildEvents for BuildLog {
    fn progress(&self, payload: images::ImageBuildProgress) {
        let mut lines = self.0.lock().unwrap();
        lines.push(payload.message);
        let excess = lines.len().saturating_sub(40);
        lines.drain(..excess);
    }

    fn finished(&self, _payload: images::ImageBuildFinished) {}
}

/// PSS-FR-26: the project image, built through the project image build.
fn build(harness: &Harness, root: &crate::fs::RootFs, image: &Image) {
    let registry = ImageBuildRegistry::default();
    let operation = format!("oat-build-{}", image.folder);
    let prepared = crate::project_settings::build_seam::prepare_project_vendor_image_build(
        root,
        &harness.store,
        "claude_code",
        &operation,
        &registry,
    )
    .unwrap_or_else(|error| panic!("the build of {} cannot start: {error}", image.name));
    let log = BuildLog::default();
    let builder = crate::project_settings::image_builder::RealImageBuilder;
    match images::run_prepared_build(prepared, &builder, &log, &registry) {
        BuildTerminal::Succeeded => {}
        BuildTerminal::Failed(diagnostic) => panic!(
            "the build of {} failed: {diagnostic}; last output: {}",
            image.name,
            log.0.lock().unwrap().join(" | ")
        ),
        BuildTerminal::Cancelled => panic!("the build of {} was cancelled", image.name),
    }
}

/// OFP-FR-BZDP, OFP-FR-XBNJ: the verdict of one production probe of the
/// project image.
fn probe(harness: &Harness, root: &crate::fs::RootFs) -> ProbeVerdict {
    let rt = runtime();
    let executor = AgentCliExecutor::default().with_session_state_root(harness.sessions_root());
    let (docker_in_docker, images) = project_launch_inputs(root.clone());
    let context = LaunchContext { images: &images, docker_in_docker, ..harness.context() };
    let outcome = rt.block_on(executor.probe_agent_image(
        &Silent,
        &context,
        ImageProbeRequest {
            command: ProbeCommand::OpenSpecVersion,
            cancellation: CancellationToken::new(),
        },
    ));
    verdict_of(&outcome)
}

/// Removes the local image of the test when the test ends, also when it
/// fails, so no run leaves an image in the daemon. An error is ignored.
struct RemoveImage<'a> {
    docker: &'a str,
    reference: String,
}

impl Drop for RemoveImage<'_> {
    fn drop(&mut self) {
        let _ =
            std::process::Command::new(self.docker).args(["rmi", "-f", &self.reference]).output();
    }
}

/// Build `image` as the project image of a fresh project and probe it.
fn build_and_probe(docker: &str, dockerfile: &str, image: &Image) -> ProbeVerdict {
    let _remove = RemoveImage { docker, reference: format!("{}:test", image.name) };
    let harness = harness_for("claude_code");
    let project = tempfile::tempdir().unwrap();
    let root = crate::fs::RootFs::for_root(project.path());
    write_contexts(project.path(), dockerfile);
    store_settings(&harness, &root, docker, image);
    build(&harness, &root, image);
    probe(&harness, &root)
}

fn agent_dockerfile() -> String {
    std::fs::read_to_string(repository_root().join(AGENT_DOCKERFILE)).expect("the agent Dockerfile")
}

// OAT-FR-NVHI, OAT-FR-TJAV, OFP-FR-BZDP, OFP-FR-XBNJ, AVI-FR-RZFR: the agent
// image of the repository holds a usable OpenSpec CLI, at the pinned version.
#[test]
#[ignore = "needs a Docker daemon and the base image of the agent Dockerfile"]
fn openspec_uat_image_agent_image_holds_the_pinned_cli() {
    let docker = docker_ready();
    let dockerfile = agent_dockerfile();
    let pinned = pinned_version(&dockerfile);
    let verdict = build_and_probe(&docker, &dockerfile, &AGENT);
    assert_eq!(verdict, ProbeVerdict::Usable(pinned), "the probe of {}", AGENT.name);
}

// OAT-FR-ISHM, OFP-FR-XBNJ: an image with no `openspec` is `not_found`, and
// one whose `openspec` reports a version below 1 is `unsupported`.
#[test]
#[ignore = "needs a Docker daemon and the base image of the agent Dockerfile"]
fn openspec_uat_image_probe_failures_against_real_containers() {
    let docker = docker_ready();
    let dockerfile = agent_dockerfile();
    let missing = build_and_probe(&docker, &dockerfile, &NO_CLI);
    assert_eq!(missing, ProbeVerdict::Unusable("not_found"), "the probe of {}", NO_CLI.name);
    let old = build_and_probe(&docker, &dockerfile, &OLD_CLI);
    assert_eq!(old, ProbeVerdict::Unusable("unsupported"), "the probe of {}", OLD_CLI.name);
}
