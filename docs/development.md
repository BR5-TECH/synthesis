# Development

This document is the reference for the commands that build, test, and check the
repository. The project overview is in the root [`README.md`](../README.md). The
architecture is in [`docs/README.md`](README.md).

A specification owns the meaning of every command below. This document states
the command and where it runs, and names the specification that owns it.

## Tasks

Every command a person or an agent runs by hand is named in the repository's
only task file, [`Taskfile.yaml`](../Taskfile.yaml), at the repository root.
Specification:
[`specifications/infra/TSK-taskfile.md`](../specifications/infra/TSK-taskfile.md).

Install the [Task](https://taskfile.dev) runner, then run `task --list` to see
the eight tasks:

```bash
task build            # bundle the desktop application for this host
task build:macos      # bundle it on a macOS host
task build:linux      # bundle it on a Linux host
task build:windows    # bundle it on a Windows host
task docker           # build every container image locally, under one tag
task specs            # check the specification corpus for broken references
task tests            # run every verification lane the pipeline runs
task tests:backend    # run the application backend lane alone
```

There are eight tasks and no others. Every task names the directory its commands
run in, so a task gives the same result whatever directory `task` was started
from. Every task runs without a prompt, reads no interactive input, and reports
its outcome as its exit status: zero when every command it ran succeeded,
non-zero otherwise. The non-zero status is the task runner's own, not the status
of the command that failed, so read the diagnostic rather than the number. No
task installs a toolchain, an SDK, or a container runtime: a prerequisite that
is missing is reported, not repaired.

Nothing outside the task file depends on it. No application code starts `task`,
and the pull-request pipeline runs its lanes directly from
[`.github/workflows/ci.yml`](../.github/workflows/ci.yml), so the pipeline needs no
task runner on a runner.

### `task build`

| Property | Value |
| --- | --- |
| Command | none of its own. It runs exactly one platform task |
| Working directory | the repository root |
| Output | the output of the platform task it ran |
| Supported hosts | macOS, Linux, Windows |
| Prerequisites | those of the platform task it ran |
| Failure | non-zero, with one diagnostic naming the detected operating system and the three supported hosts |

`build` reads the operating system of the host and dispatches to exactly one
platform task: `build:macos` on macOS, `build:linux` on Linux, `build:windows`
on Windows. It runs no second platform task and holds no build command of its
own.

On a host that is none of the three, `build` exits non-zero with one diagnostic
that names the operating system it detected and the three supported hosts. It
runs no platform task, builds nothing, and cross-compiles nothing.

### The three platform tasks

| Task | Command | Working directory | Supported host | Output |
| --- | --- | --- | --- | --- |
| `build:macos` | `pnpm tauri build --bundles app` | the repository root | macOS only | the `.app` bundle, under `src-tauri/target/release/bundle/macos/` |
| `build:linux` | `pnpm tauri build` | the repository root | Linux only | Tauri's default Linux bundle selection, under `src-tauri/target/release/bundle/` |
| `build:windows` | `pnpm tauri build` | the repository root | Windows only | Tauri's default Windows bundle selection, under `src-tauri/target/release/bundle/` |

`build:macos` names the `app` bundle target, which is Tauri's supported macOS
`.app` bundle, and that bundle is the task's output. `build:linux` and
`build:windows` name no bundle target, so each produces the default selection
`src-tauri/tauri.conf.json` configures for its host: the Debian package, the RPM
package, and the AppImage on Linux; the MSI installer and the NSIS installer on
Windows.

**These are native-host tasks.** Each builds for the host it runs on, names no
target triple, and configures no cross-compilation toolchain. Cross-compilation
is out of scope, and a later specification adds it if it is ever wanted. A
platform task that is started directly on a host that does not support its
target exits non-zero with one diagnostic that names the task, the host it
found, and the host it needs. It does not build another platform's bundle
instead.

**No platform build task publishes or deploys anything.** None pushes an
artifact to a registry or a store, uploads a bundle, creates or edits a release,
writes a version into a manifest, or changes any other release metadata. The
bundle stays in the build directory of the machine that ran the task.

#### Prerequisites of a platform task

A prerequisite that is missing fails the task with a non-zero exit status and
one diagnostic that names it. The task produces no partial bundle and reports no
success.

The task checks the host and the four prerequisites below before it starts the
build, and names the one that is missing. The operating-system SDK, the
compiler, the linker, and the tooling of a bundle target are named by the build
itself, because only the build knows which of them a selected target reaches
for.

Every platform task needs all of these:

- **Node.js** and **pnpm**, with the dependencies installed by `pnpm install`.
- The **Tauri CLI**, which is the `@tauri-apps/cli` development dependency that
  `pnpm install` obtains. The task resolves it through `pnpm`; it installs
  nothing.
- The **Rust toolchain** that
  [`src-tauri/rust-toolchain.toml`](../src-tauri/rust-toolchain.toml) pins, which
  is channel `1.95` and is the repository's single pin. The dependency tree
  needs rustc 1.95 and edition 2024, so an older toolchain fails.

Each host adds its own:

| Host | Operating-system SDK | Compiler and linker | Bundling and signing |
| --- | --- | --- | --- |
| macOS | the Xcode command line tools, which carry the macOS SDK | `clang` and `ld` from those tools | The `app` target needs no extra tool. The bundle is unsigned unless the environment supplies an Apple signing identity, and this task supplies none and notarizes nothing. |
| Linux | the WebKitGTK 4.1 and GTK 3 development packages, which carry the headers Tauri links against | `cc` and `ld`, plus `pkg-config` to find the packages above | The Debian and RPM targets need the packaging tools of the host distribution, and the AppImage target needs network access on its first run, because the Tauri CLI obtains its tooling then. |
| Windows | the Windows SDK, and the WebView2 runtime at run time | the MSVC build tools, which carry `link.exe` | The MSI target needs the WiX toolset and the NSIS target needs NSIS. The Tauri CLI obtains each on its first run, so that run needs network access. The installers are unsigned unless the environment supplies a signing certificate, and this task supplies none. |

### `task docker`

| Property | Value |
| --- | --- |
| Commands | three `docker build` commands, one per image |
| Working directory | the repository root |
| Output | three images in the local image store of the machine that ran the task |
| Supported hosts | any host with a container runtime |
| Prerequisites | the `docker` executable, and a running container runtime |
| Failure | non-zero, with one diagnostic naming the image that failed, or naming the runtime when none is available |

`docker` builds every locally buildable image the repository defines and no
other image. Each build uses the Dockerfile and the build context its owning
specification defines.

| Image | Dockerfile | Build context | Build argument |
| --- | --- | --- | --- |
| `ghcr.io/br5-tech/synthesis-agent-claude-code:<version>` | `docker/agent-images/claude-code/Dockerfile` | `docker/agent-images/claude-code` | none |
| `ghcr.io/br5-tech/synthesis-agent-codex:<version>` | `docker/agent-images/codex/Dockerfile` | `docker/agent-images/codex` | none |
| `synthesis-server:<version>` | `server/Dockerfile` | `server` | `SYNTHESIS_BUILD_VERSION=<version>` |

A vendor image keeps the repository portion of its `image_ref` in
[`docker/agent-images/manifest.toml`](../docker/agent-images/manifest.toml) and
takes the resolved version in place of the tag that reference carries. The task
reads that reference from the manifest rather than restating it, so the manifest
stays the only place in the repository that names an image reference and the two
can never drift. The
server image uses the local image name `synthesis-server`, which carries no
registry host, so a local image is never mistaken for the published one.

The server image receives the resolved version as the build argument
`SYNTHESIS_BUILD_VERSION`, so a container started from it reports that version
from `GET /v1/health`. The two vendor images take no version build argument and
receive none: each pins its vendor CLI and its browser in its own Dockerfile.

`docker` builds each image for the platform of the host it runs on and produces
no multi-platform manifest list. Publication of the server image for both
supported platforms belongs to the release workflow.

An image build that exits non-zero stops the task at once, and the diagnostic
names the image that failed. A host with no container runtime is reported as one
diagnostic that names the runtime, rather than as a partially built set reported
as success.

#### The one shared version tag

Before the first image build, `docker` resolves one local build version, from
the first of these sources that yields a value:

1. the exact Git tag at `HEAD`, when `HEAD` carries one;
2. the short Git commit hash, when Git metadata is readable and no tag points at
   `HEAD`;
3. the literal string `undefined`.

This is the Git part of the version resolution of the server crate, in the same
order. A Git command that fails, and a Git executable that is absent, do not
fail the task: the resolution falls to the next source, so a copy of the source
that holds no Git metadata resolves `undefined` and the task continues.

The version is resolved once for each invocation and is the tag of every image
that invocation builds. The task adds no suffix for an unclean working tree, no
timestamp, no branch name, and no other value. The tags are therefore
deterministic: the same resolved version produces the same tag for the same
image on every machine, and a second run on an unchanged working tree produces
the same three tags. **No image is tagged `latest`.** When the resolved version
is `undefined`, the tag is the literal string `undefined`, which is a valid tag
and which reads as the same "no version was resolvable" that `GET /v1/health`
reports from an image built that way.

#### `task docker` publishes nothing

It pushes no image, logs in to no registry, reads no registry credential and no
repository secret, writes no digest or reference into
`docker/agent-images/manifest.toml`, and changes no release metadata. Every
image it produces exists in the local image store of the machine that ran it and
nowhere else.

A locally built vendor image therefore does not become the image the application
runs. The executor runs the image that the open project names for the vendor in
`.synthesis/project.toml`, under `[dockerImages.<vendor>]`. It has no fallback to
a shipped image. A local build runs only when a project names it there.

### `task tests`

| Property | Value |
| --- | --- |
| Commands | the commands of the four verification lanes, in the pipeline's order |
| Working directory | each lane's own directory |
| Output | none. It verifies the working tree |
| Supported hosts | any host that satisfies the lanes' own prerequisites |
| Prerequisites | Node.js, pnpm and the installed dependencies, and the pinned Rust toolchain |
| Failure | non-zero, with output naming the lane that failed |

`tests` runs every verification lane
[`specifications/infra/CIP-ci-pipeline.md`](../specifications/infra/CIP-ci-pipeline.md)
defines, with each lane's commands in that specification's order and in each
lane's working directory. It adds no command of its own and changes none of
theirs.

| Lane | Commands | Working directory |
| --- | --- | --- |
| specification | `task specs` | see below |
| frontend | `pnpm test`, `pnpm build`, `pnpm exec tsc -p tsconfig.test.json --noEmit` | the repository root |
| application backend | `task tests:backend` | see below |
| agentic CLI mock | `cargo check`, `cargo test` | `tools/agentic-cli-mock/` |
| server | `cargo build --locked`, `cargo test --locked` | `server/` |

The specification lane and the application backend lane are reached through
`specs` and `tests:backend` rather than by restating their commands, so the two
can never name different commands or different directories.

`tests` reports no GitHub check and blocks no merge: `CI / gate` stays the only
required check. Because the commands are the lanes' own, a green `task tests`
predicts a green pipeline run for the same working tree.

It runs the lanes' commands and not the pipeline's own setup steps: the pipeline
installs the frontend dependencies with `pnpm install --frozen-lockfile` and
copies `src-tauri/rust-toolchain.toml` into `server/` and
`tools/agentic-cli-mock/` before their cargo steps, and `task tests` does
neither. Do both by hand to make a local run match a pipeline run exactly.

### `task specs`

| Property | Value |
| --- | --- |
| Commands | `python3 tools/spec-check/spec_check.py` |
| Working directory | the repository root |
| Output | one line per violation, then a summary counting specs, identifiers and citations |
| Hosts | any host carrying Python 3 |
| Prerequisites | Python 3. No Node, no Rust toolchain, and no install step |
| Fails when | any specification is malformed, two specifications share a code, or a cited requirement identifier resolves to nothing |

Every requirement and scenario in this project is named by identifier from Rust
and TypeScript comments, from test names, and from other specifications. `specs`
reads those references and reports each one that no specification defines, which
is what stops a rename or a retired identifier from breaking the corpus
silently. It is read-only and writes nothing. See
[`specifications/infra/SPC-specification-consistency.md`](../specifications/infra/SPC-specification-consistency.md).

### `task tests:backend`

| Property | Value |
| --- | --- |
| Commands | `cargo check`, then `cargo test` |
| Working directory | `src-tauri/` |
| Output | none. It verifies the crate |
| Supported hosts | any host that builds the application backend |
| Prerequisites | the pinned Rust toolchain, and the system packages the crate links against |
| Failure | non-zero. Either command exiting non-zero fails the task at once, and the command after it does not run |

## Development

Day-to-day commands stay runnable by hand. Every command the task file holds can
be typed directly in the working directory the task names, with the same
outcome.

```bash
pnpm install                # obtain the frontend dependencies
pnpm tauri dev              # run the full desktop application
pnpm dev                    # the frontend alone, on port 1420
cd src-tauri && cargo check # a quick backend check
```

Read [`CLAUDE.md`](../CLAUDE.md) for the repository's own rules and the definition
of done for a change. Read [`README.md`](README.md) in this directory for the
architecture.

## Recommended IDE setup

[VS Code](https://code.visualstudio.com/) with the
[Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode)
and
[rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
extensions.
