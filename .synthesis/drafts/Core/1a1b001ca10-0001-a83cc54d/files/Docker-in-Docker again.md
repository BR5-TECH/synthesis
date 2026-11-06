## Intent

Docker-in-Docker feature seems to be not functioning properly. We’re to improve test coverage and implementation of both modes of docker-in-docker.

User must be able to use both isolated and non-isolated Docker-in-Docker.

This draft is a one-time, agent-driven acceptance test (UAT). The agent builds a test image and a small test application, runs them through the production launch path in each mode, and uses the failures to find and fix the gaps in the implementation. The fixes are in scope. Known symptom: with isolation **None** and Docker-in-Docker enabled, Claude Code inside the agent container reports that no Docker is available.

The three launch shapes (per `specifications/tools/EAC-execute-agent-cli.md` EAC-FR-SNMS):

1. Isolation None, Docker-in-Docker enabled: Docker launch with the host socket.
2. Isolation Microsandbox, `under_microsandbox = host_socket`: Docker launch with the host socket.
3. Isolation Microsandbox, `under_microsandbox = vm_daemon`: launch in the micro VM with a Docker daemon of its own.

## User journey

- Developer runs one opt-in local task. For each of the three launch shapes, the task starts the production executor launch with real Docker, in a container or VM of the test image, with a scripted command in place of the vendor CLI.
- The scripted command runs the test application inside the container or VM. The application starts a Postgres container, connects to it, and runs a query.
- Developer reads one result for each shape: pass, or fail with the failing step and the cause.
- WHEN a shape fails, the agent finds the root cause, fixes the implementation, and runs the task again until all three shapes pass.

## Requirements

- **Test image.** A dedicated Dockerfile for tests, outside `docker/agent-images/`, is built locally and configured as the project's own image (per `specifications/core/PSS-project-settings-storage.md` PSS-FR-26). It is not a shipped vendor image and `specifications/infra/AVI-agent-vendor-images.md` AVI-FR-12 does not change. The image holds `dockerd`, the Docker CLI client, Playwright with a headless browser, a Rust toolchain, and a non-root user as the executor needs. Its entrypoint runs the command it receives.
- **Test application.** A small Rust application, used only by the tests, holds a Testcontainers-backed test that starts `postgres`, connects, and runs `SELECT 1`. The Postgres image is pulled from the public registry at test time. In VM mode the pull counts against the VM disk (per `specifications/ui/GLS-global-settings.md` GLS-FR-WMKD).
- **Per shape, from inside the container or VM, all of these must pass:** `docker version` reports both client and server; `docker run` of a small image succeeds; the Testcontainers test passes; the headless browser starts and renders a page (browser check).
- **Shapes 1 and 2:** the socket is at `/var/run/docker.sock` in the container whatever the host path is, the non-root user can use it through the socket group, and with the host gateway toggle on the test reaches the Postgres container through `host.docker.internal` (per EAC-FR-DKRS). No privileged mode and no added capability is allowed.
- **Shape 3:** the VM starts its own daemon from the `dockerd` of the image and mounts no host Docker socket (per EAC-FR-BJLI). Shape 3 needs KVM; the task reports a clear skip reason where KVM is missing.
- **Settings reach the executor.** Each shape is set through the stored project and global settings, read by the application as in production (per `specifications/core/PSS-project-settings-storage.md` PSS-FR-DIND and `specifications/core/GSS-global-settings-storage.md` isolation settings), not through test-only switches.
- **Root cause of the known symptom.** The agent first reproduces shape 1 and states the cause. Candidates: the Docker CLI is missing from the image; the socket is not mounted; the settings are not passed to the executor; the socket group gives no access; the Docker endpoint is not found by the CLI. Where the application is at fault, the agent fixes it. Where the project image is at fault, the Project settings text must say that the image needs the Docker CLI.
- **Delivery.** The agent leaves the Dockerfile, the test application, the tests and the opt-in task in the repository. The tests are not part of the default test task and not part of CI. They need a running Docker daemon.
- **Real behaviour.** The tests mock no Docker call and no executor step. The vendor CLI is the only part replaced, by the scripted command.

## Specifications to change

Change only the specifications that a fix or a new test touches, and keep each consistent with the code:

- `specifications/tools/EAC-execute-agent-cli.md`: EAC-FR-DKRS, EAC-FR-SNMS, EAC-FR-HWQN, and EAC-FR-DARS where a fix changes them.
- `specifications/core/PSS-project-settings-storage.md`: PSS-FR-DIND where the stored settings or their reading change.
- `specifications/ui/SET-project-settings.md`: SET-FR-NXDV, SET-FR-JMHE, SET-FR-PMHB where the text about the socket or the image changes.
- `specifications/ui/GLS-global-settings.md`: GLS-FR-WAGX where the text about the VM daemon changes.
- `specifications/infra/TSK-taskfile.md`: the opt-in task that runs the tests.