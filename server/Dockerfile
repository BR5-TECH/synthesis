# The image of the Synthesis backend microservice.
#
# Specification: specifications/server/BMS-backend-microservice.md
# Requirements: BMS-FR-17, BMS-FR-18, BMS-FR-19, BMS-FR-20, BMS-FR-21.
#
# Build the image from this directory, so the build context holds the crate and
# no checkout metadata:
#
#   docker build -t synthesis-server:local .
#   docker build --build-arg SYNTHESIS_BUILD_VERSION=v2.0.0 -t synthesis-server:v2.0.0 .
#
# BMS-FR-21: both platforms come from this one file. Each stage is resolved for
# the platform that is built, so `docker buildx build --platform
# linux/amd64,linux/arm64` compiles each architecture with that architecture's
# own compiler and publishes the two as one manifest list.

# ---------------------------------------------------------------------------
# Stage 1 — build the binary.
#
# The compiler, the toolchain, the source tree, and the build cache stay here.
# No layer of this stage reaches the final image (BMS-FR-17).
# ---------------------------------------------------------------------------
FROM rust:1.95-bookworm@sha256:6258907abe69656e41cd992e0b705cdcfabcbbe3db374f92ed2d47121282d4a1 AS build

# BMS-FR-14: the release tag the image is built from. The build script reads it
# and compiles the value into the binary. When it is not supplied, and the
# context holds no checkout, the version resolves to `undefined`.
ARG SYNTHESIS_BUILD_VERSION

WORKDIR /src

# Only the crate is copied. The build cache and the target directory of a
# developer's machine stay out of the image (see .dockerignore).
COPY Cargo.toml Cargo.lock build.rs ./
COPY src ./src

# --locked: the committed Cargo.lock is the sole source of dependency versions,
# so a lock file that disagrees with the manifest fails the build rather than
# being regenerated here (BMS-FR-24).
RUN cargo build --locked --release

# ---------------------------------------------------------------------------
# Stage 2 — the image that ships.
#
# BMS-FR-17: a distroless base, addressed by an immutable digest rather than by
# a tag alone, running as that base's non-root user.
# BMS-FR-18: it holds the binary and the CA certificate bundle the base
# provides. It holds no shell, no package manager, no compiler, no interpreter,
# and no source file.
# BMS-FR-20: no layer, variable, or file of it holds a credential. The service
# needs none to start.
# ---------------------------------------------------------------------------
FROM gcr.io/distroless/cc-debian12:nonroot@sha256:adcd20c7b4c988b73cbfbddb26d2eee574571e6d7c9ffea29b3821e0690efb77

COPY --from=build /src/target/release/synthesis-server /usr/local/bin/synthesis-server

# The non-root user of the base image. The default port is above 1024, so the
# process binds it without an added capability (BMS-FR-06, BMS-FR-17).
USER 65532:65532

# The default port of BMS-FR-05. No volume is declared: the service is stateless
# and writes nothing (BMS-FR-19).
EXPOSE 8080

# BMS-FR-19: the entrypoint is the binary itself, so the arguments a container is
# started with are the binary's own arguments and nothing wraps or rewrites them.
ENTRYPOINT ["/usr/local/bin/synthesis-server"]
