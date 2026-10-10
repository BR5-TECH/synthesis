# Synthesis agent image — Codex (AVI-FR-01).
#
# The container the agent-CLI executor launches when a project resolves to
# Codex. Structurally identical to the Claude Code image — same base family,
# same package floor, same UID/GID, same working directory, same entrypoint
# contract — so the executor's invocation differs between the two only in the
# vendor arguments themselves.
FROM node:22-bookworm-slim@sha256:d649c27dae7ba0137b3cef5dd75baa422c08dc3d9e3fc0c23dfb172dc3cc6436

# AVI-FR-02: the same package floor as the Claude Code image and nothing more —
# CA certificates, curl, Python with pip and venv, the C compiler and linker the
# Rust toolchain of AVI-FR-17 builds through, and the shared libraries and font
# the browser of AVI-FR-16 needs, for the reasons that image states. Git is
# present here for the reason it is present there: a turn that revises or judges
# a change set reads what changed rather than every file whole. What the client
# can reach stays the invocation's decision — read-only repository metadata for
# the turns entitled to it, and a mask for the semantic-rebase turn
# (`../../../specifications/tools/EAC-execute-agent-cli.md` EAC-FR-41,
# EAC-FR-FNFV). This vendor keeps `--skip-git-repo-check` in its own argument
# vector regardless: a masked turn stands in a directory whose repository it
# cannot read, and the flag is what stops the CLI refusing to start there.
#
# `pkg-config`, `libssl-dev`, and `libwebkit2gtk-4.1-dev` are the *native* floor
# under that Rust toolchain. A Rust crate that binds a system library does not
# carry that library: its `-sys` build script asks `pkg-config` where the
# headers are and stops the build when the answer is missing. The application
# this image builds is a desktop shell, so `tauri` pulls `glib-sys`,
# `gobject-sys`, and the GTK and WebKit bindings under them, and its Git support
# pulls `openssl-sys` and `libssh2-sys`. Without these three, `cargo check`
# stops inside a transitive build script before it reads one line of the
# repository's own code — which is a compiler that reports its version and can
# compile nothing the work is made of.
#
# The `-dev` packages, not stub `.pc` files. A stub gets a build past
# `pkg-config` and then measures the code against libraries that are not
# installed, which is a pass that means nothing. These three are the smallest
# set that makes `cargo test` compile, link, and run.
RUN apt-get update \
    && apt-get install --no-install-recommends --yes \
        build-essential \
        ca-certificates \
        curl \
        fonts-liberation \
        git \
        libasound2 \
        libatk-bridge2.0-0 \
        libatk1.0-0 \
        libatspi2.0-0 \
        libcairo2 \
        libcups2 \
        libdbus-1-3 \
        libdrm2 \
        libgbm1 \
        libglib2.0-0 \
        libnspr4 \
        libnss3 \
        libpango-1.0-0 \
        libssl-dev \
        libwebkit2gtk-4.1-dev \
        libx11-6 \
        libxcb1 \
        libxcomposite1 \
        libxdamage1 \
        libxext6 \
        libxfixes3 \
        libxkbcommon0 \
        libxrandr2 \
        pkg-config \
        python3 \
        python3-pip \
        python3-venv \
    && rm -rf /var/lib/apt/lists/*

# AVI-FR-17: the language toolchains the work is written in — Rust, and the
# Node side's package manager and TypeScript compiler. An agent asked to change
# a crate or a module has to build what it changed and read what the build says
# about it. Without a compiler it can only describe an edit, which is the same
# gap the missing browser of AVI-FR-16 was.
#
# Each is pinned to an exact version, for the reason AVI-FR-06 pins the vendor
# CLI: a rebuild of an unchanged definition must produce the same toolchain.
# `rustup-init` is the one part of this image that arrives over the network
# without a digest of its own, so it is fetched by version from the archive and
# checked against a recorded SHA-256 before it is allowed to run. The build
# stops on an architecture this file has no checksum for, rather than falling
# back to an unverified download.
#
# The Rust toolchain lives in `/usr/local`, not in a home directory, for the
# reason the browser does: the executor launches a container as the host user's
# UID (EAC-FR-14), and that UID has no account in this image. `RUSTUP_HOME` and
# `CARGO_HOME` name the two paths, and their *directories* are writable by every
# user, which the browser path deliberately is not: Cargo writes its registry
# index and its package cache into `CARGO_HOME` on every build, and rustup
# writes a toolchain there when a repository's `rust-toolchain.toml` asks for a
# version this image does not carry.
#
# Directories only, not `-R`. Everything those two write is a new file or a new
# directory, which needs its parent writable and nothing else, so a recursive
# `a+w` would add nothing but a compiler an agent can overwrite. Both values are
# container paths, so AVI-FR-10 is unaffected.
ARG RUST_VERSION=1.95.0
ARG RUSTUP_VERSION=1.29.0
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo
RUN set -eux; \
    arch="$(dpkg --print-architecture)"; \
    case "$arch" in \
        amd64) \
            target='x86_64-unknown-linux-gnu'; \
            sha256='4acc9acc76d5079515b46346a485974457b5a79893cfb01112423c89aeb5aa10'; \
            ;; \
        arm64) \
            target='aarch64-unknown-linux-gnu'; \
            sha256='9732d6c5e2a098d3521fca8145d826ae0aaa067ef2385ead08e6feac88fa5792'; \
            ;; \
        *) \
            echo "no rustup-init checksum recorded for architecture $arch" >&2; \
            exit 1; \
            ;; \
    esac; \
    curl --fail --silent --show-error --location \
        --output /tmp/rustup-init \
        "https://static.rust-lang.org/rustup/archive/${RUSTUP_VERSION}/${target}/rustup-init"; \
    echo "${sha256}  /tmp/rustup-init" | sha256sum --check --strict -; \
    chmod +x /tmp/rustup-init; \
    /tmp/rustup-init -y --no-modify-path --profile minimal \
        --default-toolchain "${RUST_VERSION}" \
        --component rustfmt --component clippy; \
    rm /tmp/rustup-init; \
    find "${RUSTUP_HOME}" "${CARGO_HOME}" -type d -exec chmod a+w {} +; \
    /usr/local/cargo/bin/rustc --version; \
    /usr/local/cargo/bin/cargo --version

# AVI-FR-17: pnpm because it is the package manager the work uses, and the
# TypeScript compiler because a change to a typed module is unchecked until
# `tsc` has read it. Both are installed globally by npm, which the base image
# already carries, so neither adds a package manager of its own.
ARG PNPM_VERSION=11.21.0
ARG TYPESCRIPT_VERSION=5.8.3
RUN npm install --global --no-fund --no-audit \
        "pnpm@${PNPM_VERSION}" \
        "typescript@${TYPESCRIPT_VERSION}" \
    && npm cache clean --force

# AVI-FR-06: an exact version, never a floating tag.
ARG CODEX_VERSION=0.147.0
RUN npm install --global --no-fund --no-audit \
        "@openai/codex@${CODEX_VERSION}" \
    && npm cache clean --force

# AVI-FR-16: a browser, so work on an interface can be seen rather than guessed
# at. An agent that changes a view and cannot open it has only the diff to
# report, and a screenshot is the difference between "the button is there" and
# "the code that draws the button is there".
#
# Playwright is what drives it, pinned to an exact version for the reason
# AVI-FR-06 pins the vendor CLI: the browser build a version installs is that
# version's own, and a floating install would put a different browser in a
# rebuild of an unchanged definition. Chromium alone — one engine is what
# checking a view needs, and each of the others is another 300 MB.
ARG PLAYWRIGHT_VERSION=1.62.1

# The browser is installed to a fixed path, not to a home cache. This layer runs
# as root, so a default install would land in root's own cache, which the UID
# the executor supplies (EAC-FR-14) cannot read. `PLAYWRIGHT_BROWSERS_PATH`
# names a path outside any home instead, and `a+rX` makes it readable and
# executable for whatever UID the launch supplies.
#
# That path is deliberately not writable by that user: the browser is part of
# the image, at the version this file pins. A task that needs a different build
# of it points `PLAYWRIGHT_BROWSERS_PATH` at a directory of its own under
# `/workspace`, which is where a task's own downloads belong.
#
# Its value, like that of every other `ENV` in this image, is a container path
# this file chose. AVI-FR-10 keeps every credential out of every layer, and an
# environment variable is a layer.
ENV PLAYWRIGHT_BROWSERS_PATH=/opt/ms-playwright
RUN npm install --global --no-fund --no-audit \
        "playwright@${PLAYWRIGHT_VERSION}" \
    && playwright install chromium \
    && chmod -R a+rX /opt/ms-playwright \
    && npm cache clean --force

# `HOME`, `PNPM_HOME`, and `PATH` are set here rather than left to the runtime.
# A container runtime derives a home directory from the passwd entry of the
# user it starts, and the host UID the executor supplies (EAC-FR-14) has no
# entry in this image — so `$HOME` would be `/`, which nothing may write. The
# vendor CLI's own scratch state, npm's cache, and pnpm's package store all
# land under `$HOME`, and each of them fails on a home it cannot write.
# Naming the directory here, and making it writable for whatever UID the launch
# supplies, is what keeps all three working; the container belongs to one agent
# alone, so a home every user can write is not a boundary being crossed.
#
# `PNPM_HOME` is pnpm's own directory, and pnpm refuses a global install unless
# the `bin` directory below it is on `PATH` — so that directory is on `PATH`
# here, next to Cargo's, which is what puts `cargo`, `rustc`, `rustfmt`, and
# `clippy` in reach without a path in front of them.
#
# Every one of these values is a container path. AVI-FR-10 is unaffected.
ENV HOME=/home/agent \
    PNPM_HOME=/home/agent/.local/share/pnpm \
    PATH=/usr/local/cargo/bin:/home/agent/.local/share/pnpm/bin:$PATH

# AVI-FR-03: the same fixed non-root UID/GID as the Claude Code image.
#
# `/home/agent` is load-bearing here in a way it is not for Claude Code: it is
# the mount target for the host's Codex login directory (AIC-FR-30), so the
# path the container expects and the home this user is given have to agree.
# `.codex` is created and owned in advance so a read-only bind mount lands on a
# directory that already belongs to the agent user.
RUN groupadd --gid 10001 agent \
    && useradd --uid 10001 --gid 10001 --create-home --home-dir /home/agent agent \
    && mkdir -p /home/agent/.codex /home/agent/.local/share/pnpm/bin \
    && chown -R 10001:10001 /home/agent \
    && chmod -R a+rwX /home/agent

# AVI-FR-11: the executor's mount target for the execution directory.
RUN mkdir -p /workspace && chown 10001:10001 /workspace

USER 10001:10001
WORKDIR /workspace

# AVI-FR-11: no VOLUME, no EXPOSE.
# AVI-FR-04: the vendor CLI is the entrypoint.
ENTRYPOINT ["codex"]
