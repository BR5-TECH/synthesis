# Dev container environment

**Spec code:** `DVC`

## Intent
A project can name a dev container — a `devcontainer.json` file — as the environment its agentic CLI work runs in, in place of one Docker image for each vendor. The author defines the whole environment once, with its base image, Features, compose services and lifecycle commands, and the agents run inside it as the author's own editor would. Whether the agents use the dev container is each developer's own choice and is never committed; the `devcontainer.json` it names is an ordinary project file. The application builds the dev container when the author asks, starts it once for each graduation queue, and runs each agent turn in it. The Dev Containers CLI (`@devcontainers/cli`) on the host builds and starts the container; the application reads `devcontainer.json` only for what this spec names. Out of scope: Microsandbox isolation of a dev container, the Docker-in-Docker settings of `PSS-project-settings-storage.md` PSS-FR-DIND for a dev container, and conversations and draft discussions, which run no agentic CLI.

## Functional requirements

### Project agent environment
1. **DVC-FR-EVZD** The project-local store holds the **agent environment** under the `agent_environment` section of `.synthesis/local.toml`: `source`, one of `vendor_images` and `devcontainer`, and `devcontainer_config`, a project-relative path to a `devcontainer.json` file. `source` defaults to `vendor_images`. It is never written to `.synthesis/project.toml`. A launch reads it from the active worktree (per `PSS-project-settings-storage.md` PSS-FR-03).
   - *Why:* the agent environment is a preference of each developer, so one developer can use a dev container while a teammate on the same project uses the vendor images.
2. **DVC-FR-WXYK** `save_agent_environment` refuses a `devcontainer_config` that is absolute, that leaves the project, or that names no file in the project root checkout, with `config_path_invalid` naming which of the three it is. It refuses `source = devcontainer` with no `devcontainer_config` with `config_missing`. A refused write stores nothing.
3. **DVC-FR-BGVU** `list_devcontainer_configs` returns the project-relative paths of the dev container configs in the project root checkout, in this order: `.devcontainer/devcontainer.json`, `.devcontainer.json`, then each `.devcontainer/<folder>/devcontainer.json` sorted by folder name. It reads no folder deeper than one level under `.devcontainer`, and it follows no symbolic link out of the project.
4. **DVC-FR-CHZS** The application reads the dev container config, and every file the config names by a relative path, from the project root checkout. It never reads them from the worktree of a run. A change that an agent makes in its worktree to a `devcontainer.json`, a Dockerfile or a compose file has no effect on a container the application creates.
   - *Why:* the config sets the mounts, the run arguments and the privileges of a container on the host, so an agent must not be able to change them for its next turn.
5. **DVC-FR-YZLH** Where the project's agent environment is `devcontainer`, every agentic CLI launch of the project runs in a dev container (per `../tools/EAC-execute-agent-cli.md` EAC-FR-GOAR). The project's vendor images, its Docker-in-Docker settings and the user-global isolation do not apply to that launch.

### Dev container CLI
6. **DVC-FR-WOAM** The user-global store holds the **dev container CLI configuration**: the path of the `devcontainer` executable, and its verification state, `unverified` or `verified` with the version the executable reported and the time of the verification. It is never written into a project file (per `GSS-global-settings-storage.md` GSS-FR-MVLQ). An empty path means the path that `detect_devcontainer_cli` finds.
7. **DVC-FR-BAID** `verify_devcontainer_cli` runs the executable with `--version` and a time limit of 15 seconds, and stores `verified` with the version it prints. A missing file, a file that is not executable, a run that fails, and a run over the time limit each return their own failure: `not_found`, `not_executable`, `failed`, `timed_out`. Each failure stores `unverified`.
8. **DVC-FR-XZCD** `detect_devcontainer_cli` searches the user's login `PATH` and the usual install folders of npm global packages for `devcontainer`, and returns the first executable it finds, or `not_found`. It stores nothing. `save_devcontainer_cli` with a changed path stores `unverified` and runs nothing.
9. **DVC-FR-GNOK** Every run of the dev container CLI passes `--docker-path` with the Docker CLI executable: the Docker CLI path of the Docker backend (per `GSS-global-settings-storage.md` GSS-FR-35) where it is set, otherwise the executable that `detect_docker_cli_binary` finds. This is so in Bollard / Docker Engine mode too. Where the application finds no Docker CLI, the operation fails with `docker_cli_required`.
   - *Why:* the dev container CLI and the turns of DVC-FR-DCQG drive Docker only through its CLI, whatever mode the Docker backend uses.

