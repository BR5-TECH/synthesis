# Agent vendor images

The two container images Synthesis runs an agentic CLI inside, and the manifest
that pins them. Specification: `specifications/infra/AVI-agent-vendor-images.md`.

An agent driven on the author's behalf needs a working tree, a network route to
its vendor's service, and nothing else this machine happens to have. Giving it
one of these images instead of the host's own installation is what makes a turn
reproducible from the build rather than dependent on which version of which CLI
a developer last upgraded.

## What is here

| Path | What it is |
|---|---|
| `claude-code/Dockerfile` | The Claude Code image definition (AVI-FR-01). |
| `codex/Dockerfile` | The Codex image definition (AVI-FR-01). |
| `manifest.toml` | The pinned identity of both images (AVI-FR-07). |
| `README.md` | This file. |

## What is in an image

Each image installs its vendor CLI, the packages its work needs — CA
certificates, curl, Python with `pip` and `venv`, the C compiler and linker
of `build-essential`, and `pkg-config` with the OpenSSL and WebKitGTK
development packages — one headless browser, the language toolchains the work
is written in, and **nothing else** (AVI-FR-02, AVI-FR-16, AVI-FR-17). No
editor, no second agentic CLI, no Synthesis source, and **no Git**.

Python is on that list because repository work asks for it directly: a script to
run, a test suite to start, a tool that is written in it. `python3-pip` and
`python3-venv` come with the interpreter so an agent can install what a task
needs into an environment of its own — Debian marks the system environment
externally-managed, and a virtual environment is the answer to that rather than
a flag that overrides it. No Dockerfile names a Python version: the interpreter
is the one the pinned base carries (Python 3.11 today), so a rebuild reproduces
it for the same reason it reproduces everything else in that layer.

`pkg-config`, `libssl-dev`, and `libwebkit2gtk-4.1-dev` are the native floor
beneath the Rust toolchain. A Rust crate that binds a system library does not
carry that library: its `-sys` build script asks `pkg-config` where the headers
are and stops the build when the answer is missing. The work is a desktop
shell, so `tauri` pulls the GTK and WebKit bindings and its Git support pulls
the OpenSSL ones — without these three, `cargo check` stops inside a transitive
build script before it reads a line of the repository's own code. The
development packages themselves, never a stub `.pc` file: a stub gets the build
past `pkg-config` and then measures the code against libraries the image does
not hold, which is a pass that means nothing.

Git is absent by decision. `/workspace` is the only host directory a container
is given (EAC-FR-13), a run's working copy keeps its Git metadata outside that
directory, and a Git command in a container could therefore never read the
repository it appears to stand in. Every Git operation belonging to a run is the
application's, performed on the host where the whole repository is reachable.

- **Base image** — `node:22-bookworm-slim`, pinned by digest rather than by tag
  (AVI-FR-05), so a rebuild reproduces the same foundation — including the
  Python it carries.
- **Vendor CLI** — installed at an exact version (AVI-FR-06). Never `latest`,
  and never a binary copied in from the host.
- **User** — a non-root `agent` at UID/GID **10001/10001** (AVI-FR-03), with
  `/home/agent` as its home. The executor maps the host user onto these
  (`EAC-FR-14`), which is what keeps a file the agent creates in the mounted
  worktree owned by the person who launched it.
- **Working directory** — `/workspace` (AVI-FR-11), where the executor mounts
  the execution directory. No `VOLUME` and no `EXPOSE`: what a container can
  reach is decided by the invocation, never partly by the image.
- **Entrypoint** — the vendor CLI itself (AVI-FR-04), so the arguments the
  executor generates are the CLI's own and nothing wraps or rewrites them.
- **Browser** — Chromium at a pinned Playwright version (AVI-FR-16), at
  `/opt/ms-playwright`. See below.
- **Toolchains** — Rust, pnpm, and the TypeScript compiler, each at a pinned
  version (AVI-FR-17). See below.

### The toolchains

**AVI-FR-17.** An agent asked to change a crate or a module has to build what it
changed and read what the build says about it. Without a compiler it can only
describe an edit, which is the same gap the missing browser was.

