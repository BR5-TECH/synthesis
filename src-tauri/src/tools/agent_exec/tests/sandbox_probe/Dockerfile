# EAC-FR-RVQE: the probe image. It stands in for a vendor CLI in a real-runtime
# test. It runs fixed checks in its execution directory and then answers through
# `agentic-cli-mock`, so a turn ends cleanly with no real agent.
#
# The test copies this file, `probe.sh`, a scenario template, and the mock crate
# into a temporary build context. Nothing else of the repository is sent.

FROM rust:1.95-slim-bookworm AS mock
WORKDIR /build
COPY agentic-cli-mock/Cargo.toml agentic-cli-mock/Cargo.lock ./
COPY agentic-cli-mock/src ./src
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends git ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=mock /build/target/release/agentic-cli-mock /opt/probe/agentic-cli-mock
COPY probe.sh /opt/probe/probe.sh
COPY scenario.template.json /opt/probe/scenario.template.json

# The tool the probe looks for through the image `PATH` alone, as the agent
# images put their toolchain on `PATH` (AVI-FR-17).
RUN mkdir -p /opt/probe/bin \
    && printf '#!/bin/sh\necho probe-tool\n' > /opt/probe/bin/probe-tool \
    && chmod -R a+rx /opt/probe

# The same user and home as the agent images (AVI-FR-03), so a mount shows
# what it shows to an agent.
RUN groupadd --gid 10001 agent \
    && useradd --uid 10001 --gid 10001 --create-home --home-dir /home/agent agent \
    && chmod -R a+rwX /home/agent \
    && mkdir -p /workspace && chown 10001:10001 /workspace

# `GIT_OPTIONAL_LOCKS=1` is here so the probe can show that the launch's own
# value replaces an image value (EAC-FR-PWLA).
ENV HOME=/home/agent \
    PATH=/opt/probe/bin:$PATH \
    GIT_OPTIONAL_LOCKS=1

USER 10001:10001
WORKDIR /workspace
ENTRYPOINT ["/opt/probe/probe.sh"]