### Agent CLI in the dev container
10. **DVC-FR-WZSY** `load_agent_environment` returns an **agent CLI state** for `claude_code` and for `codex`. It is `found` where the config's `features` holds the vendor's known Feature (DVC-FR-SEXK) or the last build's probe found the CLI (DVC-FR-EKAO), `missing` where that probe did not find it, and `unknown` otherwise. A probe result wins over the Feature list.
11. **DVC-FR-PFKJ** A probe result counts only while the config file has the hash that it had at the build that probed. A change of the config file, or of the selected config, makes the state of every vendor come from the Feature list again.
12. **DVC-FR-SEXK** The known Feature of Claude Code is `ghcr.io/anthropics/devcontainer-features/claude-code`. Codex has no known Feature. A key of `features` matches a known Feature where it names the same reference with any version tag or with none.
13. **DVC-FR-ABQZ** `add_devcontainer_agent_feature(vendor)` adds the vendor's known Feature, at major version `1` with empty options, to the `features` object of the selected config. Where `features` holds no `ghcr.io/devcontainers/features/node` Feature, it adds that one too, at major version `1`. It keeps every comment, every other key and the formatting of the rest of the file.
    - *Why:* the Claude Code Feature installs the CLI through npm, and it installs after the Node Feature.
14. **DVC-FR-JVKA** `add_devcontainer_agent_feature` refuses a vendor with no known Feature with `no_known_feature`. It refuses a config that is not valid JSON with comments, or whose top level is not an object, with `config_invalid`, with the parser's message and line. A refused call, and a call for a config that holds the Feature already, leave the file unchanged.
15. **DVC-FR-BCDZ** `create_devcontainer_config` writes `.devcontainer/devcontainer.json` in the project root checkout, with a name, the image `mcr.microsoft.com/devcontainers/base:bookworm`, the Node Feature and the Claude Code Feature. Then it stores that file as the `devcontainer_config`, and `source` as `devcontainer`, in the project-local store. It refuses with `config_exists` where the file exists, and it overwrites nothing.

### Build
16. **DVC-FR-OXCZ** `build_devcontainer(no_cache)` builds the selected config with `devcontainer build`, with the project root as the workspace folder and the config as `--config`, and with `--no-cache` where `no_cache` is true. It returns `{ operation_id }`. The build is local and pushes nothing to a registry. A second build of the same project while one runs is refused with `build_in_flight`.
17. **DVC-FR-RTBA** Each line that the CLI writes during a build is emitted as one `"devcontainer build progress"` event. The build ends with one `"devcontainer build finished"` event, with the outcome `succeeded` and the built image reference, `failed` and the last error line of the CLI, or `cancelled`. `cancel_devcontainer_build` stops the CLI and every process it started.
18. **DVC-FR-EKAO** After a build that succeeds, the application probes the built image once for each vendor that can run. A probe runs `command -v <cli>` (`claude`, `codex`) in a login shell, in a container with network `none`, and removes that container. It stores `found` or `missing` with the hash of the config file. A probe that cannot run stores nothing.
19. **DVC-FR-LFQQ** `load_devcontainer_build_in_flight` returns the build that runs for the project, with its operation id and the lines emitted so far, or `null`. A Settings window that opens during a build shows that build.

### Start, turns and stop
20. **DVC-FR-LKRY** A launch with a sandbox scope (per `../tools/EAC-execute-agent-cli.md` EAC-FR-QYBJ) runs in the **dev container of its scope**. The first launch of the scope starts it with `devcontainer up`, and every later launch of the scope uses it. Two scopes never share a container.
21. **DVC-FR-QGUT** `devcontainer up` receives the worktree as `--workspace-folder`, the config of DVC-FR-CHZS as `--config`, the id labels `synthesis.scope=<scope key>` and `synthesis.project=<project key>`, `--mount-workspace-git-root=false`, and a generated config as `--override-config`. The generated config is the user's config with `workspaceMount` and `workspaceFolder` set to the worktree's host path, and with the launch's mounts added to `mounts`.
    - *Why:* the `.git` file of a worktree names its git folder by its absolute host path, so the worktree must have that same path in the container.
