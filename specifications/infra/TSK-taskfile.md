# Taskfile

**Spec code:** `TSK`

## Intent
The repository's single list of the commands a person or an agent runs by hand. One `Taskfile.yaml` at the repository root names them, and `task <name>` runs each one with the working directory, the dependencies, and the failure behaviour written down rather than remembered. It exists because the commands that matter — bundle the desktop application for this machine, build every container image the repository defines, run every verification lane before opening a pull request — differ per host, per directory, and per image, and a developer or an agent that assembles them by hand assembles them differently each time. The task file holds the command; the specification that owns each command holds its meaning, and the task file restates neither. Out of scope: cross-compilation of the desktop application, which no task performs and which a later specification adds if it is ever wanted; publication of any artifact, which stays with the deliberate steps `AVI-agent-vendor-images.md` AVI-FR-09 and the release workflow of `CIP-ci-pipeline.md` CIP-FR-24 define; and the pull-request pipeline itself, which runs its lanes from `.github/workflows/ci.yml` and reads no task file.

## User stories
- As a developer, I want to run `task build` so that the desktop application is bundled for the machine I am on without me choosing the command for my operating system.
- As a developer, I want to run `task docker` so that every image the repository defines is built locally under one version tag, with nothing pushed anywhere.
- As a developer, I want to run `task tests` so that I run what the pipeline runs before I open a pull request.
- As an AI agent, I want each command to have one name, one working directory, and one failure behaviour so that I run the same command a developer runs and read the same result.

## Contract surface
This specification owns the task file, the task names, the command each task runs, the working directory each command runs in, the host detection of `build`, the version resolution and local image names of `docker`, and the documentation of all of them. It registers no Tauri command, exposes no operation, and is reachable from no running code path.

### The file
```text
Taskfile.yaml         the repository's only task file, at the repository root
docs/development.md   the documentation of every task
```

### The tasks
```text
task build           the platform task that matches the host, and no other
task build:macos     pnpm tauri build --bundles app,     repository root, macOS host
                     then the linkage check
task build:linux     pnpm tauri build                    repository root, Linux host
task build:windows   pnpm tauri build                    repository root, Windows host
task docker          the three local image builds        repository root
task specs           the specification consistency check  repository root
task tests           every verification lane             each lane's own directory
task tests:backend   cargo check, then cargo test        src-tauri/
```

### The local images `task docker` builds
```text
<repository portion of manifest image_ref>:<version>   docker/agent-images/claude-code/Dockerfile
                                                       context docker/agent-images/claude-code
<repository portion of manifest image_ref>:<version>   docker/agent-images/codex/Dockerfile
                                                       context docker/agent-images/codex
synthesis-server:<version>                             server/Dockerfile
                                                       context server/
                                                       build arg SYNTHESIS_BUILD_VERSION=<version>
```

### The local build version
```text
1. the exact Git tag at HEAD, when HEAD carries one
2. the short Git commit hash, when Git metadata is readable and no tag points at HEAD
3. the literal string `undefined`
```

One value is resolved once for each invocation of `task docker`, and it is the tag of every image that invocation builds.