| Tool | Pinned by | Where it lives |
|---|---|---|
| Rust — `rustc`, `cargo`, `rustfmt`, `clippy` | `ARG RUST_VERSION`, installed by `rustup` at `ARG RUSTUP_VERSION` | `/usr/local/rustup`, `/usr/local/cargo` |
| pnpm | `ARG PNPM_VERSION` | npm global |
| TypeScript — `tsc` | `ARG TYPESCRIPT_VERSION` | npm global |

`rustup-init` is the one part of an image that arrives over the network without
a digest of its own, so it is fetched by version from the release archive and
checked against a SHA-256 the Dockerfile records, per architecture. An
architecture the file has no checksum for stops the build rather than falling
back to an unverified download.

`build-essential` is the Rust toolchain's floor. `rustc` compiles to object code
and then calls a C linker to make a program out of it, so a Rust compiler with
no linker cannot produce a binary at all.

The Rust toolchain lives in `/usr/local`, not in a home directory, for the same
reason the browser does — but unlike the browser path, `RUSTUP_HOME` and
`CARGO_HOME` are writable by every user. Cargo writes its registry index and its
package cache into `CARGO_HOME` on every build, and `rustup` writes a toolchain
there when a repository's `rust-toolchain.toml` asks for a version the image
does not carry.

### The home directory

`HOME` is set by the image rather than left to the runtime:

```
HOME=/home/agent
PNPM_HOME=/home/agent/.local/share/pnpm
```

A container runtime derives a home from the passwd entry of the user it starts,
and the host UID the executor supplies (`EAC-FR-14`) has no entry in the image —
so `$HOME` would be `/`, which nothing may write. The vendor CLI's scratch
state, npm's cache, and pnpm's package store all land under `$HOME`, and each of
them fails on a home it cannot write. `/home/agent` is therefore writable by
whatever UID the launch supplies; the container belongs to one agent alone, so
that is not a boundary being crossed.

`$PNPM_HOME/bin` is on `PATH` next to `/usr/local/cargo/bin` — pnpm refuses a
global install when its bin directory is not on the path.

### The browser

**AVI-FR-16.** An agent asked to change an interface can open the result and
look at it, instead of reporting a diff and calling that a check. Each image
carries **Chromium** and the **Playwright** runtime that drives it, both pinned
to an exact version (`ARG PLAYWRIGHT_VERSION` in each Dockerfile — the same
version in both). Chromium alone: a second engine is several hundred megabytes
for a capability no check of a view needs. The `lib*` packages and
`fonts-liberation` in the apt list are its floor — the shared libraries that
build links against, and one font family, because a browser with no font draws
a page of empty boxes.

The browser lives at **`/opt/ms-playwright`**, named by an environment
variable:

```
PLAYWRIGHT_BROWSERS_PATH=/opt/ms-playwright
```

That path, rather than the agent user's `~/.cache/ms-playwright`, because the
executor launches a container as the **host** user's UID (`EAC-FR-14`) and that
UID has no account inside the image — a browser installed under a user's own
cache directory would be unreachable by the process that needs it. The
directory is world-readable and world-executable, and
deliberately **not** writable: the browser is part of the image, at the version
the image pins.

From inside a container:

```sh
# A rendered page, straight from the CLI.
playwright screenshot --browser chromium http://localhost:1420 /workspace/shot.png

# A project's own Playwright finds the same browser, as long as the project
# pins the same version. A project on a different version installs its own:
PLAYWRIGHT_BROWSERS_PATH=/workspace/.browsers npx playwright install chromium
```

The browser starts under the executor's launch conditions — every capability
dropped, no new privileges, an unknown UID — with no extra flags. `verify.sh`
proves that against the built image rather than leaving it to a first run.

### The Codex login mount

The Codex image pre-creates `/home/agent/.codex`, owned by the agent user. That
path is the mount target for the host's own Codex login directory, which the
executor mounts **read-only** for one container (`EAC-FR-16`). The target and
the user's home have to agree, which is why the path is fixed in both the image
and the vendor descriptor (`AIC-FR-30`).

