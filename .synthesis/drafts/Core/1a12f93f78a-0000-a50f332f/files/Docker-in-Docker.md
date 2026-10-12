## Intent

A graduation run on some projects must start Docker containers from inside the agent container. Example: tests that start Postgres or Temporal with Testcontainers. The author can give the agent container access to the host Docker daemon through its socket, so the agent starts sibling containers on the host daemon. The feature uses no privileged mode and adds no Linux capability. It gives the least privilege that lets Testcontainers work.

## User journey

- User opens Project settings, Docker section. Above the vendor tabs, the user sees one **Docker-in-Docker** checkbox. It applies to the whole project, not to one vendor tab.
- User selects the checkbox. The setting is saved at once (no dirty state), as the other Docker section fields are.
- IF enabled: the section shows a **Docker socket path** text field and a **Host gateway** toggle. The field placeholder is the platform default (for example `/var/run/docker.sock`). An empty field means the default. The toggle is off by default.
- On Windows the checkbox is disabled and states the reason (a named pipe cannot be mounted). On macOS the checkbox works, and the section states that access is best effort.
- WHEN user starts a graduation run with Docker-in-Docker enabled: the start checks that the socket exists. If it does not, the start is refused, no run is created, and the user sees an Error toast and an inline message naming the path.
- WHEN the check passes, the agent container starts with the socket mounted and the socket's group added. Tests in the container can start Docker images (Testcontainers) on the host daemon.
- IF the socket disappears after the check and the container launch fails: the run rests like a run with a missing image (today `interrupted` with `launch_failed`, with Continue offered). The user sees an Error toast. The user restores the socket and uses Continue. 

## Requirements

- Docker-in-Docker is a project-level option, disabled by default. The checkbox is shown once, at the top of the Docker section, above the vendor tabs.
- Three values are stored: the enabled flag, the socket path (empty = platform default), and the host gateway toggle. They live in a new section of the existing project-local store (`.synthesis/local.toml`, gitignored). They are never written to `.synthesis/project.toml` and never committed. New load and save commands follow the pattern of the panel-state commands (independent of the settings-tab section save model). Because the file is per worktree, the values are per worktree.
- The socket path is a Unix socket path on the host. It is independent of the Docker endpoint in Global settings. The container always sees the socket at `/var/run/docker.sock`.
- Platforms: Linux and macOS are supported (macOS is best effort: the socket GID may not give access, and then the user sees the error of this spec). Windows is not supported: the option is disabled with a reason.
- When enabled, the agent container is created with exactly these additions: the socket bind mount, the socket file's group added (`--group-add <GID>`, GID read from the socket at each launch), and, if the Host gateway toggle is on, the mapping `host.docker.internal:host-gateway` plus the environment variable `TESTCONTAINERS_HOST_OVERRIDE=host.docker.internal`. Nothing else changes: the user stays the non-root host UID, no privileged mode, no added capability, and the existing capability drop and no-new-privileges settings stay. When disabled, the launch is identical to today. Both Docker backends (Docker CLI and Bollard) produce the same container.
- The model never chooses these values. A task field, an instruction, or an agent output cannot add the mount, the group, or the host mapping.
- Preflight: at graduation start (stream start and direct start), when enabled, the start checks that the path exists and is a Unix socket. If not, it is refused with a new typed refusal (for example `docker_socket_unavailable`) that names the path. No run, queue entry, or draft lock is created. The start dialog shows an inline message, and an Error-level notification raises a toast. The same check applies to the preflight of a merge that creates a run.
- Launch check: if the socket is missing when the container is launched, the launch fails with a short detail naming the path, the run rests as a missing image does (`launch_failed`, Continue offered), and an Error-level notification raises a toast. Continue uses the current settings and socket.
- All user-visible text for this feature is clear and names the path and the action the user must take.

## Specifications to change

- `specifications/tools/EAC-execute-agent-cli.md`: EAC-FR-13 (no socket mount, no extra mount; add the Docker-in-Docker exception, its group and host mapping), EAC-FR-10 (new named constants and descriptor fields), EAC-FR-14 (add the supplementary group), EAC-FR-39 (same container under both backends), and the launch environment rules (the Testcontainers variable).
- `specifications/core/GSU-graduation-start.md`: GSU-FR-GLVQ, GSU-FR-SQRC (a fifth refusal), GSU-FR-HIPF (the preflight now reads the filesystem; it still contacts no daemon). Also `specifications/core/WKS-work-streams.md` WKS-FR-BPGM and `specifications/ui/WSS-work-stream-selector.md` WSS-FR-NPXC (refusal lists), and the start dialog spec `GSD-graduation-start-dialog.md`.
- `specifications/core/GXD-graduation-execution.md` GXD-FR-UZHX and the matching coverage row in `specifications/infra/GTE-graduation-end-to-end-tests.md`.
- `specifications/core/PSS-project-settings-storage.md`: a new project-local section and its load and save commands (PSS-FR-03, PSS-FR-05). `specifications/core/GSS-global-settings-storage.md` is not changed.
- `specifications/ui/SET-project-settings.md`: SET-FR-21 (the checkbox, path field, and toggle above the vendor tabs; immediate save; Windows and macOS states).
- `specifications/ui/NTF-notifications.md`: use the existing Error-level raise; no change expected.