## Functional requirements
1. **TSK-FR-01** One `Taskfile.yaml` at the repository root is the repository's only task file. It defines exactly eight tasks — `build`, `build:macos`, `build:linux`, `build:windows`, `docker`, `specs`, `tests`, and `tests:backend` — and no other task, so `task --list` shows that set and nothing more.
2. **TSK-FR-02** Every task names the directory its commands run in. A command that must run outside the repository root has that directory declared in the task rather than reached by a directory change inside a command line, so the task gives the same result whatever directory `task` was started from.
3. **TSK-FR-03** Every dependency between tasks is declared in the task that has it. No task depends on another having been run before it unless it declares that dependency, and no task depends on a file a previous invocation left behind.
4. **TSK-FR-04** Every task runs without a prompt, reads no interactive input, and reports its outcome as its exit status: zero when every command it ran succeeded, non-zero otherwise. A User and an AI agent therefore run the same command and read the same result.
5. **TSK-FR-05** `build` reads the operating system of the host it runs on and runs exactly one platform task: `build:macos` on macOS, `build:linux` on Linux, `build:windows` on Windows. It runs no second platform task, and it holds no build command of its own.
6. **TSK-FR-06** `build` on a host that is none of the three fails with a non-zero exit status and one diagnostic that names the operating system it detected and the three supported hosts. It runs no platform task, builds nothing, and cross-compiles nothing.
7. **TSK-FR-07** `build:macos` runs `pnpm tauri build --bundles app` and then the linkage check of `NLL-native-library-linkage.md` NLL-FR-NPMB on the bundle's main executable, both with the repository root as their working directory. The `app` bundle target is Tauri's supported macOS `.app` bundle, and the task's output is that bundle.
8. **TSK-FR-08** `build:linux` runs `pnpm tauri build` with the repository root as its working directory. The command names no bundle target, so the output is Tauri's default Linux bundle selection.
9. **TSK-FR-09** `build:windows` runs `pnpm tauri build` with the repository root as its working directory. The command names no bundle target, so the output is Tauri's default Windows bundle selection.
10. **TSK-FR-10** The three platform tasks are native-host tasks. Each builds for the host it runs on, names no target triple, and configures no cross-compilation toolchain. A platform task that is started directly on a host that does not support its target fails with a non-zero exit status and one diagnostic that names the task, the host it found, and the host it needs. It does not build another platform's bundle instead.
11. **TSK-FR-11** A platform task whose prerequisite is unavailable — the Rust toolchain `src-tauri/rust-toolchain.toml` pins, Node.js, pnpm, the Tauri CLI, the host's operating-system SDK, the host's compiler or linker, or the tooling a selected bundle target needs — fails with a non-zero exit status and one diagnostic that names the prerequisite that is missing. It produces no partial bundle and reports no success.
12. **TSK-FR-12** No platform build task publishes or deploys anything. None pushes an artifact to a registry or a store, uploads a bundle, creates or edits a release, writes a version into a manifest, or changes any other release metadata. The bundle each produces stays in the build directory of the machine that ran the task.
13. **TSK-FR-13** `docker` builds every locally buildable image the repository defines and no other image: the Claude Code vendor image, the Codex vendor image, and the server image. Each build uses the Dockerfile and the build context its owning specification defines — `docker/agent-images/<vendor>/Dockerfile` with `docker/agent-images/<vendor>` as its context (per `AVI-agent-vendor-images.md` AVI-FR-01), and `server/Dockerfile` with `server/` as its context (per `../server/BMS-backend-microservice.md` BMS-FR-17).
14. **TSK-FR-14** Before the first image build, `docker` resolves one local build version, using the Git sources of `../server/BMS-backend-microservice.md` BMS-FR-14 in that requirement's order: the exact Git tag at `HEAD` when `HEAD` carries one; otherwise the short Git commit hash when Git metadata is readable; otherwise the literal string `undefined`.
15. **TSK-FR-15** A Git command that fails, or a Git executable that is absent, does not fail `docker`. The task falls to the next source in the order of TSK-FR-14, so a checkout with no Git metadata at all resolves `undefined` and the task continues.
16. **TSK-FR-16** The version is resolved once for each invocation and is the only version that invocation uses. The task adds no suffix for an unclean working tree, no timestamp, no branch name, and no other value that the resolution of TSK-FR-14 did not produce.
17. **TSK-FR-17** Every image `docker` builds carries the resolved version as its tag, and no image it builds carries any other tag. A vendor image keeps the repository portion of its `image_ref` in `docker/agent-images/manifest.toml` and takes the resolved version in place of the tag that reference carries. The server image uses the local image name `synthesis-server` that `../server/BMS-backend-microservice.md` BMS-FR-22 documents, with the resolved version as its tag.
18. **TSK-FR-18** The local tags are deterministic: the same resolved version produces the same tag for the same image on every machine. No image is tagged `latest`. When the resolved version is `undefined`, the tag is the literal string `undefined`, which is a valid tag and which reads as the same "no version was resolvable" that `GET /v1/health` reports from an image built that way.
19. **TSK-FR-19** `docker` passes the resolved version as the build argument `SYNTHESIS_BUILD_VERSION` to every image build whose owning specification needs that argument. The server image receives it, so the container it produces reports the resolved version (per `../server/BMS-backend-microservice.md` BMS-FR-14). The two vendor images take no version build argument and receive none: each pins its vendor CLI and its browser in its own Dockerfile (per `AVI-agent-vendor-images.md` AVI-FR-06, AVI-FR-16).
20. **TSK-FR-20** `docker` publishes nothing. It pushes no image, logs in to no registry, reads no registry credential and no repository secret, writes no digest or reference into `docker/agent-images/manifest.toml`, and changes no release metadata. Every image it produces exists in the local image store of the machine that ran it and nowhere else.
21. **TSK-FR-21** A locally built vendor image does not become the image the application runs. The executor pins by the digest `docker/agent-images/manifest.toml` records (per `AVI-agent-vendor-images.md` AVI-FR-08), and `docker` leaves that manifest untouched, so a local build is never silently adopted by a running application.
22. **TSK-FR-22** `docker` builds each image for the platform of the host it runs on and produces no multi-platform manifest list. Publication of the server image for both supported platforms stays with the release workflow (per `../server/BMS-backend-microservice.md` BMS-FR-21 and `CIP-ci-pipeline.md` CIP-FR-27).
23. **TSK-FR-23** A failure inside `docker` fails the task with a non-zero exit status and names the image that failed. An image build that exits non-zero stops the task, and an absent container runtime is reported as one diagnostic that names it rather than as a partially built set reported as success.
24. **TSK-FR-QVXD** `specs` runs the specification consistency check — `python3 tools/spec-check/spec_check.py` (per `SPC-specification-consistency.md` SPC-FR-NUAB) — with the repository root as its working directory. It needs neither Node nor a Rust toolchain and depends on no other task, so it runs on a bare checkout before anything is installed.
25. **TSK-FR-24** `tests:backend` runs the commands of the application's backend verification lane — `cargo check` and then `cargo test` — with `src-tauri/` as the working directory (per `CIP-ci-pipeline.md` CIP-FR-12). Either command exiting non-zero fails the task at once, and the command after it does not run.
26. **TSK-FR-25** `tests` runs every verification lane `CIP-ci-pipeline.md` defines, with each lane's commands in that specification's order and each lane's working directory: the frontend lanes' `pnpm test` (CIP-FR-VRPM), `pnpm build`, and `pnpm exec tsc -p tsconfig.test.json --noEmit` (CIP-FR-07) from the repository root; the application backend lane (CIP-FR-12); the agentic CLI mock lane's `cargo check` and `cargo test` from `tools/agentic-cli-mock/` (CIP-FR-19); the server lane's `cargo build --locked` and `cargo test --locked` from `server/` (CIP-FR-21); and the specification lane, which it runs by depending on `specs` rather than by restating its command (CIP-FR-HZKA). It adds no command of its own and changes none of theirs.
27. **TSK-FR-26** `tests` runs the application backend lane by depending on `tests:backend` rather than by restating that lane's commands, so the two can never name different commands or different directories.
28. **TSK-FR-27** `tests` fails with a non-zero exit status when any lane it ran failed, and its output names the lane that failed. It reports no GitHub check and blocks no merge: `CI / gate` stays the only required check (per `CIP-ci-pipeline.md` CIP-FR-15). Because the commands are the lanes' own, a green `task tests` predicts a green pipeline run for the same working tree.
29. **TSK-FR-28** `docs/development.md` documents the eight tasks: for each one its name, the exact command it runs, its working directory, its output, the host it is supported on, its prerequisites, and how it fails. It documents the platform-aware dispatch of `build`, the native-host rule of TSK-FR-10, and the operating-system SDK, Rust toolchain, Node and pnpm, Tauri, compiler and linker, and signing or bundling conditions each platform task needs. It also documents the local image names of TSK-FR-17, the one shared version tag, the resolution order of TSK-FR-14, the `SYNTHESIS_BUILD_VERSION` build argument, and the rule that `docker` publishes nothing and changes no manifest and no release metadata. The root `README.md` names the commands that build and test the repository from source, and links to `docs/development.md` for this reference.
30. **TSK-FR-29** `server/README.md` documents the local build of the server image through `docker` (per `../server/BMS-backend-microservice.md` BMS-FR-22), and `docker/agent-images/README.md` documents the local build of both vendor images through it (per `AVI-agent-vendor-images.md` AVI-FR-14). Each states its own image's local name and tag, and each states that this build publishes nothing.
31. **TSK-FR-30** Nothing outside the task file depends on it. No code under `src/**`, `src-tauri/**`, or `server/**` starts `task` or reads `Taskfile.yaml`, and no workflow step of `CIP-ci-pipeline.md` invokes a task, so the pipeline's lanes run the commands that specification names whether or not a task runner is installed on a runner.

## Non-functional requirements
- The task file installs nothing and provisions nothing. It runs no package manager to obtain a toolchain, an SDK, or a container runtime; a prerequisite that is missing is reported, not repaired.
- Every command in the task file is byte-identical to the command its owning specification names. A command that changes there changes here in the same change, so the two can never drift.
- The task file holds no credential, no token, and no registry login, and it reads no repository secret.
- A task is repeatable: running it twice on an unchanged working tree produces the same artifacts and the same result, apart from the wall-clock time a warm build saves.
- The task file is the convenience layer over commands that stay runnable by hand. Every command it holds can be typed directly in the working directory the task names, with the same outcome.