## No credential is ever built in

**AVI-FR-10.** No image contains an OAuth token, an API key, a vendor login
directory, or any keychain material — in any layer, environment variable, or
file. Every credential reaches a container at run time, by the mechanisms the
executor defines: Claude Code's token as one environment variable on one
container (`EAC-FR-15`), Codex's login directory as a read-only mount
(`EAC-FR-16`).

Adding a credential to a Dockerfile would put it in a layer that outlives the
container, travels with every push, and is readable by anyone who can pull the
image. There is no build argument, no secret mount, and no `.env` convention
here for that reason.

Every environment variable an image sets — `PLAYWRIGHT_BROWSERS_PATH`,
`RUSTUP_HOME`, `CARGO_HOME`, `HOME`, `PNPM_HOME`, and `PATH` — holds a container
path this repository chose. A path is not a credential, and those six variables
are the whole of what an image's environment says.

## The manifest

`manifest.toml` is the single record of each image's reference, digest, CLI
version, UID, and GID (AVI-FR-07), and the only place the repository names any
of them. `EAC`'s vendor execution descriptors read it; nothing else does.

An image's identity downstream is its **digest**, not its tag (AVI-FR-08). The
executor pins by digest, so an image republished under the same tag is never
silently adopted by a running application.

### The build revision in a tag

A tag is the CLI version followed by a build revision — `2.1.233-2` is the
second published definition holding Claude Code 2.1.233. The rest of an image
changes independently of its vendor CLI: a package added to the floor, a
toolchain raised, a browser repinned. Publishing any of those over an unchanged
tag is exactly the substitution AVI-FR-08 exists to prevent, so **raise the
revision in the same change that touches a Dockerfile**, and record the digest
the push reported beside it.

### The unpublished sentinel

Until an image has actually been pushed there is no digest to record, and the
manifest carries an all-zero sentinel instead:

```
sha256:0000000000000000000000000000000000000000000000000000000000000000
```

This is a real value with a real meaning — *defined but not yet published* —
rather than a blank to be overlooked. `verify` fails on it by design, and a
launch against it fails as `ImageUnavailable`, which is exactly what it is.
Replace it with the digest `docker push` reported, in the same commit that
published the image.

## Commands

Building and publishing are steps a developer or a release runs **deliberately**
(AVI-FR-09). Nothing in the application builds an image, pulls one outside the
executor's pinned pull, or updates one automatically.

### Build

```sh
docker build \
  --file docker/agent-images/claude-code/Dockerfile \
  --tag ghcr.io/br5-tech/synthesis-agent-claude-code:<version> \
  docker/agent-images/claude-code

docker build \
  --file docker/agent-images/codex/Dockerfile \
  --tag ghcr.io/br5-tech/synthesis-agent-codex:<version> \
  docker/agent-images/codex
```

#### Both images at once — `task docker`

`task docker` builds both images, and the server image, from the repository
root. It is the same deliberate step under one name, documented in full in the
[repository README](../../README.md) and specified by
[`specifications/infra/TSK-taskfile.md`](../../specifications/infra/TSK-taskfile.md).

```sh
task docker
```

It resolves one local build version for the whole invocation — the exact Git tag
at `HEAD`, otherwise the short Git commit hash, otherwise the literal string
`undefined` — and tags each vendor image with the repository portion of its
`image_ref` in `manifest.toml` at that version:

```
ghcr.io/br5-tech/synthesis-agent-claude-code:<resolved version>
ghcr.io/br5-tech/synthesis-agent-codex:<resolved version>
```

The tag is never `latest`. Neither image receives a version build argument: each
pins its vendor CLI and its browser in its own Dockerfile.

**It publishes nothing, and it does not touch the manifest.** It pushes no
image, logs in to no registry, reads no credential, and writes no digest or
reference into `manifest.toml`. A locally built image therefore never becomes
the image the application runs: the executor still pins by the digest the
manifest records.

#### While an image is unpublished