22. **DVC-FR-IFBK** The launch's mounts are the mounts of a Docker launch (per `../tools/EAC-execute-agent-cli.md` EAC-FR-13): the session state folder, the Codex credential folder and the git metadata, with the same targets and the same read-only flags. `CLAUDE_CONFIG_DIR` and `CODEX_HOME` name their mount targets. The application writes the generated config under its data folder, never into the project.
23. **DVC-FR-BUXC** The application reads `containerId`, `remoteUser` and `remoteWorkspaceFolder` from the JSON result of `devcontainer up`. Then one `devcontainer exec`, with the same config arguments, prints the environment of the remote user, and the application keeps it as the **probed environment** of the container. A result that is not `success`, or has no container id, is a start failure (DVC-FR-ATEO).
24. **DVC-FR-DCQG** Each turn runs as one `docker exec -i -u <remoteUser> -w <worktree path> --env-file <file> <container> <cli> <vendor arguments>`. The env file holds the probed environment and the launch's literal variables. It has mode `0600`, sits in the application's data folder, and goes with its container. A credential passes as `-e <NAME>`, its value only in the Docker CLI's environment.
25. **DVC-FR-SFWW** A turn in a dev container has the stdin, the output limits, the time limit, the cancellation and the final event grace of a Docker launch (per `../tools/EAC-execute-agent-cli.md`). A turn that ends by its time limit, a cancellation or the final event grace ends the CLI process in the container and every process it started. The container stays.
26. **DVC-FR-ODWK** A turn whose exec ends with exit code 127 before the CLI writes any output is a pre-launch error with the cause `agent_cli_missing`, which names the CLI that the dev container does not hold.
27. **DVC-FR-OHMC** A launch with no sandbox scope runs in a dev container of its own. The application starts it with `devcontainer up` on the terms of DVC-FR-QGUT, with a scope key that is unique to the launch and with the launch's mounts, runs the one turn, and removes the container.
28. **DVC-FR-DELL** The generated config and the arguments of `devcontainer up` are the **container spec** of a scope. A launch whose spec is not equal to the spec of the scope's running container removes that container and starts a new one. A change of the user's config file, or of a file it names, is a change of the spec.
29. **DVC-FR-OJEH** The first launch of a scope in an application process removes every container that has the id label of that scope before it runs `devcontainer up`.
    - *Why:* `devcontainer up` reuses a container that has the same id labels, and a container that an earlier process left holds that process's mounts and state.
30. **DVC-FR-DTWX** The application removes the dev container of a scope where `../tools/EAC-execute-agent-cli.md` stops the VM of a scope (EAC-FR-ABHG, EAC-FR-LQRI): when its stream is deleted, when its queue becomes idle, when its project closes, and when the application exits. A dev container never outlives the application process.
31. **DVC-FR-JXEA** The removal of a dev container is `docker rm -f` of every container that has the scope's id label. For a config with `dockerComposeFile`, it is `docker compose down` of the compose project that the CLI created. A removal of a container that is not there is not an error.
32. **DVC-FR-ATEO** A `devcontainer up` that fails, that returns a result of DVC-FR-BUXC that is a failure, or that runs longer than 30 minutes is a pre-launch error with the cause `devcontainer_start_failed`. It carries the last error line of the CLI, cut to one line and at most 300 characters (per `GXD-graduation-execution.md` GXD-FR-ZPYN).
33. **DVC-FR-SKLQ** The OpenSpec image probe of `OFP-openspec-failure-policy.md` OFP-FR-RKMB is skipped for a launch in a dev container, on the terms that requirement states.

### Start preflight
34. **DVC-FR-HINW** Where the agent environment is `devcontainer`, the image preflight (per `GSU-graduation-start.md` GSU-FR-GLVQ) checks, in this order: the config is a file in the project root checkout, it is JSON with comments with an object at the top level, the dev container CLI is verified, a Docker CLI is found, and the resolved vendor's agent CLI state is not `missing`.
35. **DVC-FR-ETQN** The checks of DVC-FR-HINW refuse the start with, in the same order, `devcontainer_config_missing`, `devcontainer_config_invalid`, `devcontainer_cli_missing`, `docker_cli_required` and `devcontainer_agent_cli_missing`. A vendor that cannot run refuses with `vendor_execution_unsupported` first. The preflight runs no process and contacts no daemon (per `GSU-graduation-start.md` GSU-FR-HIPF).