That last sentence holds only once a digest has been recorded. While an entry
in `manifest.toml` carries the all-zero sentinel, there is no digest to pin to,
so the executor addresses that image by its `image_ref` — repository **and
tag** — and the tag it names is the CLI version with a build revision after it
(`2.1.233-2`), not the version `task docker` resolves. The two are different tags, and `task docker` does not build the one
the application launches.

So on a machine running an unpublished image, the tag the executor asks for is
whatever the local image store last put there — which may be an image built
before the toolchains of AVI-FR-17 existed, and an agent given no compiler
cannot build what it changed. Build that exact tag when the Dockerfile changes:

```sh
docker build \
  --file docker/agent-images/claude-code/Dockerfile \
  --tag "$(awk -F'\"' '/^\[claude_code\]/{f=1} f&&/^image_ref/{print $2; exit}' docker/agent-images/manifest.toml)" \
  docker/agent-images/claude-code
```

Every launch against an unpinned entry writes a `WARN` naming the image, so a
stale tag is visible in the log rather than only in what the agent could not do.

### Publish

```sh
docker push ghcr.io/br5-tech/synthesis-agent-claude-code:<version>
```

Then record the digest `push` reported in `manifest.toml`, replacing the
sentinel. The manifest is updated only **after** the digest it records exists in
the registry.

### Verify

```sh
docker/agent-images/verify.sh
```

Resolves each manifest digest and confirms the image at that digest reports the
manifest's `cli_version`, runs as the manifest's UID and GID, and carries the
Python floor — an interpreter the agent user can build a virtual environment
with and install into.

The rest of it runs under the executor's own launch conditions (AVI-FR-13) — a
UID with no account in the image, every capability dropped, no new privileges —
because that is the only shape in which the remaining claims mean anything:

- `$HOME` is `/home/agent` and the running user can write it.
- `CARGO_HOME` and `RUSTUP_HOME` can be written too, and a crate compiles,
  links, runs, lints, and formats.
- `pkg-config` answers for `glib-2.0`, `gtk+-3.0`, `webkit2gtk-4.1`, and
  `openssl` — the native floor a `-sys` build script asks it for, without which
  a build stops inside a transitive dependency.
- `tsc` emits JavaScript the runtime can run, and `pnpm install` writes its
  store.
- Each toolchain reports the version its Dockerfile pins, which an `ARG`
  overridden at build time would not.
- The browser starts and renders a page.

Nothing in it writes only to `/tmp`: `/tmp` is writable by every UID whatever
the image did, so a probe that stayed there would prove none of the above. A
manifest that has drifted from what was published fails a check rather than
failing a launch. It fails on the unpublished sentinel.

## Upgrading a vendor CLI

1. Change the pinned version in that vendor's `Dockerfile` (the `ARG` default).
2. Build, push, and record the new `image_ref`, `image_digest`, and
   `cli_version` in `manifest.toml`.
3. Run `verify.sh`.

Nothing else in the repository changes: the executor's descriptors read the
manifest, so no call site names a version or a digest (AVI-FR-07). A turn run
before the change and a turn run after it are distinguishable by the digest each
ran under.

Upgrading the browser or a toolchain is the same shape, with one extra rule:
change `PLAYWRIGHT_VERSION`, `RUST_VERSION`, `RUSTUP_VERSION`, `PNPM_VERSION`,
or `TYPESCRIPT_VERSION` in both Dockerfiles together. The two images share
one package floor, and a tool that differs between them makes a check depend on
which vendor a project happens to use. A change to `RUSTUP_VERSION` also means
new SHA-256 values, one per architecture, taken from
`https://static.rust-lang.org/rustup/archive/<version>/<target>/rustup-init.sha256`.

## Why these images are not built in CI

**AVI-FR-15.** `.github/workflows/ci.yml` gains no lane for them, reads no
registry secret, and its existing lanes are unaffected.

The executor's own tests reach the container runtime through its `DockerRuntime`
seam (`EAC-FR-17`), where `agentic-cli-mock` stands in for `docker` and asserts
the exact generated invocation offline. No test builds, pulls, or runs an image
defined here (AVI-FR-12), and production image resolution is identical under
test and in a shipped build — so there is no test-image variant to keep in sync.