## Contract surface

### Tauri commands

Project Settings, Docker section (per `../ui/SET-project-settings.md`):
- `"load agent environment"` → `load_agent_environment` → `AgentEnvironmentState`
- `"save agent environment"` → `save_agent_environment(environment)` (writes project-local) → `AgentEnvironmentState`, or `AgentEnvironmentError`
- `"list devcontainer configs"` → `list_devcontainer_configs` → `string[]`, project-relative
- `"create devcontainer config"` → `create_devcontainer_config` (writes the project root checkout and project-local) → `AgentEnvironmentState`, or `AgentEnvironmentError`
- `"add devcontainer agent feature (vendor)"` → `add_devcontainer_agent_feature(vendor)` (writes the selected config) → `AgentEnvironmentState`, or `AgentEnvironmentError`
- `"build devcontainer (no cache)"` → `build_devcontainer(no_cache)` → `{ operation_id }`, or `DevcontainerBuildRefusal`
- `"cancel devcontainer build"` → `cancel_devcontainer_build(operation_id)` → the build is asked to stop; the result arrives on `"devcontainer build finished"`
- `"load devcontainer build in flight"` → `load_devcontainer_build_in_flight` → `DevcontainerBuildInFlight | null`

Global Settings, Docker section (per `../ui/GLS-global-settings.md`):
- `"load devcontainer cli"` → `load_devcontainer_cli` → `DevcontainerCli`
- `"save devcontainer cli"` → `save_devcontainer_cli(path)` (writes user-global) → `DevcontainerCli`
- `"verify devcontainer cli"` → `verify_devcontainer_cli` → `DevcontainerCli`, or `DevcontainerCliFailure`
- `"detect devcontainer cli"` → `detect_devcontainer_cli` → `{ path }`, or `not_found`

```
AgentEnvironmentSource = "vendor_images" | "devcontainer"

AgentEnvironment {
  source: AgentEnvironmentSource,
  devcontainer_config: string | null,
}

AgentCliState = "found" | "missing" | "unknown"

AgentEnvironmentState {
  environment: AgentEnvironment,
  config_exists: bool,
  agent_cli: { vendor: "claude_code" | "codex", state: AgentCliState, known_feature: string | null }[],
}

AgentEnvironmentError =
  | { code: "config_path_invalid", reason: "absolute" | "leaves_project" | "not_a_file" }
  | { code: "config_missing" }
  | { code: "config_invalid", message: string, line: number | null }
  | { code: "config_exists" }
  | { code: "no_known_feature" }

DevcontainerBuildRefusal =
  | { code: "build_in_flight" }
  | { code: "devcontainer_cli_missing" }
  | { code: "docker_cli_required" }
  | { code: "devcontainer_config_missing" }

DevcontainerBuildInFlight { operation_id: string, lines: string[] }

DevcontainerCli {
  path: string,
  state: "unverified" | "verified",
  version: string | null,
  verified_at: string | null,
}

DevcontainerCliFailure = { code: "not_found" | "not_executable" | "failed" | "timed_out", message: string }
```

### Events (Tauri event bus)
- `"devcontainer build progress"` — emitted for each line of a running build. Payload: `{ operation_id, message }`.
- `"devcontainer build finished"` — emitted once when a build ends. Payload: `{ operation_id, outcome: "succeeded" | "failed" | "cancelled", image: string | null, error: string | null }`.

### Internal (Rust API, not registered as a Tauri command)
- `resolve_agent_environment(project_root)` → `AgentEnvironment`. The single read path of a launch and of the image preflight.
- `DevcontainerRuntime` — the seam a launch uses: `up(spec)` → `{ container_id, remote_user, remote_workspace_folder, probed_env }`, `exec(container, process)` → the outcome of a Docker launch, and `remove(scope)`. Tests replace it.

## Non-functional requirements
- The application logs the start, the result and the duration of each `devcontainer build`, `devcontainer up`, turn exec and removal, with the scope key and the container id. It never logs a credential value, the probed environment or the content of the config.
- Each turn in a running dev container starts in less time than a Docker launch of the same turn, because it starts no container.
- The dev container CLI is required only for a project whose agent environment is `devcontainer`. A machine without it runs every project whose agent environment is `vendor_images`.
